// Setup API endpoints
// Ported from C# InstallerSetupEndpoints.cs (installer-host plumbing/orchestration)

use crate::database::connection::DatabaseConnection;
use crate::database::migrations::MigrationRunner;
use crate::database::platform_db::PlatformDbAdapter;
use crate::database::schema_mapping;
use crate::database::schema_verifier::SchemaVerifier;
use crate::models::requests::{
    AuthMode, CheckpointSaveRequest, CommitRequest, InitRequest, SetupPlanRequest,
    SetupVerifyRequest,
};
use crate::models::responses::{
    ApiResponse, AppliedMigrationDto, CheckpointResponse, CommitResponse, InitResponse,
    LicenseSummaryDto, SetupApplyResponse, SetupCompletionStatusResponse, SetupEventDto,
    SetupPlanResponse, SetupStatusResponse, SetupVerifyCheckResult, SetupVerifyResponse,
    SupportBundleResponse,
};
use crate::models::state::AppState;
use crate::security::secret_protector::SecretProtector;
use crate::utils::logging::mask_connection_string;
use crate::utils::path_resolver::resolve_deployment_folder;
use crate::utils::validation::{validate_and_quote_sql_server_object, validate_connection_string};

use futures::TryStreamExt;
use log::{info, warn};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tauri::async_runtime;
use tauri::State;
use tokio::time::{timeout, Duration};
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tokio_retry::RetryIf;
use uuid::Uuid;

// =========================
// Tauri command handlers
// =========================

#[tauri::command]
pub fn init_setup(
    app_state: State<'_, AppState>,
    secrets: State<'_, Arc<SecretProtector>>,
    payload: Option<InitRequest>,
) -> Result<ApiResponse<InitResponse>, String> {
    async_runtime::block_on(async move {
        let started = Instant::now();
        let correlation_id = Uuid::new_v4().simple().to_string();

        info!(
            "[PHASE: setup] [STEP: init] init_setup entered (correlation_id={})",
            correlation_id
        );

        let Some(req) = payload else {
            return Ok(ApiResponse::fail("Invalid request: body is required"));
        };

        if let Err(e) = validate_connection_string(&req.config_db_connection_string) {
            return Ok(ApiResponse::fail(format!(
                "ConfigDbConnectionString is invalid: {}",
                e
            )));
        }

        let masked = mask_connection_string(&req.config_db_connection_string);
        let engine = guess_engine(&req.config_db_connection_string);

        info!(
            "[PHASE: setup] [STEP: init] Connecting to config DB (engine={}, masked_conn_str={})",
            engine, masked
        );

        let conn = match connect_with_retry(&engine, &req.config_db_connection_string).await {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    "[PHASE: setup] [STEP: init] Failed to connect to config DB: {} (masked_conn_str={})",
                    e, masked
                );
                return Ok(ApiResponse::fail(
                    "Unable to connect to config database. Verify connection string and network access.",
                ));
            }
        };

        let engine_version = match detect_engine_version(&engine, &conn).await {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    "[PHASE: setup] [STEP: init] Failed to detect engine version: {}",
                    e
                );
                return Ok(ApiResponse::fail(
                    "Unable to detect database engine version.",
                ));
            }
        };

        // Persist in-memory state so follow-up calls (status/schema/etc.) can omit secrets.
        app_state
            .set_config_db(
                engine.clone(),
                engine_version.clone(),
                req.config_db_connection_string.clone(),
            )
            .await;

        let (manifest_path, migrations_path) = match resolve_migrations_paths() {
            Ok(p) => p,
            Err(e) => {
                return Ok(ApiResponse::fail(format!(
                    "Failed to resolve migrations folder: {}",
                    e
                )))
            }
        };

        let runner = match MigrationRunner::new(
            conn.clone(),
            manifest_path,
            migrations_path,
            engine.clone(),
            engine_version.clone(),
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                return Ok(ApiResponse::fail(format!(
                    "Failed to initialize migration runner: {}",
                    e
                )))
            }
        };

        let manifest = match runner.load_manifest().await {
            Ok(m) => m,
            Err(e) => {
                return Ok(ApiResponse::fail(format!(
                    "Failed to load migration manifest: {}",
                    e
                )))
            }
        };

        // Apply core migrations only (mirrors C# PostInit behavior).
        let mut applied_core: Vec<String> = Vec::new();
        for m in &manifest.migrations {
            if !is_core_migration(&m.name) {
                continue;
            }
            match runner.apply_migration(m).await {
                Ok(()) => applied_core.push(m.name.clone()),
                Err(e) => {
                    warn!(
                        "[PHASE: setup] [STEP: init] Core migration failed: {} (migration={})",
                        e, m.name
                    );
                    return Ok(ApiResponse::ok(InitResponse {
                        success: false,
                        message: "Core migration failed".to_string(),
                        installation_id: None,
                        already_initialized: false,
                        core_migrations_applied: applied_core,
                        errors: vec![format!("Failed to apply migration {}: {}", m.name, e)],
                        correlation_id: Some(correlation_id),
                    }));
                }
            }
        }

        let platform_db = PlatformDbAdapter::new(conn.clone(), Arc::clone(&secrets));

        // Idempotency
        let existing_install_id = platform_db
            .get_setting("Setup:InstallId")
            .await
            .ok()
            .flatten();
        if let Some(id) = existing_install_id.clone().filter(|s| !s.trim().is_empty()) {
            info!(
                "[PHASE: setup] [STEP: init] init_setup idempotent (installation_id={}, duration_ms={})",
                id,
                started.elapsed().as_millis()
            );
            return Ok(ApiResponse::ok(InitResponse {
                success: true,
                message: "Platform already initialized".to_string(),
                installation_id: Some(id),
                already_initialized: true,
                core_migrations_applied: applied_core,
                errors: vec![],
                correlation_id: Some(correlation_id),
            }));
        }

        let installation_id = Uuid::new_v4().simple().to_string();
        let mut settings = HashMap::new();
        settings.insert("Setup:InstallId".to_string(), installation_id.clone());
        settings.insert("Setup:Committed".to_string(), "false".to_string());

        if let Err(e) = platform_db.set_settings(&settings).await {
            warn!(
                "[PHASE: setup] [STEP: init] Failed to persist setup settings: {}",
                e
            );
            return Ok(ApiResponse::ok(InitResponse {
                success: false,
                message: "Failed to persist setup settings".to_string(),
                installation_id: None,
                already_initialized: false,
                core_migrations_applied: applied_core,
                errors: vec![e.to_string()],
                correlation_id: Some(correlation_id),
            }));
        }

        let _ = platform_db
            .log_setup_event(
                "setup.init.success",
                "Platform initialization completed",
                Some("installer"),
                Some(
                    &serde_json::json!({
                        "correlationId": correlation_id,
                        "installationId": installation_id,
                        "coreMigrationsApplied": applied_core.len()
                    })
                    .to_string(),
                ),
            )
            .await;

        info!(
            "[PHASE: setup] [STEP: init] init_setup completed (duration_ms={})",
            started.elapsed().as_millis()
        );

        Ok(ApiResponse::ok(InitResponse {
            success: true,
            message: "Platform initialized successfully".to_string(),
            installation_id: Some(installation_id),
            already_initialized: false,
            core_migrations_applied: applied_core,
            errors: vec![],
            correlation_id: Some(correlation_id),
        }))
    })
}

