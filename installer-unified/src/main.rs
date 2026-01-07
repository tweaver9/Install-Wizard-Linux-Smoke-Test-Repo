// Prevents additional console window on Windows in release mode
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use log::{info, error};
use std::path::PathBuf;
use tauri::{Manager, Window};

mod api;
mod database;
mod installation;
mod licensing;
mod security;
mod utils;
mod models;

use utils::logging;
use utils::path_resolver;
use utils::os_detection;

#[derive(Clone)]
struct AppState {
    deployment_folder: PathBuf,
    log_folder: PathBuf,
}

#[tauri::command]
async fn get_app_state(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "deployment_folder": state.deployment_folder.to_string_lossy(),
        "log_folder": state.log_folder.to_string_lossy(),
    }))
}

fn main() {
    // Initialize logging first
    let log_folder = path_resolver::resolve_log_folder();
    logging::initialize(&log_folder).expect("Failed to initialize logging");
    
    info!("[PHASE: initialization] Installer starting");
    info!("[PHASE: initialization] Log directory: {}", log_folder.display());
    
    // Detect OS
    let os = os_detection::detect_os();
    info!("[PHASE: initialization] Operating system: {:?}", os);
    
    // Resolve deployment folder
    let deployment_folder = path_resolver::resolve_deployment_folder();
    info!("[PHASE: initialization] Deployment folder: {}", deployment_folder.display());
    
    // Create app state
    let app_state = AppState {
        deployment_folder: deployment_folder.clone(),
        log_folder: log_folder.clone(),
    };
    
    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_app_state,
        ])
        .setup(|app| {
            let window = app.get_window("main").unwrap();
            
            // Wait for WebView to be ready
            info!("[PHASE: initialization] Waiting for WebView to be ready...");
            
            // Emit ready event after a short delay
            let window_clone = window.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                info!("[PHASE: initialization] Installer ready for user interaction");
                window_clone.emit("installer-ready", serde_json::json!({
                    "timestamp": chrono::Utc::now(),
                    "version": env!("CARGO_PKG_VERSION")
                })).unwrap();
            });
            
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

