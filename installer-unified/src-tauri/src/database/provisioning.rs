// Database provisioning module for Phase 9: Create NEW Database
//
// Supports:
// - SQL Server: CREATE DATABASE with optional sizing (SIZE/MAXSIZE/FILEGROWTH via ALTER)
// - PostgreSQL: CREATE DATABASE with optional OWNER (no sizing knobs - PostgreSQL grows with disk)
//
// Key design decisions:
// - SQL Server sizing is applied via ALTER DATABASE MODIFY FILE after creation
// - PostgreSQL does NOT support SQL-Server-style sizing; we do not fake it
// - All queries use parameterized inputs where possible, bracket-quoted identifiers for safety
// - Privilege checks use IS_SRVROLEMEMBER / HAS_PERMS_BY_NAME (SQL Server) or pg_roles (Postgres)

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};

// =============================================================================
// Types
// =============================================================================

/// SQL Server sizing configuration (all optional; defaults = server defaults)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlServerSizingConfig {
    /// Initial data file size in MB (0 = server default)
    pub initial_data_size_mb: u32,
    /// Initial log file size in MB (0 = server default)
    pub initial_log_size_mb: u32,
    /// Max data file size in MB (0 = UNLIMITED)
    pub max_data_size_mb: u32,
    /// Max log file size in MB (0 = UNLIMITED)
    pub max_log_size_mb: u32,
    /// Data file growth: positive = MB, negative = percent (e.g., -10 = 10%)
    pub data_filegrowth: i32,
    /// Log file growth: positive = MB, negative = percent
    pub log_filegrowth: i32,
}

/// PostgreSQL creation options
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostgresCreateOptions {
    /// Optional owner role name (if empty, defaults to current role)
    pub owner: Option<String>,
}

/// Result of privilege check
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanCreateDatabaseResult {
    pub can_create: bool,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_role: Option<String>,
}

/// Result of database existence check
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseExistsResult {
    pub exists: bool,
    pub db_name: String,
}

/// Result of database creation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDatabaseResult {
    pub created: bool,
    pub db_name: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sizing_applied: Option<bool>,
}

// =============================================================================
// Validation
// =============================================================================

/// Validate database name (letters, numbers, underscore only; 1-128 chars)
pub fn validate_db_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Database name is required.".to_string());
    }
    if name.len() > 128 {
        return Err("Database name must be 128 characters or fewer.".to_string());
    }
    // Allow letters, numbers, underscore (conservative for cross-platform)
    let re = Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap();
    if !re.is_match(name) {
        return Err(
            "Database name must start with a letter or underscore and contain only letters, numbers, and underscores.".to_string(),
        );
    }
    // Reserved words (basic check)
    let reserved = ["master", "tempdb", "model", "msdb", "postgres", "template0", "template1"];
    if reserved.iter().any(|r| r.eq_ignore_ascii_case(name)) {
        return Err(format!("'{}' is a reserved database name.", name));
    }
    Ok(())
}

/// Validate sizing config (SQL Server)
pub fn validate_sizing_config(cfg: &SqlServerSizingConfig) -> Result<(), String> {
    // Max must be >= initial if both are set
    if cfg.max_data_size_mb > 0 && cfg.initial_data_size_mb > cfg.max_data_size_mb {
        return Err("Initial data size cannot exceed max data size.".to_string());
    }
    if cfg.max_log_size_mb > 0 && cfg.initial_log_size_mb > cfg.max_log_size_mb {
        return Err("Initial log size cannot exceed max log size.".to_string());
    }
    // Growth percent must be 1-100
    if cfg.data_filegrowth < 0 && (cfg.data_filegrowth < -100 || cfg.data_filegrowth > -1) {
        return Err("Data filegrowth percent must be between 1% and 100%.".to_string());
    }
    if cfg.log_filegrowth < 0 && (cfg.log_filegrowth < -100 || cfg.log_filegrowth > -1) {
        return Err("Log filegrowth percent must be between 1% and 100%.".to_string());
    }
    Ok(())
}

// =============================================================================
// SQL Server SQL Generation (safe, bracket-quoted)
// =============================================================================

/// Bracket-quote a SQL Server identifier
fn bracket_quote(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}

/// Generate CREATE DATABASE statement for SQL Server (no sizing - sizing applied via ALTER)
pub fn sql_server_create_db_stmt(db_name: &str) -> String {
    format!("CREATE DATABASE {};", bracket_quote(db_name))
}