#[tauri::command]
pub fn plan_setup(
    app_state: State<'_, AppState>,
    payload: Option<SetupPlanRequest>,
) -> Result<ApiResponse<SetupPlanResponse>, String> {
    async_runtime::block_on(async move {
        info!("[PHASE: setup] [STEP: plan] plan_setup requested");
        let Some(req) = payload else {
            return Ok(ApiResponse::fail("Invalid request: body is required"));
        };

        if let Err(msgs) = validate_setup_plan_request(&req) {
            return Ok(ApiResponse::fail(format!(
                "Validation failed: {}",
                msgs.join("; ")
            )));
        }

        let engine = guess_engine(&req.config_db.connection_string);
        let conn = match connect_with_retry(&engine, &req.config_db.connection_string).await {
            Ok(c) => c,
            Err(_) => {
                return Ok(ApiResponse::fail(
                    "Unable to connect to config database. Verify connection string and network access.",
                ))
            }
        };
        let engine_version = match detect_engine_version(&engine, &conn).await {
            Ok(v) => v,
            Err(_) => {
                return Ok(ApiResponse::fail(
                    "Unable to detect database engine version.",
                ))
            }
        };

        app_state
            .set_config_db(
                engine.clone(),
                engine_version.clone(),
                req.config_db.connection_string.clone(),
            )
            .await;

        let (manifest_path, migrations_path) = match resolve_migrations_paths() {
            Ok(p) => p,
            Err(e) => {
                return Ok(ApiResponse::fail(format!(
                    "Failed to resolve migrations folder: {}",
                    e
                )))
            }
        };

        let runner = match MigrationRunner::new(
            conn.clone(),
            manifest_path,
            migrations_path,
            engine.clone(),
            engine_version.clone(),
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                return Ok(ApiResponse::fail(format!(
                    "Failed to initialize migration runner: {}",
                    e
                )))
            }
        };

        let manifest = match runner.load_manifest().await {
            Ok(m) => m,
            Err(e) => {
                return Ok(ApiResponse::fail(format!(
                    "Failed to load migration manifest: {}",
                    e
                )))
            }
        };

        let applied = match runner.get_applied_migration_names().await {
            Ok(a) => a,
            Err(e) => {
                return Ok(ApiResponse::fail(format!(
                    "Failed to query applied migrations: {}",
                    e
                )))
            }
        };

        let pending: Vec<String> = manifest
            .migrations
            .iter()
            .filter(|m| !applied.contains(&m.name))
            .map(|m| m.name.clone())
            .collect();

        let instance_settings = build_instance_settings(&req);
        let mut actions: Vec<String> = Vec::new();
        if !pending.is_empty() {
            actions.push(format!("Apply {} pending migration(s)", pending.len()));
        } else {
            actions.push("No pending migrations to apply".to_string());
        }
        actions.push(format!(
            "Write {} instance setting(s)",
            instance_settings.len()
        ));
        actions.push("Verify connectivity to call data source".to_string());
        actions.push("Verify schema mapping exists for source".to_string());

        let mut warnings: Vec<String> = Vec::new();
        if !matches!(req.auth_mode, AuthMode::External) {
            warnings.push(format!(
                "Auth mode '{:?}' is not yet implemented. Only 'External' is currently supported.",
                req.auth_mode
            ));
        }

        Ok(ApiResponse::ok(SetupPlanResponse {
            auth_mode: req.auth_mode.clone(),
            actions,
            instance_settings,
            migrations_to_apply: pending,
            warnings,
        }))
    })
}

