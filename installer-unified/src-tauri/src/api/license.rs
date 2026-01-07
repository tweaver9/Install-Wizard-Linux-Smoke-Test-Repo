// License API endpoints
// Ported from C# InstallerLicenseEndpoints.cs

use crate::database::connection::DatabaseConnection;
use crate::database::platform_db::PlatformDbAdapter;
use crate::licensing::token as token_verifier;
use crate::models::requests::LicenseVerifyRequest;
use crate::models::responses::{
    ApiResponse, LicenseEntitlementDto, LicenseStatusResponse, LicenseVerifyResponse,
};
use crate::models::state::AppState;
use crate::security::crypto::secret_fingerprint;
use crate::security::secret_protector::SecretProtector;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use chrono::{DateTime, Utc};
use log::{error, info, warn};
use regex::Regex;
use ring::pbkdf2;
use ring::signature;
use ring::signature::UnparsedPublicKey;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use tauri::State;
use tokio::time::{timeout, Duration};
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tokio_retry::RetryIf;
use uuid::Uuid;

#[tauri::command]
pub async fn verify_license(
    app_state: State<'_, AppState>,
    secrets: State<'_, Arc<SecretProtector>>,
    payload: Option<LicenseVerifyRequest>,
) -> Result<ApiResponse<LicenseVerifyResponse>, String> {
    let correlation_id = Uuid::new_v4().simple().to_string();
    info!(
        "[PHASE: license_verification] [STEP: verify] verify_license requested (correlation_id={})",
        correlation_id
    );

    let Some(mut req) = payload else {
        return Ok(ApiResponse::ok(LicenseVerifyResponse {
            success: false,
            message: "Invalid request. Request body is required.".to_string(),
            entitlement: None,
            correlation_id,
        }));
    };

    // Normalize + validate license key
    req.license_key = req.license_key.trim().to_ascii_uppercase();
    if req.license_key.trim().is_empty() {
        return Ok(ApiResponse::ok(LicenseVerifyResponse {
            success: false,
            message: "License key is required".to_string(),
            entitlement: None,
            correlation_id,
        }));
    }

    let key_re = match Regex::new(r"^[A-Z0-9]{4}(-[A-Z0-9]{4}){3}$") {
        Ok(re) => re,
        Err(e) => {
            error!(
                "[PHASE: license_verification] [STEP: verify] Internal error compiling license key regex: {} (correlation_id={})",
                e, correlation_id
            );
            return Ok(ApiResponse::ok(LicenseVerifyResponse {
                success: false,
                message: "Internal error initializing license validation. Please check logs."
                    .to_string(),
                entitlement: None,
                correlation_id,
            }));
        }
    };
    if !key_re.is_match(&req.license_key) {
        return Ok(ApiResponse::ok(LicenseVerifyResponse {
            success: false,
            message: format!(
                "Invalid license key format. Expected format: XXXX-XXXX-XXXX-XXXX (A-Z0-9 only). Received length: {}",
                req.license_key.len()
            ),
            entitlement: None,
            correlation_id,
        }));
    }

    let mode = req.mode.trim().to_ascii_lowercase();
    if mode != "online" && mode != "offline" {
        return Ok(ApiResponse::ok(LicenseVerifyResponse {
            success: false,
            message: "Invalid mode. Must be 'online' or 'offline'".to_string(),
            entitlement: None,
            correlation_id,
        }));
    }

    if mode == "offline"
        && req
            .offline_bundle
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
    {
        return Ok(ApiResponse::ok(LicenseVerifyResponse {
            success: false,
            message: "Offline bundle is required for offline verification".to_string(),
            entitlement: None,
            correlation_id,
        }));
    }

    // Verify (online/offline)
    let verification = if mode == "online" {
        match verify_online_with_retry(&req.license_key, req.ops_api_base_url.as_deref()).await {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    "[PHASE: license_verification] [STEP: verify] Online verification failed: {} (correlation_id={})",
                    e, correlation_id
                );
                return Ok(ApiResponse::ok(LicenseVerifyResponse {
                    success: false,
                    message:
                        "An error occurred during online license verification. Please check logs."
                            .to_string(),
                    entitlement: None,
                    correlation_id,
                }));
            }
        }
    } else {
        match verify_offline(
            &req.license_key,
            req.offline_bundle.as_deref().unwrap_or(""),
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    "[PHASE: license_verification] [STEP: verify] Offline verification failed: {} (correlation_id={})",
                    e, correlation_id
                );
                return Ok(ApiResponse::ok(LicenseVerifyResponse {
                    success: false,
                    message:
                        "An error occurred during offline license verification. Please check logs."
                            .to_string(),
                    entitlement: None,
                    correlation_id,
                }));
            }
        }
    };

    if !verification.is_valid {
        // Best-effort: log to DB if initialized
        best_effort_log_event(
            &app_state,
            &secrets,
            "license_verify_failed",
            &verification.message,
        )
        .await;

        return Ok(ApiResponse::ok(LicenseVerifyResponse {
            success: false,
            message: verification.message,
            entitlement: None,
            correlation_id,
        }));
    }

    let now = Utc::now();

    // SECURITY: Verify the signed JWT token locally (fail-closed). This is the authoritative
    // source of truth for expiry/features.
    let token_payload = match token_verifier::verify_and_parse(Some(&verification.signed_token)) {
        Some(p) => p,
        None => {
            best_effort_log_event(
                &app_state,
                &secrets,
                "license_verify_failed",
                "Signed token verification failed (invalid signature/claims).",
            )
            .await;
            return Ok(ApiResponse::ok(LicenseVerifyResponse {
                success: false,
                message: "License verification failed: signed token could not be validated."
                    .to_string(),
                entitlement: None,
                correlation_id,
            }));
        }
    };

    // Optional defense-in-depth: if token carries installId, ensure it matches the offline bundle.
    if mode == "offline" {
        if let (Some(token_install_id), Some(bundle_install_id)) = (
            token_payload
                .install_id
                .as_ref()
                .filter(|s| !s.trim().is_empty()),
            verification
                .install_id
                .as_ref()
                .filter(|s| !s.trim().is_empty()),
        ) {
            if !token_install_id.eq_ignore_ascii_case(bundle_install_id) {
                best_effort_log_event(
                    &app_state,
                    &secrets,
                    "license_verify_failed",
                    "InstallId mismatch between signed token and offline bundle.",
                )
                .await;
                return Ok(ApiResponse::ok(LicenseVerifyResponse {
                    success: false,
                    message:
                        "License verification failed: token is not bound to this installation."
                            .to_string(),
                    entitlement: None,
                    correlation_id,
                }));
            }
        }
    }

    let authoritative_expires_at_utc = token_payload.expires_at_utc;
    let authoritative_grace_until_utc = token_payload.grace_until_utc;
    let authoritative_status = token_verifier::determine_status(
        now,
        authoritative_expires_at_utc,
        authoritative_grace_until_utc,
    );
    let authoritative_features_json =
        serde_json::to_string(&token_payload.features).unwrap_or_else(|_| "{}".to_string());
    let entitlement_features = token_payload
        .features
        .iter()
        .filter(|(_, enabled)| **enabled)
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();

    // Persist license state (best-effort if DB not initialized)
    if let Some((engine, _ver, config_cs)) = app_state.get_config_db().await {
        if let Ok(conn) = connect_with_retry(&engine, &config_cs).await {
            let platform_db = PlatformDbAdapter::new(conn.clone(), Arc::clone(&secrets));

            // Offline install_id binding enforcement (fail-closed when DB is reachable)
            if mode == "offline" {
                let mut bundle_settings: HashMap<String, String> = HashMap::new();

                if let Some(bundle_install_id) = verification
                    .install_id
                    .as_ref()
                    .filter(|s| !s.trim().is_empty())
                {
                    let local_install_id = platform_db
                        .get_setting("Setup:InstallId")
                        .await
                        .ok()
                        .flatten();
                    if let Some(local) = local_install_id.as_ref().filter(|s| !s.trim().is_empty())
                    {
                        if !local.eq_ignore_ascii_case(bundle_install_id) {
                            let msg = format!(
                                "License bundle is bound to installation '{}' but this system is '{}'. This bundle cannot be used on this installation.",
                                bundle_install_id, local
                            );
                            let _ = platform_db
                                .log_setup_event(
                                    "license_verify_failed",
                                    "Install ID mismatch - bundle not for this installation",
                                    Some("installer"),
                                    Some(
                                        &serde_json::json!({
                                            "correlationId": correlation_id,
                                            "localInstallId": local,
                                            "bundleInstallId": bundle_install_id
                                        })
                                        .to_string(),
                                    ),
                                )
                                .await;
                            return Ok(ApiResponse::ok(LicenseVerifyResponse {
                                success: false,
                                message: msg,
                                entitlement: None,
                                correlation_id,
                            }));
                        }
                    } else {
                        // First-time setup: set install id from bundle
                        bundle_settings
                            .insert("Setup:InstallId".to_string(), bundle_install_id.to_string());
                    }
                }

                // Persist bootstrap secret (encrypted-at-rest via PlatformDbAdapter secret key list)
                if let Some(secret) = verification
                    .bootstrap_secret
                    .as_ref()
                    .filter(|s| !s.trim().is_empty())
                {
                    bundle_settings.insert("Setup:BootstrapSecret".to_string(), secret.to_string());
                }

                // Branding defaults (safe to store as plaintext)
                if let Some(b) = verification.branding.as_ref() {
                    if let Some(v) = b.client_name.as_ref().filter(|s| !s.trim().is_empty()) {
                        bundle_settings.insert("Branding:ClientName".to_string(), v.to_string());
                    }
                    if let Some(v) = b.support_email.as_ref().filter(|s| !s.trim().is_empty()) {
                        bundle_settings.insert("Branding:SupportEmail".to_string(), v.to_string());
                    }
                    if let Some(v) = b.support_phone.as_ref().filter(|s| !s.trim().is_empty()) {
                        bundle_settings.insert("Branding:SupportPhone".to_string(), v.to_string());
                    }
                    if let Some(v) = b.support_url.as_ref().filter(|s| !s.trim().is_empty()) {
                        bundle_settings.insert("Branding:SupportUrl".to_string(), v.to_string());
                    }
                    if let Some(v) = b.logo_url.as_ref().filter(|s| !s.trim().is_empty()) {
                        bundle_settings.insert("Branding:LogoUrl".to_string(), v.to_string());
                    }
                    if let Some(v) = b.theme_name.as_ref().filter(|s| !s.trim().is_empty()) {
                        bundle_settings.insert("Branding:ThemeName".to_string(), v.to_string());
                    }
                }

                // Constraints (serialize lists/objects as JSON strings)
                if let Some(c) = verification.constraints.as_ref() {
                    if let Some(v) = c.max_users {
                        bundle_settings.insert("Licensing:MaxUsers".to_string(), v.to_string());
                    }
                    if let Some(v) = c.allowed_regions.as_ref().filter(|v| !v.is_empty()) {
                        if let Ok(json) = serde_json::to_string(v) {
                            bundle_settings.insert("Licensing:AllowedRegions".to_string(), json);
                        }
                    }
                    if let Some(v) = c.feature_flags.as_ref().filter(|v| !v.is_empty()) {
                        if let Ok(json) = serde_json::to_string(v) {
                            bundle_settings.insert("Licensing:FeatureFlags".to_string(), json);
                        }
                    }
                    if let Some(v) = c.enabled_modules.as_ref().filter(|v| !v.is_empty()) {
                        if let Ok(json) = serde_json::to_string(v) {
                            bundle_settings.insert("Licensing:EnabledModules".to_string(), json);
                        }
                    }
                }

                // Client binding hint (non-secret)
                if !verification.license_id.trim().is_empty() {
                    bundle_settings.insert(
                        "Licensing:ClientId".to_string(),
                        verification.license_id.clone(),
                    );
                }

                if !bundle_settings.is_empty() {
                    let _ = platform_db.set_settings(&bundle_settings).await;
                }
            }

            // Save license state
            let installation_token = Uuid::new_v4().simple().to_string();
            let _ = platform_db
                .save_license_state(
                    &mode,
                    &mask_license_key(&req.license_key),
                    &secret_fingerprint(&req.license_key),
                    &authoritative_status,
                    &verification.client_name,
                    &verification.license_id,
                    verification.issued_at_utc,
                    authoritative_expires_at_utc,
                    authoritative_grace_until_utc,
                    &authoritative_features_json,
                    now,
                    &installation_token,
                    Some(&verification.signed_token),
                    Some(now),
                    Some(authoritative_expires_at_utc),
                )
                .await;

            let _ = platform_db
                .log_setup_event(
                    "license_verify_success",
                    &format!("License verified successfully in {} mode", mode),
                    Some("installer"),
                    Some(
                        &serde_json::json!({ "correlationId": correlation_id, "mode": mode })
                            .to_string(),
                    ),
                )
                .await;
        }
    } else {
        warn!(
            "[PHASE: license_verification] [STEP: verify] DB not initialized; skipping license state persistence (correlation_id={})",
            correlation_id
        );
    }

    Ok(ApiResponse::ok(LicenseVerifyResponse {
        success: true,
        message: "License verified successfully".to_string(),
        entitlement: Some(LicenseEntitlementDto {
            license_mode: mode,
            expires_at_utc: Some(authoritative_expires_at_utc),
            grace_until_utc: Some(authoritative_grace_until_utc),
            features: entitlement_features,
            client_id: Some(verification.license_id.clone()),
            last_verified_at_utc: now,
        }),
        correlation_id,
    }))
}

