// License token verification (JWT RS256)
// Ported from C# `Cadalytix.Core.Licensing.LicenseTokenVerifier`.
//
// SECURITY:
// - FAIL-CLOSED: if signature validation fails, return None.
// - Enforce issuer + audience.
// - Enforce RS256 algorithm.
// - Do not rely on cached DB columns for expiry/features; use the signed token payload.

use std::collections::HashMap;

use chrono::{DateTime, Duration, TimeZone, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use log::{debug, warn};
use serde_json::Value;

/// Embedded RSA public key for verifying the signed license token (JWT RS256).
/// Matches the C# reference implementation.
const OPS_PUBLIC_KEY_PEM: &str = r#"-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAv9ZZD+JPTfK7HrA9HyPV
pGvsKINOJ0oM97P9DSgyu9itWMlmJIktrC0Nr1SCtvZRkMtmwANOf038/exk76MP
rJ0l78NIxAGDpAcGOLnHk/f1oB7B6yaWoErfhN5BxNUMaEiX1x3hFxiWf+RxlIUB
q9DAV7ZdM0Tl2a1LNl3qnjcbozpUtf3jUDcMOWnMr7iYA50QPmxJiDZ4DaKV3bKD
gtjYa5gXe/4aUEiAkOmELcwWAMrU3F+bSNLGVtmbgacAO8w9ygLhqdtf7qK1U3YR
/GxSjTXu1zhjUWJtMWgtz/BqpZfT5ZaOjntFF0EoOSaKnjZbKCODAc45nFyBamcv
hQIDAQAB
-----END PUBLIC KEY-----"#;

#[derive(Debug, Clone)]
pub struct VerifiedLicensePayload {
    #[allow(dead_code)]
    pub license_id: Option<String>,
    #[allow(dead_code)]
    pub client_name: Option<String>,
    #[allow(dead_code)]
    pub client_id: Option<String>, // JWT "sub"
    pub install_id: Option<String>, // JWT "cadalytix_install_id"
    #[allow(dead_code)]
    pub issued_at_utc: DateTime<Utc>,
    pub expires_at_utc: DateTime<Utc>,
    pub grace_until_utc: DateTime<Utc>,
    pub features: HashMap<String, bool>,
}

/// Verify and parse a signed license token (JWT RS256).
/// Returns None on any validation error (fail-closed).
pub fn verify_and_parse(signed_token: Option<&str>) -> Option<VerifiedLicensePayload> {
    let Some(token) = signed_token.map(str::trim).filter(|s| !s.is_empty()) else {
        warn!("[PHASE: license_verification] [STEP: token_verify] Signed token is null/empty");
        return None;
    };

    let header = match jsonwebtoken::decode_header(token) {
        Ok(h) => h,
        Err(e) => {
            warn!(
                "[PHASE: license_verification] [STEP: token_verify] Failed to decode JWT header: {}",
                e
            );
            return None;
        }
    };

    if header.alg != Algorithm::RS256 {
        warn!(
            "[PHASE: license_verification] [STEP: token_verify] Token algorithm is not RS256: {:?}",
            header.alg
        );
        return None;
    }

    let key = match DecodingKey::from_rsa_pem(OPS_PUBLIC_KEY_PEM.as_bytes()) {
        Ok(k) => k,
        Err(e) => {
            warn!(
                "[PHASE: license_verification] [STEP: token_verify] Failed to load embedded RSA public key: {}",
                e
            );
            return None;
        }
    };

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&["https://ops.cadalytix.com"]);
    validation.set_audience(&["cadalytix-client"]);
    // We support grace periods: don't hard-fail on exp/nbf at the library layer.
    validation.validate_exp = false;
    validation.validate_nbf = false;

    let data = match jsonwebtoken::decode::<Value>(token, &key, &validation) {
        Ok(d) => d,
        Err(e) => {
            warn!(
                "[PHASE: license_verification] [STEP: token_verify] Signature verification / decode failed: {}",
                e
            );
            return None;
        }
    };

    let claims = data.claims;

    // Manual time sanity checks (similar to C#):
    // - exp is required
    // - iat/nbf cannot be in the far future (beyond 5m skew)
    let now = Utc::now();
    let skew = Duration::minutes(5);

    let exp = match parse_numeric_timestamp(claims.get("exp")) {
        Some(dt) => dt,
        None => {
            warn!(
                "[PHASE: license_verification] [STEP: token_verify] Token missing/invalid exp claim"
            );
            return None;
        }
    };

    if let Some(iat) = parse_numeric_timestamp(claims.get("iat")) {
        if iat > now + skew {
            warn!(
                "[PHASE: license_verification] [STEP: token_verify] Token iat is in the future: {}",
                iat
            );
            return None;
        }
    }

    if let Some(nbf) = parse_numeric_timestamp(claims.get("nbf")) {
        if nbf > now + skew {
            warn!(
                "[PHASE: license_verification] [STEP: token_verify] Token nbf is in the future: {}",
                nbf
            );
            return None;
        }
    }

    // Identity claims
    let client_id = claims
        .get("sub")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let install_id = claims
        .get("cadalytix_install_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Custom claims (with backward compatibility)
    let license_id = claims
        .get("jti")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            claims
                .get("lic")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });

    let client_name = claims
        .get("client")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let grace_until_utc = claims
        .get("cadalytix_grace_until_utc")
        .or_else(|| claims.get("grace"))
        .and_then(parse_timestamp_claim)
        .unwrap_or(DateTime::<Utc>::MIN_UTC);

    let features = claims
        .get("cadalytix_features")
        .or_else(|| claims.get("features"))
        .map(parse_features_claim)
        .unwrap_or_default();

    // issued_at_utc: prefer iat, else min (matches C# behavior of allowing missing iat)
    let issued_at_utc =
        parse_numeric_timestamp(claims.get("iat")).unwrap_or(DateTime::<Utc>::MIN_UTC);

    debug!(
        "[PHASE: license_verification] [STEP: token_verify] Token verified (license_id={:?}, client_id={:?})",
        license_id, client_id
    );

    Some(VerifiedLicensePayload {
        license_id,
        client_name,
        client_id,
        install_id,
        issued_at_utc,
        expires_at_utc: exp,
        grace_until_utc,
        features,
    })
}

