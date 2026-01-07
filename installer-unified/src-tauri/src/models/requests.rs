// API request models
// Ported from C# contracts under `src/Cadalytix.Contracts/*`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =========================
// Setup
// =========================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AuthMode {
    External,
    LocalInClient,
    HostedByCadalytix,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallDataConfig {
    pub connection_string: String,
    pub source_object_name: String,
    #[serde(default = "default_source_name")]
    pub source_name: String,
}

fn default_source_name() -> String {
    "default".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAuthHeadersConfig {
    #[serde(default = "default_true")]
    pub trust_headers: bool,
    #[serde(default = "default_user_header")]
    pub user_header: String,
    #[serde(default = "default_roles_header")]
    pub roles_header: String,
}

fn default_true() -> bool {
    true
}

fn default_user_header() -> String {
    "x-cadalytix-user".to_string()
}

fn default_roles_header() -> String {
    "x-cadalytix-roles".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigDbConfig {
    pub connection_string: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupPlanRequest {
    pub auth_mode: AuthMode,
    pub call_data: CallDataConfig,
    pub external_auth_headers: Option<ExternalAuthHeadersConfig>,
    pub config_db: ConfigDbConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitRequest {
    pub config_db_connection_string: String,
    pub call_data_connection_string: String,
    pub source_object_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitRequest {
    pub config_db_connection_string: String,
    pub call_data_connection_string: String,
    pub auth_mode: String,
    pub source_name: String,
    pub source_object_name: String,
    #[serde(default)]
    pub mappings: HashMap<String, String>,
    #[serde(default)]
    pub auth_settings: HashMap<String, String>,
    pub dashboard_url: Option<String>,
    pub initial_ingest_start_date: Option<String>,
    pub initial_ingest_end_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupVerifyRequest {
    pub config_db_connection_string: Option<String>,
    pub expected_committed: Option<bool>,
    pub call_data_connection_string: Option<String>,
    pub source_object_name: Option<String>,
}

// =========================
// License
// =========================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseVerifyRequest {
    #[serde(default = "default_license_mode")]
    pub mode: String, // "online" or "offline"
    pub license_key: String,
    pub offline_bundle: Option<String>,
    pub ops_api_base_url: Option<String>,
}

fn default_license_mode() -> String {
    "online".to_string()
}

// =========================
// Preflight
// =========================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightHostRequestDto {
    pub strict_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightPermissionsRequestDto {
    pub config_db_connection_string: String,
    pub call_data_connection_string: String,
    #[serde(default = "default_true")]
    pub require_config_db_ddl: bool,
    #[serde(default = "default_true")]
    pub require_config_db_dml: bool,
    #[serde(default = "default_true")]
    pub require_call_data_read: bool,
    pub source_object_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightDataSourceRequestDto {
    pub call_data_connection_string: String,
    pub source_object_name: String,
    pub date_from_iso: Option<String>,
    pub date_to_iso: Option<String>,
    #[serde(default = "default_sample_limit")]
    pub sample_limit: i32,
    /// Explicitly labeled demo mode (no database required). Used to demonstrate schema mapping UX.
    #[serde(default)]
    pub demo_mode: bool,
}

fn default_sample_limit() -> i32 {
    10
}

// =========================
// Schema
// =========================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifySchemaRequest {
    #[serde(default = "default_engine_sqlserver")]
    pub engine: String,
    pub connection_string: Option<String>,
}

fn default_engine_sqlserver() -> String {
    "sqlserver".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyAllRequest {
    pub config_db_connection_string: Option<String>,
    pub call_data_connection_string: Option<String>,
    pub source_object_name: Option<String>,
    #[serde(default = "default_engine_sqlserver")]
    pub engine: String,
}

// =========================
// Wizard checkpoints
// =========================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointSaveRequest {
    pub step_name: String,
    pub state_json: String,
}