#[tauri::command]
pub fn apply_setup(
    app_state: State<'_, AppState>,
    secrets: State<'_, Arc<SecretProtector>>,
    payload: Option<SetupPlanRequest>,
) -> Result<ApiResponse<SetupApplyResponse>, String> {
    async_runtime::block_on(async move {
        info!("[PHASE: setup] [STEP: apply] apply_setup requested");
        let Some(req) = payload else {
            return Ok(ApiResponse::fail("Invalid request: body is required"));
        };

        let mut response = SetupApplyResponse {
            success: false,
            actions_performed: vec![],
            migrations_applied: vec![],
            errors: vec![],
            warnings: vec![],
        };

        if let Err(msgs) = validate_setup_plan_request(&req) {
            response.errors.extend(msgs);
            return Ok(ApiResponse::ok(response));
        }

        if !matches!(req.auth_mode, AuthMode::External) {
            response.errors.push(format!(
                "Auth mode '{:?}' is not yet implemented. Only 'External' is currently supported.",
                req.auth_mode
            ));
            return Ok(ApiResponse::ok(response));
        }

        let engine = guess_engine(&req.config_db.connection_string);
        let conn = match connect_with_retry(&engine, &req.config_db.connection_string).await {
            Ok(c) => c,
            Err(_) => {
                response
                    .errors
                    .push("Unable to connect to config database.".to_string());
                return Ok(ApiResponse::ok(response));
            }
        };
        let engine_version = match detect_engine_version(&engine, &conn).await {
            Ok(v) => v,
            Err(_) => {
                response
                    .errors
                    .push("Unable to detect database engine version.".to_string());
                return Ok(ApiResponse::ok(response));
            }
        };

        app_state
            .set_config_db(
                engine.clone(),
                engine_version.clone(),
                req.config_db.connection_string.clone(),
            )
            .await;

        let (manifest_path, migrations_path) = match resolve_migrations_paths() {
            Ok(p) => p,
            Err(e) => {
                response
                    .errors
                    .push(format!("Failed to resolve migrations folder: {}", e));
                return Ok(ApiResponse::ok(response));
            }
        };

        let runner = match MigrationRunner::new(
            conn.clone(),
            manifest_path,
            migrations_path,
            engine.clone(),
            engine_version.clone(),
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                response
                    .errors
                    .push(format!("Failed to initialize migration runner: {}", e));
                return Ok(ApiResponse::ok(response));
            }
        };

        match runner.apply_all_pending().await {
            Ok(applied) => {
                response.migrations_applied = applied.clone();
                if !applied.is_empty() {
                    response.actions_performed.push(format!(
                        "Applied {} migration(s): {}",
                        applied.len(),
                        applied.join(", ")
                    ));
                } else {
                    response
                        .actions_performed
                        .push("No pending migrations to apply".to_string());
                }
            }
            Err(e) => {
                response
                    .errors
                    .push(format!("Failed to apply migrations: {}", e));
                return Ok(ApiResponse::ok(response));
            }
        }

        // Verify call data connectivity (SQL Server only for now)
        if guess_engine(&req.call_data.connection_string) == "sqlserver" {
            match DatabaseConnection::sql_server(&req.call_data.connection_string).await {
                Ok(call_conn) => {
                    let Some(client_arc) = call_conn.as_sql_server() else {
                        response
                            .warnings
                            .push("Call data connection is not SQL Server.".to_string());
                        return Ok(ApiResponse::ok(response));
                    };
                    let mut client = client_arc.lock().await;
                    match validate_and_quote_sql_server_object(&req.call_data.source_object_name) {
                        Ok(quoted) => {
                            let sql = format!("SELECT TOP 1 * FROM {}", quoted);
                            if client.simple_query(sql).await.is_ok() {
                                response.actions_performed.push(format!(
                                    "Verified connectivity to call data source: {}",
                                    req.call_data.source_object_name
                                ));
                            } else {
                                response.errors.push(format!(
                                    "Failed to connect to call data source '{}'",
                                    req.call_data.source_object_name
                                ));
                                return Ok(ApiResponse::ok(response));
                            }
                        }
                        Err(e) => {
                            response.errors.push(format!(
                                "Invalid source object name '{}': {}",
                                req.call_data.source_object_name, e
                            ));
                            return Ok(ApiResponse::ok(response));
                        }
                    }
                }
                Err(_) => {
                    response
                        .errors
                        .push("Failed to connect to call data database.".to_string());
                    return Ok(ApiResponse::ok(response));
                }
            }
        } else {
            response.warnings.push(
                "Call data connectivity check is not implemented for this engine yet.".to_string(),
            );
        }

        // Verify schema mapping exists (best-effort)
        match schema_mapping::get_mappings(&conn, &req.call_data.source_name).await {
            Ok(map) => {
                if map.is_empty() {
                    response.warnings.push(format!(
                        "Schema mapping for source '{}' does not exist. You will need to create it manually.",
                        req.call_data.source_name
                    ));
                } else {
                    response.actions_performed.push(format!(
                        "Verified schema mapping exists for source: {}",
                        req.call_data.source_name
                    ));
                }
            }
            Err(e) => response
                .warnings
                .push(format!("Could not verify schema mapping: {}", e)),
        }

        // Persist instance settings
        let settings = build_instance_settings(&req);
        let platform_db = PlatformDbAdapter::new(conn.clone(), Arc::clone(&secrets));
        if let Err(e) = platform_db.set_settings(&settings).await {
            response
                .errors
                .push(format!("Failed to save instance settings: {}", e));
            return Ok(ApiResponse::ok(response));
        }
        response
            .actions_performed
            .push(format!("Wrote {} instance setting(s)", settings.len()));

        response.success = true;
        Ok(ApiResponse::ok(response))
    })
}

