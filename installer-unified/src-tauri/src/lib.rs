// CADalytix Unified Cross-Platform Installer
// Main library entry point

mod api;
mod archiver;
mod database;
mod installation;
mod licensing;
mod models;
mod security;
mod tui;
mod utils;

use log::{error, info, warn};
use std::path::PathBuf;
use tauri::async_runtime;
use tauri::{Emitter, Manager};
use tokio::time::{sleep, Duration};

/// Initialize logging system with dual format (JSON + human-readable)
fn init_logging(with_stdout: bool) -> Result<(), Box<dyn std::error::Error>> {
    let log_dir = utils::path_resolver::resolve_log_folder()?;
    std::fs::create_dir_all(&log_dir)?;

    let timestamp = chrono::Utc::now().format("%Y-%m-%d-%H%M%S");

    // JSON log file for structured parsing
    let json_log_file = log_dir.join(format!("installer-{}.log", timestamp));

    // Human-readable log file (.txt)
    let txt_log_file = log_dir.join(format!("installer-{}.txt", timestamp));

    // Configure dual-format logging:
    // - JSON format to .log file
    // - Human-readable format to .txt file
    // - Optional: human-readable to stdout (disabled for TUI to avoid corrupting the terminal UI)
    let mut dispatch = fern::Dispatch::new().level(log::LevelFilter::Debug);

    if with_stdout {
        dispatch = dispatch.chain(
            fern::Dispatch::new()
                .format(move |out, message, record| {
                    let timestamp_local = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
                    let message_str = format!("{}", message);
                    let (phase, step, cleaned_message) =
                        utils::logging::parse_log_metadata(&message_str);
                    let txt_line = utils::logging::format_human_readable_log(
                        &timestamp_local.to_string(),
                        record.level(),
                        record.target(),
                        &cleaned_message,
                        phase.as_deref(),
                        step.as_deref(),
                    );
                    out.finish(format_args!("{}", txt_line));
                })
                .chain(std::io::stdout()),
        );
    }

    dispatch = dispatch
        .chain(
            fern::Dispatch::new()
                .format(move |out, message, record| {
                    let timestamp_utc = chrono::Utc::now().to_rfc3339();
                    let message_str = format!("{}", message);
                    let (phase, step, cleaned_message) =
                        utils::logging::parse_log_metadata(&message_str);
                    let json_line = utils::logging::format_json_log(
                        &timestamp_utc,
                        record.level(),
                        record.target(),
                        &cleaned_message,
                        phase.as_deref(),
                        step.as_deref(),
                        None, // details - can be extended later
                        None, // context - can be extended later
                        None, // performance - can be extended later
                    );
                    out.finish(format_args!("{}\n", json_line));
                })
                .chain(fern::log_file(json_log_file)?),
        )
        .chain(
            fern::Dispatch::new()
                .format(move |out, message, record| {
                    let timestamp_local = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
                    let message_str = format!("{}", message);
                    let (phase, step, cleaned_message) =
                        utils::logging::parse_log_metadata(&message_str);
                    let txt_line = utils::logging::format_human_readable_log(
                        &timestamp_local.to_string(),
                        record.level(),
                        record.target(),
                        &cleaned_message,
                        phase.as_deref(),
                        step.as_deref(),
                    );
                    out.finish(format_args!("{}\n", txt_line));
                })
                .chain(fern::log_file(txt_log_file)?),
        );

    dispatch.apply()?;

    log::info!(
        "[PHASE: initialization] Logging initialized, log directory: {:?}",
        log_dir
    );
    Ok(())
}