#[tauri::command]
pub async fn get_license_status(
    app_state: State<'_, AppState>,
    secrets: State<'_, Arc<SecretProtector>>,
) -> Result<ApiResponse<LicenseStatusResponse>, String> {
    info!("[PHASE: license_verification] [STEP: status] get_license_status requested");

    let Some((engine, _ver, config_cs)) = app_state.get_config_db().await else {
        return Ok(ApiResponse::ok(LicenseStatusResponse {
            is_active: false,
            entitlement: None,
            message: "No license configured".to_string(),
        }));
    };

    let conn = match connect_with_retry(&engine, &config_cs).await {
        Ok(c) => c,
        Err(_) => {
            return Ok(ApiResponse::ok(LicenseStatusResponse {
                is_active: false,
                entitlement: None,
                message: "Failed to retrieve license status (database unavailable)".to_string(),
            }))
        }
    };

    let platform_db = PlatformDbAdapter::new(conn, Arc::clone(&secrets));
    let license_state = platform_db.get_license_state().await.unwrap_or_default();

    let Some(state) = license_state else {
        return Ok(ApiResponse::ok(LicenseStatusResponse {
            is_active: false,
            entitlement: None,
            message: "No license configured".to_string(),
        }));
    };

    // Never return secrets to UI
    let mode = state
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let last_verified = state
        .get("lastVerifiedAtUtc")
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let now = Utc::now();
    let signed_token = state.get("signedTokenBlob").and_then(|v| v.as_str());
    let Some(payload) = token_verifier::verify_and_parse(signed_token) else {
        return Ok(ApiResponse::ok(LicenseStatusResponse {
            is_active: false,
            entitlement: None,
            message: "Invalid or missing signed license token".to_string(),
        }));
    };

    let status =
        token_verifier::determine_status(now, payload.expires_at_utc, payload.grace_until_utc);
    let is_active = status != "expired";
    let message = match status.as_str() {
        "active" => "License is active",
        "grace" => "License is in grace period",
        _ => "License has expired",
    }
    .to_string();

    let features = payload
        .features
        .iter()
        .filter(|(_, enabled)| **enabled)
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();

    Ok(ApiResponse::ok(LicenseStatusResponse {
        is_active,
        entitlement: Some(LicenseEntitlementDto {
            license_mode: mode,
            expires_at_utc: Some(payload.expires_at_utc),
            grace_until_utc: Some(payload.grace_until_utc),
            features,
            client_id: state
                .get("licenseId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            last_verified_at_utc: last_verified,
        }),
        message,
    }))
}

// =========================
// Verification helpers
// =========================

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct VerificationOutcome {
    is_valid: bool,
    // Some callers only use `is_valid` + `message`; keep status for future UI/state reporting.
    #[allow(dead_code)]
    status: String,
    message: String,
    client_name: String,
    license_id: String,
    issued_at_utc: DateTime<Utc>,
    #[allow(dead_code)]
    expires_at_utc: DateTime<Utc>,
    #[allow(dead_code)]
    grace_until_utc: DateTime<Utc>,
    // Stored for support diagnostics; not always read by the current flow.
    #[allow(dead_code)]
    features_json: String,
    signed_token: String,
    // Offline-only extras (for binding + bootstrapping)
    install_id: Option<String>,
    bootstrap_secret: Option<String>,
    branding: Option<BundleBranding>,
    constraints: Option<BundleConstraints>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BundleBranding {
    client_name: Option<String>,
    logo_url: Option<String>,
    theme_name: Option<String>,
    support_email: Option<String>,
    support_phone: Option<String>,
    support_url: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BundleConstraints {
    max_users: Option<i32>,
    allowed_regions: Option<Vec<String>>,
    feature_flags: Option<HashMap<String, bool>>,
    enabled_modules: Option<Vec<String>>,
}

async fn verify_online_with_retry(
    license_key: &str,
    ops_api_base_url: Option<&str>,
) -> anyhow::Result<VerificationOutcome> {
    let base = ops_api_base_url
        .unwrap_or("https://ops.cadalytix.com")
        .trim_end_matches('/');
    let url = format!("{}/licensing/verify", base);

    let attempt = || async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(12))
            .build()?;

        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Req<'a> {
            license_key: &'a str,
            client_fingerprint: String,
            requested_features: Option<Vec<String>>,
        }

        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Resp {
            valid: bool,
            client_name: Option<String>,
            license_id: Option<String>,
            issued_at_utc: Option<DateTime<Utc>>,
            expires_at_utc: Option<DateTime<Utc>>,
            grace_until_utc: Option<DateTime<Utc>>,
            features: Option<HashMap<String, serde_json::Value>>,
            error_message: Option<String>,
            signed_token: Option<String>,
        }

        let req_body = Req {
            license_key,
            client_fingerprint: format!("{}|{}", hostname_best_effort(), std::env::consts::OS),
            requested_features: None,
        };

        let resp = client.post(&url).json(&req_body).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("HTTP {}", resp.status()));
        }

        let parsed: Resp = resp.json().await?;
        if !parsed.valid {
            return Ok(VerificationOutcome {
                is_valid: false,
                status: "invalid".to_string(),
                message: parsed
                    .error_message
                    .unwrap_or_else(|| "License verification failed".to_string()),
                client_name: parsed.client_name.unwrap_or_default(),
                license_id: parsed.license_id.unwrap_or_default(),
                issued_at_utc: Utc::now(),
                expires_at_utc: Utc::now(),
                grace_until_utc: Utc::now(),
                features_json: "{}".to_string(),
                signed_token: String::new(),
                install_id: None,
                bootstrap_secret: None,
                branding: None,
                constraints: None,
            });
        }

        let signed = parsed.signed_token.unwrap_or_default();
        if signed.trim().is_empty() {
            return Ok(VerificationOutcome {
                is_valid: false,
                status: "invalid".to_string(),
                message: "SECURITY GAP: Licensing server response missing signedToken.".to_string(),
                client_name: parsed.client_name.unwrap_or_default(),
                license_id: parsed.license_id.unwrap_or_default(),
                issued_at_utc: Utc::now(),
                expires_at_utc: Utc::now(),
                grace_until_utc: Utc::now(),
                features_json: "{}".to_string(),
                signed_token: String::new(),
                install_id: None,
                bootstrap_secret: None,
                branding: None,
                constraints: None,
            });
        }

        let issued = parsed.issued_at_utc.unwrap_or_else(Utc::now);
        let expires = parsed.expires_at_utc.unwrap_or_else(Utc::now);
        let grace = parsed.grace_until_utc.unwrap_or_else(Utc::now);
        let now = Utc::now();
        let status = determine_status(now, expires, grace);

        let features_json = serde_json::to_string(&parsed.features.unwrap_or_default())
            .unwrap_or_else(|_| "{}".to_string());

        Ok(VerificationOutcome {
            is_valid: true,
            status,
            message: "License verified successfully".to_string(),
            client_name: parsed.client_name.unwrap_or_default(),
            license_id: parsed.license_id.unwrap_or_default(),
            issued_at_utc: issued,
            expires_at_utc: expires,
            grace_until_utc: grace,
            features_json,
            signed_token: signed,
            install_id: None,
            bootstrap_secret: None,
            branding: None,
            constraints: None,
        })
    };

    let retry_strategy = ExponentialBackoff::from_millis(150)
        .factor(2)
        .max_delay(Duration::from_secs(2))
        .take(3)
        .map(jitter);

    RetryIf::spawn(retry_strategy, attempt, |e: &anyhow::Error| {
        let msg = e.to_string().to_ascii_lowercase();
        msg.contains("timeout")
            || msg.contains("timed out")
            || msg.contains("network")
            || msg.contains("connection")
    })
    .await
}

