// Schema verification
// Ported from C# schema verification logic
// Verifies database schema completeness and correctness

use anyhow::{Context, Result};
use futures::TryStreamExt;
use log::{info, warn};
use sqlx::{Pool, Postgres};
use std::collections::HashSet;
use tiberius::{Query, QueryItem};

use crate::database::connection::DatabaseConnection;

/// Schema verification result
#[derive(Debug, Clone)]
pub struct SchemaVerificationResult {
    pub valid: bool,
    pub missing_tables: Vec<String>,
    pub missing_columns: Vec<(String, String)>, // (table, column)
    #[allow(dead_code)]
    pub errors: Vec<String>,
}

/// Schema verifier for validating database schema
pub struct SchemaVerifier {
    connection: DatabaseConnection,
}

impl SchemaVerifier {
    /// Create a new schema verifier
    pub fn new(connection: DatabaseConnection) -> Self {
        info!("[PHASE: database] [STEP: schema_verifier_init] Creating schema verifier");
        SchemaVerifier { connection }
    }

    /// Verify the complete database schema
    /// Checks for required tables and columns
    pub async fn verify_schema(
        &self,
        expected_tables: &[String],
        expected_columns: &[(&str, &str)],
    ) -> Result<SchemaVerificationResult> {
        info!("[PHASE: database] [STEP: verify_schema] Starting schema verification");

        match &self.connection {
            DatabaseConnection::Postgres(pool) => {
                self.verify_schema_postgres(pool, expected_tables, expected_columns)
                    .await
            }
            DatabaseConnection::SqlServer(_) => {
                self.verify_schema_sql_server(expected_tables, expected_columns)
                    .await
            }
        }
    }

    /// Verify schema in PostgreSQL
    async fn verify_schema_postgres(
        &self,
        pool: &Pool<Postgres>,
        expected_tables: &[String],
        expected_columns: &[(&str, &str)],
    ) -> Result<SchemaVerificationResult> {
        // Get existing tables in cadalytix_config schema
        let tables: Vec<String> = sqlx::query_scalar::<_, String>(
            r#"
            SELECT tablename
            FROM pg_tables
            WHERE schemaname = 'cadalytix_config'
            "#,
        )
        .fetch_all(pool)
        .await
        .with_context(|| "Failed to query tables from PostgreSQL")?;

        let existing_tables: HashSet<String> = tables.into_iter().collect();
        let expected_tables_set: HashSet<String> = expected_tables.iter().cloned().collect();

        // Find missing tables
        let missing_tables: Vec<String> = expected_tables_set
            .difference(&existing_tables)
            .cloned()
            .collect();

        // Verify columns
        let mut missing_columns = Vec::new();
        for (table, column) in expected_columns {
            let exists: bool = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM information_schema.columns
                    WHERE table_schema = 'cadalytix_config'
                    AND table_name = $1
                    AND column_name = $2
                )
                "#,
            )
            .bind(table)
            .bind(column)
            .fetch_one(pool)
            .await
            .with_context(|| format!("Failed to verify column {}.{}", table, column))?;

