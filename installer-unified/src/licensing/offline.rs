// Offline license verification
use log::info;

pub async fn verify_offline(key: &str, bundle: &[u8]) -> Result<serde_json::Value, String> {
    info!("[LICENSE] Offline verification");
    // TODO: Implement offline license verification
    Ok(serde_json::json!({}))
}