fn hostname_best_effort() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

async fn verify_offline(
    license_key: &str,
    offline_bundle: &str,
) -> anyhow::Result<VerificationOutcome> {
    // Bundle string: {iv}:{ciphertext}:{tag}:{signature} (base64 parts), encrypted JSON.
    let bundle_bytes = base64::engine::general_purpose::STANDARD
        .decode(offline_bundle.as_bytes())
        .unwrap_or_else(|_| offline_bundle.as_bytes().to_vec());

    let bundle_str = String::from_utf8_lossy(&bundle_bytes).to_string();
    let parts: Vec<&str> = bundle_str.split(':').collect();
    if parts.len() != 4 {
        return Ok(VerificationOutcome {
            is_valid: false,
            status: "invalid".to_string(),
            message: "Invalid bundle format".to_string(),
            client_name: String::new(),
            license_id: String::new(),
            issued_at_utc: Utc::now(),
            expires_at_utc: Utc::now(),
            grace_until_utc: Utc::now(),
            features_json: "{}".to_string(),
            signed_token: String::new(),
            install_id: None,
            bootstrap_secret: None,
            branding: None,
            constraints: None,
        });
    }

    let iv = base64::engine::general_purpose::STANDARD.decode(parts[0])?;
    let ciphertext = base64::engine::general_purpose::STANDARD.decode(parts[1])?;
    let tag = base64::engine::general_purpose::STANDARD.decode(parts[2])?;
    let sig = base64::engine::general_purpose::STANDARD.decode(parts[3])?;

    let data_to_verify = format!("{}:{}:{}", parts[0], parts[1], parts[2]).into_bytes();

    // Embedded Ed25519 public key (from C# OfflineLicenseVerifier)
    let pub_key = base64::engine::general_purpose::STANDARD
        .decode("3q2+7w4AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=")?;
    let pk = UnparsedPublicKey::new(&signature::ED25519, pub_key);
    if pk.verify(&data_to_verify, &sig).is_err() {
        return Ok(VerificationOutcome {
            is_valid: false,
            status: "invalid".to_string(),
            message: "Bundle signature verification failed - bundle may be tampered".to_string(),
            client_name: String::new(),
            license_id: String::new(),
            issued_at_utc: Utc::now(),
            expires_at_utc: Utc::now(),
            grace_until_utc: Utc::now(),
            features_json: "{}".to_string(),
            signed_token: String::new(),
            install_id: None,
            bootstrap_secret: None,
            branding: None,
            constraints: None,
        });
    }

    // Derive AES-256 key from license key + iv salt (PBKDF2-SHA256, 100k)
    let mut key = [0u8; 32];
    let iterations = NonZeroU32::new(100_000).ok_or_else(|| {
        anyhow::anyhow!("Internal error: PBKDF2 iteration count must be non-zero")
    })?;
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        iterations,
        &iv,
        license_key.as_bytes(),
        &mut key,
    );

    if iv.len() != 12 {
        return Ok(VerificationOutcome {
            is_valid: false,
            status: "invalid".to_string(),
            message: "Invalid IV length in bundle".to_string(),
            client_name: String::new(),
            license_id: String::new(),
            issued_at_utc: Utc::now(),
            expires_at_utc: Utc::now(),
            grace_until_utc: Utc::now(),
            features_json: "{}".to_string(),
            signed_token: String::new(),
            install_id: None,
            bootstrap_secret: None,
            branding: None,
            constraints: None,
        });
    }

    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|_| anyhow::anyhow!("Internal error: invalid AES-256 key length"))?;
    let nonce = Nonce::from_slice(&iv);

    // aes-gcm expects ciphertext||tag
    let mut ct = ciphertext;
    ct.extend_from_slice(&tag);
    let plaintext = cipher
        .decrypt(nonce, ct.as_ref())
        .map_err(|_| anyhow::anyhow!("Failed to decrypt bundle"))?;

    #[derive(Debug, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[allow(dead_code)]
    struct OfflineBundle {
        license_id: String,
        client_name: String,
        install_id: String,
        issued_at_utc: DateTime<Utc>,
        expires_at_utc: DateTime<Utc>,
        grace_until_utc: DateTime<Utc>,
        features: Option<HashMap<String, serde_json::Value>>,
        #[serde(default)]
        #[allow(dead_code)]
        allowed_deployment_modes: Vec<String>,
        #[serde(default)]
        #[allow(dead_code)]
        allowed_base_url_strategies: Vec<String>,
        #[allow(dead_code)]
        public_notes: Option<String>,
        #[serde(default)]
        #[allow(dead_code)]
        bundle_version: String,
        #[allow(dead_code)]
        signature: Option<String>,
        signed_token: Option<String>,
        bootstrap_secret: Option<String>,
        branding: Option<BundleBranding>,
        constraints: Option<BundleConstraints>,
    }

    let bundle: OfflineBundle = serde_json::from_slice(&plaintext)?;
    if bundle
        .signed_token
        .as_ref()
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
    {
        return Ok(VerificationOutcome {
            is_valid: false,
            status: "invalid".to_string(),
            message: "SECURITY GAP: Offline bundle must contain SignedToken field.".to_string(),
            client_name: bundle.client_name,
            license_id: bundle.license_id,
            issued_at_utc: bundle.issued_at_utc,
            expires_at_utc: bundle.expires_at_utc,
            grace_until_utc: bundle.grace_until_utc,
            features_json: "{}".to_string(),
            signed_token: String::new(),
            install_id: Some(bundle.install_id),
            bootstrap_secret: bundle.bootstrap_secret,
            branding: None,
            constraints: None,
        });
    }

    if bundle.install_id.trim().is_empty() {
        return Ok(VerificationOutcome {
            is_valid: false,
            status: "invalid".to_string(),
            message: "Offline bundle must contain InstallId field.".to_string(),
            client_name: bundle.client_name,
            license_id: bundle.license_id,
            issued_at_utc: bundle.issued_at_utc,
            expires_at_utc: bundle.expires_at_utc,
            grace_until_utc: bundle.grace_until_utc,
            features_json: "{}".to_string(),
            signed_token: String::new(),
            install_id: None,
            bootstrap_secret: bundle.bootstrap_secret,
            branding: None,
            constraints: None,
        });
    }

    let now = Utc::now();
    if now < bundle.issued_at_utc {
        return Ok(VerificationOutcome {
            is_valid: false,
            status: "invalid".to_string(),
            message: "License not yet valid (issued in future)".to_string(),
            client_name: bundle.client_name,
            license_id: bundle.license_id,
            issued_at_utc: bundle.issued_at_utc,
            expires_at_utc: bundle.expires_at_utc,
            grace_until_utc: bundle.grace_until_utc,
            features_json: "{}".to_string(),
            signed_token: bundle.signed_token.unwrap_or_default(),
            install_id: Some(bundle.install_id),
            bootstrap_secret: bundle.bootstrap_secret,
            branding: None,
            constraints: None,
        });
    }

    let status = determine_status(now, bundle.expires_at_utc, bundle.grace_until_utc);
    let features_json = serde_json::to_string(&bundle.features.unwrap_or_default())
        .unwrap_or_else(|_| "{}".to_string());

    Ok(VerificationOutcome {
        is_valid: true,
        status,
        message: "License verified successfully".to_string(),
        client_name: bundle.client_name,
        license_id: bundle.license_id,
        issued_at_utc: bundle.issued_at_utc,
        expires_at_utc: bundle.expires_at_utc,
        grace_until_utc: bundle.grace_until_utc,
        features_json,
        signed_token: bundle.signed_token.unwrap_or_default(),
        install_id: Some(bundle.install_id),
        bootstrap_secret: bundle.bootstrap_secret,
        branding: bundle.branding,
        constraints: bundle.constraints,
    })
}

fn determine_status(now: DateTime<Utc>, expires: DateTime<Utc>, grace: DateTime<Utc>) -> String {
    if now <= expires {
        "active".to_string()
    } else if now <= grace {
        "grace".to_string()
    } else {
        "expired".to_string()
    }
}

fn mask_license_key(license_key: &str) -> String {
    let parts: Vec<&str> = license_key.split('-').collect();
    if parts.len() != 4 {
        return "****-****-****-****".to_string();
    }
    format!("{}-****-****-{}", parts[0], parts[3])
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

async fn best_effort_log_event(
    app_state: &AppState,
    secrets: &Arc<SecretProtector>,
    event: &str,
    description: &str,
) {
    if let Some((engine, _ver, cs)) = app_state.get_config_db().await {
        if let Ok(conn) = connect_with_retry(&engine, &cs).await {
            let platform_db = PlatformDbAdapter::new(conn, Arc::clone(secrets));
            let _ = platform_db
                .log_setup_event(event, description, Some("installer"), None)
                .await;
        }
    }
}
