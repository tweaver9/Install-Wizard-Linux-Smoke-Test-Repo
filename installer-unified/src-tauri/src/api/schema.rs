// Schema API endpoints
// Ported from the installer host schema verification endpoints.

use crate::database::connection::DatabaseConnection;
use crate::database::platform_db::PlatformDbAdapter;
use crate::database::schema_verifier::SchemaVerifier;
use crate::licensing::token as token_verifier;
use crate::models::requests::{VerifyAllRequest, VerifySchemaRequest};
use crate::models::responses::{
    ApiResponse, SetupVerifyCheckResult, VerifyAllResponse, VerifySchemaResponse,
};
use crate::models::state::AppState;
use crate::security::secret_protector::SecretProtector;

use log::info;
use std::sync::Arc;
use tauri::State;
use tokio::time::{timeout, Duration};
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tokio_retry::RetryIf;

#[tauri::command]
pub async fn verify_schema(
    app_state: State<'_, AppState>,
    secrets: State<'_, Arc<SecretProtector>>,
    payload: Option<VerifySchemaRequest>,
) -> Result<ApiResponse<VerifySchemaResponse>, String> {
    info!("[PHASE: schema_verification] [STEP: verify] verify_schema requested");

    let req = payload.unwrap_or(VerifySchemaRequest {
        engine: "sqlserver".to_string(),
        connection_string: None,
    });

    let (engine, conn_str) = match resolve_engine_and_conn_str(
        &app_state,
        &req.engine,
        req.connection_string.as_deref(),
    )
    .await
    {
        Ok(v) => v,
        Err(msg) => return Ok(ApiResponse::fail(msg)),
    };

    let conn = match connect_with_retry(&engine, &conn_str).await {
        Ok(c) => c,
        Err(_) => {
            return Ok(ApiResponse::fail(
                "Unable to connect to database for schema verification.",
            ))
        }
    };

    // Ensure core tables exist (uses our ported SchemaVerifier expectations)
    let verifier = SchemaVerifier::new(conn.clone());
    let results = match verifier.verify_all_schemas().await {
        Ok(r) => r,
        Err(e) => {
            return Ok(ApiResponse::ok(VerifySchemaResponse {
                is_valid: false,
                summary: format!("Schema verification failed: {}", e),
                total_issues: 1,
                missing_schemas: vec![],
                missing_tables: vec![],
                missing_columns: vec![],
                missing_indexes: vec![],
                type_mismatches: vec![],
                nullability_mismatches: vec![],
            }))
        }
    };

    // We only verify cadalytix_config right now.
    let (_, res) = results.into_iter().next().unwrap_or_else(|| {
        (
            "cadalytix_config".to_string(),
            crate::database::schema_verifier::SchemaVerificationResult {
                valid: false,
                missing_tables: vec!["<no result>".to_string()],
                missing_columns: vec![],
                errors: vec!["No schema verification result returned".to_string()],
            },
        )
    });

    let missing_tables = res
        .missing_tables
        .iter()
        .map(|t| format!("cadalytix_config.{}", t))
        .collect::<Vec<_>>();
    let missing_columns = res
        .missing_columns
        .iter()
        .map(|(t, c)| format!("cadalytix_config.{}.{}", t, c))
        .collect::<Vec<_>>();

    let total_issues = (missing_tables.len() + missing_columns.len()) as i32;

    let summary = if res.valid {
        "Schema verification passed. All expected objects exist and match the manifest.".to_string()
    } else {
        format!(
            "Schema verification failed: {} missing table(s), {} missing column(s).",
            missing_tables.len(),
            missing_columns.len()
        )
    };

    // Best-effort: record an audit event (safe; no secrets).
    let platform_db = PlatformDbAdapter::new(conn, Arc::clone(&secrets));
    let _ = platform_db
        .log_setup_event(
            if res.valid {
                "schema.verify.pass"
            } else {
                "schema.verify.fail"
            },
            &summary,
            Some("installer"),
            None,
        )
        .await;

    Ok(ApiResponse::ok(VerifySchemaResponse {
        is_valid: res.valid,
        summary,
        total_issues,
        missing_schemas: vec![],
        missing_tables,
        missing_columns,
        missing_indexes: vec![],
        type_mismatches: vec![],
        nullability_mismatches: vec![],
    }))
}

