// Logging utilities
// Structured logging with JSON and human-readable formats

use log::Level;
use serde_json::json;
use std::collections::HashMap;

/// Mask sensitive data in logs
pub fn mask_sensitive(input: &str) -> String {
    if input.len() <= 8 {
        return "***".to_string();
    }

    let visible = 4;
    let start = &input[..visible.min(input.len())];
    let end = &input[input.len().saturating_sub(visible)..];

    format!("{}...{}", start, end)
}

/// Mask connection string
pub fn mask_connection_string(conn_str: &str) -> String {
    let s = conn_str.trim();
    if s.is_empty() {
        return String::new();
    }

    // Handle Postgres URL-style connection strings:
    //   postgresql://user:pass@host:port/db?sslmode=require
    // Mask only credentials; keep host/db visible for troubleshooting.
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("postgres://") || lower.starts_with("postgresql://") {
        if let Some(masked) = mask_url_userinfo_password(s) {
            return masked;
        }
        // If parsing fails, fall back to a fully-masked placeholder rather than leaking secrets.
        return "***".to_string();
    }

    // Default: semicolon-separated key/value connection strings (SQL Server + some Postgres formats).
    let mut out_parts: Vec<String> = Vec::new();
    for part in s.split(';') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        out_parts.push(mask_kv_part(p));
    }
    out_parts.join(";")
}

fn mask_kv_part(part: &str) -> String {
    let Some((k, v)) = part.split_once('=') else {
        return part.to_string();
    };
    let key = k.trim();
    let val = v.trim();

    let norm_key = key.to_ascii_lowercase().replace([' ', '_'], "");

    if norm_key == "password" || norm_key == "pwd" {
        return format!("{}=***", key);
    }

    if norm_key == "userid" || norm_key == "user" || norm_key == "username" || norm_key == "uid" {
        return format!("{}={}", key, mask_sensitive(val));
    }

    part.to_string()
}

fn mask_url_userinfo_password(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    let scheme = &url[..scheme_end];
    let after_scheme = &url[scheme_end + 3..];

    let (userinfo, rest) = match after_scheme.split_once('@') {
        Some((u, r)) => (u, r),
        None => return Some(url.to_string()),
    };
    if userinfo.trim().is_empty() {
        return Some(url.to_string());
    }

    // userinfo is typically "user:pass" (password may contain ':'; split once).
    let (user, pass_opt) = match userinfo.split_once(':') {
        Some((u, p)) => (u, Some(p)),
        None => (userinfo, None),
    };

    let masked_user = if user.trim().is_empty() {
        user.to_string()
    } else {
        mask_sensitive(user)
    };

    let rebuilt = match pass_opt {
        Some(_pass) => format!("{scheme}://{masked_user}:***@{rest}"),
        None => format!("{scheme}://{masked_user}@{rest}"),
    };
    Some(rebuilt)
}

/// Parse phase and step from log message
/// Extracts [PHASE: ...] and [STEP: ...] patterns
pub fn parse_log_metadata(message: &str) -> (Option<String>, Option<String>, String) {
    let mut phase = None;
    let mut step = None;
    let mut cleaned_message = message.to_string();

    // Extract [PHASE: ...]
    if let Some(start) = message.find("[PHASE:") {
        if let Some(end) = message[start..].find(']') {
            let phase_str = &message[start + 7..start + end].trim();
            phase = Some(phase_str.to_string());
            cleaned_message = format!("{} {}", &message[..start], &message[start + end + 1..])
                .trim()
                .to_string();
        }
    }

    // Extract [STEP: ...]
    if let Some(start) = cleaned_message.find("[STEP:") {
        if let Some(end) = cleaned_message[start..].find(']') {
            let step_str = &cleaned_message[start + 6..start + end].trim();
            step = Some(step_str.to_string());
            cleaned_message = format!(
                "{} {}",
                &cleaned_message[..start],
                &cleaned_message[start + end + 1..]
            )
            .trim()
            .to_string();
        }
    }

    (phase, step, cleaned_message)
}