/// Resolve deployment folder (absolute path)
fn resolve_deployment_folder() -> PathBuf {
    // Prefer the folder where the EXE is running from
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(dir) = exe_path.parent() {
            return dir.to_path_buf();
        }
    }

    // Fallback: current working directory
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run_gui() {
    // Initialize logging first
    if let Err(e) = init_logging(true) {
        eprintln!("Failed to initialize logging: {}", e);
    }

    info!(
        "[PHASE: initialization] Installer starting at {}",
        chrono::Utc::now()
    );

    let deployment_folder = resolve_deployment_folder();
    info!(
        "[PHASE: initialization] Deployment folder: {:?}",
        deployment_folder
    );

    // Secret protector (encryption-at-rest for DB secrets)
    let log_dir = match utils::path_resolver::resolve_log_folder() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to resolve log folder for secret protector: {}", e);
            // Fallback: keep logs + secrets near deployment (best-effort)
            deployment_folder.join("Prod_Wizard_Log")
        }
    };
    let secret_key_path = security::secret_protector::default_key_path(&log_dir);
    let secret_protector = std::sync::Arc::new(security::secret_protector::SecretProtector::new(
        secret_key_path,
    ));

    let run_result = tauri::Builder::default()
        .manage(models::state::AppState::default())
        .manage(secret_protector)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            info!("[PHASE: initialization] Tauri application setup");

            let app_handle = app.handle().clone();

            // Initialize backend services (lazy, on-demand)
            info!("[PHASE: initialization] Backend services initialized");

            // Emit ready event after a short delay to ensure UI is loaded
            let app_handle_clone = app_handle.clone();
            async_runtime::spawn(async move {
                sleep(Duration::from_millis(500)).await;
                if let Some(window) = app_handle_clone.get_webview_window("main") {
                    let _ = window.center();
                    let payload = serde_json::json!({
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                        "version": "0.1.0"
                    });

                    if let Err(e) = window.emit("installer-ready", payload) {
                        error!(
                            "[PHASE: initialization] [STEP: emit_ready] Failed to emit installer-ready event: {}",
                            e
                        );
                    } else {
                        info!("[PHASE: initialization] [STEP: emit_ready] Emitted installer-ready event to UI");
                    }
                } else {
                    warn!(
                        "[PHASE: initialization] [STEP: emit_ready] Main window not found; skipping installer-ready emit"
                    );
                }
            });


            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // UI helper + installer orchestration commands
            api::installer::file_exists,
            api::installer::get_free_space_bytes,
            api::installer::create_support_bundle,
            api::installer::test_db_connection,
            api::installer::start_install,
            api::installer::cancel_install,
            // Setup API handlers
            api::setup::init_setup,
            api::setup::plan_setup,
            api::setup::apply_setup,
            api::setup::commit_setup,
            api::setup::verify_setup,
            api::setup::get_setup_status,
            api::setup::get_setup_completion_status,
            api::setup::get_latest_checkpoint,
            api::setup::save_checkpoint,
            api::setup::get_support_bundle,
            // License API handlers
            api::license::verify_license,
            api::license::get_license_status,
            // Preflight API handlers
            api::preflight::preflight_host,
            api::preflight::preflight_permissions,
            api::preflight::preflight_datasource,
            // Schema API handlers
            api::schema::verify_schema,
            api::schema::verify_all_schemas,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = run_result {
        error!("[PHASE: initialization] Tauri run error: {}", e);
        eprintln!("Error while running tauri application: {}", e);
    }
}

/// Headless terminal UI wizard (Linux servers / no-display environments)
pub fn run_tui() {
    // Initialize logging (no stdout to avoid corrupting the TUI)
    if let Err(e) = init_logging(false) {
        eprintln!("Failed to initialize logging: {}", e);
    }

    info!(
        "[PHASE: initialization] Headless TUI installer starting at {}",
        chrono::Utc::now()
    );

    let deployment_folder = resolve_deployment_folder();
    info!(
        "[PHASE: initialization] [STEP: deployment_folder] Deployment folder: {:?}",
        deployment_folder
    );

    // Secret protector (encryption-at-rest for DB secrets)
    let log_dir = match utils::path_resolver::resolve_log_folder() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to resolve log folder for secret protector: {}", e);
            deployment_folder.join("Prod_Wizard_Log")
        }
    };
    let secret_key_path = security::secret_protector::default_key_path(&log_dir);
    let secret_protector = std::sync::Arc::new(security::secret_protector::SecretProtector::new(
        secret_key_path,
    ));

    if let Err(e) = tui::run(secret_protector) {
        error!("[PHASE: tui] [STEP: fatal] TUI exited with error: {:?}", e);
        eprintln!("Installer error: {}", e);
    }
}

