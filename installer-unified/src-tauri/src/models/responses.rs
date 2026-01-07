// API response models
// Ported from C# contracts under `src/Cadalytix.Contracts/*`

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use super::requests::AuthMode;

// =========================
// Generic wrapper (matches frontend ApiResponse<T>)
// =========================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            message: None,
        }
    }

    #[allow(dead_code)]
    pub fn ok_with_message(data: T, message: impl Into<String>) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            message: Some(message.into()),
        }
    }

    pub fn fail(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
            message: None,
        }
    }
}

// =========================
// Setup
// =========================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupPlanResponse {
    pub auth_mode: AuthMode,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default)]
    pub instance_settings: HashMap<String, String>,
    #[serde(default)]
    pub migrations_to_apply: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupApplyResponse {
    pub success: bool,
    #[serde(default)]
    pub actions_performed: Vec<String>,
    #[serde(default)]
    pub migrations_applied: Vec<String>,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedMigrationDto {
    pub name: String,
    pub applied_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatusResponse {
    pub auth_mode: Option<AuthMode>,
    #[serde(default)]
    pub applied_migrations: Vec<AppliedMigrationDto>,
    pub source_object_name: Option<String>,
    pub source_name: Option<String>,
    pub schema_mapping_exists: bool,
    pub mapping_completeness: i32,
    pub is_configured: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupCompletionStatusResponse {
    pub is_complete: bool,
    pub dashboard_url: Option<String>,
    pub initial_ingest_start_date: Option<String>,
    pub initial_ingest_end_date: Option<String>,
    pub committed_utc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointResponse {
    pub step_name: String,
    pub state_json: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitResponse {
    pub success: bool,
    pub message: String,
    pub installation_id: Option<String>,
    pub already_initialized: bool,
    #[serde(default)]
    pub core_migrations_applied: Vec<String>,
    #[serde(default)]
    pub errors: Vec<String>,
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitResponse {
    pub success: bool,
    pub message: String,
    #[serde(default)]
    pub migrations_applied: Vec<String>,
    #[serde(default)]
    pub actions_performed: Vec<String>,
    #[serde(default)]
    pub errors: Vec<String>,
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupVerifyCheckResult {
    pub id: String,
    pub label: String,
    pub status: String, // "pass" | "fail"
    pub message: String,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupVerifyResponse {
    pub success: bool,
    #[serde(default)]
    pub checks: Vec<SetupVerifyCheckResult>,
    #[serde(default)]
    pub errors: Vec<String>,
}

// =========================
// Support bundle (PHI-safe)
// =========================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportBundleResponse {
    pub app_version: String,
    pub build_hash: String,
    pub generated_at_utc: DateTime<Utc>,
    #[serde(default)]
    pub config_fingerprints: HashMap<String, String>,
    #[serde(default)]
    pub applied_migrations: Vec<AppliedMigrationDto>,
    #[serde(default)]
    pub environment_info: HashMap<String, Value>,
    #[serde(default)]
    pub schema_column_names: Vec<String>,
    pub license_summary: Option<LicenseSummaryDto>,
    #[serde(default)]
    pub recent_events: Vec<SetupEventDto>,
    pub phi_statement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseSummaryDto {
    pub mode: String,
    pub status: String,
    pub expires_at_utc: DateTime<Utc>,
    pub last_verified_at_utc: DateTime<Utc>,
    #[serde(default)]
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupEventDto {
    pub event_type: String,
    pub description: String,
    pub actor: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

// =========================
// Schema
// =========================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifySchemaResponse {
    pub is_valid: bool,
    pub summary: String,
    pub total_issues: i32,
    #[serde(default)]
    pub missing_schemas: Vec<String>,
    #[serde(default)]
    pub missing_tables: Vec<String>,
    #[serde(default)]
    pub missing_columns: Vec<String>,
    #[serde(default)]
    pub missing_indexes: Vec<String>,
    #[serde(default)]
    pub type_mismatches: Vec<String>,
    #[serde(default)]
    pub nullability_mismatches: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyAllResponse {
    pub success: bool,
    pub summary: String,
    #[serde(default)]
    pub checks: Vec<SetupVerifyCheckResult>,
    pub schema_verification: Option<VerifySchemaResponse>,
    #[serde(default)]
    pub errors: Vec<String>,
}

// =========================
// License
// =========================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseEntitlementDto {
    pub license_mode: String,
    pub expires_at_utc: Option<DateTime<Utc>>,
    pub grace_until_utc: Option<DateTime<Utc>>,
    #[serde(default)]
    pub features: Vec<String>,
    pub client_id: Option<String>,
    pub last_verified_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseVerifyResponse {
    pub success: bool,
    pub message: String,
    pub entitlement: Option<LicenseEntitlementDto>,
    pub correlation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseStatusResponse {
    pub is_active: bool,
    pub entitlement: Option<LicenseEntitlementDto>,
    pub message: String,
}

// =========================
// Preflight
// =========================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightCheckDto {
    pub name: String,
    pub status: String, // Pass | Warn | Fail
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightHostResponseDto {
    pub machine_name: String,
    pub os_description: String,
    pub is_windows: bool,
    pub is_windows_server: bool,
    pub is_domain_joined: bool,
    pub is_iis_hosting: bool,
    pub is_container: bool,
    #[serde(default)]
    pub checks: Vec<PreflightCheckDto>,
    pub overall_status: String, // Pass | Warn | Fail
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightPermissionsResponseDto {
    #[serde(default)]
    pub checks: Vec<PreflightCheckDto>,
    pub overall_status: String, // Pass | Fail
    pub recommended_remediation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredColumnDto {
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleStatsDto {
    pub sample_count: i32,
    pub min_call_received_at: Option<String>,
    pub max_call_received_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightDataSourceResponseDto {
    #[serde(default)]
    pub checks: Vec<PreflightCheckDto>,
    pub overall_status: String, // Pass | Fail
    #[serde(default)]
    pub discovered_columns: Vec<DiscoveredColumnDto>,
    pub sample_stats: SampleStatsDto,
}
