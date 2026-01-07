use regex::Regex;
use log::warn;

pub fn validate_connection_string(connection_string: &str, engine: &str) -> Result<(), String> {
    match engine {
        "sqlserver" => validate_sql_server_connection_string(connection_string),
        "postgres" => validate_postgres_connection_string(connection_string),
        _ => Err(format!("Unknown database engine: {}", engine)),
    }
}

fn validate_sql_server_connection_string(conn_str: &str) -> Result<(), String> {
    // Basic validation for SQL Server connection string
    // Format: Server=server;Database=db;User Id=user;Password=pass;
    if conn_str.is_empty() {
        return Err("Connection string cannot be empty".to_string());
    }
    
    // Check for dangerous characters that could lead to injection
    if conn_str.contains("--") || conn_str.contains("/*") || conn_str.contains("*/") {
        return Err("Connection string contains potentially dangerous characters".to_string());
    }
    
    // Basic format check
    if !conn_str.contains("Server=") && !conn_str.contains("server=") {
        warn!("[VALIDATION] SQL Server connection string may be missing Server parameter");
    }
    
    Ok(())
}

fn validate_postgres_connection_string(conn_str: &str) -> Result<(), String> {
    // Basic validation for PostgreSQL connection string
    // Format: postgresql://user:pass@server:port/dbname
    if conn_str.is_empty() {
        return Err("Connection string cannot be empty".to_string());
    }
    
    // Check for dangerous characters
    if conn_str.contains("--") || conn_str.contains("/*") || conn_str.contains("*/") {
        return Err("Connection string contains potentially dangerous characters".to_string());
    }
    
    // Basic format check
    if !conn_str.starts_with("postgresql://") && !conn_str.starts_with("postgres://") {
        warn!("[VALIDATION] PostgreSQL connection string may be in wrong format");
    }
    
    Ok(())
}

pub fn validate_database_name(name: &str, engine: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Database name cannot be empty".to_string());
    }
    
    match engine {
        "sqlserver" => {
            // SQL Server naming rules
            if name.len() > 128 {
                return Err("Database name exceeds 128 characters".to_string());
            }
            
            // Check for reserved words (basic check)
            let reserved = ["master", "tempdb", "model", "msdb"];
            if reserved.contains(&name.to_lowercase().as_str()) {
                return Err(format!("Database name '{}' is reserved", name));
            }
        }
        "postgres" => {
            // PostgreSQL naming rules
            if name.len() > 63 {
                return Err("Database name exceeds 63 characters".to_string());
            }
        }
        _ => {}
    }
    
    Ok(())
}

pub fn mask_connection_string(conn_str: &str) -> String {
    // Mask passwords in connection strings for logging
    let mut masked = conn_str.to_string();
    
    // Mask SQL Server passwords
    let sql_server_pattern = Regex::new(r"(?i)(Password|Pwd)=[^;]+").unwrap();
    masked = sql_server_pattern.replace_all(&masked, "Password=***").to_string();
    
    // Mask PostgreSQL passwords
    let postgres_pattern = Regex::new(r"://[^:]+:[^@]+@").unwrap();
    masked = postgres_pattern.replace_all(&masked, "://***:***@").to_string();
    
    masked
}