/// Non-interactive TUI smoke mode (for automated checks).
/// Renders a single frame and exits (restores terminal).
pub fn run_tui_smoke(target: Option<String>) {
    // Initialize logging (no stdout to avoid corrupting the terminal)
    if let Err(e) = init_logging(false) {
        eprintln!("Failed to initialize logging: {}", e);
    }

    info!(
        "[PHASE: initialization] Headless TUI smoke starting at {}",
        chrono::Utc::now()
    );

    let deployment_folder = resolve_deployment_folder();
    info!(
        "[PHASE: initialization] [STEP: deployment_folder] Deployment folder: {:?}",
        deployment_folder
    );

    // Secret protector (encryption-at-rest for DB secrets)
    let log_dir = match utils::path_resolver::resolve_log_folder() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to resolve log folder for secret protector: {}", e);
            deployment_folder.join("Prod_Wizard_Log")
        }
    };
    let secret_key_path = security::secret_protector::default_key_path(&log_dir);
    let secret_protector = std::sync::Arc::new(security::secret_protector::SecretProtector::new(
        secret_key_path,
    ));

    let target = target.as_deref().unwrap_or("welcome");
    if let Err(e) = tui::smoke(secret_protector, target) {
        error!(
            "[PHASE: tui] [STEP: smoke] TUI smoke exited with error: {:?}",
            e
        );
        eprintln!("Installer error: {}", e);
        std::process::exit(1);
    }
}

/// Non-interactive install contract smoke (for automated verification / log capture).
/// Writes deterministic transcript artifacts under `Prod_Wizard_Log/` and exits 0/1.
pub fn run_install_contract_smoke() {
    // Initialize logging
    if let Err(e) = init_logging(false) {
        eprintln!("Failed to initialize logging: {}", e);
    }

    info!(
        "[PHASE: initialization] Install contract smoke starting at {}",
        chrono::Utc::now()
    );

    let deployment_folder = resolve_deployment_folder();
    info!(
        "[PHASE: initialization] [STEP: deployment_folder] Deployment folder: {:?}",
        deployment_folder
    );

    // Secret protector (encryption-at-rest for DB secrets)
    let log_dir = match utils::path_resolver::resolve_log_folder() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to resolve log folder for secret protector: {}", e);
            deployment_folder.join("Prod_Wizard_Log")
        }
    };
    let secret_key_path = security::secret_protector::default_key_path(&log_dir);
    let secret_protector = std::sync::Arc::new(security::secret_protector::SecretProtector::new(
        secret_key_path,
    ));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let result = match rt {
        Ok(rt) => rt.block_on(api::installer::install_contract_smoke(secret_protector)),
        Err(e) => Err(anyhow::anyhow!(
            "Failed to create async runtime for contract smoke: {}",
            e
        )),
    };

    if let Err(e) = result {
        error!(
            "[PHASE: install] [STEP: contract_smoke] Smoke exited with error: {:?}",
            e
        );
        eprintln!("Installer error: {}", e);
        std::process::exit(1);
    }
}

