// Input validation utilities

use anyhow::Result;
use regex::Regex;

/// Validate and safely quote a SQL Server multi-part object identifier (e.g. dbo.Table or schema.view).
///
/// Security: this is used to prevent SQL injection when we need to interpolate an identifier (not a value).
/// We intentionally only allow simple identifiers (letters/numbers/underscore) and dot-separated parts.
pub fn validate_and_quote_sql_server_object(object_name: &str) -> Result<String> {
    let s = object_name.trim();
    if s.is_empty() {
        return Err(anyhow::anyhow!("SourceObjectName is required"));
    }

    // Reject obvious injection / batch separators
    let lowered = s.to_ascii_lowercase();
    if lowered.contains(';')
        || lowered.contains("--")
        || lowered.contains("/*")
        || lowered.contains("*/")
    {
        return Err(anyhow::anyhow!(
            "SourceObjectName contains invalid characters"
        ));
    }

    // Allow up to 3 parts: [db].[schema].[object] or [schema].[object] or [object]
    let parts: Vec<&str> = s.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err(anyhow::anyhow!(
            "SourceObjectName must be one-, two-, or three-part name (e.g. dbo.Table)"
        ));
    }

    let ident_re = Regex::new(r"^[A-Za-z0-9_]+$").map_err(|e| {
        anyhow::anyhow!("Internal error: failed to compile identifier regex: {}", e)
    })?;
    let mut quoted_parts = Vec::new();

    for raw in parts {
        let p = raw.trim().trim_matches(['[', ']', '"', '\'']);
        if p.is_empty() {
            return Err(anyhow::anyhow!(
                "SourceObjectName contains an empty identifier part"
            ));
        }
        if !ident_re.is_match(p) {
            return Err(anyhow::anyhow!(
                "SourceObjectName contains invalid identifier: '{}'",
                p
            ));
        }
        quoted_parts.push(format!("[{}]", p));
    }

    Ok(quoted_parts.join("."))
}

/// Validate database name (SQL Server)
#[allow(dead_code)]
pub fn validate_sql_server_database_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(anyhow::anyhow!("Database name cannot be empty"));
    }

    if name.len() > 128 {
        return Err(anyhow::anyhow!(
            "Database name cannot exceed 128 characters"
        ));
    }

    // SQL Server naming rules
    if name.starts_with(' ') || name.ends_with(' ') {
        return Err(anyhow::anyhow!(
            "Database name cannot start or end with spaces"
        ));
    }

    // Check for invalid characters
    let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    if name.chars().any(|c| invalid_chars.contains(&c)) {
        return Err(anyhow::anyhow!("Database name contains invalid characters"));
    }

    Ok(())
}

/// Validate database name (PostgreSQL)
#[allow(dead_code)]
pub fn validate_postgres_database_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(anyhow::anyhow!("Database name cannot be empty"));
    }

    if name.len() > 63 {
        return Err(anyhow::anyhow!("Database name cannot exceed 63 characters"));
    }

    // PostgreSQL naming rules (can use quoted identifiers for special cases)
    // Basic validation: no null bytes
    if name.contains('\0') {
        return Err(anyhow::anyhow!("Database name cannot contain null bytes"));
    }

    Ok(())
}

/// Validate connection string format (basic)
pub fn validate_connection_string(conn_str: &str) -> Result<()> {
    if conn_str.is_empty() {
        return Err(anyhow::anyhow!("Connection string cannot be empty"));
    }

    // Basic validation - more specific validation in database module
    Ok(())
}
