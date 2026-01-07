// Preflight endpoints (port from C# InstallerPreflightEndpoints.cs)
use log::info;

#[tauri::command]
pub async fn check_host() -> Result<serde_json::Value, String> {
    info!("[API] check_host called");
    // TODO: Port from C# InstallerPreflightEndpoints.PostHost
    Ok(serde_json::json!({
        "status": "ok"
    }))
}