/// Deterministic mapping contract + persistence proof runner (for automated verification / log capture).
/// Writes `B3_mapping_persist_smoke_transcript.log` under `Prod_Wizard_Log/` and exits 0/1.
pub fn run_mapping_persist_smoke() {
    // Initialize logging
    if let Err(e) = init_logging(false) {
        eprintln!("Failed to initialize logging: {}", e);
    }

    info!(
        "[PHASE: initialization] Mapping persist smoke starting at {}",
        chrono::Utc::now()
    );

    let deployment_folder = resolve_deployment_folder();
    info!(
        "[PHASE: initialization] [STEP: deployment_folder] Deployment folder: {:?}",
        deployment_folder
    );

    // Secret protector (encryption-at-rest for DB secrets)
    let log_dir = match utils::path_resolver::resolve_log_folder() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to resolve log folder for secret protector: {}", e);
            deployment_folder.join("Prod_Wizard_Log")
        }
    };
    let secret_key_path = security::secret_protector::default_key_path(&log_dir);
    let secret_protector = std::sync::Arc::new(security::secret_protector::SecretProtector::new(
        secret_key_path,
    ));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let result = match rt {
        Ok(rt) => rt.block_on(api::installer::mapping_persist_smoke(secret_protector)),
        Err(e) => Err(anyhow::anyhow!(
            "Failed to create async runtime for mapping persist smoke: {}",
            e
        )),
    };

    if let Err(e) = result {
        error!(
            "[PHASE: mapping] [STEP: persist_smoke] Smoke exited with error: {:?}",
            e
        );
        eprintln!("Installer error: {}", e);
        std::process::exit(1);
    }
}

/// Non-interactive archive pipeline dry-run (for deterministic verification / log capture).
/// Writes `B2_archive_pipeline_dryrun_transcript.log` under `Prod_Wizard_Log/` and exits 0/1.
pub fn run_archive_dry_run() {
    // Initialize logging
    if let Err(e) = init_logging(false) {
        eprintln!("Failed to initialize logging: {}", e);
    }

    info!(
        "[PHASE: initialization] Archive dry-run starting at {}",
        chrono::Utc::now()
    );

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let result = match rt {
        Ok(rt) => rt.block_on(archiver::archive_dry_run()),
        Err(e) => Err(anyhow::anyhow!(
            "Failed to create async runtime for archive dry-run: {}",
            e
        )),
    };

    if let Err(e) = result {
        error!(
            "[PHASE: archive] [STEP: dry_run] Dry-run exited with error: {:?}",
            e
        );
        eprintln!("Installer error: {}", e);
        std::process::exit(1);
    }
}

/// D2 Database Setup proof mode (deterministic).
/// Writes `D2_db_setup_smoke_transcript.log` under `Prod_Wizard_Log/` and exits 0/1.
pub fn run_db_setup_smoke() {
    // Initialize logging
    if let Err(e) = init_logging(false) {
        eprintln!("Failed to initialize logging: {}", e);
    }

    info!(
        "[PHASE: initialization] D2 Database Setup smoke starting at {}",
        chrono::Utc::now()
    );

    let deployment_folder = resolve_deployment_folder();
    info!(
        "[PHASE: initialization] [STEP: deployment_folder] Deployment folder: {:?}",
        deployment_folder
    );

    // Secret protector (encryption-at-rest for DB secrets)
    let log_dir = match utils::path_resolver::resolve_log_folder() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to resolve log folder for secret protector: {}", e);
            deployment_folder.join("Prod_Wizard_Log")
        }
    };
    let secret_key_path = security::secret_protector::default_key_path(&log_dir);
    let secret_protector = std::sync::Arc::new(security::secret_protector::SecretProtector::new(
        secret_key_path,
    ));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let result = match rt {
        Ok(rt) => rt.block_on(api::installer::db_setup_smoke(secret_protector)),
        Err(e) => Err(anyhow::anyhow!(
            "Failed to create async runtime for D2 db setup smoke: {}",
            e
        )),
    };

    if let Err(e) = result {
        error!(
            "[PHASE: db_setup] [STEP: smoke] D2 smoke exited with error: {:?}",
            e
        );
        eprintln!("Installer error: {}", e);
        std::process::exit(1);
    }
}
