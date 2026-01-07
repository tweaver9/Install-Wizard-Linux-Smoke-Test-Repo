// License endpoints (port from C# InstallerLicenseEndpoints.cs)
use log::info;

#[tauri::command]
pub async fn verify_license(license_path: String) -> Result<serde_json::Value, String> {
    info!("[API] verify_license called: {}", license_path);
    // TODO: Port from C# InstallerLicenseEndpoints.PostVerify
    Ok(serde_json::json!({
        "status": "ok"
    }))
}