#[tauri::command]
pub fn commit_setup(
    app_state: State<'_, AppState>,
    secrets: State<'_, Arc<SecretProtector>>,
    payload: Option<CommitRequest>,
) -> Result<ApiResponse<CommitResponse>, String> {
    async_runtime::block_on(async move {
        info!("[PHASE: setup] [STEP: commit] commit_setup requested");
        let Some(req) = payload else {
            return Ok(ApiResponse::fail("Invalid request: body is required"));
        };

        let correlation_id = Uuid::new_v4().simple().to_string();

        // Basic validation (fail-closed)
        let mut errors: Vec<String> = Vec::new();
        if validate_connection_string(&req.config_db_connection_string).is_err() {
            errors.push("ConfigDbConnectionString is required".to_string());
        }
        if validate_connection_string(&req.call_data_connection_string).is_err() {
            errors.push("CallDataConnectionString is required".to_string());
        }
        if req.source_object_name.trim().is_empty() {
            errors.push("SourceObjectName is required".to_string());
        }
        if req.auth_mode.trim().is_empty() {
            errors.push("AuthMode is required".to_string());
        }
        if req.mappings.is_empty() {
            errors.push("At least one mapping is required".to_string());
        }
        if !errors.is_empty() {
            return Ok(ApiResponse::ok(CommitResponse {
                success: false,
                message: "Validation failed".to_string(),
                migrations_applied: vec![],
                actions_performed: vec![],
                errors,
                correlation_id: Some(correlation_id),
            }));
        }

        let engine = guess_engine(&req.config_db_connection_string);
        let conn = match connect_with_retry(&engine, &req.config_db_connection_string).await {
            Ok(c) => c,
            Err(_) => {
                return Ok(ApiResponse::ok(CommitResponse {
                    success: false,
                    message: "Unable to connect to config database.".to_string(),
                    migrations_applied: vec![],
                    actions_performed: vec![],
                    errors: vec!["Unable to connect to config database.".to_string()],
                    correlation_id: Some(correlation_id),
                }))
            }
        };
        let engine_version = match detect_engine_version(&engine, &conn).await {
            Ok(v) => v,
            Err(_) => {
                return Ok(ApiResponse::ok(CommitResponse {
                    success: false,
                    message: "Unable to detect database engine version.".to_string(),
                    migrations_applied: vec![],
                    actions_performed: vec![],
                    errors: vec!["Unable to detect database engine version.".to_string()],
                    correlation_id: Some(correlation_id),
                }))
            }
        };

        app_state
            .set_config_db(
                engine.clone(),
                engine_version.clone(),
                req.config_db_connection_string.clone(),
            )
            .await;

        let (manifest_path, migrations_path) = match resolve_migrations_paths() {
            Ok(p) => p,
            Err(e) => {
                return Ok(ApiResponse::ok(CommitResponse {
                    success: false,
                    message: "Failed to resolve migrations folder.".to_string(),
                    migrations_applied: vec![],
                    actions_performed: vec![],
                    errors: vec![e.to_string()],
                    correlation_id: Some(correlation_id),
                }))
            }
        };

        let runner = match MigrationRunner::new(
            conn.clone(),
            manifest_path,
            migrations_path,
            engine.clone(),
            engine_version.clone(),
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                return Ok(ApiResponse::ok(CommitResponse {
                    success: false,
                    message: "Failed to initialize migration runner.".to_string(),
                    migrations_applied: vec![],
                    actions_performed: vec![],
                    errors: vec![e.to_string()],
                    correlation_id: Some(correlation_id),
                }))
            }
        };

        let applied = match runner.apply_all_pending().await {
            Ok(v) => v,
            Err(e) => {
                return Ok(ApiResponse::ok(CommitResponse {
                    success: false,
                    message: "Commit failed while applying migrations.".to_string(),
                    migrations_applied: vec![],
                    actions_performed: vec![],
                    errors: vec![e.to_string()],
                    correlation_id: Some(correlation_id),
                }))
            }
        };

        let platform_db = PlatformDbAdapter::new(conn.clone(), Arc::clone(&secrets));

        let mut actions: Vec<String> = Vec::new();

        // Persist settings
        let mut settings: HashMap<String, String> = HashMap::new();
        settings.insert("Auth:Mode".to_string(), req.auth_mode.clone());
        settings.insert(
            "Data:CallData:SourceName".to_string(),
            req.source_name.clone(),
        );
        settings.insert(
            "Data:CallData:SourceObjectName".to_string(),
            req.source_object_name.clone(),
        );
        settings.insert("Setup:Committed".to_string(), "true".to_string());
        settings.insert(
            "Setup:CommittedUtc".to_string(),
            chrono::Utc::now().to_rfc3339(),
        );

        if let Some(url) = req.dashboard_url.as_ref().filter(|s| !s.trim().is_empty()) {
            settings.insert("Setup:DashboardUrl".to_string(), url.clone());
        }
        if let Some(s) = req
            .initial_ingest_start_date
            .as_ref()
            .filter(|s| !s.trim().is_empty())
        {
            settings.insert("Setup:InitialIngestStartDate".to_string(), s.clone());
        }
        if let Some(s) = req
            .initial_ingest_end_date
            .as_ref()
            .filter(|s| !s.trim().is_empty())
        {
            settings.insert("Setup:InitialIngestEndDate".to_string(), s.clone());
        }

        for (k, v) in &req.auth_settings {
            settings.insert(k.clone(), v.clone());
        }

        if let Err(e) = platform_db.set_settings(&settings).await {
            return Ok(ApiResponse::ok(CommitResponse {
                success: false,
                message: "Commit failed while saving settings.".to_string(),
                migrations_applied: applied,
                actions_performed: actions,
                errors: vec![e.to_string()],
                correlation_id: Some(correlation_id),
            }));
        }
        actions.push("Saved instance settings".to_string());

        // Persist schema mappings
        for (canonical, source) in &req.mappings {
            if let Err(e) =
                schema_mapping::upsert_mapping(&conn, &req.source_name, canonical, source).await
            {
                return Ok(ApiResponse::ok(CommitResponse {
                    success: false,
                    message: "Commit failed while saving schema mappings.".to_string(),
                    migrations_applied: applied,
                    actions_performed: actions,
                    errors: vec![e.to_string()],
                    correlation_id: Some(correlation_id),
                }));
            }
        }
        actions.push(format!("Saved {} schema mapping(s)", req.mappings.len()));

        let _ = platform_db.clear_checkpoints().await;
        actions.push("Cleared wizard checkpoints".to_string());

        let _ = platform_db
            .log_setup_event(
                "setup.commit.completed",
                "Setup commit completed successfully",
                Some("installer"),
                Some(
                    &serde_json::json!({
                        "correlationId": correlation_id,
                        "migrationsApplied": applied.len()
                    })
                    .to_string(),
                ),
            )
            .await;

        Ok(ApiResponse::ok(CommitResponse {
            success: true,
            message: "Setup committed successfully".to_string(),
            migrations_applied: applied,
            actions_performed: actions,
            errors: vec![],
            correlation_id: Some(correlation_id),
        }))
    })
}