/// Generate ALTER DATABASE MODIFY FILE statement for sizing
pub fn sql_server_alter_file_stmt(
    db_name: &str,
    logical_file_name: &str,
    size_mb: u32,
    max_size_mb: u32,
    filegrowth: i32,
) -> String {
    let mut parts = vec![format!("NAME = N'{}'", logical_file_name.replace('\'', "''"))];
    if size_mb > 0 {
        parts.push(format!("SIZE = {}MB", size_mb));
    }
    if filegrowth != 0 {
        if filegrowth > 0 {
            parts.push(format!("FILEGROWTH = {}MB", filegrowth));
        } else {
            parts.push(format!("FILEGROWTH = {}%", filegrowth.abs()));
        }
    }
    if max_size_mb > 0 {
        parts.push(format!("MAXSIZE = {}MB", max_size_mb));
    } else {
        parts.push("MAXSIZE = UNLIMITED".to_string());
    }
    format!(
        "ALTER DATABASE {} MODIFY FILE ( {} );",
        bracket_quote(db_name),
        parts.join(", ")
    )
}

/// SQL to check if current user can create databases (SQL Server)
pub fn sql_server_can_create_db_query() -> &'static str {
    r#"
    SELECT
        CASE
            WHEN IS_SRVROLEMEMBER('sysadmin') = 1 THEN 1
            WHEN IS_SRVROLEMEMBER('dbcreator') = 1 THEN 1
            WHEN HAS_PERMS_BY_NAME(NULL, NULL, 'CREATE ANY DATABASE') = 1 THEN 1
            ELSE 0
        END AS can_create,
        CASE
            WHEN IS_SRVROLEMEMBER('sysadmin') = 1 THEN 'sysadmin'
            WHEN IS_SRVROLEMEMBER('dbcreator') = 1 THEN 'dbcreator'
            WHEN HAS_PERMS_BY_NAME(NULL, NULL, 'CREATE ANY DATABASE') = 1 THEN 'CREATE ANY DATABASE'
            ELSE 'none'
        END AS detected_role
    "#
}

/// SQL to check if database exists (SQL Server)
pub fn sql_server_db_exists_query(db_name: &str) -> String {
    format!(
        "SELECT CASE WHEN DB_ID(N'{}') IS NOT NULL THEN 1 ELSE 0 END AS db_exists;",
        db_name.replace('\'', "''")
    )
}

/// SQL to get logical file names for a database (SQL Server)
pub fn sql_server_get_file_names_query(db_name: &str) -> String {
    format!(
        r#"
        SELECT name, type_desc
        FROM {}.sys.database_files
        WHERE type IN (0, 1)
        ORDER BY type;
        "#,
        bracket_quote(db_name)
    )
}

// =============================================================================
// PostgreSQL SQL Generation
// =============================================================================

/// Double-quote a PostgreSQL identifier
fn pg_quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Generate CREATE DATABASE statement for PostgreSQL
pub fn postgres_create_db_stmt(db_name: &str, owner: Option<&str>) -> String {
    let mut stmt = format!("CREATE DATABASE {};", pg_quote_ident(db_name));
    if let Some(o) = owner {
        if !o.trim().is_empty() {
            stmt = format!(
                "CREATE DATABASE {} OWNER {};",
                pg_quote_ident(db_name),
                pg_quote_ident(o)
            );
        }
    }
    stmt
}

/// SQL to check if current user can create databases (PostgreSQL)
pub fn postgres_can_create_db_query() -> &'static str {
    r#"
    SELECT
        rolcreatedb AS can_create,
        rolname AS detected_role
    FROM pg_roles
    WHERE rolname = current_user;
    "#
}