/// Format log entry as JSON for structured logging
#[allow(clippy::too_many_arguments)]
pub fn format_json_log(
    timestamp: &str,
    level: Level,
    target: &str,
    message: &str,
    phase: Option<&str>,
    step: Option<&str>,
    details: Option<&HashMap<String, serde_json::Value>>,
    context: Option<&HashMap<String, serde_json::Value>>,
    performance: Option<&HashMap<String, serde_json::Value>>,
) -> String {
    let mut log_entry = json!({
        "timestamp": timestamp,
        "level": level.as_str(),
        "target": target,
        "message": message,
    });

    if let Some(phase) = phase {
        log_entry["phase"] = json!(phase);
    }

    if let Some(step) = step {
        log_entry["step"] = json!(step);
    }

    if let Some(details) = details {
        log_entry["details"] = json!(details);
    }

    if let Some(context) = context {
        log_entry["context"] = json!(context);
    }

    if let Some(performance) = performance {
        log_entry["performance"] = json!(performance);
    }

    serde_json::to_string(&log_entry).unwrap_or_else(|_| "{}".to_string())
}

/// Format log entry as human-readable text
pub fn format_human_readable_log(
    timestamp: &str,
    level: Level,
    target: &str,
    message: &str,
    phase: Option<&str>,
    step: Option<&str>,
) -> String {
    let mut log_line = format!("[{}] [{}]", timestamp, level.as_str());

    if let Some(phase) = phase {
        log_line.push_str(&format!(" [PHASE: {}]", phase));
    }

    if let Some(step) = step {
        log_line.push_str(&format!(" [STEP: {}]", step));
    }

    log_line.push_str(&format!(" [{}] {}", target, message));
    log_line
}