fn parse_numeric_timestamp(v: Option<&Value>) -> Option<DateTime<Utc>> {
    let secs = v.and_then(|x| {
        x.as_i64()
            .or_else(|| x.as_u64().map(|u| u as i64))
            .or_else(|| x.as_str()?.parse::<i64>().ok())
    });
    let secs = secs?;
    Utc.timestamp_opt(secs, 0).single()
}

fn parse_timestamp_claim(v: &Value) -> Option<DateTime<Utc>> {
    match v {
        Value::Number(n) => n
            .as_i64()
            .and_then(|secs| Utc.timestamp_opt(secs, 0).single()),
        Value::String(s) => {
            // unix seconds or ISO/RFC3339
            if let Ok(secs) = s.parse::<i64>() {
                return Utc.timestamp_opt(secs, 0).single();
            }
            chrono::DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
                .or_else(|| {
                    chrono::DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f%#z")
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                })
        }
        _ => None,
    }
}

fn parse_features_claim(v: &Value) -> HashMap<String, bool> {
    match v {
        Value::Object(map) => map
            .iter()
            .map(|(k, val)| {
                let enabled = match val {
                    Value::Bool(b) => *b,
                    Value::Null => false,
                    _ => true,
                };
                (k.clone(), enabled)
            })
            .collect(),
        Value::Array(arr) => {
            let mut out = HashMap::new();
            for item in arr {
                if let Some(s) = item.as_str() {
                    out.insert(s.to_string(), true);
                }
            }
            out
        }
        Value::String(s) => {
            // Some issuers embed features as a JSON string.
            match serde_json::from_str::<Value>(s) {
                Ok(parsed) => parse_features_claim(&parsed),
                Err(_) => HashMap::new(),
            }
        }
        _ => HashMap::new(),
    }
}

/// Determine license status string ("active" | "grace" | "expired") from authoritative token times.
pub fn determine_status(
    now: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    grace_until: DateTime<Utc>,
) -> String {
    if now <= expires_at {
        "active".to_string()
    } else if now <= grace_until {
        "grace".to_string()
    } else {
        "expired".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_and_parse_rejects_empty() {
        assert!(verify_and_parse(None).is_none());
        assert!(verify_and_parse(Some("")).is_none());
        assert!(verify_and_parse(Some("   ")).is_none());
    }

    #[test]
    fn parse_features_object_and_array() {
        let obj = serde_json::json!({"a": true, "b": false, "c": 1});
        let m = parse_features_claim(&obj);
        assert_eq!(m.get("a"), Some(&true));
        assert_eq!(m.get("b"), Some(&false));
        assert_eq!(m.get("c"), Some(&true));

        let arr = serde_json::json!(["x", "y"]);
        let m = parse_features_claim(&arr);
        assert_eq!(m.get("x"), Some(&true));
        assert_eq!(m.get("y"), Some(&true));
    }

    #[test]
    fn determine_status_matches_expected() {
        let now = Utc::now();
        assert_eq!(
            determine_status(now, now + Duration::hours(1), now + Duration::days(1)),
            "active"
        );
        assert_eq!(
            determine_status(now, now - Duration::hours(1), now + Duration::hours(1)),
            "grace"
        );
        assert_eq!(
            determine_status(now, now - Duration::days(2), now - Duration::days(1)),
            "expired"
        );
    }
}