#[tauri::command]
pub fn verify_setup(
    app_state: State<'_, AppState>,
    secrets: State<'_, Arc<SecretProtector>>,
    payload: Option<SetupVerifyRequest>,
) -> Result<ApiResponse<SetupVerifyResponse>, String> {
    async_runtime::block_on(async move {
        info!("[PHASE: setup] [STEP: verify] verify_setup requested");
        let req = payload.unwrap_or(SetupVerifyRequest {
            config_db_connection_string: None,
            expected_committed: None,
            call_data_connection_string: None,
            source_object_name: None,
        });

        let mut checks: Vec<SetupVerifyCheckResult> = Vec::new();
        let mut failures: Vec<String> = Vec::new();

        let config_conn_str = match req.config_db_connection_string.clone() {
            Some(s) if !s.trim().is_empty() => Some(s),
            _ => app_state.get_config_db().await.map(|(_, _, cs)| cs),
        };

        let Some(config_conn_str) = config_conn_str else {
            return Ok(ApiResponse::ok(SetupVerifyResponse {
                success: false,
                checks,
                errors: vec!["Config database connection string is not available.".to_string()],
            }));
        };

        let engine = guess_engine(&config_conn_str);
        let conn = match connect_with_retry(&engine, &config_conn_str).await {
            Ok(c) => c,
            Err(_) => {
                checks.push(SetupVerifyCheckResult {
                    id: "config_db_connectivity".to_string(),
                    label: "Config DB reachable".to_string(),
                    status: "fail".to_string(),
                    message: "Unable to connect to config database.".to_string(),
                    duration_ms: 0,
                });
                return Ok(ApiResponse::ok(SetupVerifyResponse {
                    success: false,
                    checks,
                    errors: vec!["Unable to connect to config database.".to_string()],
                }));
            }
        };

        let platform_db = PlatformDbAdapter::new(conn.clone(), Arc::clone(&secrets));
        let verifier = SchemaVerifier::new(conn.clone());

        checks.push(SetupVerifyCheckResult {
            id: "config_db_connectivity".to_string(),
            label: "Config DB reachable".to_string(),
            status: "pass".to_string(),
            message: "Config database connection succeeded.".to_string(),
            duration_ms: 0,
        });

        match verifier.verify_all_schemas().await {
            Ok(results) => {
                let ok = results.iter().all(|(_, r)| r.valid);
                if !ok {
                    failures.push("core_tables".to_string());
                }
                checks.push(SetupVerifyCheckResult {
                    id: "core_tables".to_string(),
                    label: "Core setup tables exist".to_string(),
                    status: if ok {
                        "pass".to_string()
                    } else {
                        "fail".to_string()
                    },
                    message: if ok {
                        "All core setup tables are present.".to_string()
                    } else {
                        "Core tables/columns are missing.".to_string()
                    },
                    duration_ms: 0,
                });
            }
            Err(e) => {
                failures.push("core_tables".to_string());
                checks.push(SetupVerifyCheckResult {
                    id: "core_tables".to_string(),
                    label: "Core setup tables exist".to_string(),
                    status: "fail".to_string(),
                    message: format!("Schema verification failed: {}", e),
                    duration_ms: 0,
                });
            }
        }

        let install_id = platform_db
            .get_setting("Setup:InstallId")
            .await
            .ok()
            .flatten();
        let ok_install = install_id
            .as_ref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        if !ok_install {
            failures.push("install_id".to_string());
        }
        checks.push(SetupVerifyCheckResult {
            id: "install_id".to_string(),
            label: "Setup InstallId exists".to_string(),
            status: if ok_install {
                "pass".to_string()
            } else {
                "fail".to_string()
            },
            message: if ok_install {
                "Setup InstallId is present.".to_string()
            } else {
                "Setup InstallId is not set.".to_string()
            },
            duration_ms: 0,
        });

        let committed_val = platform_db
            .get_setting("Setup:Committed")
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
        let committed = committed_val.eq_ignore_ascii_case("true");
        let ok_committed = match req.expected_committed {
            Some(expected) => committed == expected,
            None => true,
        };
        if !ok_committed {
            failures.push("committed_flag".to_string());
        }
        checks.push(SetupVerifyCheckResult {
            id: "committed_flag".to_string(),
            label: "Setup committed flag matches expected state".to_string(),
            status: if ok_committed {
                "pass".to_string()
            } else {
                "fail".to_string()
            },
            message: if ok_committed {
                format!(
                    "Setup:Committed is {}.",
                    committed.to_string().to_lowercase()
                )
            } else {
                format!(
                    "Setup:Committed is {} but expected {}.",
                    committed.to_string().to_lowercase(),
                    req.expected_committed.unwrap_or(false)
                )
            },
            duration_ms: 0,
        });

        let mut missing = Vec::new();
        if platform_db
            .get_setting("Auth:Mode")
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
            .trim()
            .is_empty()
        {
            missing.push("Auth:Mode");
        }
        if platform_db
            .get_setting("Data:CallData:SourceName")
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
            .trim()
            .is_empty()
        {
            missing.push("Data:CallData:SourceName");
        }
        if platform_db
            .get_setting("Data:CallData:SourceObjectName")
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
            .trim()
            .is_empty()
        {
            missing.push("Data:CallData:SourceObjectName");
        }
        let ok_worker = missing.is_empty();
        if !ok_worker {
            failures.push("worker_prereqs".to_string());
        }
        checks.push(SetupVerifyCheckResult {
            id: "worker_prereqs".to_string(),
            label: "Worker prerequisites configured".to_string(),
            status: if ok_worker {
                "pass".to_string()
            } else {
                "fail".to_string()
            },
            message: if ok_worker {
                "Required configuration values are present.".to_string()
            } else {
                format!("Missing configuration values: {}", missing.join(", "))
            },
            duration_ms: 0,
        });

        let success = failures.is_empty();
        let mut errors = Vec::new();
        if !success {
            errors.push("One or more verification checks failed.".to_string());
        }

        Ok(ApiResponse::ok(SetupVerifyResponse {
            success,
            checks,
            errors,
        }))
    })
}