// =============================================================================
// Phase 6 Unit Tests: Secret Masking (prevents regression of D2/D5 contract)
// =============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // A) Secret masking - connection strings (lock down "no secrets leak" rule)
    // -------------------------------------------------------------------------

    #[test]
    fn mask_connection_string_sql_server_masks_password() {
        let conn = "Server=localhost,1433;Database=cadalytix;User Id=sa;Password=PASSWORD_SHOULD_BE_REDACTED;";
        let masked = mask_connection_string(conn);

        // Password MUST be replaced with ***
        assert!(
            masked.contains("Password=***"),
            "Password should be masked: {}",
            masked
        );
        // Raw password MUST NOT appear
        assert!(
            !masked.contains("SuperSecret123"),
            "Raw password leaked: {}",
            masked
        );
        // Host/DB should remain visible for troubleshooting
        assert!(
            masked.contains("Server=localhost"),
            "Server should be visible: {}",
            masked
        );
        assert!(
            masked.contains("Database=cadalytix"),
            "Database should be visible: {}",
            masked
        );
    }

    #[test]
    fn mask_connection_string_sql_server_masks_pwd_shorthand() {
        let conn = "Server=myserver;Database=mydb;Uid=myuser;Pwd=PASSWORD_SHOULD_BE_REDACTED;";
        let masked = mask_connection_string(conn);

        assert!(
            masked.contains("Pwd=***"),
            "Pwd should be masked: {}",
            masked
        );
        assert!(
            !masked.contains("MyP@ssw0rd!"),
            "Raw password leaked: {}",
            masked
        );
    }

    #[test]
    fn mask_connection_string_postgres_url_masks_password() {
        let conn = "postgresql://admin:secretpassword@localhost:5432/cadalytix?sslmode=require";
        let masked = mask_connection_string(conn);

        // Password MUST be replaced with ***
        assert!(
            masked.contains(":***@"),
            "Password should be masked in URL: {}",
            masked
        );
        // Raw password MUST NOT appear
        assert!(
            !masked.contains("secretpassword"),
            "Raw password leaked: {}",
            masked
        );
        // Host/DB should remain visible
        assert!(
            masked.contains("localhost:5432"),
            "Host should be visible: {}",
            masked
        );
        assert!(
            masked.contains("/cadalytix"),
            "Database should be visible: {}",
            masked
        );
    }

    #[test]
    fn mask_connection_string_handles_empty() {
        assert_eq!(mask_connection_string(""), "");
        assert_eq!(mask_connection_string("   "), "");
    }

    #[test]
    fn mask_connection_string_no_credentials_unchanged() {
        let conn = "Server=localhost;Database=test;Integrated Security=true;";
        let masked = mask_connection_string(conn);
        // Should remain unchanged (no password to mask)
        assert!(!masked.contains("***"), "No masking needed: {}", masked);
    }

    #[test]
    fn mask_sensitive_short_values_fully_masked() {
        assert_eq!(mask_sensitive("abc"), "***");
        assert_eq!(mask_sensitive("12345678"), "***");
    }

    #[test]
    fn mask_sensitive_long_values_partially_masked() {
        let masked = mask_sensitive("abcdefghijklmnop");
        assert!(
            masked.contains("..."),
            "Long value should be partially masked: {}",
            masked
        );
        // First 4 and last 4 chars should be visible
        assert!(
            masked.starts_with("abcd"),
            "Start should be visible: {}",
            masked
        );
        assert!(
            masked.ends_with("mnop"),
            "End should be visible: {}",
            masked
        );
    }

    // -------------------------------------------------------------------------
    // B) Comprehensive secret pattern tests (prevent regression)
    // -------------------------------------------------------------------------

    #[test]
    fn mask_connection_string_never_leaks_common_patterns() {
        // List of connection strings with various password patterns
        let test_cases = vec![
            ("Password=secret123", "Password=***"),
            ("Pwd=secret123", "Pwd=***"),
            ("password=secret123", "password=***"),
            ("PWD=secret123", "PWD=***"),
        ];

        for (input, expected_pattern) in test_cases {
            let masked = mask_connection_string(input);
            assert!(
                masked.contains(expected_pattern),
                "Input '{}' should produce '{}', got '{}'",
                input,
                expected_pattern,
                masked
            );
            assert!(
                !masked.contains("secret123"),
                "Input '{}' leaked raw password in '{}'",
                input,
                masked
            );
        }
    }

    #[test]
    fn mask_connection_string_user_id_is_partially_visible() {
        // User ID should be partially masked (first/last 4 chars) for troubleshooting
        let conn = "User Id=administrator;Password=secret;";
        let masked = mask_connection_string(conn);

        // Password must be fully masked
        assert!(
            masked.contains("Password=***"),
            "Password should be masked: {}",
            masked
        );
        // User should be partially visible (not fully hidden)
        // Administrator -> "admi...ator" or similar
        assert!(
            !masked.contains("administrator"),
            "Full user leaked: {}",
            masked
        );
    }

    // -------------------------------------------------------------------------
    // C) Edge cases
    // -------------------------------------------------------------------------

    #[test]
    fn mask_connection_string_password_with_special_chars() {
        let conn = "Server=s;Password=P@ss=w;ord!;Database=d;";
        let masked = mask_connection_string(conn);

        // Should handle = in password value
        assert!(
            masked.contains("Password=***"),
            "Password should be masked: {}",
            masked
        );
        // Raw password fragments should not appear
        assert!(
            !masked.contains("P@ss"),
            "Raw password fragment leaked: {}",
            masked
        );
    }

    #[test]
    fn mask_postgres_url_no_password() {
        // URL with user but no password
        let conn = "postgresql://admin@localhost:5432/db";
        let masked = mask_connection_string(conn);

        // User should be masked but no :*** needed
        assert!(!masked.contains(":***@"), "No password to mask: {}", masked);
        assert!(
            masked.contains("@localhost"),
            "Host should be visible: {}",
            masked
        );
    }
}
