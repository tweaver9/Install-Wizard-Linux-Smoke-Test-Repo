// Setup endpoints (port from C# InstallerSetupEndpoints.cs)
use log::info;

#[tauri::command]
pub async fn init_setup() -> Result<serde_json::Value, String> {
    info!("[API] init_setup called");
    // TODO: Port from C# InstallerSetupEndpoints.PostInit
    Ok(serde_json::json!({
        "status": "ok"
    }))
}

#[tauri::command]
pub async fn plan_setup(request: serde_json::Value) -> Result<serde_json::Value, String> {
    info!("[API] plan_setup called");
    // TODO: Port from C# InstallerSetupEndpoints.PostPlan
    Ok(serde_json::json!({
        "status": "ok"
    }))
}