            if !exists {
                missing_columns.push((table.to_string(), column.to_string()));
            }
        }

        let valid = missing_tables.is_empty() && missing_columns.is_empty();

        if valid {
            info!(
                "[PHASE: database] [STEP: verify_schema] Schema verification passed for PostgreSQL"
            );
        } else {
            warn!(
                "[PHASE: database] [STEP: verify_schema] Schema verification found issues: {} missing tables, {} missing columns",
                missing_tables.len(),
                missing_columns.len()
            );
        }

        Ok(SchemaVerificationResult {
            valid,
            missing_tables,
            missing_columns,
            errors: Vec::new(),
        })
    }

    /// Verify schema in SQL Server
    async fn verify_schema_sql_server(
        &self,
        expected_tables: &[String],
        expected_columns: &[(&str, &str)],
    ) -> Result<SchemaVerificationResult> {
        let client_arc = self
            .connection
            .as_sql_server()
            .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;

        let mut client = client_arc.lock().await;

        // Get existing tables in cadalytix_config schema
        let query_str = r#"
            SELECT TABLE_NAME
            FROM INFORMATION_SCHEMA.TABLES
            WHERE TABLE_SCHEMA = 'cadalytix_config'
        "#;

        // Scope the stream so we can reuse `client` for subsequent queries.
        let existing_tables: HashSet<String> = {
            let query = Query::new(query_str);
            let mut stream = query
                .query(&mut *client)
                .await
                .with_context(|| "Failed to query tables from SQL Server")?;

            let mut existing_tables = HashSet::new();
            while let Some(item) = stream
                .try_next()
                .await
                .with_context(|| "Failed to read from query result")?
            {
                if let QueryItem::Row(row) = item {
                    let table_name: String = row
                        .get::<&str, _>(0)
                        .ok_or_else(|| anyhow::anyhow!("TABLE_NAME is null"))?
                        .to_string();
                    existing_tables.insert(table_name);
                }
            }

            existing_tables
        };

        let expected_tables_set: HashSet<String> = expected_tables.iter().cloned().collect();

        // Find missing tables
        let missing_tables: Vec<String> = expected_tables_set
            .difference(&existing_tables)
            .cloned()
            .collect();

        // Verify columns
        let mut missing_columns = Vec::new();
        for (table, column) in expected_columns {
            let query_str = r#"
                SELECT COUNT(*)
                FROM INFORMATION_SCHEMA.COLUMNS
                WHERE TABLE_SCHEMA = 'cadalytix_config'
                AND TABLE_NAME = @P1
                AND COLUMN_NAME = @P2
            "#;

            let mut query = Query::new(query_str);
            query.bind(*table);
            query.bind(*column);

            let mut stream = query
                .query(&mut *client)
                .await
                .with_context(|| format!("Failed to verify column {}.{}", table, column))?;

            let mut exists = false;
            while let Some(item) = stream.try_next().await.with_context(|| {
                format!(
                    "Failed to read column verification result for {}.{}",
                    table, column
                )
            })? {
                if let QueryItem::Row(row) = item {
                    let count: i32 = row
                        .get::<i32, _>(0)
                        .ok_or_else(|| anyhow::anyhow!("COUNT result is null"))?;
                    exists = count > 0;
                    break;
                }
            }

            if !exists {
                missing_columns.push((table.to_string(), column.to_string()));
            }
        }

        let valid = missing_tables.is_empty() && missing_columns.is_empty();

        if valid {
            info!(
                "[PHASE: database] [STEP: verify_schema] Schema verification passed for SQL Server"
            );
        } else {
            warn!(
                "[PHASE: database] [STEP: verify_schema] Schema verification found issues: {} missing tables, {} missing columns",
                missing_tables.len(),
                missing_columns.len()
            );
        }

        Ok(SchemaVerificationResult {
            valid,
            missing_tables,
            missing_columns,
            errors: Vec::new(),
        })
    }

    /// Verify all schemas (convenience method)
    pub async fn verify_all_schemas(&self) -> Result<Vec<(String, SchemaVerificationResult)>> {
        info!("[PHASE: database] [STEP: verify_all_schemas] Starting verification of all schemas");

        // Define expected tables and columns for cadalytix_config schema.
        //
        // These are created by the versioned migration set (002/007/008/009 + 010/011 enhancements).
        let expected_tables = vec![
            "instance_settings".to_string(),
            "applied_migrations".to_string(),
            "wizard_checkpoints".to_string(),
            "license_state".to_string(),
            "setup_events".to_string(),
        ];

        let expected_columns = vec![
            // instance_settings (key/value)
            ("instance_settings", "key"),
            ("instance_settings", "value"),
            ("instance_settings", "updated_at"),
            // applied_migrations (enhanced by migration 010)
            ("applied_migrations", "migration_name"),
            ("applied_migrations", "applied_at"),
            ("applied_migrations", "checksum"),
            ("applied_migrations", "migration_group"),
            ("applied_migrations", "engine"),
            ("applied_migrations", "execution_time_ms"),
            ("applied_migrations", "applied_by"),
            // wizard_checkpoints
            ("wizard_checkpoints", "step_name"),
            ("wizard_checkpoints", "state_json"),
            ("wizard_checkpoints", "updated_at"),
            // license_state (011 adds signed_token_blob + anti-backdating columns)
            ("license_state", "id"),
            ("license_state", "mode"),
            ("license_state", "license_key_masked"),
            ("license_state", "license_key_hash"),
            ("license_state", "status"),
            ("license_state", "client_name"),
            ("license_state", "license_id"),
            ("license_state", "issued_at_utc"),
            ("license_state", "expires_at_utc"),
            ("license_state", "grace_until_utc"),
            ("license_state", "last_verified_at_utc"),
            ("license_state", "features_json"),
            ("license_state", "installation_token"),
            ("license_state", "signed_token_blob"),
            ("license_state", "last_seen_now_utc"),
            ("license_state", "last_seen_expires_utc"),
            ("license_state", "created_at"),
            ("license_state", "updated_at"),
            // setup_events
            ("setup_events", "id"),
            ("setup_events", "event_type"),
            ("setup_events", "description"),
            ("setup_events", "actor"),
            ("setup_events", "metadata"),
            ("setup_events", "occurred_at"),
        ];

        let result = self
            .verify_schema(&expected_tables, &expected_columns)
            .await?;

        Ok(vec![("cadalytix_config".to_string(), result)])
    }
}