/// SQL to check if database exists (PostgreSQL)
pub fn postgres_db_exists_query(db_name: &str) -> String {
    format!(
        "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = '{}') AS db_exists;",
        db_name.replace('\'', "''")
    )
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_db_name_valid() {
        assert!(validate_db_name("MyDatabase").is_ok());
        assert!(validate_db_name("my_db_123").is_ok());
        assert!(validate_db_name("_private").is_ok());
        assert!(validate_db_name("A").is_ok());
    }

    #[test]
    fn test_validate_db_name_invalid() {
        assert!(validate_db_name("").is_err());
        assert!(validate_db_name("123abc").is_err()); // starts with number
        assert!(validate_db_name("my-db").is_err()); // hyphen
        assert!(validate_db_name("my db").is_err()); // space
        assert!(validate_db_name("master").is_err()); // reserved
        assert!(validate_db_name("postgres").is_err()); // reserved
    }

    #[test]
    fn test_validate_sizing_config_valid() {
        let cfg = SqlServerSizingConfig {
            initial_data_size_mb: 100,
            initial_log_size_mb: 50,
            max_data_size_mb: 1000,
            max_log_size_mb: 500,
            data_filegrowth: 64,
            log_filegrowth: -10, // 10%
        };
        assert!(validate_sizing_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_sizing_config_invalid() {
        let cfg = SqlServerSizingConfig {
            initial_data_size_mb: 1000,
            max_data_size_mb: 100, // max < initial
            ..Default::default()
        };
        assert!(validate_sizing_config(&cfg).is_err());
    }

    #[test]
    fn test_sql_server_create_db_stmt() {
        let stmt = sql_server_create_db_stmt("TestDB");
        assert_eq!(stmt, "CREATE DATABASE [TestDB];");
    }

    #[test]
    fn test_sql_server_create_db_stmt_injection() {
        // Bracket injection attempt
        let stmt = sql_server_create_db_stmt("Test]DB");
        assert_eq!(stmt, "CREATE DATABASE [Test]]DB];");
    }

    #[test]
    fn test_sql_server_alter_file_stmt() {
        let stmt = sql_server_alter_file_stmt("MyDB", "MyDB_Data", 100, 1000, 64);
        assert!(stmt.contains("ALTER DATABASE [MyDB]"));
        assert!(stmt.contains("SIZE = 100MB"));
        assert!(stmt.contains("MAXSIZE = 1000MB"));
        assert!(stmt.contains("FILEGROWTH = 64MB"));
    }

    #[test]
    fn test_sql_server_alter_file_stmt_percent_growth() {
        let stmt = sql_server_alter_file_stmt("MyDB", "MyDB_Log", 50, 0, -10);
        assert!(stmt.contains("FILEGROWTH = 10%"));
        assert!(stmt.contains("MAXSIZE = UNLIMITED"));
    }

    #[test]
    fn test_postgres_create_db_stmt() {
        let stmt = postgres_create_db_stmt("testdb", None);
        assert_eq!(stmt, "CREATE DATABASE \"testdb\";");
    }

    #[test]
    fn test_postgres_create_db_stmt_with_owner() {
        let stmt = postgres_create_db_stmt("testdb", Some("myuser"));
        assert_eq!(stmt, "CREATE DATABASE \"testdb\" OWNER \"myuser\";");
    }

    #[test]
    fn test_postgres_create_db_stmt_injection() {
        let stmt = postgres_create_db_stmt("test\"db", Some("my\"user"));
        assert_eq!(stmt, "CREATE DATABASE \"test\"\"db\" OWNER \"my\"\"user\";");
    }

    #[test]
    fn test_sql_server_db_exists_query() {
        let q = sql_server_db_exists_query("MyDB");
        assert!(q.contains("DB_ID(N'MyDB')"));
    }

    #[test]
    fn test_postgres_db_exists_query() {
        let q = postgres_db_exists_query("mydb");
        assert!(q.contains("datname = 'mydb'"));
    }

    // Phase 9: Additional tests for SQL generation edge cases

    #[test]
    fn test_validate_db_name_too_long() {
        let long_name = "a".repeat(129);
        assert!(validate_db_name(&long_name).is_err());
    }

    #[test]
    fn test_sql_server_get_file_names_query() {
        let q = sql_server_get_file_names_query("TestDB");
        assert!(q.contains("[TestDB]"));
        assert!(q.contains("sys.database_files"));
    }

    #[test]
    fn test_sql_server_alter_file_stmt_defaults() {
        // When all values are 0, should use defaults
        let stmt = sql_server_alter_file_stmt("MyDB", "MyDB_Data", 0, 0, 0);
        // Should still generate valid SQL even with defaults
        assert!(stmt.contains("ALTER DATABASE [MyDB]"));
    }

    #[test]
    fn test_bracket_quote_escaping() {
        // Test that bracket quoting properly escapes brackets
        let stmt = sql_server_create_db_stmt("Test[DB]Name");
        assert!(stmt.contains("[Test[DB]]Name]"));
    }

    #[test]
    fn test_double_quote_escaping() {
        // Test that double-quote escaping works for Postgres
        let stmt = postgres_create_db_stmt("test\"db\"name", None);
        assert!(stmt.contains("\"test\"\"db\"\"name\""));
    }
}

