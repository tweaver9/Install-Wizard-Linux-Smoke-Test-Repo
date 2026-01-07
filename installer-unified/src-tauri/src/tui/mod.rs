//! Headless Linux Terminal UI (TUI) wizard.
//!
//! Requirements (UI spec):
//! - Centered "installer window" frame titled "CADalytix Setup"
//! - Left banner panel with ASCII logo
//! - Main content panel with classic wizard pages (no stepper)
//! - Bottom button row: [ Back ] [ Next ] [ Cancel ]
//! - Modal confirmations (Cancel, replace mapping, etc.)
//!
//! Note: Logging is file-only in TUI mode (stdout logging is disabled) to avoid corrupting the terminal UI.

use crate::api::installer::{
    self, ArchivePolicyConfig, ArchiveScheduleConfig, HotRetentionConfig, InstallArtifacts,
    MappingSourceField, MappingState, MappingTargetField, ProgressEmitter, ProgressPayload,
    StartInstallRequest, StorageConfig,
};
use crate::api::preflight;
use crate::models::requests::PreflightDataSourceRequestDto;
use crate::models::responses::DiscoveredColumnDto;
use crate::security::secret_protector::SecretProtector;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use log::info;
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Terminal;
use std::collections::HashMap;
use std::io::{self, Stdout};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