#[tauri::command]
pub async fn verify_all_schemas(
    app_state: State<'_, AppState>,
    secrets: State<'_, Arc<SecretProtector>>,
    payload: Option<VerifyAllRequest>,
) -> Result<ApiResponse<VerifyAllResponse>, String> {
    info!("[PHASE: schema_verification] [STEP: verify_all] verify_all_schemas requested");

    let req = payload.unwrap_or(VerifyAllRequest {
        config_db_connection_string: None,
        call_data_connection_string: None,
        source_object_name: None,
        engine: "sqlserver".to_string(),
    });

    let (engine, conn_str) = match resolve_engine_and_conn_str(
        &app_state,
        &req.engine,
        req.config_db_connection_string.as_deref(),
    )
    .await
    {
        Ok(v) => v,
        Err(msg) => return Ok(ApiResponse::fail(msg)),
    };

    let conn = match connect_with_retry(&engine, &conn_str).await {
        Ok(c) => c,
        Err(_) => {
            return Ok(ApiResponse::ok(VerifyAllResponse {
                success: false,
                summary: "Verification failed: unable to connect to config database.".to_string(),
                checks: vec![SetupVerifyCheckResult {
                    id: "config_db_connectivity".to_string(),
                    label: "Config DB reachable".to_string(),
                    status: "fail".to_string(),
                    message: "Unable to connect to config database.".to_string(),
                    duration_ms: 0,
                }],
                schema_verification: None,
                errors: vec!["Unable to connect to config database.".to_string()],
            }))
        }
    };

    let platform_db = PlatformDbAdapter::new(conn.clone(), Arc::clone(&secrets));

    let mut checks: Vec<SetupVerifyCheckResult> = Vec::new();
    checks.push(SetupVerifyCheckResult {
        id: "config_db_connectivity".to_string(),
        label: "Config DB reachable".to_string(),
        status: "pass".to_string(),
        message: "Config database connection succeeded.".to_string(),
        duration_ms: 0,
    });

    // Committed flag
    let committed = platform_db
        .get_setting("Setup:Committed")
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
        .eq_ignore_ascii_case("true");
    checks.push(SetupVerifyCheckResult {
        id: "committed_flag".to_string(),
        label: "Setup committed".to_string(),
        status: if committed {
            "pass".to_string()
        } else {
            "fail".to_string()
        },
        message: format!(
            "Setup:Committed is {}.",
            committed.to_string().to_lowercase()
        ),
        duration_ms: 0,
    });

    // License token presence + signature verification (fail-closed)
    let license_state = platform_db.get_license_state().await.ok().flatten();
    let token_str = license_state
        .as_ref()
        .and_then(|v| v.get("signedTokenBlob"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let has_token = token_str
        .as_ref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    let token_valid = token_str
        .as_deref()
        .and_then(|t| token_verifier::verify_and_parse(Some(t)))
        .is_some();
    checks.push(SetupVerifyCheckResult {
        id: "license_token".to_string(),
        label: "License token valid".to_string(),
        status: if token_valid {
            "pass".to_string()
        } else {
            "fail".to_string()
        },
        message: if !has_token {
            "No signed license token found.".to_string()
        } else if token_valid {
            "Signed license token verified (RS256).".to_string()
        } else {
            "Signed license token present but failed verification.".to_string()
        },
        duration_ms: 0,
    });

    // Schema verification
    let schema_verifier = SchemaVerifier::new(conn.clone());
    let schema_results = schema_verifier.verify_all_schemas().await.ok();
    let schema_verification = schema_results.and_then(|mut v| v.pop()).map(|(_, r)| {
        let missing_tables = r
            .missing_tables
            .iter()
            .map(|t| format!("cadalytix_config.{}", t))
            .collect::<Vec<_>>();
        let missing_columns = r
            .missing_columns
            .iter()
            .map(|(t, c)| format!("cadalytix_config.{}.{}", t, c))
            .collect::<Vec<_>>();
        let total_issues = (missing_tables.len() + missing_columns.len()) as i32;
        let summary = if r.valid {
            "Schema verification passed. All expected objects exist and match the manifest."
                .to_string()
        } else {
            format!(
                "Schema verification failed: {} missing table(s), {} missing column(s).",
                missing_tables.len(),
                missing_columns.len()
            )
        };
        VerifySchemaResponse {
            is_valid: r.valid,
            summary,
            total_issues,
            missing_schemas: vec![],
            missing_tables,
            missing_columns,
            missing_indexes: vec![],
            type_mismatches: vec![],
            nullability_mismatches: vec![],
        }
    });

    let schema_valid = schema_verification
        .as_ref()
        .map(|s| s.is_valid)
        .unwrap_or(false);

    let failed_checks = checks.iter().filter(|c| c.status == "fail").count();
    let success = failed_checks == 0 && schema_valid;

    let summary = if success {
        format!("All {} checks passed and schema verified.", checks.len())
    } else {
        let mut parts = Vec::new();
        if failed_checks > 0 {
            parts.push(format!("{} check(s) failed", failed_checks));
        }
        if !schema_valid {
            parts.push(format!(
                "{} schema issue(s)",
                schema_verification
                    .as_ref()
                    .map(|s| s.total_issues)
                    .unwrap_or(0)
            ));
        }
        format!("Verification failed: {}.", parts.join(", "))
    };

    Ok(ApiResponse::ok(VerifyAllResponse {
        success,
        summary,
        checks,
        schema_verification,
        errors: if success {
            vec![]
        } else {
            vec!["One or more verification checks failed.".to_string()]
        },
    }))
}

async fn resolve_engine_and_conn_str(
    app_state: &AppState,
    engine_hint: &str,
    conn_str_override: Option<&str>,
) -> Result<(String, String), String> {
    let engine = engine_hint.trim().to_ascii_lowercase();
    if let Some(cs) = conn_str_override.filter(|s| !s.trim().is_empty()) {
        return Ok((engine, cs.to_string()));
    }

    if let Some((_eng, _ver, cs)) = app_state.get_config_db().await {
        return Ok((engine, cs));
    }

    Err(
        "Config database connection string is not available. Provide connectionString in request."
            .to_string(),
    )
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