#[tauri::command]
pub fn get_setup_status(
    app_state: State<'_, AppState>,
    secrets: State<'_, Arc<SecretProtector>>,
) -> Result<ApiResponse<SetupStatusResponse>, String> {
    async_runtime::block_on(async move {
        info!("[PHASE: setup] [STEP: status] get_setup_status requested");

        let Some((_engine, _engine_version, config_conn_str)) = app_state.get_config_db().await
        else {
            return Ok(ApiResponse::ok(SetupStatusResponse {
                auth_mode: None,
                applied_migrations: vec![],
                source_object_name: None,
                source_name: None,
                schema_mapping_exists: false,
                mapping_completeness: 0,
                is_configured: false,
                message: Some(
                    "Database not yet configured. Complete setup wizard to configure.".to_string(),
                ),
            }));
        };

        let engine = guess_engine(&config_conn_str);
        let conn = match connect_with_retry(&engine, &config_conn_str).await {
            Ok(c) => c,
            Err(_) => {
                return Ok(ApiResponse::fail(
                    "Failed to connect to config database. Please verify connection string and network access.",
                ))
            }
        };

        let platform_db = PlatformDbAdapter::new(conn.clone(), Arc::clone(&secrets));
        let settings = match platform_db.get_all_settings().await {
            Ok(s) => s,
            Err(e) => {
                return Ok(ApiResponse::fail(format!(
                    "Failed to get setup status: {}",
                    e
                )))
            }
        };

        let applied = platform_db
            .get_applied_migrations_brief()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(name, applied_at)| AppliedMigrationDto { name, applied_at })
            .collect::<Vec<_>>();

        let auth_mode = settings.get("Auth:Mode").and_then(|v| match v.as_str() {
            "External" => Some(AuthMode::External),
            "LocalInClient" => Some(AuthMode::LocalInClient),
            "HostedByCadalytix" => Some(AuthMode::HostedByCadalytix),
            _ => None,
        });

        let source_object_name = settings.get("Data:CallData:SourceObjectName").cloned();
        let source_name = settings.get("Data:CallData:SourceName").cloned();

        let (schema_mapping_exists, mapping_completeness) = if let Some(sn) =
            source_name.as_ref().filter(|s| !s.trim().is_empty())
        {
            match schema_mapping::get_mappings(&conn, sn).await {
                Ok(map) => {
                    let exists = !map.is_empty();
                    let completeness = map.values().filter(|v| !v.trim().is_empty()).count() as i32;
                    (exists, completeness)
                }
                Err(_) => (false, 0),
            }
        } else {
            (false, 0)
        };

        let is_configured = auth_mode.is_some()
            && source_object_name
                .as_ref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false)
            && source_name
                .as_ref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false)
            && schema_mapping_exists;

        Ok(ApiResponse::ok(SetupStatusResponse {
            auth_mode,
            applied_migrations: applied,
            source_object_name,
            source_name,
            schema_mapping_exists,
            mapping_completeness,
            is_configured,
            message: None,
        }))
    })
}

// =========================
// Additional setup endpoints (resume + diagnostics)
// =========================

#[tauri::command]
pub fn get_setup_completion_status(
    app_state: State<'_, AppState>,
    secrets: State<'_, Arc<SecretProtector>>,
) -> Result<ApiResponse<SetupCompletionStatusResponse>, String> {
    async_runtime::block_on(async move {
        info!("[PHASE: setup] [STEP: completion_status] get_setup_completion_status requested");

        let Some((engine, _engine_version, config_conn_str)) = app_state.get_config_db().await
        else {
            return Ok(ApiResponse::ok(SetupCompletionStatusResponse {
                is_complete: false,
                dashboard_url: None,
                initial_ingest_start_date: None,
                initial_ingest_end_date: None,
                committed_utc: None,
            }));
        };

        let conn = match connect_with_retry(&engine, &config_conn_str).await {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    "[PHASE: setup] [STEP: completion_status] DB unavailable; fail-closed (error={})",
                    e
                );
                return Ok(ApiResponse::ok(SetupCompletionStatusResponse {
                    is_complete: false,
                    dashboard_url: None,
                    initial_ingest_start_date: None,
                    initial_ingest_end_date: None,
                    committed_utc: None,
                }));
            }
        };

        let platform_db = PlatformDbAdapter::new(conn, Arc::clone(&secrets));

        // Fail-closed semantics: any error => not complete.
        let committed = platform_db
            .get_setting("Setup:Committed")
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
            .eq_ignore_ascii_case("true");

        if !committed {
            return Ok(ApiResponse::ok(SetupCompletionStatusResponse {
                is_complete: false,
                dashboard_url: None,
                initial_ingest_start_date: None,
                initial_ingest_end_date: None,
                committed_utc: None,
            }));
        }

        let dashboard_url = platform_db
            .get_setting("Setup:DashboardUrl")
            .await
            .ok()
            .flatten();
        let start = platform_db
            .get_setting("Setup:InitialIngestStartDate")
            .await
            .ok()
            .flatten();
        let end = platform_db
            .get_setting("Setup:InitialIngestEndDate")
            .await
            .ok()
            .flatten();
        let committed_utc = platform_db
            .get_setting("Setup:CommittedUtc")
            .await
            .ok()
            .flatten();

        Ok(ApiResponse::ok(SetupCompletionStatusResponse {
            is_complete: true,
            dashboard_url,
            initial_ingest_start_date: start,
            initial_ingest_end_date: end,
            committed_utc,
        }))
    })
}