const ASCII_LOGO: &str = r#"██████╗ █████╗ ██████╗  █████╗ ██╗  ██╗   ██╗████████╗██╗██╗  ██╗
██╔════╝██╔══██╗██╔══██╗██╔══██╗██║  ╚██╗ ██╔╝╚══██╔══╝██║╚██╗██╔╝
██║     ███████║██║  ██║███████║██║   ╚████╔╝    ██║   ██║ ╚███╔╝
██║     ██╔══██║██║  ██║██╔══██║██║    ╚██╔╝     ██║   ██║ ██╔██╗
╚██████╗██║  ██║██████╔╝██║  ██║███████╗██║      ██║   ██║██╔╝ ██╗
╚═════╝╚═╝  ╚═╝╚═════╝ ╚═╝  ╚═╝╚══════╝╚═╝      ╚║   ╚═╝╚═╝  ╚═╝"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallMode {
    Windows,
    Docker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Platform,
    Welcome,
    License,
    InstallType,
    Destination,
    DataSource,
    Database,
    Storage,
    Retention,
    Archive,
    Consent,
    Mapping,
    Ready,
    Installing,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ButtonFocus {
    Back,
    Next,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Modal {
    ConfirmCancel,
    Message {
        title: String,
        body: String,
        return_to: Option<Page>,
    },
    BrowseFolder {
        current: std::path::PathBuf,
        entries: Vec<std::path::PathBuf>,
        selected: usize,
    },
    ConfirmMapping {
        title: String,
        body: String,
        actions: Vec<MappingModalAction>,
        selected: usize,
        pending: PendingMapping,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallationType {
    Typical,
    Custom,
    ImportConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataSourceKind {
    Local,
    Remote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DbKind {
    Local,
    Remote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NewDbLocation {
    ThisMachine,
    SpecificPath,
}

impl NewDbLocation {
    fn as_str(&self) -> &'static str {
        match self {
            NewDbLocation::ThisMachine => "This machine (default location)",
            NewDbLocation::SpecificPath => "Specific drive / path (advanced)",
        }
    }

    fn as_id(&self) -> &'static str {
        match self {
            NewDbLocation::ThisMachine => "this_machine",
            NewDbLocation::SpecificPath => "specific_path",
        }
    }

    fn toggle(&self) -> Self {
        match self {
            NewDbLocation::ThisMachine => NewDbLocation::SpecificPath,
            NewDbLocation::SpecificPath => NewDbLocation::ThisMachine,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingHostedWhere {
    OnPrem,
    AwsRdsAurora,
    AzureSqlMi,
    GcpCloudSql,
    Neon,
    Supabase,
    Other,
}

impl ExistingHostedWhere {
    fn as_str(&self) -> &'static str {
        match self {
            ExistingHostedWhere::OnPrem => "On-prem / self-hosted / unknown",
            ExistingHostedWhere::AwsRdsAurora => "AWS RDS / Aurora",
            ExistingHostedWhere::AzureSqlMi => "Azure SQL / SQL MI",
            ExistingHostedWhere::GcpCloudSql => "GCP Cloud SQL",
            ExistingHostedWhere::Neon => "Neon",
            ExistingHostedWhere::Supabase => "Supabase",
            ExistingHostedWhere::Other => "Other",
        }
    }

    fn as_id(&self) -> &'static str {
        match self {
            ExistingHostedWhere::OnPrem => "on_prem",
            ExistingHostedWhere::AwsRdsAurora => "aws_rds",
            ExistingHostedWhere::AzureSqlMi => "azure_sql",
            ExistingHostedWhere::GcpCloudSql => "gcp_cloud_sql",
            ExistingHostedWhere::Neon => "neon",
            ExistingHostedWhere::Supabase => "supabase",
            ExistingHostedWhere::Other => "other",
        }
    }

    fn next(&self) -> Self {
        match self {
            ExistingHostedWhere::OnPrem => ExistingHostedWhere::AwsRdsAurora,
            ExistingHostedWhere::AwsRdsAurora => ExistingHostedWhere::AzureSqlMi,
            ExistingHostedWhere::AzureSqlMi => ExistingHostedWhere::GcpCloudSql,
            ExistingHostedWhere::GcpCloudSql => ExistingHostedWhere::Neon,
            ExistingHostedWhere::Neon => ExistingHostedWhere::Supabase,
            ExistingHostedWhere::Supabase => ExistingHostedWhere::Other,
            ExistingHostedWhere::Other => ExistingHostedWhere::OnPrem,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DbEngine {
    SqlServer,
    Postgres,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DbTestStatus {
    Idle,
    Testing,
    Success,
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StorageMode {
    Defaults,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StorageLocation {
    System,
    Attached,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetentionPolicy {
    Rolling18,
    Rolling12,
    MaxDisk,
    KeepEverything,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HotRetentionChoice {
    Months12,
    Months18,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveFormatChoice {
    ZipNdjson,
    ZipCsv,
}

#[derive(Debug, Clone)]
struct TextInput {
    value: String,
    cursor: usize,
    masked: bool,
}

impl TextInput {
    fn new(value: impl Into<String>, masked: bool) -> Self {
        let v = value.into();
        Self {
            cursor: v.len(),
            value: v,
            masked,
        }
    }

    fn display(&self) -> String {
        if self.masked {
            "*".repeat(self.value.chars().count())
        } else {
            self.value.clone()
        }
    }

    fn set(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.cursor = self.value.len();
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char(c) => {
                self.value.insert(self.cursor, c);
                self.cursor = (self.cursor + 1).min(self.value.len());
                true
            }
            KeyCode::Backspace => {
                if self.cursor > 0 && !self.value.is_empty() {
                    let idx = self.cursor - 1;
                    self.value.remove(idx);
                    self.cursor = idx;
                }
                true
            }
            KeyCode::Delete => {
                if self.cursor < self.value.len() && !self.value.is_empty() {
                    self.value.remove(self.cursor);
                }
                true
            }
            KeyCode::Left => {
                self.cursor = self.cursor.saturating_sub(1);
                true
            }
            KeyCode::Right => {
                self.cursor = (self.cursor + 1).min(self.value.len());
                true
            }
            KeyCode::Home => {
                self.cursor = 0;
                true
            }
            KeyCode::End => {
                self.cursor = self.value.len();
                true
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusTarget {
    Field(usize),
    Button(ButtonFocus),
    Mapping(MappingFocus),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MappingFocus {
    DemoToggle,
    OverrideToggle,
    SourceSearch,
    SourceList,
    TargetSearch,
    TargetList,
}

#[derive(Debug, Clone)]
struct SourceField {
    id: String,
    raw_name: String,
    display_name: String,
}

#[derive(Debug, Clone)]
struct TargetField {
    id: String,
    name: String,
    required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MappingModalAction {
    Add,
    Replace,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingMapping {
    source_id: String,
    target_id: String,
}

#[derive(Debug, Clone)]
enum UiMsg {
    DbTestComplete {
        success: bool,
        message: String,
    },
    MappingScanComplete {
        success: bool,
        message: String,
        columns: Vec<DiscoveredColumnDto>,
    },
    InstallProgress(ProgressPayload),
    InstallFinished {
        success: bool,
        message: String,
        correlation_id: String,
        artifacts: Option<InstallArtifacts>,
    },
}

struct WizardState {
    page: Page,
    install_mode: InstallMode,
    platform_selected: InstallMode,
    license_accepted: bool,
    modal: Option<Modal>,
    focus: FocusTarget,
    quit: bool,

    // Page state
    installation_type: InstallationType,
    import_config_path: TextInput,
    import_config_error: Option<String>,

    license_scroll: u16,

    destination_path: TextInput,
    destination_error: Option<String>,

    data_source_kind: DataSourceKind,
    source_object_name: TextInput,
    call_data_host: TextInput,
    call_data_port: TextInput,
    call_data_database: TextInput,
    call_data_user: TextInput,
    call_data_password: TextInput,

    db_kind: DbKind,
    db_engine: DbEngine,
    db_use_conn_string: bool,
    db_host: TextInput,
    db_port: TextInput,
    db_database: TextInput,
    db_user: TextInput,
    db_password: TextInput,
    db_ssl_mode: String, // "disable" | "prefer" | "require"
    db_conn_string: TextInput,
    db_test_status: DbTestStatus,
    db_test_message: String,

    // D2 Database Setup Wizard (New vs Existing)
    new_db_location: NewDbLocation,
    new_db_specific_path: TextInput,
    new_db_max_size_gb: TextInput,
    existing_hosted_where: ExistingHostedWhere,

    storage_mode: StorageMode,
    storage_location: StorageLocation,
    storage_custom_path: TextInput,
    retention_policy: RetentionPolicy,
    max_disk_gb: TextInput,

    // Retention + Archive policy (Phase 5 extension)
    hot_retention_choice: HotRetentionChoice,
    hot_retention_custom_months: TextInput,
    archive_format: ArchiveFormatChoice,
    archive_destination: TextInput,
    archive_max_usage_gb: TextInput,
    archive_schedule_day_of_month: TextInput,
    archive_schedule_time_local: TextInput,
    archive_catch_up_on_startup: bool,
    consent_to_sync: bool,
    consent_details_expanded: bool,

    // Schema mapping (B3/B4)
    mapping_demo_mode: bool,
    mapping_override: bool,
    mapping_scanning: bool,
    mapping_scan_error: Option<String>,
    source_fields: Vec<SourceField>,
    target_fields: Vec<TargetField>,
    source_search: TextInput,
    target_search: TextInput,
    selected_source_id: Option<String>,
    selected_target_id: Option<String>,
    source_list_index: usize,
    target_list_index: usize,
    source_to_targets: HashMap<String, Vec<String>>,
    target_to_source: HashMap<String, String>,

    // Installing status
    install_progress: Option<ProgressPayload>,
    install_detail: Vec<String>,
    install_correlation_id: Option<String>,
    install_artifacts: Option<InstallArtifacts>,
}

impl WizardState {
    fn new() -> Self {
        Self {
            page: Page::Platform,
            install_mode: InstallMode::Windows,
            platform_selected: InstallMode::Windows,
            license_accepted: false,
            modal: None,
            focus: FocusTarget::Button(ButtonFocus::Next),
            quit: false,

            installation_type: InstallationType::Typical,
            import_config_path: TextInput::new("", false),
            import_config_error: None,

            license_scroll: 0,

            destination_path: TextInput::new("C:\\Program Files\\CADalytix", false),
            destination_error: None,

            data_source_kind: DataSourceKind::Local,
            source_object_name: TextInput::new("dbo.CallData", false),
            call_data_host: TextInput::new("localhost", false),
            call_data_port: TextInput::new("1433", false),
            call_data_database: TextInput::new("", false),
            call_data_user: TextInput::new("", false),
            call_data_password: TextInput::new("", true),

            db_kind: DbKind::Local,
            db_engine: DbEngine::SqlServer,
            db_use_conn_string: false,
            db_host: TextInput::new("localhost", false),
            db_port: TextInput::new("5432", false),
            db_database: TextInput::new("cadalytix", false),
            db_user: TextInput::new("cadalytix_admin", false),
            db_password: TextInput::new("", true),
            db_ssl_mode: "prefer".to_string(),
            db_conn_string: TextInput::new("", false),
            db_test_status: DbTestStatus::Idle,
            db_test_message: String::new(),

            new_db_location: NewDbLocation::ThisMachine,
            new_db_specific_path: TextInput::new("", false),
            new_db_max_size_gb: TextInput::new("50", false),
            existing_hosted_where: ExistingHostedWhere::OnPrem,

            storage_mode: StorageMode::Defaults,
            storage_location: StorageLocation::System,
            storage_custom_path: TextInput::new("", false),
            retention_policy: RetentionPolicy::Rolling18,
            max_disk_gb: TextInput::new("100", false),

            hot_retention_choice: HotRetentionChoice::Months18,
            hot_retention_custom_months: TextInput::new("24", false),
            archive_format: ArchiveFormatChoice::ZipNdjson,
            archive_destination: TextInput::new("", false),
            archive_max_usage_gb: TextInput::new("10", false),
            archive_schedule_day_of_month: TextInput::new("1", false),
            archive_schedule_time_local: TextInput::new("00:05", false),
            archive_catch_up_on_startup: true,
            consent_to_sync: false,
            consent_details_expanded: false,

            mapping_demo_mode: false,
            mapping_override: false,
            mapping_scanning: false,
            mapping_scan_error: None,
            source_fields: Vec::new(),
            target_fields: default_target_fields(),
            source_search: TextInput::new("", false),
            target_search: TextInput::new("", false),
            selected_source_id: None,
            selected_target_id: None,
            source_list_index: 0,
            target_list_index: 0,
            source_to_targets: HashMap::new(),
            target_to_source: HashMap::new(),

            install_progress: None,
            install_detail: Vec::new(),
            install_correlation_id: None,
            install_artifacts: None,
        }
    }
}

fn default_target_fields() -> Vec<TargetField> {
    vec![
        TargetField {
            id: "CallReceivedAt".to_string(),
            name: "Call Received At".to_string(),
            required: true,
        },
        TargetField {
            id: "IncidentNumber".to_string(),
            name: "Incident Number".to_string(),
            required: true,
        },
        TargetField {
            id: "City".to_string(),
            name: "City".to_string(),
            required: false,
        },
        TargetField {
            id: "State".to_string(),
            name: "State".to_string(),
            required: false,
        },
        TargetField {
            id: "Zip".to_string(),
            name: "Zip".to_string(),
            required: false,
        },
        TargetField {
            id: "Address".to_string(),
            name: "Address".to_string(),
            required: false,
        },
        TargetField {
            id: "Latitude".to_string(),
            name: "Latitude".to_string(),
            required: false,
        },
        TargetField {
            id: "Longitude".to_string(),
            name: "Longitude".to_string(),
            required: false,
        },
        TargetField {
            id: "UnitId".to_string(),
            name: "Unit ID".to_string(),
            required: false,
        },
        TargetField {
            id: "Disposition".to_string(),
            name: "Disposition".to_string(),
            required: false,
        },
    ]
}

fn page_title(page: Page, _mode: InstallMode) -> &'static str {
    match page {
        Page::Platform => "CADalytix Setup",
        Page::Welcome => "Welcome to the CADalytix Setup Wizard",
        Page::License => "License Agreement",
        Page::InstallType => "Installation Type",
        Page::Destination => "Destination Folder",
        Page::DataSource => "Data Source",
        Page::Database => "Database Setup",
        Page::Storage => "Database Storage",
        Page::Retention => "Hot Retention",
        Page::Archive => "Archive Policy",
        Page::Consent => "Support Improvements",
        Page::Mapping => "Schema Mapping",
        Page::Ready => "Ready to Install",
        Page::Installing => "Installing CADalytix",
        Page::Complete => "Completed",
    }
}

fn next_label(page: Page) -> &'static str {
    match page {
        Page::Ready => "Install",
        Page::Complete => "Finish",
        _ => "Next",
    }
}

fn can_go_back(page: Page) -> bool {
    !matches!(
        page,
        Page::Platform | Page::Welcome | Page::Installing | Page::Complete
    )
}

fn can_go_next(state: &WizardState) -> bool {
    match state.page {
        Page::Platform => false,
        Page::Welcome => true,
        Page::License => state.license_accepted,
        Page::InstallType => match state.installation_type {
            InstallationType::ImportConfig => {
                !state.import_config_path.value.trim().is_empty()
                    && state.import_config_error.is_none()
            }
            _ => true,
        },
        Page::Destination => {
            !state.destination_path.value.trim().is_empty() && state.destination_error.is_none()
        }
        Page::Database => {
            if state.db_kind == DbKind::Local {
                // Create NEW CADalytix Database
                if state.new_db_location == NewDbLocation::SpecificPath
                    && state.new_db_specific_path.value.trim().is_empty()
                {
                    return false;
                }
                let gb = state
                    .new_db_max_size_gb
                    .value
                    .trim()
                    .parse::<u32>()
                    .unwrap_or(0);
                gb > 0
            } else {
                // Use EXISTING Database
                matches!(state.db_test_status, DbTestStatus::Success)
            }
        }
        Page::Storage => {
            if state.storage_mode == StorageMode::Custom {
                if state.storage_location == StorageLocation::Custom
                    && state.storage_custom_path.value.trim().is_empty()
                {
                    return false;
                }
                if state.retention_policy == RetentionPolicy::MaxDisk
                    && state.max_disk_gb.value.trim().is_empty()
                {
                    return false;
                }
            }
            true
        }
        Page::Retention => {
            if state.hot_retention_choice != HotRetentionChoice::Custom {
                return true;
            }
            let n = state
                .hot_retention_custom_months
                .value
                .trim()
                .parse::<u32>()
                .unwrap_or(0);
            n > 0 && n <= 240
        }
        Page::Archive => {
            if state.archive_destination.value.trim().is_empty() {
                return false;
            }
            let gb = state
                .archive_max_usage_gb
                .value
                .trim()
                .parse::<u32>()
                .unwrap_or(0);
            if gb == 0 {
                return false;
            }
            let day = state
                .archive_schedule_day_of_month
                .value
                .trim()
                .parse::<u8>()
                .unwrap_or(0);
            if !(1..=28).contains(&day) {
                return false;
            }
            is_valid_time_hhmm(state.archive_schedule_time_local.value.trim())
        }
        Page::Consent => true,
        Page::Mapping => {
            if state.mapping_scanning {
                return false;
            }
            if state.mapping_scan_error.is_some() {
                return false;
            }
            if state.source_fields.is_empty() {
                return false;
            }
            // Required target fields must be mapped before proceeding.
            for t in state.target_fields.iter().filter(|t| t.required) {
                if !state.target_to_source.contains_key(&t.id) {
                    return false;
                }
            }
            true
        }
        Page::Installing => false,
        _ => true,
    }
}

fn is_valid_time_hhmm(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return false;
    }
    let hh = parts[0].parse::<u32>().ok();
    let mm = parts[1].parse::<u32>().ok();
    match (hh, mm) {
        (Some(hh), Some(mm)) => hh <= 23 && mm <= 59,
        _ => false,
    }
}

fn page_field_count(state: &WizardState) -> usize {
    match state.page {
        Page::InstallType => match state.installation_type {
            InstallationType::ImportConfig => 1,
            _ => 0,
        },
        Page::Destination => 1,
        Page::DataSource => 6,
        Page::Database => {
            if state.db_kind == DbKind::Local {
                // Create NEW CADalytix Database branch
                if state.new_db_location == NewDbLocation::SpecificPath {
                    2 // path + max size
                } else {
                    1 // max size
                }
            } else if state.db_use_conn_string {
                1
            } else {
                // Existing DB details mode requires host/server, port, db name, username, password, TLS.
                6
            }
        }
        Page::Storage => {
            if state.storage_mode != StorageMode::Custom {
                return 0;
            }
            let mut n = 0;
            if state.storage_location == StorageLocation::Custom {
                n += 1;
            }
            if state.retention_policy == RetentionPolicy::MaxDisk {
                n += 1;
            }
            n
        }
        Page::Retention => {
            if state.hot_retention_choice == HotRetentionChoice::Custom {
                1
            } else {
                0
            }
        }
        Page::Archive => 4,
        _ => 0,
    }
}

fn focused_text_input_mut(state: &mut WizardState) -> Option<&mut TextInput> {
    match state.focus {
        FocusTarget::Mapping(MappingFocus::SourceSearch) => return Some(&mut state.source_search),
        FocusTarget::Mapping(MappingFocus::TargetSearch) => return Some(&mut state.target_search),
        FocusTarget::Field(_) => {}
        _ => return None,
    }

    let FocusTarget::Field(idx) = state.focus else {
        return None;
    };

    match state.page {
        Page::InstallType => match state.installation_type {
            InstallationType::ImportConfig if idx == 0 => Some(&mut state.import_config_path),
            _ => None,
        },
        Page::Destination => {
            if idx == 0 {
                Some(&mut state.destination_path)
            } else {
                None
            }
        }
        Page::DataSource => match idx {
            0 => Some(&mut state.call_data_database),
            1 => Some(&mut state.call_data_user),
            2 => Some(&mut state.call_data_password),
            3 => Some(&mut state.call_data_host),
            4 => Some(&mut state.call_data_port),
            5 => Some(&mut state.source_object_name),
            _ => None,
        },
        Page::Database => {
            if state.db_kind == DbKind::Local {
                // Create NEW CADalytix Database branch
                if state.new_db_location == NewDbLocation::SpecificPath {
                    match idx {
                        0 => Some(&mut state.new_db_specific_path),
                        1 => Some(&mut state.new_db_max_size_gb),
                        _ => None,
                    }
                } else if idx == 0 {
                    Some(&mut state.new_db_max_size_gb)
                } else {
                    None
                }
            } else if state.db_use_conn_string {
                if idx == 0 {
                    Some(&mut state.db_conn_string)
                } else {
                    None
                }
            } else {
                // Existing DB details mode: TLS is a non-text selection and is handled by Left/Right.
                match idx {
                    0 => Some(&mut state.db_host),
                    1 => Some(&mut state.db_port),
                    2 => Some(&mut state.db_database),
                    3 => Some(&mut state.db_user),
                    4 => Some(&mut state.db_password),
                    _ => None,
                }
            }
        }
        Page::Storage => {
            if state.storage_mode != StorageMode::Custom {
                return None;
            }

            let mut i = 0usize;
            if state.storage_location == StorageLocation::Custom {
                if idx == i {
                    return Some(&mut state.storage_custom_path);
                }
                i += 1;
            }
            if state.retention_policy == RetentionPolicy::MaxDisk && idx == i {
                return Some(&mut state.max_disk_gb);
            }
            None
        }
        Page::Retention => {
            if state.hot_retention_choice != HotRetentionChoice::Custom {
                return None;
            }
            if idx == 0 {
                Some(&mut state.hot_retention_custom_months)
            } else {
                None
            }
        }
        Page::Archive => match idx {
            0 => Some(&mut state.archive_destination),
            1 => Some(&mut state.archive_max_usage_gb),
            2 => Some(&mut state.archive_schedule_day_of_month),
            3 => Some(&mut state.archive_schedule_time_local),
            _ => None,
        },
        _ => None,
    }
}

fn update_page_validation(state: &mut WizardState) {
    match state.page {
        Page::InstallType => {
            if state.installation_type == InstallationType::ImportConfig {
                let p = state.import_config_path.value.trim();
                if p.is_empty() {
                    state.import_config_error = None;
                } else if !std::path::Path::new(p).exists() {
                    state.import_config_error =
                        Some("Selected file could not be read.".to_string());
                } else {
                    state.import_config_error = None;
                }
            } else {
                state.import_config_error = None;
            }
        }
        Page::Destination => {
            let p = state.destination_path.value.trim();
            state.destination_error = if p.is_empty() {
                Some("Destination folder is required.".to_string())
            } else {
                None
            };
        }
        _ => {}
    }
}

fn browse_folder_entries(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return out,
    };

    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            out.push(p);
        }
    }

    out.sort_by_key(|a| a.to_string_lossy().to_lowercase());
    out
}

fn disambiguate_source_columns(cols: &[DiscoveredColumnDto]) -> Vec<SourceField> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for c in cols {
        *counts.entry(c.name.clone()).or_insert(0) += 1;
    }

    cols.iter()
        .enumerate()
        .map(|(idx, c)| {
            let total = *counts.get(&c.name).unwrap_or(&1);
            let ord = seen.entry(c.name.clone()).or_insert(0);
            let ordinal = *ord;
            *ord = ord.saturating_add(1);
            let display = if total > 1 {
                format!("{} ({})", c.name, ordinal + 1)
            } else {
                c.name.clone()
            };
            SourceField {
                id: make_stable_source_id(&c.name, ordinal).unwrap_or_else(|| idx.to_string()),
                raw_name: c.name.clone(),
                display_name: display,
            }
        })
        .collect()
}

fn make_stable_source_id(raw_name: &str, ordinal: usize) -> Option<String> {
    let base = sanitize_source_id_base(raw_name);
    if base.is_empty() {
        return None;
    }
    Some(format!("{}__{}", base, ordinal))
}

fn sanitize_source_id_base(raw_name: &str) -> String {
    let mut out = String::new();
    let mut prev_underscore = false;
    for ch in raw_name.chars() {
        let ok = ch.is_ascii_alphanumeric() || ch == '_';
        let c = if ok { ch } else { '_' };
        if c == '_' {
            if prev_underscore {
                continue;
            }
            prev_underscore = true;
        } else {
            prev_underscore = false;
        }
        out.push(c);
    }
    out.trim_matches('_').to_string()
}

fn mapping_source_display(state: &WizardState, source_id: &str) -> String {
    state
        .source_fields
        .iter()
        .find(|s| s.id == source_id)
        .map(|s| s.display_name.clone())
        .unwrap_or_else(|| source_id.to_string())
}

fn mapping_target_name(state: &WizardState, target_id: &str) -> String {
    state
        .target_fields
        .iter()
        .find(|t| t.id == target_id)
        .map(|t| t.name.clone())
        .unwrap_or_else(|| target_id.to_string())
}

fn mapping_source_raw(state: &WizardState, source_id: &str) -> String {
    state
        .source_fields
        .iter()
        .find(|s| s.id == source_id)
        .map(|s| s.raw_name.clone())
        .unwrap_or_else(|| source_id.to_string())
}

fn build_call_data_connection_string(state: &WizardState) -> String {
    let host = if state.call_data_host.value.trim().is_empty() {
        "localhost"
    } else {
        state.call_data_host.value.trim()
    };
    let port = if state.call_data_port.value.trim().is_empty() {
        "1433"
    } else {
        state.call_data_port.value.trim()
    };
    let server = format!("{},{}", host, port);
    let db = state.call_data_database.value.trim();
    let user = state.call_data_user.value.trim();
    let pass = &state.call_data_password.value;
    format!(
        "Server={};Database={};User Id={};Password={};TrustServerCertificate=true;Encrypt=false;",
        server, db, user, pass
    )
}

fn filtered_source_ids(state: &WizardState) -> Vec<String> {
    let q = state.source_search.value.trim().to_ascii_lowercase();
    state
        .source_fields
        .iter()
        .filter(|s| q.is_empty() || s.display_name.to_ascii_lowercase().contains(&q))
        .map(|s| s.id.clone())
        .collect()
}

fn filtered_target_ids(state: &WizardState) -> Vec<String> {
    let q = state.target_search.value.trim().to_ascii_lowercase();
    state
        .target_fields
        .iter()
        .filter(|t| q.is_empty() || t.name.to_ascii_lowercase().contains(&q))
        .map(|t| t.id.clone())
        .collect()
}

fn start_mapping_scan(state: &mut WizardState, tx: &mpsc::Sender<UiMsg>) {
    if state.mapping_scanning {
        return;
    }
    state.mapping_scanning = true;
    state.mapping_scan_error = None;
    state.source_fields = Vec::new();
    state.selected_source_id = None;
    state.selected_target_id = None;
    state.source_list_index = 0;
    state.target_list_index = 0;

    let payload = PreflightDataSourceRequestDto {
        call_data_connection_string: build_call_data_connection_string(state),
        source_object_name: state.source_object_name.value.clone(),
        date_from_iso: None,
        date_to_iso: None,
        sample_limit: 10,
        demo_mode: state.mapping_demo_mode,
    };

    let tx = tx.clone();
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        match rt {
            Ok(rt) => {
                let res = rt.block_on(preflight::preflight_datasource(payload));
                match res {
                    Ok(api) => {
                        if api.success {
                            let cols = api.data.map(|d| d.discovered_columns).unwrap_or_default();
                            let _ = tx.send(UiMsg::MappingScanComplete {
                                success: true,
                                message: String::new(),
                                columns: cols,
                            });
                        } else {
                            let msg = api
                                .error
                                .unwrap_or_else(|| "Unable to scan source fields.".to_string());
                            let _ = tx.send(UiMsg::MappingScanComplete {
                                success: false,
                                message: msg,
                                columns: Vec::new(),
                            });
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(UiMsg::MappingScanComplete {
                            success: false,
                            message: e,
                            columns: Vec::new(),
                        });
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(UiMsg::MappingScanComplete {
                    success: false,
                    message: format!("Internal error starting scan: {}", e),
                    columns: Vec::new(),
                });
            }
        }
    });
}

fn unassign_selected(state: &mut WizardState) {
    let (Some(source_id), Some(target_id)) = (
        state.selected_source_id.clone(),
        state.selected_target_id.clone(),
    ) else {
        return;
    };
    if state.target_to_source.get(&target_id).map(|s| s.as_str()) != Some(source_id.as_str()) {
        return;
    }

    remove_target_from_old_source(state, &target_id);
    state.selected_target_id = None;
}

fn remove_target_from_old_source(state: &mut WizardState, target_id: &str) {
    let Some(old_source) = state.target_to_source.get(target_id).cloned() else {
        return;
    };

    state.target_to_source.remove(target_id);
    if let Some(arr) = state.source_to_targets.get_mut(&old_source) {
        arr.retain(|t| t != target_id);
    }
}

fn remove_all_targets_from_source(state: &mut WizardState, source_id: &str) {
    let targets = state
        .source_to_targets
        .get(source_id)
        .cloned()
        .unwrap_or_default();
    for t in targets {
        state.target_to_source.remove(&t);
    }
    state
        .source_to_targets
        .insert(source_id.to_string(), Vec::new());
}

/// Apply a mapping from a source field to a target field.
/// If `add` is true and override mode is enabled, append the target to the source's target list.
/// Otherwise, replace the source's mapping(s) with the single target.
fn apply_mapping(state: &mut WizardState, source_id: &str, target_id: &str, add: bool) {
    // Target exclusivity: if the target is mapped elsewhere, remove it first.
    remove_target_from_old_source(state, target_id);

    let add_effective = add && state.mapping_override;
    if !add_effective {
        remove_all_targets_from_source(state, source_id);
        state
            .source_to_targets
            .insert(source_id.to_string(), vec![target_id.to_string()]);
    } else {
        let entry = state
            .source_to_targets
            .entry(source_id.to_string())
            .or_default();
        if !entry.iter().any(|t| t == target_id) {
            entry.push(target_id.to_string());
        }
    }

    state
        .target_to_source
        .insert(target_id.to_string(), source_id.to_string());
    state.selected_source_id = Some(source_id.to_string());
    state.selected_target_id = Some(target_id.to_string());
}

fn attempt_map(state: &mut WizardState, source_id: &str, target_id: &str) {
    let target_already_mapped_to = state.target_to_source.get(target_id).cloned();
    let source_already_mapped_to = state
        .source_to_targets
        .get(source_id)
        .cloned()
        .unwrap_or_default();

    let target_name = mapping_target_name(state, target_id);
    let source_name = mapping_source_display(state, source_id);

    // Unlink rule: selecting an already-mapped pair toggles it off.
    if target_already_mapped_to.as_deref() == Some(source_id) {
        remove_target_from_old_source(state, target_id);
        state.selected_source_id = Some(source_id.to_string());
        state.selected_target_id = Some(target_id.to_string());
        return;
    }

    // CASE C / A (target already mapped)
    if let Some(old_source) = target_already_mapped_to.clone().filter(|s| s != source_id) {
        let old_source_name = mapping_source_display(state, &old_source);
        let has_source_mapping = !source_already_mapped_to.is_empty()
            && !source_already_mapped_to.contains(&target_id.to_string());

        if has_source_mapping {
            if state.mapping_override {
                state.modal = Some(Modal::ConfirmMapping {
                    title: "Source already mapped".to_string(),
                    body: format!(
                        "Target \"{}\" is currently mapped to Source \"{}\".\n\nSource \"{}\" is currently mapped to: {}.\n\nWhat would you like to do?",
                        target_name,
                        old_source_name,
                        source_name,
                        source_already_mapped_to
                            .iter()
                            .map(|t| mapping_target_name(state, t))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    actions: vec![MappingModalAction::Add, MappingModalAction::Replace, MappingModalAction::Cancel],
                    selected: 0,
                    pending: PendingMapping {
                        source_id: source_id.to_string(),
                        target_id: target_id.to_string(),
                    },
                });
                return;
            }

            let old_target = mapping_target_name(state, &source_already_mapped_to[0]);
            state.modal = Some(Modal::ConfirmMapping {
                title: "Replace mapping?".to_string(),
                body: format!(
                    "Target \"{}\" is currently mapped to Source \"{}\".\nSource \"{}\" is currently mapped to Target \"{}\".\n\nDo you want to replace these mappings with Source \"{}\" → Target \"{}\"?",
                    target_name, old_source_name, source_name, old_target, source_name, target_name
                ),
                actions: vec![MappingModalAction::Replace, MappingModalAction::Cancel],
                selected: 0,
                pending: PendingMapping {
                    source_id: source_id.to_string(),
                    target_id: target_id.to_string(),
                },
            });
            return;
        }

        // CASE A — Target already mapped
        state.modal = Some(Modal::ConfirmMapping {
            title: "Replace mapping?".to_string(),
            body: format!(
                "Target \"{}\" is currently mapped to Source \"{}\".\nDo you want to replace it with Source \"{}\"?",
                target_name, old_source_name, source_name
            ),
            actions: vec![MappingModalAction::Replace, MappingModalAction::Cancel],
            selected: 0,
            pending: PendingMapping {
                source_id: source_id.to_string(),
                target_id: target_id.to_string(),
            },
        });
        return;
    }

    // CASE B — Source already mapped (override OFF)
    if !state.mapping_override
        && !source_already_mapped_to.is_empty()
        && !source_already_mapped_to.iter().any(|t| t == target_id)
    {
        let old_target_name = mapping_target_name(state, &source_already_mapped_to[0]);
        state.modal = Some(Modal::ConfirmMapping {
            title: "Replace mapping?".to_string(),
            body: format!(
                "Source \"{}\" is currently mapped to Target \"{}\".\nDo you want to replace it with Target \"{}\"?",
                source_name, old_target_name, target_name
            ),
            actions: vec![MappingModalAction::Replace, MappingModalAction::Cancel],
            selected: 0,
            pending: PendingMapping {
                source_id: source_id.to_string(),
                target_id: target_id.to_string(),
            },
        });
        return;
    }

    // Source already mapped (override ON) — Add/Replace/Cancel
    if state.mapping_override
        && !source_already_mapped_to.is_empty()
        && !source_already_mapped_to.iter().any(|t| t == target_id)
    {
        state.modal = Some(Modal::ConfirmMapping {
            title: "Source already mapped".to_string(),
            body: format!(
                "Source \"{}\" is currently mapped to: {}.\nWhat would you like to do?",
                source_name,
                source_already_mapped_to
                    .iter()
                    .map(|t| mapping_target_name(state, t))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            actions: vec![
                MappingModalAction::Add,
                MappingModalAction::Replace,
                MappingModalAction::Cancel,
            ],
            selected: 0,
            pending: PendingMapping {
                source_id: source_id.to_string(),
                target_id: target_id.to_string(),
            },
        });
        return;
    }

    // No conflicts
    apply_mapping(state, source_id, target_id, state.mapping_override);
}

fn can_cancel(page: Page) -> bool {
    !matches!(page, Page::Complete)
}

fn next_page(page: Page) -> Page {
    match page {
        Page::Platform => Page::Welcome,
        Page::Welcome => Page::License,
        Page::License => Page::InstallType,
        Page::InstallType => Page::Destination,
        Page::Destination => Page::DataSource,
        Page::DataSource => Page::Database,
        Page::Database => Page::Storage,
        Page::Storage => Page::Retention,
        Page::Retention => Page::Archive,
        Page::Archive => Page::Consent,
        Page::Consent => Page::Mapping,
        Page::Mapping => Page::Ready,
        Page::Ready => Page::Installing,
        Page::Installing => Page::Complete,
        Page::Complete => Page::Platform,
    }
}

fn prev_page(page: Page) -> Page {
    match page {
        Page::Platform => Page::Platform,
        Page::Welcome => Page::Platform,
        Page::License => Page::Welcome,
        Page::InstallType => Page::License,
        Page::Destination => Page::InstallType,
        Page::DataSource => Page::Destination,
        Page::Database => Page::DataSource,
        Page::Storage => Page::Database,
        Page::Retention => Page::Storage,
        Page::Archive => Page::Retention,
        Page::Consent => Page::Archive,
        Page::Mapping => Page::Consent,
        Page::Ready => Page::Mapping,
        Page::Installing => Page::Installing,
        Page::Complete => Page::Complete,
    }
}

pub fn run(secrets: Arc<SecretProtector>) -> Result<()> {
    info!("[PHASE: tui] [STEP: start] Starting TUI wizard");

    let mut terminal = setup_terminal()?;
    let result = run_loop(&mut terminal, secrets);
    restore_terminal(&mut terminal)?;

    result
}

fn new_real_wizard_state() -> WizardState {
    // Real interactive run: DO NOT seed any sample/demo values here.
    // Only `smoke(...)` is allowed to inject sample state.
    WizardState::new()
}

fn new_smoke_wizard_state(target: &str) -> WizardState {
    // Smoke-only: seeded state for deterministic page rendering in CI/tooling.
    let mut state = WizardState::new();
    state.install_mode = InstallMode::Windows;
    state.db_engine = DbEngine::SqlServer;
    set_focused_button(&mut state, ButtonFocus::Next);

    match target {
        "license" => {
            state.page = Page::License;
        }
        "destination" => {
            state.page = Page::Destination;
            state.destination_path.set("C:\\CADalytix");
        }
        "db" => {
            state.page = Page::Database;
            // Show EXISTING Database branch in smoke so connection fields render.
            state.db_kind = DbKind::Remote;
            state.db_use_conn_string = false;
            state.existing_hosted_where = ExistingHostedWhere::OnPrem;
            state.db_host.set("localhost");
            state.db_port.set("5432");
            state.db_database.set("cadalytix");
            state.db_user.set("cadalytix_admin");
            state.db_password.set("********");
            state.db_test_status = DbTestStatus::Idle;
            state.db_test_message = "Press T to Test Connection.".to_string();
        }
        "storage" => {
            state.page = Page::Storage;
            state.storage_mode = StorageMode::Custom;
            state.storage_location = StorageLocation::Custom;
            state.storage_custom_path.set("D:\\CADalytixData");
            state.retention_policy = RetentionPolicy::MaxDisk;
            state.max_disk_gb.set("250");
        }
        "retention" => {
            state.page = Page::Retention;
            state.hot_retention_choice = HotRetentionChoice::Custom;
            state.hot_retention_custom_months.set("24");
        }
        "archive" => {
            state.page = Page::Archive;
            state.archive_format = ArchiveFormatChoice::ZipNdjson;
            state.archive_destination.set("E:\\CADalytixArchive");
            state.archive_max_usage_gb.set("50");
            state.archive_schedule_day_of_month.set("1");
            state.archive_schedule_time_local.set("00:05");
            state.archive_catch_up_on_startup = true;
        }
        "consent" => {
            state.page = Page::Consent;
            state.consent_to_sync = false;
            state.consent_details_expanded = true;
        }
        "progress" => {
            state.page = Page::Installing;
            state.install_progress = Some(ProgressPayload {
                correlation_id: "smoke".to_string(),
                step: "migrations".to_string(),
                severity: "info".to_string(),
                phase: "install".to_string(),
                percent: 42,
                message: "Applying migrations...".to_string(),
                elapsed_ms: Some(1234),
                eta_ms: Some(5678),
            });
            state.install_detail = vec![
                "Starting installation...".to_string(),
                "Applying migrations... (1/3)".to_string(),
                "Applying migrations... (2/3)".to_string(),
            ];
        }
        "mapping" => {
            state.page = Page::Mapping;
            state.mapping_demo_mode = true;
            state.mapping_override = true;
            state.mapping_scanning = false;
            state.mapping_scan_error = None;
            state.source_fields = vec![
                SourceField {
                    id: "City__0".to_string(),
                    raw_name: "City".to_string(),
                    display_name: "City (1)".to_string(),
                },
                SourceField {
                    id: "City__1".to_string(),
                    raw_name: "City".to_string(),
                    display_name: "City (2)".to_string(),
                },
                SourceField {
                    id: "IncidentNumber__0".to_string(),
                    raw_name: "IncidentNumber".to_string(),
                    display_name: "IncidentNumber".to_string(),
                },
            ];
            state.source_to_targets = HashMap::from([
                ("City__0".to_string(), vec!["City".to_string()]),
                (
                    "IncidentNumber__0".to_string(),
                    vec!["IncidentNumber".to_string()],
                ),
            ]);
            state.target_to_source = HashMap::from([
                ("City".to_string(), "City__0".to_string()),
                (
                    "IncidentNumber".to_string(),
                    "IncidentNumber__0".to_string(),
                ),
            ]);
            state.selected_source_id = Some("City__0".to_string());
            state.selected_target_id = Some("City".to_string());
            state.focus = FocusTarget::Mapping(MappingFocus::SourceList);
        }
        "ready" => {
            state.page = Page::Ready;
            state.install_mode = InstallMode::Windows;
            state.installation_type = InstallationType::Typical;
            state.destination_path.set("C:\\CADalytix");
            state.db_engine = DbEngine::SqlServer;
            state.db_kind = DbKind::Local;
            state.db_host.set("localhost");
            state.db_database.set("cadalytix");
            state.db_user.set("cadalytix_admin");
            state.db_password.set("********");
            state.db_test_status = DbTestStatus::Success;
            state.db_test_message = "Connection successful.".to_string();
            state.storage_mode = StorageMode::Defaults;
            state.hot_retention_choice = HotRetentionChoice::Months18;
            state.archive_format = ArchiveFormatChoice::ZipNdjson;
            state.archive_destination.set("E:\\CADalytixArchive");
            state.archive_max_usage_gb.set("10");
            state.archive_schedule_day_of_month.set("1");
            state.archive_schedule_time_local.set("00:05");
            state.archive_catch_up_on_startup = true;
            state.consent_to_sync = false;

            // Minimal mapping completeness for Ready page gating/summary.
            state.source_fields = vec![
                SourceField {
                    id: "CallReceivedAt__0".to_string(),
                    raw_name: "CallReceivedAt".to_string(),
                    display_name: "CallReceivedAt".to_string(),
                },
                SourceField {
                    id: "IncidentNumber__0".to_string(),
                    raw_name: "IncidentNumber".to_string(),
                    display_name: "IncidentNumber".to_string(),
                },
            ];
            state.target_fields = vec![
                TargetField {
                    id: "CallReceivedAt".to_string(),
                    name: "Call Received At".to_string(),
                    required: true,
                },
                TargetField {
                    id: "IncidentNumber".to_string(),
                    name: "Incident Number".to_string(),
                    required: true,
                },
            ];
            state.source_to_targets = HashMap::from([
                (
                    "CallReceivedAt__0".to_string(),
                    vec!["CallReceivedAt".to_string()],
                ),
                (
                    "IncidentNumber__0".to_string(),
                    vec!["IncidentNumber".to_string()],
                ),
            ]);
            state.target_to_source = HashMap::from([
                (
                    "CallReceivedAt".to_string(),
                    "CallReceivedAt__0".to_string(),
                ),
                (
                    "IncidentNumber".to_string(),
                    "IncidentNumber__0".to_string(),
                ),
            ]);
        }
        _ => {
            // default: welcome
            state.page = Page::Welcome;
        }
    }

    state
}

/// Non-interactive smoke mode: render a single frame and exit.
/// Target pages: welcome|license|destination|db|storage|retention|archive|consent|mapping|ready|progress
pub fn smoke(_secrets: Arc<SecretProtector>, target: &str) -> Result<()> {
    info!(
        "[PHASE: tui] [STEP: smoke] Rendering single-frame TUI smoke target={}",
        target
    );

    let t = target.trim().to_ascii_lowercase();
    let state = new_smoke_wizard_state(t.as_str());

    // Use an in-memory backend so this can be executed in CI/tooling without
    // manipulating the real terminal (no raw mode / alternate screen).
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|f| draw(f.size(), f, &state))?;

    Ok(())
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    secrets: Arc<SecretProtector>,
) -> Result<()> {
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();
    let mut state = new_real_wizard_state();
    let (tx, rx) = mpsc::channel::<UiMsg>();

    while !state.quit {
        drain_messages(&mut state, &rx);
        terminal.draw(|f| draw(f.size(), f, &state))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_millis(0));

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => handle_key(&mut state, key.code, &tx, &secrets),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    Ok(())
}

fn focused_button(state: &WizardState) -> ButtonFocus {
    match state.focus {
        FocusTarget::Button(b) => b,
        _ => ButtonFocus::Next,
    }
}

fn set_focused_button(state: &mut WizardState, b: ButtonFocus) {
    state.focus = FocusTarget::Button(b);
}

fn drain_messages(state: &mut WizardState, rx: &mpsc::Receiver<UiMsg>) {
    while let Ok(msg) = rx.try_recv() {
        match msg {
            UiMsg::DbTestComplete { success, message } => {
                state.db_test_status = if success {
                    DbTestStatus::Success
                } else {
                    DbTestStatus::Fail
                };
                state.db_test_message = message;
            }
            UiMsg::MappingScanComplete {
                success,
                message,
                columns,
            } => {
                state.mapping_scanning = false;
                if success {
                    if columns.is_empty() {
                        state.mapping_scan_error = Some(
                            "No headers could be detected for the selected source.".to_string(),
                        );
                        state.source_fields = Vec::new();
                    } else {
                        state.mapping_scan_error = None;
                        state.source_fields = disambiguate_source_columns(&columns);
                        state.source_list_index = 0;
                        state.target_list_index = 0;
                        state.selected_source_id =
                            state.source_fields.first().map(|s| s.id.clone());
                        state.selected_target_id = None;
                    }
                } else {
                    state.mapping_scan_error = Some(message);
                    state.source_fields = Vec::new();
                }
            }
            UiMsg::InstallProgress(p) => {
                if state.page == Page::Installing {
                    if state.install_correlation_id.is_none() {
                        state.install_correlation_id = Some(p.correlation_id.clone());
                    }
                    if !p.message.trim().is_empty() {
                        state.install_detail.push(p.message.clone());
                        if state.install_detail.len() > 20 {
                            let start = state.install_detail.len().saturating_sub(20);
                            state.install_detail = state.install_detail[start..].to_vec();
                        }
                    }
                    state.install_progress = Some(p);
                }
            }
            UiMsg::InstallFinished {
                success,
                message,
                correlation_id,
                artifacts,
            } => {
                state.install_correlation_id = Some(correlation_id);
                state.install_artifacts = artifacts;
                if success {
                    state.page = Page::Complete;
                } else {
                    state.modal = Some(Modal::Message {
                        title: "Installation failed".to_string(),
                        body: message,
                        return_to: Some(Page::Ready),
                    });
                }
            }
        }
    }
}

fn handle_key(
    state: &mut WizardState,
    code: KeyCode,
    tx: &mpsc::Sender<UiMsg>,
    secrets: &Arc<SecretProtector>,
) {
    // Modal handling
    if let Some(modal) = state.modal.clone() {
        match modal {
            Modal::ConfirmCancel => match code {
                KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                    let next = match focused_button(state) {
                        ButtonFocus::Cancel => ButtonFocus::Next,
                        _ => ButtonFocus::Cancel,
                    };
                    set_focused_button(state, next);
                }
                KeyCode::Enter => {
                    let confirm = focused_button(state) == ButtonFocus::Cancel;
                    state.modal = None;

                    if confirm {
                        if state.page == Page::Installing {
                            // Best-effort cancellation request.
                            let _ = installer::cancel_install();
                            state
                                .install_detail
                                .push("Cancelling installation...".to_string());
                            state.focus = FocusTarget::Button(ButtonFocus::Cancel);
                        } else {
                            state.quit = true;
                        }
                    }
                }
                KeyCode::Esc => {
                    state.modal = None;
                }
                _ => {}
            },
            Modal::Message {
                title: _,
                body: _,
                return_to,
            } => match code {
                KeyCode::Enter | KeyCode::Esc => {
                    state.modal = None;
                    if let Some(p) = return_to {
                        state.page = p;
                    }
                }
                _ => {}
            },
            Modal::ConfirmMapping {
                title,
                body,
                actions,
                mut selected,
                pending,
            } => {
                match code {
                    KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                        if !actions.is_empty() {
                            selected = (selected + 1) % actions.len();
                        }
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        if let Some(i) = actions
                            .iter()
                            .position(|a| *a == MappingModalAction::Replace)
                        {
                            selected = i;
                        }
                        let action = actions
                            .get(selected)
                            .copied()
                            .unwrap_or(MappingModalAction::Cancel);
                        state.modal = None;
                        match action {
                            MappingModalAction::Add => {
                                apply_mapping(state, &pending.source_id, &pending.target_id, true);
                            }
                            MappingModalAction::Replace => {
                                apply_mapping(state, &pending.source_id, &pending.target_id, false);
                            }
                            MappingModalAction::Cancel => {}
                        }
                        return;
                    }
                    // "O" = override/add additional target when available.
                    KeyCode::Char('o') | KeyCode::Char('O') => {
                        if let Some(i) = actions.iter().position(|a| *a == MappingModalAction::Add)
                        {
                            let _ = i;
                            state.modal = None;
                            apply_mapping(state, &pending.source_id, &pending.target_id, true);
                            return;
                        }
                    }
                    KeyCode::Enter => {
                        let action = actions
                            .get(selected)
                            .copied()
                            .unwrap_or(MappingModalAction::Cancel);
                        state.modal = None;
                        match action {
                            MappingModalAction::Add => {
                                apply_mapping(state, &pending.source_id, &pending.target_id, true);
                            }
                            MappingModalAction::Replace => {
                                apply_mapping(state, &pending.source_id, &pending.target_id, false);
                            }
                            MappingModalAction::Cancel => {}
                        }
                        return;
                    }
                    KeyCode::Esc => {
                        state.modal = None;
                        return;
                    }
                    _ => {}
                }

                state.modal = Some(Modal::ConfirmMapping {
                    title,
                    body,
                    actions,
                    selected,
                    pending,
                });
            }
            Modal::BrowseFolder {
                mut current,
                mut entries,
                mut selected,
            } => {
                match code {
                    KeyCode::Up => {
                        selected = selected.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        if !entries.is_empty() {
                            selected = (selected + 1).min(entries.len().saturating_sub(1));
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(next) = entries.get(selected).cloned() {
                            current = next;
                            entries = browse_folder_entries(&current);
                            selected = 0;
                        }
                    }
                    KeyCode::Backspace => {
                        if let Some(parent) = current.parent() {
                            current = parent.to_path_buf();
                            entries = browse_folder_entries(&current);
                            selected = 0;
                        }
                    }
                    KeyCode::Char('s') | KeyCode::Char('S') => {
                        state
                            .destination_path
                            .set(current.to_string_lossy().to_string());
                        state.modal = None;
                        update_page_validation(state);
                        return;
                    }
                    KeyCode::Esc => {
                        state.modal = None;
                        return;
                    }
                    _ => {}
                }

                // Update modal state
                state.modal = Some(Modal::BrowseFolder {
                    current,
                    entries,
                    selected,
                });
            }
        }
        return;
    }

    // Global keys
    if matches!(code, KeyCode::Esc) && can_cancel(state.page) {
        state.modal = Some(Modal::ConfirmCancel);
        set_focused_button(state, ButtonFocus::Next); // "No"
        return;
    }

    // Text input handling (when a field is focused)
    if let Some(input) = focused_text_input_mut(state) {
        if input.handle_key(code) {
            update_page_validation(state);
            return;
        }
    }

    match state.page {
        Page::Platform => match code {
            KeyCode::Left | KeyCode::Right => {
                state.platform_selected = match state.platform_selected {
                    InstallMode::Windows => InstallMode::Docker,
                    InstallMode::Docker => InstallMode::Windows,
                };
            }
            KeyCode::Enter => {
                state.install_mode = state.platform_selected;
                // Default DB engine per mode.
                state.db_engine = if state.install_mode == InstallMode::Windows {
                    DbEngine::SqlServer
                } else {
                    DbEngine::Postgres
                };
                state.page = Page::Welcome;
            }
            _ => {}
        },
        Page::License => match code {
            KeyCode::Char(' ') => state.license_accepted = !state.license_accepted,
            KeyCode::PageDown => state.license_scroll = state.license_scroll.saturating_add(1),
            KeyCode::PageUp => state.license_scroll = state.license_scroll.saturating_sub(1),
            KeyCode::Enter => {
                if can_go_next(state) {
                    state.page = next_page(state.page);
                }
            }
            _ => {}
        },
        _ => match code {
            KeyCode::Char(' ') if state.page == Page::Mapping => match state.focus {
                FocusTarget::Mapping(MappingFocus::DemoToggle) => {
                    state.mapping_demo_mode = !state.mapping_demo_mode;
                    start_mapping_scan(state, tx);
                }
                FocusTarget::Mapping(MappingFocus::OverrideToggle) => {
                    state.mapping_override = !state.mapping_override;
                }
                _ => {}
            },
            KeyCode::Char('o') | KeyCode::Char('O') if state.page == Page::Mapping => {
                // Keyboard convenience: toggle override mode.
                state.mapping_override = !state.mapping_override;
            }
            KeyCode::Char('/') if state.page == Page::Mapping => {
                state.focus = match state.focus {
                    FocusTarget::Mapping(MappingFocus::TargetList)
                    | FocusTarget::Mapping(MappingFocus::TargetSearch) => {
                        FocusTarget::Mapping(MappingFocus::TargetSearch)
                    }
                    _ => FocusTarget::Mapping(MappingFocus::SourceSearch),
                };
            }
            KeyCode::Char('u') | KeyCode::Char('U') if state.page == Page::Mapping => {
                unassign_selected(state);
            }
            KeyCode::Up | KeyCode::Down if state.page == Page::Mapping => {
                if matches!(state.focus, FocusTarget::Mapping(MappingFocus::SourceList)) {
                    let ids = filtered_source_ids(state);
                    if ids.is_empty() {
                        return;
                    }
                    if matches!(code, KeyCode::Up) {
                        state.source_list_index = state.source_list_index.saturating_sub(1);
                    } else {
                        state.source_list_index =
                            (state.source_list_index + 1).min(ids.len().saturating_sub(1));
                    }
                    state.selected_source_id = ids.get(state.source_list_index).cloned();
                    // When source changes, clear target selection (preview still shows mapped targets).
                    state.selected_target_id = None;
                }
                if matches!(state.focus, FocusTarget::Mapping(MappingFocus::TargetList)) {
                    let ids = filtered_target_ids(state);
                    if ids.is_empty() {
                        return;
                    }
                    if matches!(code, KeyCode::Up) {
                        state.target_list_index = state.target_list_index.saturating_sub(1);
                    } else {
                        state.target_list_index =
                            (state.target_list_index + 1).min(ids.len().saturating_sub(1));
                    }
                    state.selected_target_id = ids.get(state.target_list_index).cloned();
                }
            }
            KeyCode::Up | KeyCode::Down if state.page == Page::DataSource => {
                state.data_source_kind = match state.data_source_kind {
                    DataSourceKind::Local => DataSourceKind::Remote,
                    DataSourceKind::Remote => DataSourceKind::Local,
                };
            }
            KeyCode::Up | KeyCode::Down if state.page == Page::Database => {
                state.db_kind = match state.db_kind {
                    DbKind::Local => DbKind::Remote,
                    DbKind::Remote => DbKind::Local,
                };
                // Switching branches resets any prior test state.
                state.db_test_status = DbTestStatus::Idle;
                state.db_test_message.clear();
                // Reset focus to the first field if the current page has fields.
                if page_field_count(state) > 0 {
                    state.focus = FocusTarget::Field(0);
                } else {
                    set_focused_button(state, ButtonFocus::Next);
                }
            }
            KeyCode::Up if state.page == Page::InstallType => {
                state.installation_type = match state.installation_type {
                    InstallationType::Typical => InstallationType::ImportConfig,
                    InstallationType::Custom => InstallationType::Typical,
                    InstallationType::ImportConfig => InstallationType::Custom,
                };
                update_page_validation(state);
            }
            KeyCode::Down if state.page == Page::InstallType => {
                state.installation_type = match state.installation_type {
                    InstallationType::Typical => InstallationType::Custom,
                    InstallationType::Custom => InstallationType::ImportConfig,
                    InstallationType::ImportConfig => InstallationType::Typical,
                };
                update_page_validation(state);
            }
            KeyCode::Left | KeyCode::Right if state.page == Page::Database => {
                // DB Setup wizard toggles (non-text controls).
                if state.db_kind == DbKind::Local {
                    // Create NEW branch: location toggle when not editing a text field.
                    if !matches!(state.focus, FocusTarget::Field(_)) {
                        state.new_db_location = state.new_db_location.toggle();
                        update_page_validation(state);
                    }
                } else {
                    // Existing branch: hosted-where selection, or TLS selection when focused.
                    if !state.db_use_conn_string && matches!(state.focus, FocusTarget::Field(5)) {
                        // TLS selection (cycle disable/prefer/require)
                        state.db_ssl_mode = match state.db_ssl_mode.as_str() {
                            "disable" => "prefer".to_string(),
                            "prefer" => "require".to_string(),
                            _ => "disable".to_string(),
                        };
                        update_page_validation(state);
                    }

                    if !matches!(state.focus, FocusTarget::Field(_)) {
                        state.existing_hosted_where = state.existing_hosted_where.next();
                        // Best-effort inference: if the hosting implies an engine, update defaults.
                        state.db_engine = match state.existing_hosted_where {
                            ExistingHostedWhere::AzureSqlMi => DbEngine::SqlServer,
                            ExistingHostedWhere::Neon | ExistingHostedWhere::Supabase => {
                                DbEngine::Postgres
                            }
                            _ => state.db_engine,
                        };
                        update_page_validation(state);
                    }
                }
            }
            KeyCode::Char(' ') if state.page == Page::Database => {
                // Existing DB only: toggle connection mode (connection string vs details)
                if state.db_kind == DbKind::Remote {
                    state.db_use_conn_string = !state.db_use_conn_string;
                    // Reset test status when switching modes.
                    state.db_test_status = DbTestStatus::Idle;
                    state.db_test_message.clear();
                    // Reset focus to first field if any.
                    if page_field_count(state) > 0 {
                        state.focus = FocusTarget::Field(0);
                    }
                }
            }
            KeyCode::Char('t') | KeyCode::Char('T') if state.page == Page::Database => {
                // Existing DB only: Start DB connection test in a background task.
                if state.db_kind == DbKind::Local {
                    return;
                }

                // No connection attempt until required fields exist.
                if state.db_use_conn_string {
                    if state.db_conn_string.value.trim().is_empty() {
                        state.db_test_status = DbTestStatus::Fail;
                        state.db_test_message =
                            "Missing required inputs: Connection string.".to_string();
                        return;
                    }
                } else {
                    let mut missing = Vec::new();
                    if state.db_host.value.trim().is_empty() {
                        missing.push("Host");
                    }
                    if state.db_port.value.trim().is_empty() {
                        missing.push("Port");
                    }
                    if state.db_database.value.trim().is_empty() {
                        missing.push("Database");
                    }
                    if state.db_user.value.trim().is_empty() {
                        missing.push("Username");
                    }
                    if state.db_password.value.trim().is_empty() {
                        missing.push("Password");
                    }
                    if !missing.is_empty() {
                        state.db_test_status = DbTestStatus::Fail;
                        state.db_test_message =
                            format!("Missing required inputs: {}.", missing.join(", "));
                        return;
                    }
                }

                state.db_test_status = DbTestStatus::Testing;
                state.db_test_message = "Testing connection...".to_string();

                let guess_engine_from_conn_str = |conn_str: &str| -> DbEngine {
                    let s = conn_str.trim().to_ascii_lowercase();
                    if s.starts_with("postgres://")
                        || s.starts_with("postgresql://")
                        || s.contains("host=")
                    {
                        DbEngine::Postgres
                    } else {
                        DbEngine::SqlServer
                    }
                };

                let conn_str = if state.db_use_conn_string
                    && !state.db_conn_string.value.trim().is_empty()
                {
                    state.db_conn_string.value.trim().to_string()
                } else {
                    // Build a structured connection string from fields (details mode).
                    let engine = match state.existing_hosted_where {
                        ExistingHostedWhere::AzureSqlMi => DbEngine::SqlServer,
                        ExistingHostedWhere::Neon | ExistingHostedWhere::Supabase => {
                            DbEngine::Postgres
                        }
                        _ => {
                            // Heuristic fallback: common port values.
                            if state.db_port.value.trim() == "1433" {
                                DbEngine::SqlServer
                            } else {
                                DbEngine::Postgres
                            }
                        }
                    };
                    state.db_engine = engine;

                    match engine {
                        DbEngine::Postgres => {
                            let port = if state.db_port.value.trim().is_empty() {
                                "5432"
                            } else {
                                state.db_port.value.trim()
                            };
                            let ssl = state.db_ssl_mode.trim();
                            let host = if state.db_host.value.trim().is_empty() {
                                "localhost"
                            } else {
                                state.db_host.value.trim()
                            };
                            let db = if state.db_database.value.trim().is_empty() {
                                "cadalytix"
                            } else {
                                state.db_database.value.trim()
                            };
                            let user = state.db_user.value.trim();
                            let pass = &state.db_password.value;
                            format!(
                                "postgresql://{}:{}@{}:{}/{}?sslmode={}",
                                user, pass, host, port, db, ssl
                            )
                        }
                        DbEngine::SqlServer => {
                            let host = if state.db_host.value.trim().is_empty() {
                                "localhost"
                            } else {
                                state.db_host.value.trim()
                            };
                            let port = state.db_port.value.trim();
                            let server = if port.is_empty() {
                                host.to_string()
                            } else {
                                format!("{},{}", host, port)
                            };
                            let db = if state.db_database.value.trim().is_empty() {
                                "cadalytix"
                            } else {
                                state.db_database.value.trim()
                            };
                            let user = state.db_user.value.trim();
                            let pass = &state.db_password.value;
                            let encrypt = if state.db_ssl_mode.trim() == "disable" {
                                "false"
                            } else {
                                "true"
                            };
                            format!(
                                "Server={};Database={};User Id={};Password={};TrustServerCertificate=true;Encrypt={};",
                                server, db, user, pass, encrypt
                            )
                        }
                    }
                };

                let engine = if state.db_use_conn_string {
                    match guess_engine_from_conn_str(&conn_str) {
                        DbEngine::Postgres => "postgres".to_string(),
                        DbEngine::SqlServer => "sqlserver".to_string(),
                    }
                } else {
                    match state.db_engine {
                        DbEngine::Postgres => "postgres".to_string(),
                        DbEngine::SqlServer => "sqlserver".to_string(),
                    }
                };

                let tx = tx.clone();
                thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build();
                    match rt {
                        Ok(rt) => {
                            let req = crate::api::installer::TestDbConnectionRequest {
                                engine,
                                connection_string: conn_str,
                            };
                            let res =
                                rt.block_on(crate::api::installer::test_db_connection(Some(req)));
                            match res {
                                Ok(r) => {
                                    let _ = tx.send(UiMsg::DbTestComplete {
                                        success: r.success,
                                        message: if r.success {
                                            "Connection successful.".to_string()
                                        } else {
                                            format!("Connection failed: {}", r.message)
                                        },
                                    });
                                }
                                Err(e) => {
                                    let _ = tx.send(UiMsg::DbTestComplete {
                                        success: false,
                                        message: format!("Connection failed: {}", e),
                                    });
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(UiMsg::DbTestComplete {
                                success: false,
                                message: format!("Internal error: {}", e),
                            });
                        }
                    }
                });
            }
            KeyCode::Char('b') | KeyCode::Char('B') if state.page == Page::Destination => {
                // Browse-like folder picker (TUI).
                let raw = state.destination_path.value.trim();
                let current = if raw.is_empty() {
                    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
                } else {
                    std::path::PathBuf::from(raw)
                };
                let entries = browse_folder_entries(&current);
                state.modal = Some(Modal::BrowseFolder {
                    current,
                    entries,
                    selected: 0,
                });
            }
            KeyCode::Up | KeyCode::Down if state.page == Page::Storage => {
                // Toggle defaults/custom
                state.storage_mode = match state.storage_mode {
                    StorageMode::Defaults => StorageMode::Custom,
                    StorageMode::Custom => StorageMode::Defaults,
                };
                // Reset focus into the page fields (if any)
                if page_field_count(state) > 0 {
                    state.focus = FocusTarget::Field(0);
                } else {
                    set_focused_button(state, ButtonFocus::Next);
                }
            }
            KeyCode::Left | KeyCode::Right
                if state.page == Page::Storage && state.storage_mode == StorageMode::Custom =>
            {
                // Cycle storage location
                state.storage_location = match state.storage_location {
                    StorageLocation::System => StorageLocation::Attached,
                    StorageLocation::Attached => StorageLocation::Custom,
                    StorageLocation::Custom => StorageLocation::System,
                };
                if page_field_count(state) > 0 && !matches!(state.focus, FocusTarget::Field(_)) {
                    state.focus = FocusTarget::Field(0);
                }
            }
            KeyCode::Char('p') | KeyCode::Char('P')
                if state.page == Page::Storage && state.storage_mode == StorageMode::Custom =>
            {
                // Cycle retention policy
                state.retention_policy = match state.retention_policy {
                    RetentionPolicy::Rolling18 => RetentionPolicy::Rolling12,
                    RetentionPolicy::Rolling12 => RetentionPolicy::MaxDisk,
                    RetentionPolicy::MaxDisk => RetentionPolicy::KeepEverything,
                    RetentionPolicy::KeepEverything => RetentionPolicy::Rolling18,
                };
                if page_field_count(state) > 0 && !matches!(state.focus, FocusTarget::Field(_)) {
                    state.focus = FocusTarget::Field(0);
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R') if state.page == Page::Retention => {
                state.hot_retention_choice = match state.hot_retention_choice {
                    HotRetentionChoice::Months12 => HotRetentionChoice::Months18,
                    HotRetentionChoice::Months18 => HotRetentionChoice::Custom,
                    HotRetentionChoice::Custom => HotRetentionChoice::Months12,
                };
                if page_field_count(state) > 0 {
                    state.focus = FocusTarget::Field(0);
                } else {
                    set_focused_button(state, ButtonFocus::Next);
                }
            }
            KeyCode::Char('f') | KeyCode::Char('F') if state.page == Page::Archive => {
                state.archive_format = match state.archive_format {
                    ArchiveFormatChoice::ZipNdjson => ArchiveFormatChoice::ZipCsv,
                    ArchiveFormatChoice::ZipCsv => ArchiveFormatChoice::ZipNdjson,
                };
            }
            KeyCode::Char(' ')
                if state.page == Page::Archive && !matches!(state.focus, FocusTarget::Field(_)) =>
            {
                state.archive_catch_up_on_startup = !state.archive_catch_up_on_startup;
            }
            KeyCode::Char(' ') if state.page == Page::Consent => {
                state.consent_to_sync = !state.consent_to_sync;
            }
            KeyCode::Char('d') | KeyCode::Char('D') if state.page == Page::Consent => {
                state.consent_details_expanded = !state.consent_details_expanded;
            }
            KeyCode::Tab => {
                if state.page == Page::Mapping {
                    state.focus = match state.focus {
                        FocusTarget::Mapping(MappingFocus::DemoToggle) => {
                            FocusTarget::Mapping(MappingFocus::OverrideToggle)
                        }
                        FocusTarget::Mapping(MappingFocus::OverrideToggle) => {
                            FocusTarget::Mapping(MappingFocus::SourceSearch)
                        }
                        FocusTarget::Mapping(MappingFocus::SourceSearch) => {
                            FocusTarget::Mapping(MappingFocus::SourceList)
                        }
                        FocusTarget::Mapping(MappingFocus::SourceList) => {
                            FocusTarget::Mapping(MappingFocus::TargetSearch)
                        }
                        FocusTarget::Mapping(MappingFocus::TargetSearch) => {
                            FocusTarget::Mapping(MappingFocus::TargetList)
                        }
                        FocusTarget::Mapping(MappingFocus::TargetList) => {
                            FocusTarget::Button(ButtonFocus::Back)
                        }
                        FocusTarget::Button(ButtonFocus::Back) => {
                            FocusTarget::Button(ButtonFocus::Next)
                        }
                        FocusTarget::Button(ButtonFocus::Next) => {
                            FocusTarget::Button(ButtonFocus::Cancel)
                        }
                        FocusTarget::Button(ButtonFocus::Cancel) => {
                            FocusTarget::Mapping(MappingFocus::DemoToggle)
                        }
                        _ => FocusTarget::Mapping(MappingFocus::DemoToggle),
                    };
                    return;
                }

                let fields = page_field_count(state);
                if fields == 0 {
                    let next = match focused_button(state) {
                        ButtonFocus::Back => ButtonFocus::Next,
                        ButtonFocus::Next => ButtonFocus::Cancel,
                        ButtonFocus::Cancel => ButtonFocus::Back,
                    };
                    set_focused_button(state, next);
                } else {
                    state.focus = match state.focus {
                        FocusTarget::Button(ButtonFocus::Back) => {
                            FocusTarget::Button(ButtonFocus::Next)
                        }
                        FocusTarget::Button(ButtonFocus::Next) => {
                            FocusTarget::Button(ButtonFocus::Cancel)
                        }
                        FocusTarget::Button(ButtonFocus::Cancel) => FocusTarget::Field(0),
                        FocusTarget::Field(i) => {
                            if i + 1 < fields {
                                FocusTarget::Field(i + 1)
                            } else {
                                FocusTarget::Button(ButtonFocus::Back)
                            }
                        }
                        FocusTarget::Mapping(_) => FocusTarget::Button(ButtonFocus::Back),
                    };
                }
            }
            KeyCode::Enter => {
                if state.page == Page::Mapping {
                    match state.focus {
                        FocusTarget::Mapping(MappingFocus::SourceSearch) => {
                            state.focus = FocusTarget::Mapping(MappingFocus::SourceList);
                            return;
                        }
                        FocusTarget::Mapping(MappingFocus::TargetSearch) => {
                            state.focus = FocusTarget::Mapping(MappingFocus::TargetList);
                            return;
                        }
                        FocusTarget::Mapping(MappingFocus::SourceList) => {
                            let ids = filtered_source_ids(state);
                            state.selected_source_id = ids.get(state.source_list_index).cloned();
                            state.selected_target_id = None;
                            return;
                        }
                        FocusTarget::Mapping(MappingFocus::TargetList) => {
                            let src = state.selected_source_id.clone().or_else(|| {
                                filtered_source_ids(state)
                                    .get(state.source_list_index)
                                    .cloned()
                            });
                            let tgt = state.selected_target_id.clone().or_else(|| {
                                filtered_target_ids(state)
                                    .get(state.target_list_index)
                                    .cloned()
                            });
                            if let (Some(source_id), Some(target_id)) = (src, tgt) {
                                attempt_map(state, &source_id, &target_id);
                            }
                            return;
                        }
                        _ => {}
                    }
                }

                match focused_button(state) {
                    ButtonFocus::Back => {
                        if can_go_back(state.page) {
                            state.page = prev_page(state.page);
                        }
                    }
                    ButtonFocus::Next => {
                        if state.page == Page::Complete {
                            state.quit = true;
                            return;
                        }

                        if can_go_next(state) {
                            // Installing: start the install run on Ready.
                            if state.page == Page::Ready {
                                state.page = Page::Installing;
                                state.install_detail.clear();
                                state.install_progress = Some(ProgressPayload {
                                    correlation_id: "pending".to_string(),
                                    step: "start".to_string(),
                                    severity: "info".to_string(),
                                    phase: "install".to_string(),
                                    percent: 0,
                                    message: "Starting installation...".to_string(),
                                    elapsed_ms: None,
                                    eta_ms: None,
                                });

                                let req = build_install_request(state);
                                let secrets = Arc::clone(secrets);
                                let tx = tx.clone();
                                thread::spawn(move || {
                                    let correlation_id = Uuid::new_v4().to_string();
                                    let tx_progress = tx.clone();
                                    let progress_emitter: ProgressEmitter =
                                        Arc::new(move |p: ProgressPayload| {
                                            let _ = tx_progress.send(UiMsg::InstallProgress(p));
                                        });

                                    let rt = tokio::runtime::Builder::new_current_thread()
                                        .enable_all()
                                        .build();
                                    match rt {
                                        Ok(rt) => {
                                            let result = rt.block_on(installer::run_installation(
                                                secrets,
                                                req,
                                                correlation_id.clone(),
                                                progress_emitter,
                                            ));
                                            match result {
                                                Ok(artifacts) => {
                                                    let _ = tx.send(UiMsg::InstallFinished {
                                                        success: true,
                                                        message: "Installation complete."
                                                            .to_string(),
                                                        correlation_id,
                                                        artifacts: Some(artifacts),
                                                    });
                                                }
                                                Err(e) => {
                                                    let _ = tx.send(UiMsg::InstallFinished {
                                                        success: false,
                                                        message: e.to_string(),
                                                        correlation_id,
                                                        artifacts: None,
                                                    });
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            let _ = tx.send(UiMsg::InstallFinished {
                                                success: false,
                                                message: format!(
                                                    "Internal error starting installer: {}",
                                                    e
                                                ),
                                                correlation_id,
                                                artifacts: None,
                                            });
                                        }
                                    }
                                });
                            } else {
                                state.page = next_page(state.page);
                                // Reset focus on each navigation
                                if state.page == Page::Mapping {
                                    state.focus = FocusTarget::Mapping(MappingFocus::SourceList);
                                    start_mapping_scan(state, tx);
                                } else if page_field_count(state) > 0 {
                                    state.focus = FocusTarget::Field(0);
                                } else {
                                    set_focused_button(state, ButtonFocus::Next);
                                }
                            }
                        }
                    }
                    ButtonFocus::Cancel => {
                        if can_cancel(state.page) {
                            state.modal = Some(Modal::ConfirmCancel);
                            set_focused_button(state, ButtonFocus::Next);
                        }
                    }
                }
            }
            _ => {}
        },
    }
}

fn build_install_request(state: &WizardState) -> StartInstallRequest {
    // For now, reuse the Phase 5 placeholder runner:
    // - Config DB connection string uses the DB Setup page values
    // - Call data uses the Data Source page values
    let config_db = if state.db_kind == DbKind::Local {
        // Create NEW CADalytix Database: connection string may not exist yet.
        String::new()
    } else if state.db_use_conn_string && !state.db_conn_string.value.trim().is_empty() {
        state.db_conn_string.value.trim().to_string()
    } else {
        match state.db_engine {
            DbEngine::Postgres => {
                let port = if state.db_port.value.trim().is_empty() {
                    "5432"
                } else {
                    state.db_port.value.trim()
                };
                let ssl = state.db_ssl_mode.trim();
                let user = state.db_user.value.trim();
                let pass = &state.db_password.value;
                let host = if state.db_host.value.trim().is_empty() {
                    "localhost"
                } else {
                    state.db_host.value.trim()
                };
                let db = if state.db_database.value.trim().is_empty() {
                    "cadalytix"
                } else {
                    state.db_database.value.trim()
                };
                format!(
                    "postgresql://{}:{}@{}:{}/{}?sslmode={}",
                    user, pass, host, port, db, ssl
                )
            }
            DbEngine::SqlServer => {
                let host = if state.db_host.value.trim().is_empty() {
                    "localhost"
                } else {
                    state.db_host.value.trim()
                };
                let port = if state.db_port.value.trim().is_empty() {
                    "1433"
                } else {
                    state.db_port.value.trim()
                };
                let db = if state.db_database.value.trim().is_empty() {
                    "cadalytix"
                } else {
                    state.db_database.value.trim()
                };
                let user = state.db_user.value.trim();
                let pass = &state.db_password.value;
                // TLS toggle is represented as "disable|prefer|require" in the TUI state;
                // for SQL Server we map it to Encrypt=true/false (TrustServerCertificate=true for now).
                let encrypt = matches!(
                    state.db_ssl_mode.trim().to_ascii_lowercase().as_str(),
                    "require" | "true"
                );
                format!(
                    "Server={},{};Database={};User Id={};Password={};TrustServerCertificate=true;Encrypt={};",
                    host,
                    port,
                    db,
                    user,
                    pass,
                    if encrypt { "true" } else { "false" }
                )
            }
        }
    };

    let call_data = {
        let host = if state.call_data_host.value.trim().is_empty() {
            "localhost"
        } else {
            state.call_data_host.value.trim()
        };
        let port = if state.call_data_port.value.trim().is_empty() {
            "1433"
        } else {
            state.call_data_port.value.trim()
        };
        let server = format!("{},{}", host, port);
        let db = state.call_data_database.value.trim();
        let user = state.call_data_user.value.trim();
        let pass = &state.call_data_password.value;
        format!("Server={};Database={};User Id={};Password={};TrustServerCertificate=true;Encrypt=false;", server, db, user, pass)
    };

    let storage = StorageConfig {
        mode: match state.storage_mode {
            StorageMode::Defaults => "defaults".to_string(),
            StorageMode::Custom => "custom".to_string(),
        },
        location: match state.storage_location {
            StorageLocation::System => "system".to_string(),
            StorageLocation::Attached => "attached".to_string(),
            StorageLocation::Custom => "custom".to_string(),
        },
        custom_path: state.storage_custom_path.value.clone(),
        retention_policy: match state.retention_policy {
            RetentionPolicy::Rolling18 => "18".to_string(),
            RetentionPolicy::Rolling12 => "12".to_string(),
            RetentionPolicy::MaxDisk => "max".to_string(),
            RetentionPolicy::KeepEverything => "keep".to_string(),
        },
        max_disk_gb: state.max_disk_gb.value.clone(),
    };

    let mut mappings: HashMap<String, String> = HashMap::new();
    for (target_id, source_id) in state.target_to_source.iter() {
        mappings.insert(target_id.clone(), mapping_source_raw(state, source_id));
    }

    let hot_months = match state.hot_retention_choice {
        HotRetentionChoice::Months12 => 12,
        HotRetentionChoice::Months18 => 18,
        HotRetentionChoice::Custom => state
            .hot_retention_custom_months
            .value
            .trim()
            .parse::<u32>()
            .unwrap_or(18)
            .max(1),
    };
    let hot_retention = HotRetentionConfig { months: hot_months };

    let archive_format = match state.archive_format {
        ArchiveFormatChoice::ZipNdjson => "zip+ndjson".to_string(),
        ArchiveFormatChoice::ZipCsv => "zip+csv".to_string(),
    };
    let max_usage_gb = state
        .archive_max_usage_gb
        .value
        .trim()
        .parse::<u32>()
        .unwrap_or(0);
    let schedule_day_of_month = state
        .archive_schedule_day_of_month
        .value
        .trim()
        .parse::<u8>()
        .unwrap_or(1)
        .clamp(1, 28);
    let schedule_time_local = state.archive_schedule_time_local.value.trim().to_string();
    let archive_policy = ArchivePolicyConfig {
        format: archive_format,
        destination_path: state.archive_destination.value.trim().to_string(),
        max_usage_gb,
        schedule: ArchiveScheduleConfig {
            day_of_month: schedule_day_of_month,
            time_local: schedule_time_local,
        },
        catch_up_on_startup: state.archive_catch_up_on_startup,
    };

    let max_db_size_gb = state
        .new_db_max_size_gb
        .value
        .trim()
        .parse::<u32>()
        .unwrap_or(0);
    let db_setup = installer::DbSetupConfig {
        mode: if state.db_kind == DbKind::Local {
            "create_new".to_string()
        } else {
            "existing".to_string()
        },
        new_location: state.new_db_location.as_id().to_string(),
        new_specific_path: state.new_db_specific_path.value.trim().to_string(),
        max_db_size_gb,
        existing_hosted_where: state.existing_hosted_where.as_id().to_string(),
        existing_connect_mode: if state.db_use_conn_string {
            "connection_string".to_string()
        } else {
            "details".to_string()
        },
    };

    let mapping_state = Some(MappingState {
        mapping_override: state.mapping_override,
        source_fields: state
            .source_fields
            .iter()
            .map(|s| MappingSourceField {
                id: s.id.clone(),
                raw_name: s.raw_name.clone(),
                display_name: s.display_name.clone(),
            })
            .collect(),
        target_fields: state
            .target_fields
            .iter()
            .map(|t| MappingTargetField {
                id: t.id.clone(),
                name: t.name.clone(),
                required: t.required,
            })
            .collect(),
        source_to_targets: state.source_to_targets.clone(),
        target_to_source: state.target_to_source.clone(),
    });

    StartInstallRequest {
        install_mode: match state.install_mode {
            InstallMode::Windows => "windows".to_string(),
            InstallMode::Docker => "docker".to_string(),
        },
        installation_type: match state.installation_type {
            InstallationType::Typical => "typical".to_string(),
            InstallationType::Custom => "custom".to_string(),
            InstallationType::ImportConfig => "import".to_string(),
        },
        destination_folder: state.destination_path.value.clone(),
        config_db_connection_string: config_db,
        call_data_connection_string: call_data,
        source_object_name: state.source_object_name.value.clone(),
        db_setup,
        storage,
        hot_retention,
        archive_policy,
        consent_to_sync: state.consent_to_sync,
        mappings,
        mapping_override: state.mapping_override,
        mapping_state,
    }
}

fn draw(area: Rect, f: &mut ratatui::Frame<'_>, state: &WizardState) {
    let (window_area, outer) = centered_window(area, 100, 30);

    // Outer frame
    let outer_block = Block::default()
        .borders(Borders::ALL)
        .title("CADalytix Setup");
    f.render_widget(outer_block, window_area);

    // Inner layout: banner + content + buttons row
    let inner = window_area.inner(&ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)].as_ref())
        .split(inner);

    let body = rows[0];
    let buttons = rows[1];

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(34), Constraint::Min(0)].as_ref())
        .split(body);

    // Left banner
    let banner_block = Block::default().borders(Borders::ALL);
    let logo = Paragraph::new(ASCII_LOGO)
        .block(banner_block)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });
    f.render_widget(logo, cols[0]);

    // Right content
    let title = page_title(state.page, state.install_mode);
    let content_text = match state.page {
        Page::Platform => {
            let w = if state.platform_selected == InstallMode::Windows {
                "[Windows]"
            } else {
                " Windows "
            };
            let d = if state.platform_selected == InstallMode::Docker {
                "[Docker / Linux]"
            } else {
                " Docker / Linux "
            };
            Text::from(vec![
                Line::from("Select installation mode:"),
                Line::from(""),
                Line::from(format!("  {}    {}", w, d)),
                Line::from(""),
                Line::from("Use Left/Right to change selection, Enter to select."),
            ])
        }
        Page::Welcome => {
            let mode = match state.install_mode {
                InstallMode::Windows => "Windows",
                InstallMode::Docker => "Docker / Linux",
            };
            Text::from(vec![
                Line::from("This wizard will guide you through installing CADalytix."),
                Line::from(""),
                Line::from(format!("Mode: {}", mode)),
            ])
        }
        Page::License => {
            let accept = if state.license_accepted { "[x]" } else { "[ ]" };
            let license_lines: Vec<&str> = vec![
                "LICENSE TEXT NOT PROVIDED.",
                "",
                "Place your license text (EULA) under Prod_Install_Wizard_Deployment/licenses/ and wire the loader.",
                "",
                "This TUI currently uses a placeholder license body.",
                "",
                "Use PageUp/PageDown to scroll.",
                "",
                "By proceeding, you acknowledge you have read and understood the license agreement.",
                "",
                "— End of placeholder license —",
            ];

            let offset = (state.license_scroll as usize).min(license_lines.len().saturating_sub(1));
            let visible = 8usize;
            let mut lines = Vec::new();
            for l in license_lines.iter().skip(offset).take(visible) {
                lines.push(Line::from(*l));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(format!(
                "{} I accept the terms of the license agreement",
                accept
            )));
            lines.push(Line::from(""));
            lines.push(Line::from("Space toggles the checkbox. PgUp/PgDn scroll."));
            Text::from(lines)
        }
        Page::InstallType => {
            let typical = if state.installation_type == InstallationType::Typical {
                "(x)"
            } else {
                "( )"
            };
            let custom = if state.installation_type == InstallationType::Custom {
                "(x)"
            } else {
                "( )"
            };
            let import = if state.installation_type == InstallationType::ImportConfig {
                "(x)"
            } else {
                "( )"
            };

            let mut lines = vec![
                Line::from("Select the type of installation you want."),
                Line::from(""),
                Line::from(format!("{} Typical (Recommended)", typical)),
                Line::from(format!("{} Custom", custom)),
                Line::from(format!("{} Import configuration file…", import)),
            ];

            if state.installation_type == InstallationType::ImportConfig {
                let prefix = if matches!(state.focus, FocusTarget::Field(0)) {
                    ">"
                } else {
                    " "
                };
                lines.push(Line::from(""));
                lines.push(Line::from(format!(
                    "{} Config file: {}",
                    prefix, state.import_config_path.value
                )));
                if let Some(err) = state.import_config_error.as_ref() {
                    lines.push(Line::from(format!("Error: {}", err)));
                }
                lines.push(Line::from("Tab to edit the path."));
            } else {
                lines.push(Line::from(""));
                lines.push(Line::from("Use Up/Down to change selection."));
            }

            Text::from(lines)
        }
        Page::Destination => {
            let prefix = if matches!(state.focus, FocusTarget::Field(0)) {
                ">"
            } else {
                " "
            };
            let mut lines = vec![
                Line::from("Choose the folder where CADalytix will be installed."),
                Line::from(""),
                Line::from(format!(
                    "{} Install path: {}",
                    prefix, state.destination_path.value
                )),
                Line::from("Required space: ~2–5 GB"),
            ];
            if let Some(err) = state.destination_error.as_ref() {
                lines.push(Line::from(format!("Error: {}", err)));
            }
            lines.push(Line::from(""));
            lines.push(Line::from("Tab to edit the path. Press B to browse."));
            Text::from(lines)
        }
        Page::DataSource => {
            let local = state.data_source_kind == DataSourceKind::Local;
            let r_local = if local { "(x)" } else { "( )" };
            let r_remote = if local { "( )" } else { "(x)" };

            let p0 = if matches!(state.focus, FocusTarget::Field(0)) {
                ">"
            } else {
                " "
            };
            let p1 = if matches!(state.focus, FocusTarget::Field(1)) {
                ">"
            } else {
                " "
            };
            let p2 = if matches!(state.focus, FocusTarget::Field(2)) {
                ">"
            } else {
                " "
            };
            let p3 = if matches!(state.focus, FocusTarget::Field(3)) {
                ">"
            } else {
                " "
            };
            let p4 = if matches!(state.focus, FocusTarget::Field(4)) {
                ">"
            } else {
                " "
            };
            let p5 = if matches!(state.focus, FocusTarget::Field(5)) {
                ">"
            } else {
                " "
            };

            Text::from(vec![
                Line::from(format!(
                    "{} Use this server/host (local environment)",
                    r_local
                )),
                Line::from(format!(
                    "{} Connect to an existing remote system/database",
                    r_remote
                )),
                Line::from(""),
                Line::from(format!(
                    "{} Database: {}",
                    p0, state.call_data_database.value
                )),
                Line::from(format!("{} Username: {}", p1, state.call_data_user.value)),
                Line::from(format!(
                    "{} Password: {}",
                    p2,
                    state.call_data_password.display()
                )),
                Line::from(format!("{} Host: {}", p3, state.call_data_host.value)),
                Line::from(format!("{} Port: {}", p4, state.call_data_port.value)),
                Line::from(format!(
                    "{} Source object name: {}",
                    p5, state.source_object_name.value
                )),
                Line::from(""),
                Line::from("Tab cycles fields."),
            ])
        }
        Page::Database => {
            let create_new = state.db_kind == DbKind::Local;
            let r_new = if create_new { "(x)" } else { "( )" };
            let r_existing = if create_new { "( )" } else { "(x)" };

            let mut lines = vec![
                Line::from(
                    "Do you want CADalytix to create a NEW database, or use an EXISTING database?",
                ),
                Line::from(""),
                Line::from(format!("{} Create NEW CADalytix Database", r_new)),
                Line::from(format!("{} Use EXISTING Database", r_existing)),
                Line::from(""),
            ];

            if create_new {
                lines.push(Line::from(
                    "Where should the new CADalytix database be created?",
                ));
                let r_machine = if state.new_db_location == NewDbLocation::ThisMachine {
                    "(x)"
                } else {
                    "( )"
                };
                let r_path = if state.new_db_location == NewDbLocation::SpecificPath {
                    "(x)"
                } else {
                    "( )"
                };
                lines.push(Line::from(format!(
                    "{} {}",
                    r_machine,
                    NewDbLocation::ThisMachine.as_str()
                )));
                lines.push(Line::from(format!(
                    "{} {}",
                    r_path,
                    NewDbLocation::SpecificPath.as_str()
                )));
                lines.push(Line::from("Left/Right changes location. Tab edits fields."));

                let f = |i: usize| {
                    if matches!(state.focus, FocusTarget::Field(j) if j == i) {
                        ">"
                    } else {
                        " "
                    }
                };

                lines.push(Line::from(""));
                if state.new_db_location == NewDbLocation::SpecificPath {
                    lines.push(Line::from(format!(
                        "{} Database path: {}",
                        f(0),
                        state.new_db_specific_path.value
                    )));
                    lines.push(Line::from(format!(
                        "{} Max DB size / storage allocation (GB): {}",
                        f(1),
                        state.new_db_max_size_gb.value
                    )));
                } else {
                    lines.push(Line::from(format!(
                        "{} Max DB size / storage allocation (GB): {}",
                        f(0),
                        state.new_db_max_size_gb.value
                    )));
                }

                lines.push(Line::from(""));
                lines.push(Line::from(
                    "Hot retention and archive policy are configured on the next pages.",
                ));
            } else {
                lines.push(Line::from(
                    "Where is the existing database hosted? (No login required)",
                ));
                lines.push(Line::from(format!(
                    "Hosted where: {} (Left/Right to change)",
                    state.existing_hosted_where.as_str()
                )));
                lines.push(Line::from(""));
                lines.push(Line::from("How do you want to connect?"));
                let r_conn = if state.db_use_conn_string {
                    "(x)"
                } else {
                    "( )"
                };
                let r_details = if state.db_use_conn_string {
                    "( )"
                } else {
                    "(x)"
                };
                lines.push(Line::from(format!("{} Connection string", r_conn)));
                lines.push(Line::from(format!(
                    "{} Enter connection details (host/server, port, db name, username, password, TLS)",
                    r_details
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(
                    "CADalytix does not ask you to log in to AWS/Azure/GCP and does not scan your cloud.",
                ));
                lines.push(Line::from(
                    "You only provide a database endpoint (connection string or host/port/user/password) with explicit permissions.",
                ));

                if state.db_use_conn_string {
                    let p0 = if matches!(state.focus, FocusTarget::Field(0)) {
                        ">"
                    } else {
                        " "
                    };
                    lines.push(Line::from(""));
                    lines.push(Line::from(format!(
                        "{} Connection string: {}",
                        p0, state.db_conn_string.value
                    )));
                    lines.push(Line::from("Tab to edit. Space switches connection mode."));
                } else {
                    let f = |i: usize| {
                        if matches!(state.focus, FocusTarget::Field(j) if j == i) {
                            ">"
                        } else {
                            " "
                        }
                    };
                    let tls_prefix = if matches!(state.focus, FocusTarget::Field(5)) {
                        ">"
                    } else {
                        " "
                    };

                    lines.push(Line::from(""));
                    lines.push(Line::from(format!(
                        "{} Host: {}",
                        f(0),
                        state.db_host.value
                    )));
                    lines.push(Line::from(format!(
                        "{} Port: {}",
                        f(1),
                        state.db_port.value
                    )));
                    lines.push(Line::from(format!(
                        "{} Database: {}",
                        f(2),
                        state.db_database.value
                    )));
                    lines.push(Line::from(format!(
                        "{} Username: {}",
                        f(3),
                        state.db_user.value
                    )));
                    lines.push(Line::from(format!(
                        "{} Password: {}",
                        f(4),
                        state.db_password.display()
                    )));
                    lines.push(Line::from(format!(
                        "{} TLS: {} (Left/Right to change)",
                        tls_prefix, state.db_ssl_mode
                    )));
                    lines.push(Line::from(""));
                    lines.push(Line::from("Press T to Test Connection."));
                }
            }

            let status = match state.db_test_status {
                DbTestStatus::Idle => "Idle",
                DbTestStatus::Testing => "Testing",
                DbTestStatus::Success => "Success",
                DbTestStatus::Fail => "Fail",
            };
            if !state.db_test_message.trim().is_empty() {
                lines.push(Line::from(format!(
                    "Test result: {} — {}",
                    status, state.db_test_message
                )));
            } else {
                lines.push(Line::from(format!("Test result: {}", status)));
            }

            Text::from(lines)
        }
        Page::Storage => {
            let defaults = if state.storage_mode == StorageMode::Defaults {
                "(x)"
            } else {
                "( )"
            };
            let custom = if state.storage_mode == StorageMode::Custom {
                "(x)"
            } else {
                "( )"
            };
            let mut lines = vec![
                Line::from("Configure database storage."),
                Line::from(""),
                Line::from(format!("{} Use defaults (Recommended)", defaults)),
                Line::from(format!("{} Customize storage", custom)),
            ];

            if state.storage_mode == StorageMode::Custom {
                let location = match state.storage_location {
                    StorageLocation::System => "Use system disk",
                    StorageLocation::Attached => "Use attached drive",
                    StorageLocation::Custom => "Use custom path",
                };
                let retention = match state.retention_policy {
                    RetentionPolicy::Rolling18 => "Rolling 18 months (Recommended)",
                    RetentionPolicy::Rolling12 => "Rolling 12 months",
                    RetentionPolicy::MaxDisk => "Max disk usage",
                    RetentionPolicy::KeepEverything => "Keep everything (Not recommended)",
                };
                lines.push(Line::from(""));
                lines.push(Line::from(format!(
                    "Storage location: {} (Left/Right to change)",
                    location
                )));
                if state.storage_location == StorageLocation::Custom {
                    let p = if matches!(state.focus, FocusTarget::Field(0)) {
                        ">"
                    } else {
                        " "
                    };
                    lines.push(Line::from(format!(
                        "{} Custom path: {}",
                        p, state.storage_custom_path.value
                    )));
                }
                lines.push(Line::from(format!(
                    "Retention policy: {} (P to change)",
                    retention
                )));
                if state.retention_policy == RetentionPolicy::MaxDisk {
                    let idx = if state.storage_location == StorageLocation::Custom {
                        1
                    } else {
                        0
                    };
                    let p = if matches!(state.focus, FocusTarget::Field(i) if i == idx) {
                        ">"
                    } else {
                        " "
                    };
                    lines.push(Line::from(format!(
                        "{} Max disk usage (GB): {}",
                        p, state.max_disk_gb.value
                    )));
                }
                lines.push(Line::from(""));
                lines.push(Line::from(
                    "Up/Down toggles defaults/custom. Tab edits fields.",
                ));
            } else {
                lines.push(Line::from(""));
                lines.push(Line::from("Up/Down toggles defaults/custom."));
            }

            Text::from(lines)
        }
        Page::Retention => {
            let r12 = if state.hot_retention_choice == HotRetentionChoice::Months12 {
                "(x)"
            } else {
                "( )"
            };
            let r18 = if state.hot_retention_choice == HotRetentionChoice::Months18 {
                "(x)"
            } else {
                "( )"
            };
            let r_custom = if state.hot_retention_choice == HotRetentionChoice::Custom {
                "(x)"
            } else {
                "( )"
            };
            let p0 = if matches!(state.focus, FocusTarget::Field(0)) {
                ">"
            } else {
                " "
            };

            let mut lines = vec![
                Line::from("Choose how long to keep hot data in the database."),
                Line::from(""),
                Line::from(format!("{} 12 months (Recommended)", r12)),
                Line::from(format!("{} 18 months (Recommended)", r18)),
                Line::from(format!("{} Custom months:", r_custom)),
            ];

            if state.hot_retention_choice == HotRetentionChoice::Custom {
                lines.push(Line::from(format!(
                    "{} Months: {}",
                    p0, state.hot_retention_custom_months.value
                )));
                let n = state
                    .hot_retention_custom_months
                    .value
                    .trim()
                    .parse::<u32>()
                    .unwrap_or(0);
                if n == 0 || n > 240 {
                    lines.push(Line::from("Error: Enter a valid number of months."));
                }
                lines.push(Line::from(""));
                lines.push(Line::from("Tab to edit the months value."));
            } else {
                lines.push(Line::from(""));
                lines.push(Line::from("Use R to change selection."));
            }

            lines.push(Line::from("R cycles 12/18/custom."));
            Text::from(lines)
        }
        Page::Archive => {
            let r_ndjson = if state.archive_format == ArchiveFormatChoice::ZipNdjson {
                "(x)"
            } else {
                "( )"
            };
            let r_csv = if state.archive_format == ArchiveFormatChoice::ZipCsv {
                "(x)"
            } else {
                "( )"
            };

            let p0 = if matches!(state.focus, FocusTarget::Field(0)) {
                ">"
            } else {
                " "
            };
            let p1 = if matches!(state.focus, FocusTarget::Field(1)) {
                ">"
            } else {
                " "
            };
            let p2 = if matches!(state.focus, FocusTarget::Field(2)) {
                ">"
            } else {
                " "
            };
            let p3 = if matches!(state.focus, FocusTarget::Field(3)) {
                ">"
            } else {
                " "
            };

            let catch_up = if state.archive_catch_up_on_startup {
                "[x]"
            } else {
                "[ ]"
            };

            let mut lines = vec![
                Line::from("Configure cold storage (archive) settings."),
                Line::from(""),
                Line::from(format!("{} ZIP + NDJSON (Preferred)", r_ndjson)),
                Line::from(format!("{} ZIP + CSV", r_csv)),
                Line::from(""),
                Line::from(format!(
                    "{} Destination folder: {}",
                    p0, state.archive_destination.value
                )),
                Line::from(format!(
                    "{} Max archive usage cap (GB): {}",
                    p1, state.archive_max_usage_gb.value
                )),
                Line::from(format!(
                    "{} Schedule day (1-28): {}",
                    p2, state.archive_schedule_day_of_month.value
                )),
                Line::from(format!(
                    "{} Schedule time (HH:MM): {}",
                    p3, state.archive_schedule_time_local.value
                )),
                Line::from(format!("{} Catch-up on startup (Space)", catch_up)),
            ];

            // Inline validation errors (Windows-installer tone; block Next when invalid).
            if state.archive_destination.value.trim().is_empty() {
                lines.push(Line::from("Error: Archive destination folder is required."));
            } else if state
                .archive_max_usage_gb
                .value
                .trim()
                .parse::<u32>()
                .unwrap_or(0)
                == 0
            {
                lines.push(Line::from(
                    "Error: Max archive usage must be a positive number.",
                ));
            } else {
                let day = state
                    .archive_schedule_day_of_month
                    .value
                    .trim()
                    .parse::<u32>()
                    .unwrap_or(0);
                if !(1..=28).contains(&day) {
                    lines.push(Line::from("Error: Schedule day must be between 1 and 28."));
                } else if !is_valid_time_hhmm(state.archive_schedule_time_local.value.trim()) {
                    lines.push(Line::from("Error: Schedule time must be HH:MM."));
                }
            }

            lines.push(Line::from(""));
            lines.push(Line::from("Tab cycles fields. F changes format."));
            Text::from(lines)
        }
        Page::Consent => {
            let c = if state.consent_to_sync { "[x]" } else { "[ ]" };
            let mut lines = vec![
                Line::from(format!(
                    "{} Allow CADalytix to receive install metadata + schema mapping to for support improvements",
                    c
                )),
                Line::from(""),
                Line::from("Space toggles consent. D shows details."),
            ];
            if state.consent_details_expanded {
                lines.push(Line::from(""));
                lines.push(Line::from(
                    "Exactly what is sent (no passwords or connection strings):",
                ));
                lines.push(Line::from("- Installer version and timestamp"));
                lines.push(Line::from("- Install mode (Windows / Docker)"));
                lines.push(Line::from("- Storage/retention/archive settings"));
                lines.push(Line::from(
                    "- Schema mapping (field names + chosen targets)",
                ));
                lines.push(Line::from("- Aggregate counts"));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(
                "This setting is stored locally. Network sync is not performed in this phase.",
            ));
            Text::from(lines)
        }
        Page::Mapping => Text::from(vec![
            Line::from("Schema Mapping"),
            Line::from(""),
            Line::from("This page will be implemented next (two-pane mapper)."),
            Line::from(""),
            Line::from("Select Next to continue."),
        ]),
        Page::Ready => Text::from(vec![
            Line::from("Setup is ready to begin installation."),
            Line::from(""),
            Line::from(format!(
                "Mode: {}",
                match state.install_mode {
                    InstallMode::Windows => "Windows",
                    InstallMode::Docker => "Docker / Linux",
                }
            )),
            Line::from(format!("Install path: {}", state.destination_path.value)),
            Line::from(format!(
                "Config DB engine: {}",
                match state.db_engine {
                    DbEngine::SqlServer => "SQL Server",
                    DbEngine::Postgres => "PostgreSQL",
                }
            )),
            Line::from(format!(
                "Hot retention: {} months",
                match state.hot_retention_choice {
                    HotRetentionChoice::Months12 => 12,
                    HotRetentionChoice::Months18 => 18,
                    HotRetentionChoice::Custom => state
                        .hot_retention_custom_months
                        .value
                        .trim()
                        .parse::<u32>()
                        .unwrap_or(18),
                }
            )),
            Line::from(format!(
                "Archive format: {}",
                match state.archive_format {
                    ArchiveFormatChoice::ZipNdjson => "ZIP + NDJSON",
                    ArchiveFormatChoice::ZipCsv => "ZIP + CSV",
                }
            )),
            Line::from(format!(
                "Archive destination: {}",
                if state.archive_destination.value.trim().is_empty() {
                    "(not set)"
                } else {
                    state.archive_destination.value.trim()
                }
            )),
            Line::from(format!(
                "Archive cap (GB): {}",
                state.archive_max_usage_gb.value.trim()
            )),
            Line::from(format!(
                "Archive schedule: day {} at {}",
                state.archive_schedule_day_of_month.value.trim(),
                state.archive_schedule_time_local.value.trim()
            )),
            Line::from(format!(
                "Consent to Sync: {}",
                if state.consent_to_sync { "Yes" } else { "No" }
            )),
            Line::from("Passwords are not shown here."),
            Line::from(""),
            Line::from("Select Install to begin."),
        ]),
        Page::Installing => {
            let pct = state
                .install_progress
                .as_ref()
                .map(|p| p.percent)
                .unwrap_or(0);
            let msg = state
                .install_progress
                .as_ref()
                .map(|p| p.message.clone())
                .unwrap_or_default();
            let width = 30usize;
            let filled = ((pct as usize) * width) / 100;
            let bar = format!(
                "[{}{}] {}%",
                "#".repeat(filled),
                " ".repeat(width.saturating_sub(filled)),
                pct
            );

            let mut lines = vec![
                Line::from(bar),
                Line::from(format!("Current action: {}", msg)),
                Line::from(""),
            ];

            for l in state.install_detail.iter().rev().take(10).rev() {
                lines.push(Line::from(l.clone()));
            }
            if state.install_detail.is_empty() {
                lines.push(Line::from("(no details yet)"));
            }

            Text::from(lines)
        }
        Page::Complete => {
            let mut lines = vec![Line::from("CADalytix Setup has completed."), Line::from("")];
            if let Some(a) = state.install_artifacts.as_ref() {
                if let Some(lf) = a.log_folder.as_ref().filter(|s| !s.trim().is_empty()) {
                    lines.push(Line::from(format!("Log folder: {}", lf)));
                }
                if let Some(p) = a.manifest_path.as_ref().filter(|s| !s.trim().is_empty()) {
                    lines.push(Line::from(format!("Install manifest: {}", p)));
                }
                if let Some(p) = a.mapping_path.as_ref().filter(|s| !s.trim().is_empty()) {
                    lines.push(Line::from(format!("Mapping: {}", p)));
                }
                if let Some(p) = a.config_path.as_ref().filter(|s| !s.trim().is_empty()) {
                    lines.push(Line::from(format!("Install config: {}", p)));
                }
                lines.push(Line::from(""));
            }
            lines.push(Line::from("Select Finish to exit."));
            Text::from(lines)
        }
    };

    let content_block = Block::default().borders(Borders::ALL).title(title);
    f.render_widget(content_block, cols[1]);
    let content_inner = cols[1].inner(&ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });

    if state.page == Page::Mapping {
        draw_mapping_page(f, content_inner, state);
    } else {
        let content = Paragraph::new(content_text)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false });
        f.render_widget(content, content_inner);
    }

    // Bottom buttons row (right-aligned)
    draw_buttons(f, buttons, state);

    // Modal overlay
    if let Some(modal) = state.modal.as_ref() {
        match modal {
            Modal::ConfirmCancel => draw_cancel_modal(f, window_area, state),
            Modal::Message {
                title,
                body,
                return_to: _,
            } => draw_message_modal(f, window_area, title, body, state),
            Modal::ConfirmMapping {
                title,
                body,
                actions,
                selected,
                pending: _,
            } => draw_confirm_mapping_modal(f, window_area, title, body, actions, *selected),
            Modal::BrowseFolder {
                current,
                entries,
                selected,
            } => draw_browse_folder_modal(f, window_area, current, entries, *selected),
        }
    }

    // Prevent unused warning for outer
    let _ = outer;
}

fn centered_window(area: Rect, width: u16, height: u16) -> (Rect, Rect) {
    let w = width.min(area.width.saturating_sub(2)).max(60);
    let h = height.min(area.height.saturating_sub(2)).max(20);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let rect = Rect {
        x,
        y,
        width: w,
        height: h,
    };
    (rect, rect)
}

fn draw_buttons(f: &mut ratatui::Frame<'_>, area: Rect, state: &WizardState) {
    let back_enabled = can_go_back(state.page);
    let next_enabled = can_go_next(state);
    let cancel_enabled = can_cancel(state.page);

    let back = button_text(
        "Back",
        matches!(state.focus, FocusTarget::Button(ButtonFocus::Back)),
        back_enabled,
    );
    let next = button_text(
        next_label(state.page),
        matches!(state.focus, FocusTarget::Button(ButtonFocus::Next)),
        next_enabled,
    );
    let cancel = button_text(
        "Cancel",
        matches!(state.focus, FocusTarget::Button(ButtonFocus::Cancel)),
        cancel_enabled,
    );

    let line = Line::from(vec![
        back,
        ratatui::text::Span::raw(" "),
        next,
        ratatui::text::Span::raw(" "),
        cancel,
    ]);

    let p = Paragraph::new(Text::from(line)).alignment(Alignment::Right);
    f.render_widget(p, area);
}

fn draw_mapping_page(f: &mut ratatui::Frame<'_>, area: Rect, state: &WizardState) {
    let top_h = 6u16.min(area.height.saturating_sub(6)).max(3);
    let preview_h = 4u16.min(area.height.saturating_sub(3)).max(3);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(top_h),
                Constraint::Min(0),
                Constraint::Length(preview_h),
            ]
            .as_ref(),
        )
        .split(area);

    let required_unmapped: Vec<String> = state
        .target_fields
        .iter()
        .filter(|t| t.required && !state.target_to_source.contains_key(&t.id))
        .map(|t| t.name.clone())
        .collect();

    let mapped_count = state.target_to_source.len();
    let unassigned_sources = state
        .source_fields
        .iter()
        .filter(|s| {
            state
                .source_to_targets
                .get(&s.id)
                .map(|v| v.is_empty())
                .unwrap_or(true)
        })
        .count();

    // Top info + toggles
    let scan_suffix = if state.mapping_scanning {
        " (Scanning...)"
    } else {
        ""
    };
    let focus_demo = matches!(state.focus, FocusTarget::Mapping(MappingFocus::DemoToggle));
    let focus_override = matches!(
        state.focus,
        FocusTarget::Mapping(MappingFocus::OverrideToggle)
    );
    let demo_prefix = if focus_demo { ">" } else { " " };
    let ov_prefix = if focus_override { ">" } else { " " };
    let demo = if state.mapping_demo_mode { "x" } else { " " };
    let ov = if state.mapping_override { "x" } else { " " };

    let mut top_lines: Vec<Line> = Vec::new();
    top_lines.push(Line::from(format!(
        "We found {} fields in your source export.{}",
        state.source_fields.len(),
        scan_suffix
    )));
    top_lines.push(Line::from(format!(
        "{} [{}] Override: Allow a source field to map to multiple targets",
        ov_prefix, ov
    )));
    top_lines.push(Line::from(format!(
        "{} [{}] Demo mode: Use sample source headers (no database connection)",
        demo_prefix, demo
    )));
    if let Some(err) = state.mapping_scan_error.as_ref() {
        top_lines.push(Line::from(format!("Error: {}", err)));
    }
    if !required_unmapped.is_empty() {
        top_lines.push(Line::from(format!(
            "Required fields not mapped: {}",
            required_unmapped.join(", ")
        )));
    }
    top_lines.push(Line::from(
        "Select a source field, then select a target field. (U = Unassign, O = Override, / = Search)",
    ));

    let top = Paragraph::new(Text::from(top_lines)).wrap(Wrap { trim: false });
    f.render_widget(top, rows[0]);

    // Middle: two panes
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(rows[1]);

    // Source pane
    let source_block = Block::default()
        .borders(Borders::ALL)
        .title("Source Fields");
    f.render_widget(source_block, cols[0]);
    let source_inner = cols[0].inner(&ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    let source_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)].as_ref())
        .split(source_inner);

    let src_search_prefix = if matches!(
        state.focus,
        FocusTarget::Mapping(MappingFocus::SourceSearch)
    ) {
        ">"
    } else {
        " "
    };
    let src_search = Paragraph::new(Text::from(Line::from(format!(
        "{} Search source fields… {}",
        src_search_prefix, state.source_search.value
    ))))
    .wrap(Wrap { trim: false });
    f.render_widget(src_search, source_rows[0]);

    let src_q = state.source_search.value.trim().to_ascii_lowercase();
    let filtered_sources: Vec<&SourceField> = state
        .source_fields
        .iter()
        .filter(|s| src_q.is_empty() || s.display_name.to_ascii_lowercase().contains(&src_q))
        .collect();
    let src_sel = if filtered_sources.is_empty() {
        0
    } else {
        state
            .source_list_index
            .min(filtered_sources.len().saturating_sub(1))
    };
    let src_list_h = source_rows[1].height as usize;
    let src_start = src_sel.saturating_sub(src_list_h / 2);
    let src_end = (src_start + src_list_h).min(filtered_sources.len());
    let src_focus_list = matches!(state.focus, FocusTarget::Mapping(MappingFocus::SourceList));

    let mut src_lines: Vec<Line> = Vec::new();
    if filtered_sources.is_empty() {
        src_lines.push(Line::from("(no matches)"));
    } else {
        for (i, s) in filtered_sources
            .iter()
            .enumerate()
            .take(src_end)
            .skip(src_start)
        {
            let mapped = state
                .source_to_targets
                .get(&s.id)
                .map(|v| !v.is_empty())
                .unwrap_or(false);
            let prefix = if mapped { "* " } else { "  " };
            let selected = i == src_sel;
            let style = if selected && src_focus_list {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            src_lines.push(Line::from(ratatui::text::Span::styled(
                format!("{}{}", prefix, s.display_name),
                style,
            )));
        }
    }
    let src_list = Paragraph::new(Text::from(src_lines)).wrap(Wrap { trim: false });
    f.render_widget(src_list, source_rows[1]);

    // Target pane
    let target_block = Block::default()
        .borders(Borders::ALL)
        .title("Target Fields");
    f.render_widget(target_block, cols[1]);
    let target_inner = cols[1].inner(&ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    let target_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)].as_ref())
        .split(target_inner);

    let tgt_search_prefix = if matches!(
        state.focus,
        FocusTarget::Mapping(MappingFocus::TargetSearch)
    ) {
        ">"
    } else {
        " "
    };
    let tgt_search = Paragraph::new(Text::from(Line::from(format!(
        "{} Search target fields… {}",
        tgt_search_prefix, state.target_search.value
    ))))
    .wrap(Wrap { trim: false });
    f.render_widget(tgt_search, target_rows[0]);

    let tgt_q = state.target_search.value.trim().to_ascii_lowercase();
    let filtered_targets: Vec<&TargetField> = state
        .target_fields
        .iter()
        .filter(|t| tgt_q.is_empty() || t.name.to_ascii_lowercase().contains(&tgt_q))
        .collect();
    let tgt_sel = if filtered_targets.is_empty() {
        0
    } else {
        state
            .target_list_index
            .min(filtered_targets.len().saturating_sub(1))
    };
    let tgt_list_h = target_rows[1].height as usize;
    let tgt_start = tgt_sel.saturating_sub(tgt_list_h / 2);
    let tgt_end = (tgt_start + tgt_list_h).min(filtered_targets.len());
    let tgt_focus_list = matches!(state.focus, FocusTarget::Mapping(MappingFocus::TargetList));

    let selected_source_id = state
        .selected_source_id
        .clone()
        .or_else(|| filtered_sources.get(src_sel).map(|s| s.id.clone()));

    let mut tgt_lines: Vec<Line> = Vec::new();
    if filtered_targets.is_empty() {
        tgt_lines.push(Line::from("(no matches)"));
    } else {
        for (i, t) in filtered_targets
            .iter()
            .enumerate()
            .take(tgt_end)
            .skip(tgt_start)
        {
            let mapped_source = state.target_to_source.get(&t.id).cloned();
            let mapped = mapped_source.is_some();
            let prefix = if mapped { "* " } else { "  " };
            let selected = i == tgt_sel;
            let mut style = Style::default();
            if selected && tgt_focus_list {
                style = style.add_modifier(Modifier::REVERSED);
            } else if mapped_source
                .as_deref()
                .zip(selected_source_id.as_deref())
                .map(|(a, b)| a == b)
                .unwrap_or(false)
            {
                style = style.add_modifier(Modifier::BOLD);
            }

            let mut line = format!("{}{}", prefix, t.name);
            if t.required {
                line.push_str(" (required)");
            }
            if let Some(ms) = mapped_source {
                line.push_str(&format!(
                    " — mapped to {}",
                    mapping_source_display(state, &ms)
                ));
            }
            tgt_lines.push(Line::from(ratatui::text::Span::styled(line, style)));
        }
    }
    let tgt_list = Paragraph::new(Text::from(tgt_lines)).wrap(Wrap { trim: false });
    f.render_widget(tgt_list, target_rows[1]);

    // Bottom preview strip (always visible)
    let src_name = selected_source_id
        .as_deref()
        .map(|id| mapping_source_display(state, id))
        .unwrap_or_default();
    let targets = selected_source_id
        .as_deref()
        .and_then(|id| state.source_to_targets.get(id))
        .cloned()
        .unwrap_or_default();
    let target_names = targets
        .iter()
        .map(|t| mapping_target_name(state, t))
        .collect::<Vec<_>>()
        .join(", ");
    let preview_lines = vec![
        Line::from(format!("Source: {}", src_name)),
        Line::from("  ↓"),
        Line::from(format!("Target(s): {}", target_names)),
        Line::from(format!(
            "Mapped: {} / Target fields: {} — Unassigned source fields: {}",
            mapped_count,
            state.target_fields.len(),
            unassigned_sources
        )),
    ];
    let preview = Paragraph::new(Text::from(preview_lines)).wrap(Wrap { trim: false });
    f.render_widget(preview, rows[2]);
}

fn button_text(label: &str, focused: bool, enabled: bool) -> ratatui::text::Span<'static> {
    let mut style = Style::default();
    if !enabled {
        style = style.fg(Color::DarkGray);
    }
    if focused && enabled {
        style = style.add_modifier(Modifier::REVERSED);
    }
    ratatui::text::Span::styled(format!("[ {} ]", label), style)
}

fn draw_cancel_modal(f: &mut ratatui::Frame<'_>, window_area: Rect, state: &WizardState) {
    let modal_w = 56u16.min(window_area.width.saturating_sub(4)).max(40);
    let modal_h = 7u16;
    let x = window_area.x + (window_area.width.saturating_sub(modal_w)) / 2;
    let y = window_area.y + (window_area.height.saturating_sub(modal_h)) / 2;
    let area = Rect {
        x,
        y,
        width: modal_w,
        height: modal_h,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Cancel Setup?");
    let body = Paragraph::new(Text::from(vec![
        Line::from("If you cancel now, the installation may be incomplete."),
        Line::from(""),
        Line::from(""),
    ]))
    .block(block)
    .wrap(Wrap { trim: false });
    f.render_widget(body, area);

    // Buttons: [Yes, cancel] [No] (primary on right)
    let buttons_area = Rect {
        x: area.x + 1,
        y: area.y + area.height - 2,
        width: area.width - 2,
        height: 1,
    };

    let yes_focused = focused_button(state) == ButtonFocus::Cancel;
    let no_focused = focused_button(state) == ButtonFocus::Next;
    let yes = ratatui::text::Span::styled(
        "[ Yes, cancel ]",
        if yes_focused {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        },
    );
    let no = ratatui::text::Span::styled(
        "[ No ]",
        if no_focused {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        },
    );

    let line = Line::from(vec![yes, ratatui::text::Span::raw(" "), no]);
    let p = Paragraph::new(Text::from(line)).alignment(Alignment::Right);
    f.render_widget(p, buttons_area);
}

fn draw_message_modal(
    f: &mut ratatui::Frame<'_>,
    window_area: Rect,
    title: &str,
    body: &str,
    state: &WizardState,
) {
    let modal_w = 70u16.min(window_area.width.saturating_sub(4)).max(40);
    let modal_h = 10u16.min(window_area.height.saturating_sub(4)).max(7);
    let x = window_area.x + (window_area.width.saturating_sub(modal_w)) / 2;
    let y = window_area.y + (window_area.height.saturating_sub(modal_h)) / 2;
    let area = Rect {
        x,
        y,
        width: modal_w,
        height: modal_h,
    };

    let block = Block::default().borders(Borders::ALL).title(title);
    let p = Paragraph::new(Text::from(body.to_string()))
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);

    let buttons_area = Rect {
        x: area.x + 1,
        y: area.y + area.height - 2,
        width: area.width - 2,
        height: 1,
    };
    let ok = ratatui::text::Span::styled(
        "[ OK ]",
        if matches!(state.focus, FocusTarget::Button(ButtonFocus::Next)) {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        },
    );
    let line = Line::from(vec![ok]);
    let p = Paragraph::new(Text::from(line)).alignment(Alignment::Right);
    f.render_widget(p, buttons_area);
}

fn draw_confirm_mapping_modal(
    f: &mut ratatui::Frame<'_>,
    window_area: Rect,
    title: &str,
    body: &str,
    actions: &[MappingModalAction],
    selected: usize,
) {
    let modal_w = 76u16.min(window_area.width.saturating_sub(4)).max(44);
    let modal_h = 12u16.min(window_area.height.saturating_sub(4)).max(8);
    let x = window_area.x + (window_area.width.saturating_sub(modal_w)) / 2;
    let y = window_area.y + (window_area.height.saturating_sub(modal_h)) / 2;
    let area = Rect {
        x,
        y,
        width: modal_w,
        height: modal_h,
    };

    let block = Block::default().borders(Borders::ALL).title(title);
    f.render_widget(block, area);

    let inner = area.inner(&ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)].as_ref())
        .split(inner);

    let p = Paragraph::new(Text::from(body.to_string())).wrap(Wrap { trim: false });
    f.render_widget(p, rows[0]);

    let label = |a: MappingModalAction| match a {
        MappingModalAction::Add => "Add",
        MappingModalAction::Replace => "Replace",
        MappingModalAction::Cancel => "Cancel",
    };

    let mut spans: Vec<ratatui::text::Span> = Vec::new();
    for (i, a) in actions.iter().copied().enumerate() {
        if i > 0 {
            spans.push(ratatui::text::Span::raw(" "));
        }
        let s = ratatui::text::Span::styled(
            format!("[ {} ]", label(a)),
            if i == selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            },
        );
        spans.push(s);
    }

    let line = Line::from(spans);
    let p = Paragraph::new(Text::from(line)).alignment(Alignment::Right);
    f.render_widget(p, rows[1]);
}

fn draw_browse_folder_modal(
    f: &mut ratatui::Frame<'_>,
    window_area: Rect,
    current: &std::path::Path,
    entries: &[std::path::PathBuf],
    selected: usize,
) {
    let modal_w = 78u16.min(window_area.width.saturating_sub(4)).max(48);
    let modal_h = 16u16.min(window_area.height.saturating_sub(4)).max(10);
    let x = window_area.x + (window_area.width.saturating_sub(modal_w)) / 2;
    let y = window_area.y + (window_area.height.saturating_sub(modal_h)) / 2;
    let area = Rect {
        x,
        y,
        width: modal_w,
        height: modal_h,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Browse for Folder");
    f.render_widget(block, area);

    let inner = area.inner(&ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(2),
                Constraint::Min(0),
                Constraint::Length(2),
            ]
            .as_ref(),
        )
        .split(inner);

    let header = Paragraph::new(Text::from(vec![
        Line::from(format!("Current: {}", current.to_string_lossy())),
        Line::from("Enter=open  Backspace=up  S=select  Esc=cancel"),
    ]))
    .wrap(Wrap { trim: true });
    f.render_widget(header, rows[0]);

    // List
    let list_height = rows[1].height as usize;
    let start = selected.saturating_sub(list_height / 2);
    let end = (start + list_height).min(entries.len());

    let mut lines: Vec<Line> = Vec::new();
    if entries.is_empty() {
        lines.push(Line::from("(no subfolders)"));
    } else {
        for (i, p) in entries.iter().enumerate().take(end).skip(start) {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("<folder>");
            let focused = i == selected;
            let style = if focused {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            lines.push(Line::from(ratatui::text::Span::styled(
                name.to_string(),
                style,
            )));
        }
    }

    let list = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
    f.render_widget(list, rows[1]);

    let footer = Paragraph::new(Text::from(
        "Select will set the install path to the current folder.",
    ))
    .alignment(Alignment::Left)
    .wrap(Wrap { trim: true });
    f.render_widget(footer, rows[2]);
}