#[tauri::command]
pub fn get_latest_checkpoint(
    app_state: State<'_, AppState>,
    secrets: State<'_, Arc<SecretProtector>>,
) -> Result<ApiResponse<CheckpointResponse>, String> {
    async_runtime::block_on(async move {
        info!("[PHASE: setup] [STEP: checkpoint_latest] get_latest_checkpoint requested");

        let Some((engine, _engine_version, config_conn_str)) = app_state.get_config_db().await
        else {
            return Ok(ApiResponse::fail("Database not configured."));
        };

        let conn = match connect_with_retry(&engine, &config_conn_str).await {
            Ok(c) => c,
            Err(_) => return Ok(ApiResponse::fail("Unable to connect to config database.")),
        };

        let platform_db = PlatformDbAdapter::new(conn, Arc::clone(&secrets));
        Ok(match platform_db.get_latest_checkpoint().await {
            Ok(Some((step_name, state_json, updated_at))) => ApiResponse::ok(CheckpointResponse {
                step_name,
                state_json,
                updated_at,
            }),
            Ok(None) => ApiResponse::fail("No checkpoint found."),
            Err(e) => ApiResponse::fail(format!("Failed to load checkpoint: {}", e)),
        })
    })
}

#[tauri::command]
pub fn save_checkpoint(
    app_state: State<'_, AppState>,
    secrets: State<'_, Arc<SecretProtector>>,
    payload: Option<CheckpointSaveRequest>,
) -> Result<ApiResponse<serde_json::Value>, String> {
    async_runtime::block_on(async move {
        info!("[PHASE: setup] [STEP: checkpoint_save] save_checkpoint requested");

        let Some(req) = payload else {
            return Ok(ApiResponse::fail("Invalid request: body is required"));
        };
        if req.step_name.trim().is_empty() {
            return Ok(ApiResponse::fail("StepName is required"));
        }
        if req.state_json.trim().is_empty() {
            return Ok(ApiResponse::fail("StateJson is required"));
        }

        let Some((engine, _engine_version, config_conn_str)) = app_state.get_config_db().await
        else {
            return Ok(ApiResponse::fail("Database not configured."));
        };

        let conn = match connect_with_retry(&engine, &config_conn_str).await {
            Ok(c) => c,
            Err(_) => return Ok(ApiResponse::fail("Unable to connect to config database.")),
        };

        let platform_db = PlatformDbAdapter::new(conn, Arc::clone(&secrets));
        Ok(
            match platform_db
                .save_checkpoint(&req.step_name, &req.state_json)
                .await
            {
                Ok(()) => ApiResponse::ok(serde_json::json!({ "saved": true })),
                Err(e) => ApiResponse::fail(format!("Failed to save checkpoint: {}", e)),
            },
        )
    })
}

#[tauri::command]
pub fn get_support_bundle(
    app_state: State<'_, AppState>,
    secrets: State<'_, Arc<SecretProtector>>,
) -> Result<ApiResponse<SupportBundleResponse>, String> {
    async_runtime::block_on(async move {
        info!("[PHASE: setup] [STEP: support_bundle] get_support_bundle requested");

        let Some((engine, _engine_version, config_conn_str)) = app_state.get_config_db().await
        else {
            return Ok(ApiResponse::fail("Database not configured."));
        };

        let conn = match connect_with_retry(&engine, &config_conn_str).await {
            Ok(c) => c,
            Err(_) => return Ok(ApiResponse::fail("Unable to connect to config database.")),
        };

        let platform_db = PlatformDbAdapter::new(conn, Arc::clone(&secrets));

        // Applied migrations (safe metadata)
        let applied = platform_db
            .get_applied_migrations_brief()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(name, applied_at)| AppliedMigrationDto { name, applied_at })
            .collect::<Vec<_>>();

        // Recent events (safe / no PHI by contract)
        let recent_events = platform_db
            .get_setup_events(50)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(
                |(event_type, description, actor, occurred_at)| SetupEventDto {
                    event_type,
                    description,
                    actor,
                    occurred_at,
                },
            )
            .collect::<Vec<_>>();

        // Config fingerprints: keys only, all values redacted
        let mut config_fingerprints = HashMap::new();
        if let Ok(keys) = platform_db.get_setting_keys().await {
            for k in keys {
                config_fingerprints.insert(k, "***REDACTED***".to_string());
            }
        }

        // Environment info (safe subset)
        let mut environment_info: HashMap<String, serde_json::Value> = HashMap::new();
        environment_info.insert(
            "machineName".to_string(),
            serde_json::json!(std::env::var("COMPUTERNAME")
                .or_else(|_| std::env::var("HOSTNAME"))
                .unwrap_or_else(|_| "unknown".to_string())),
        );
        environment_info.insert("os".to_string(), serde_json::json!(std::env::consts::OS));
        environment_info.insert(
            "arch".to_string(),
            serde_json::json!(std::env::consts::ARCH),
        );
        environment_info.insert(
            "cpuCount".to_string(),
            serde_json::json!(std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)),
        );

        // License summary (never include secrets)
        let license_summary = match platform_db.get_license_state().await.ok().flatten() {
            Some(state) => {
                let mode = state
                    .get("mode")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let status = state
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let expires_at = state
                    .get("expiresAtUtc")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc));
                let last_verified = state
                    .get("lastVerifiedAtUtc")
                    .and_then(|v| v.as_str())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc));
                let features_json = state
                    .get("featuresJson")
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}");

                if let (Some(expires_at_utc), Some(last_verified_at_utc)) =
                    (expires_at, last_verified)
                {
                    let features = parse_feature_keys(features_json);
                    Some(LicenseSummaryDto {
                        mode,
                        status,
                        expires_at_utc,
                        last_verified_at_utc,
                        features,
                    })
                } else {
                    None
                }
            }
            None => None,
        };

        Ok(ApiResponse::ok(SupportBundleResponse {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            build_hash: option_env!("GIT_COMMIT").unwrap_or("dev").to_string(),
            generated_at_utc: chrono::Utc::now(),
            config_fingerprints,
            applied_migrations: applied,
            environment_info,
            schema_column_names: vec![],
            license_summary,
            recent_events,
            phi_statement: "This bundle contains NO patient health information (PHI), NO call records, NO addresses, and NO personally identifiable information.".to_string(),
        }))
    })
}

// =========================
// Helpers
// =========================

fn resolve_migrations_paths() -> anyhow::Result<(PathBuf, PathBuf)> {
    let deployment = resolve_deployment_folder()?;
    let migrations_path = deployment.join("installer").join("migrations");
    let manifest_path = migrations_path.join("manifest_versioned.json");
    Ok((manifest_path, migrations_path))
}

fn is_core_migration(name: &str) -> bool {
    name.contains("_001_")
        || name.contains("_002_")
        || name.contains("_007_")
        || name.contains("_008_")
        || name.contains("_009_")
        || name.contains("_010_")
        || name.contains("_011_")
}

fn guess_engine(conn_str: &str) -> String {
    let s = conn_str.to_ascii_lowercase();
    if s.starts_with("postgres://") || s.starts_with("postgresql://") || s.contains("host=") {
        "postgres".to_string()
    } else {
        "sqlserver".to_string()
    }
}

async fn connect_with_retry(engine: &str, conn_str: &str) -> anyhow::Result<DatabaseConnection> {
    let attempt = || async {
        let timed = match engine {
            "postgres" => {
                timeout(
                    Duration::from_secs(20),
                    DatabaseConnection::postgres(conn_str),
                )
                .await
            }
            _ => {
                timeout(
                    Duration::from_secs(20),
                    DatabaseConnection::sql_server(conn_str),
                )
                .await
            }
        };

        let inner = timed.map_err(|_| anyhow::anyhow!("Connection attempt timed out"))?;
        inner
    };

    let retry_strategy = ExponentialBackoff::from_millis(100)
        .factor(2)
        .max_delay(Duration::from_secs(2))
        .take(3)
        .map(jitter);

    RetryIf::spawn(retry_strategy, attempt, |e: &anyhow::Error| {
        let msg = e.to_string().to_ascii_lowercase();
        msg.contains("timed out")
            || msg.contains("timeout")
            || msg.contains("network")
            || msg.contains("connection")
            || msg.contains("i/o")
            || msg.contains("reset")
            || msg.contains("refused")
    })
    .await
}

async fn detect_engine_version(engine: &str, conn: &DatabaseConnection) -> anyhow::Result<String> {
    match engine {
        "postgres" => {
            let pool = conn
                .as_postgres()
                .ok_or_else(|| anyhow::anyhow!("Not a Postgres connection"))?;
            let v: String = sqlx::query_scalar("SHOW server_version")
                .fetch_one(pool)
                .await?;
            let major = v
                .split('.')
                .next()
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(17);
            Ok(format!("{}", major))
        }
        _ => {
            use tiberius::QueryItem;
            let client_arc = conn
                .as_sql_server()
                .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
            let mut client = client_arc.lock().await;
            let mut stream = client
                .simple_query("SELECT CAST(SERVERPROPERTY('ProductMajorVersion') AS INT)")
                .await?;
            while let Some(item) = stream.try_next().await? {
                if let QueryItem::Row(row) = item {
                    let major = row.get::<i32, _>(0).unwrap_or(16);
                    let year = match major {
                        16 => "2022",
                        15 => "2019",
                        14 => "2017",
                        13 => "2016",
                        12 => "2014",
                        _ => "2022",
                    };
                    return Ok(year.to_string());
                }
            }
            Ok("2022".to_string())
        }
    }
}

fn validate_setup_plan_request(req: &SetupPlanRequest) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    if req.call_data.connection_string.trim().is_empty() {
        errors.push("CallData.ConnectionString is required".to_string());
    }
    if req.call_data.source_object_name.trim().is_empty() {
        errors.push("CallData.SourceObjectName is required".to_string());
    } else if guess_engine(&req.call_data.connection_string) == "sqlserver"
        && validate_and_quote_sql_server_object(&req.call_data.source_object_name).is_err()
    {
        errors.push("CallData.SourceObjectName is invalid".to_string());
    }
    if req.call_data.source_name.trim().is_empty() {
        errors.push("CallData.SourceName is required".to_string());
    }
    if req.config_db.connection_string.trim().is_empty() {
        errors.push("ConfigDb.ConnectionString is required".to_string());
    }
    if matches!(req.auth_mode, AuthMode::External) {
        match &req.external_auth_headers {
            None => {
                errors.push("ExternalAuthHeaders is required when AuthMode is External".to_string())
            }
            Some(h) => {
                if h.user_header.trim().is_empty() {
                    errors.push("ExternalAuthHeaders.UserHeader is required".to_string());
                }
                if h.roles_header.trim().is_empty() {
                    errors.push("ExternalAuthHeaders.RolesHeader is required".to_string());
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn build_instance_settings(req: &SetupPlanRequest) -> HashMap<String, String> {
    let mut settings = HashMap::new();
    settings.insert("Auth:Mode".to_string(), format!("{:?}", req.auth_mode));
    settings.insert(
        "Data:CallData:SourceObjectName".to_string(),
        req.call_data.source_object_name.clone(),
    );
    settings.insert(
        "Data:CallData:SourceName".to_string(),
        req.call_data.source_name.clone(),
    );

    if matches!(req.auth_mode, AuthMode::External) {
        if let Some(h) = &req.external_auth_headers {
            settings.insert(
                "Auth:External:TrustHeaders".to_string(),
                h.trust_headers.to_string(),
            );
            settings.insert(
                "Auth:External:UserHeader".to_string(),
                h.user_header.clone(),
            );
            settings.insert(
                "Auth:External:RolesHeader".to_string(),
                h.roles_header.clone(),
            );
        }
    }

    settings
}

fn parse_feature_keys(features_json: &str) -> Vec<String> {
    let v: serde_json::Value =
        serde_json::from_str(features_json).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(obj) = v.as_object() {
        return obj.keys().cloned().collect();
    }
    if let Some(arr) = v.as_array() {
        return arr
            .iter()
            .filter_map(|x| x.as_str().map(|s| s.to_string()))
            .collect();
    }
    vec![]
}
