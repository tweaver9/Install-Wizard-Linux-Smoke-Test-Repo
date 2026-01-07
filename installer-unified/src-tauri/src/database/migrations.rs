// Migration runner
// Ported from C# ManifestBasedMigrationRunner.cs
// Implements manifest-based migration execution with transaction safety and checksum validation

use anyhow::{Context, Result};
use chrono::Utc;
use log::info;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Pool, Postgres};
use std::collections::HashSet;
use std::path::PathBuf;
use tiberius::Query;
use tokio::fs;

use crate::database::connection::DatabaseConnection;

/// Versioned manifest schema (manifest_versioned.json).
///
/// This matches the `db/migrations/generate_manifest.ps1` output and supports:
/// - multi-version SQL Server (2014-2022) + Postgres (13-17)
/// - deterministic ordering
/// - per-file SHA256 checksums
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct VersionedMigrationManifest {
    #[serde(default)]
    bundles: Vec<VersionedManifestBundle>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct VersionedManifestBundle {
    name: String,
    order: i32,
    #[serde(default)]
    migrations: Vec<VersionedManifestMigration>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct VersionedManifestMigration {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    group: Option<String>,
    engine: String,
    #[serde(rename = "engineVersion")]
    engine_version: String,
    order: i32,
    checksum: String,
    #[serde(default)]
    dependencies: serde_json::Value,
}

/// Migration manifest structure
/// Represents the manifest.json file that defines migration order and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationManifest {
    pub engine: String,
    pub engine_version: String,
    pub migrations: Vec<MigrationEntry>,
}

/// Individual migration entry in the manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationEntry {
    pub name: String,
    pub file: String,
    pub order: u32,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub migration_group: Option<String>,
}

/// Migration runner for executing database migrations
/// Supports both PostgreSQL (via sqlx) and SQL Server (via tiberius)
pub struct MigrationRunner {
    connection: DatabaseConnection,
    manifest_path: PathBuf,
    migrations_path: PathBuf,
    engine: String,
    engine_version: String,
}

impl MigrationRunner {
    /// Create a new migration runner
    pub async fn new(
        connection: DatabaseConnection,
        manifest_path: PathBuf,
        migrations_path: PathBuf,
        engine: String,
        engine_version: String,
    ) -> Result<Self> {
        info!(
            "[PHASE: database] [STEP: migration_runner_init] Creating migration runner for {} {}",
            engine, engine_version
        );

        Ok(MigrationRunner {
            connection,
            manifest_path,
            migrations_path,
            engine,
            engine_version,
        })
    }

    /// Load and parse the migration manifest
    pub async fn load_manifest(&self) -> Result<MigrationManifest> {
        info!(
            "[PHASE: database] [STEP: load_manifest] Loading manifest from: {:?}",
            self.manifest_path
        );

        let content = fs::read_to_string(&self.manifest_path)
            .await
            .with_context(|| format!("Failed to read manifest file: {:?}", self.manifest_path))?;

        // Parse versioned manifest and select migrations for the requested engine + engine_version.
        let versioned: VersionedMigrationManifest =
            serde_json::from_str(&content).with_context(|| {
                format!(
                    "Failed to parse versioned manifest JSON: {:?}",
                    self.manifest_path
                )
            })?;

        let wanted_engine = normalize_engine(&self.engine);
        let wanted_version = self.engine_version.trim().to_string();

        let mut migrations: Vec<MigrationEntry> = Vec::new();

        for bundle in &versioned.bundles {
            for m in &bundle.migrations {
                if normalize_engine(&m.engine) != wanted_engine {
                    continue;
                }
                if m.engine_version != wanted_version {
                    continue;
                }

                // NOTE: In the versioned manifest, `name` is also the relative file path
                // (e.g. "SQL/v2022/SQL_v2022_001_create_...sql").
                let group = m.group.clone().or_else(|| Some(bundle.name.clone()));

                migrations.push(MigrationEntry {
                    name: m.name.clone(),
                    file: m.name.clone(),
                    order: m.order.max(0) as u32,
                    checksum: Some(m.checksum.clone()),
                    migration_group: group,
                });
            }
        }

        // Deterministic ordering: bundle-order is implicit in the migration `order`, but we also
        // stabilize by name so ties don't change execution order.
        migrations.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.name.cmp(&b.name)));

        if migrations.is_empty() {
            anyhow::bail!(
                "No migrations found in manifest for engine='{}' version='{}' (manifest_path={:?})",
                wanted_engine,
                wanted_version,
                self.manifest_path
            );
        }

        info!(
            "[PHASE: database] [STEP: load_manifest] Loaded {} migrations for {} {}",
            migrations.len(),
            wanted_engine,
            wanted_version
        );

        Ok(MigrationManifest {
            engine: wanted_engine,
            engine_version: wanted_version,
            migrations,
        })
    }

    /// Get applied migration names from the database.
    ///
    /// NOTE:
    /// - On a brand new database, `applied_migrations` may not exist until migration `002` runs.
    /// - We intentionally only read `migration_name` so this works before/after the `010` enhancement.
    pub async fn get_applied_migration_names(&self) -> Result<HashSet<String>> {
        info!(
            "[PHASE: database] [STEP: get_applied_migrations] Querying applied migrations for {} {}",
            self.engine, self.engine_version
        );

        match &self.connection {
            DatabaseConnection::Postgres(pool) => {
                self.get_applied_migration_names_postgres(pool).await
            }
            DatabaseConnection::SqlServer(_) => self.get_applied_migration_names_sql_server().await,
        }
    }

    async fn get_applied_migration_names_postgres(
        &self,
        pool: &Pool<Postgres>,
    ) -> Result<HashSet<String>> {
        let table_exists: bool = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM information_schema.tables
                WHERE table_schema = 'cadalytix_config'
                  AND table_name = 'applied_migrations'
            )
            "#,
        )
        .fetch_one(pool)
        .await
        .with_context(|| "Failed to check applied_migrations table existence (PostgreSQL)")?;

        if !table_exists {
            return Ok(HashSet::new());
        }

        let names: Vec<String> = sqlx::query_scalar::<_, String>(
            r#"
            SELECT migration_name
            FROM cadalytix_config.applied_migrations
            "#,
        )
        .fetch_all(pool)
        .await
        .with_context(|| "Failed to query applied migration names (PostgreSQL)")?;

        Ok(names.into_iter().collect())
    }

    async fn get_applied_migration_names_sql_server(&self) -> Result<HashSet<String>> {
        use futures::TryStreamExt;
        use tiberius::QueryItem;

        let client_arc = self
            .connection
            .as_sql_server()
            .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
        let mut client = client_arc.lock().await;

        // Check table exists (scope the stream so we can reuse `client` afterward).
        let table_exists = {
            let exists_query = Query::new(
                r#"
                SELECT COUNT(*)
                FROM INFORMATION_SCHEMA.TABLES
                WHERE TABLE_SCHEMA = 'cadalytix_config'
                  AND TABLE_NAME = 'applied_migrations'
                "#,
            );
            let mut stream = exists_query.query(&mut *client).await.with_context(|| {
                "Failed to check applied_migrations table existence (SQL Server)"
            })?;

            let mut table_exists = false;
            while let Some(item) = stream.try_next().await? {
                if let QueryItem::Row(row) = item {
                    let count: i32 = row
                        .get::<i32, _>(0)
                        .ok_or_else(|| anyhow::anyhow!("COUNT(*) is null"))?;
                    table_exists = count > 0;
                    break;
                }
            }

            table_exists
        };

        if !table_exists {
            return Ok(HashSet::new());
        }

        let query = Query::new(
            r#"
            SELECT migration_name
            FROM cadalytix_config.applied_migrations
            "#,
        );
        let mut stream = query
            .query(&mut *client)
            .await
            .with_context(|| "Failed to query applied migration names (SQL Server)")?;

        let mut names = HashSet::new();
        while let Some(item) = stream.try_next().await? {
            if let QueryItem::Row(row) = item {
                let name: String = row
                    .get::<&str, _>(0)
                    .ok_or_else(|| anyhow::anyhow!("migration_name is null"))?
                    .to_string();
                names.insert(name);
            }
        }

        Ok(names)
    }

    /// Apply a single migration
    pub async fn apply_migration(&self, migration: &MigrationEntry) -> Result<()> {
        info!(
            "[PHASE: database] [STEP: apply_migration] Applying migration: {}",
            migration.name
        );

        let start_time = Utc::now();

        // Read migration SQL file
        let migration_file = self
            .migrations_path
            .join(manifest_relative_path(&migration.file));
        let sql_bytes = fs::read(&migration_file)
            .await
            .with_context(|| format!("Failed to read migration file: {:?}", migration_file))?;
        let sql_content = String::from_utf8(sql_bytes.clone())
            .with_context(|| format!("Migration file is not valid UTF-8: {:?}", migration_file))?;

        // Compute checksum (SHA256 of raw file bytes)
        let computed_checksum = sha256_hex(&sql_bytes);

        // Verify checksum if specified in manifest
        if let Some(expected_checksum) = &migration.checksum {
            if computed_checksum != *expected_checksum {
                anyhow::bail!(
                    "Checksum mismatch for migration {}: expected {}, computed {}",
                    migration.name,
                    expected_checksum,
                    computed_checksum
                );
            }
        }

        // Execute migration in transaction
        match &self.connection {
            DatabaseConnection::Postgres(pool) => {
                self.apply_migration_postgres(
                    pool,
                    migration,
                    &sql_content,
                    &computed_checksum,
                    start_time,
                )
                .await
            }
            DatabaseConnection::SqlServer(_) => {
                self.apply_migration_sql_server(
                    migration,
                    &sql_content,
                    &computed_checksum,
                    start_time,
                )
                .await
            }
        }
    }

    /// Apply migration to PostgreSQL
    async fn apply_migration_postgres(
        &self,
        pool: &Pool<Postgres>,
        migration: &MigrationEntry,
        sql_content: &str,
        checksum: &str,
        start_time: chrono::DateTime<Utc>,
    ) -> Result<()> {
        // Begin transaction
        let mut tx = pool
            .begin()
            .await
            .with_context(|| "Failed to begin transaction")?;

        // Execute migration SQL (may contain multiple statements)
        sqlx::raw_sql(sql_content)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("Failed to execute migration SQL: {}", migration.name))?;

        // Calculate execution time
        let execution_time_ms = (Utc::now() - start_time).num_milliseconds() as i32;

        // Record applied migration (upsert metadata when the enhanced columns exist).
        self.record_applied_migration_postgres(
            &mut tx,
            migration,
            checksum,
            execution_time_ms,
            "INSTALLER",
        )
        .await
        .with_context(|| format!("Failed to record applied migration: {}", migration.name))?;

        // Commit transaction
        tx.commit()
            .await
            .with_context(|| "Failed to commit transaction")?;

        info!(
            "[PHASE: database] [STEP: apply_migration] Successfully applied migration: {} ({}ms)",
            migration.name, execution_time_ms
        );

        Ok(())
    }

    /// Apply migration to SQL Server
    async fn apply_migration_sql_server(
        &self,
        migration: &MigrationEntry,
        sql_content: &str,
        checksum: &str,
        start_time: chrono::DateTime<Utc>,
    ) -> Result<()> {
        use futures::TryStreamExt;

        let client_arc = self
            .connection
            .as_sql_server()
            .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;

        let mut client = client_arc.lock().await;

        // SQL Server migration files use `GO` batch separators which are not valid T-SQL.
        // Split into batches and execute each batch in a single transaction.
        let batches = split_sql_server_batches(sql_content);

        // Begin transaction
        {
            let mut stream = client
                .simple_query("BEGIN TRANSACTION")
                .await
                .with_context(|| "Failed to begin SQL Server transaction")?;
            while stream.try_next().await?.is_some() {}
        }

        let exec_result: Result<i32> = (async {
            for (idx, batch) in batches.iter().enumerate() {
                let sql = batch.trim();
                if sql.is_empty() {
                    continue;
                }

                let mut stream = client.simple_query(sql).await.with_context(|| {
                    format!(
                        "Failed to execute SQL Server migration batch {} for {}",
                        idx + 1,
                        migration.name
                    )
                })?;

                // Drain all result sets.
                while stream
                    .try_next()
                    .await
                    .with_context(|| {
                        format!(
                            "Failed reading results for {} batch {}",
                            migration.name,
                            idx + 1
                        )
                    })?
                    .is_some()
                {}
            }

            // Calculate execution time
            let execution_time_ms = (Utc::now() - start_time).num_milliseconds() as i32;

            // Record applied migration (upsert metadata when the enhanced columns exist).
            self.record_applied_migration_sql_server(
                &mut client,
                migration,
                checksum,
                execution_time_ms,
                "INSTALLER",
            )
            .await
            .with_context(|| format!("Failed to record applied migration: {}", migration.name))?;

            Ok(execution_time_ms)
        })
        .await;

        match exec_result {
            Ok(execution_time_ms) => {
                let mut stream = client
                    .simple_query("COMMIT TRANSACTION")
                    .await
                    .with_context(|| "Failed to commit SQL Server transaction")?;
                while stream.try_next().await?.is_some() {}

                info!(
                    "[PHASE: database] [STEP: apply_migration] Successfully applied SQL Server migration: {} ({}ms)",
                    migration.name,
                    execution_time_ms
                );

                Ok(())
            }
            Err(e) => {
                // Rollback on any error.
                if let Ok(mut stream) = client.simple_query("ROLLBACK TRANSACTION").await {
                    let _ = stream.try_next().await;
                }
                Err(e).with_context(|| {
                    format!(
                        "Migration failed, transaction rolled back: {}",
                        migration.name
                    )
                })
            }
        }
    }

    async fn record_applied_migration_postgres(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        migration: &MigrationEntry,
        checksum: &str,
        execution_time_ms: i32,
        applied_by: &str,
    ) -> Result<()> {
        let table_exists: bool = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM information_schema.tables
                WHERE table_schema = 'cadalytix_config'
                  AND table_name = 'applied_migrations'
            )
            "#,
        )
        .fetch_one(&mut **tx)
        .await
        .with_context(|| "Failed to check applied_migrations table existence (PostgreSQL)")?;

        if !table_exists {
            // Happens for migration 001 on a fresh DB (table is created in migration 002).
            return Ok(());
        }

        let cols: Vec<String> = sqlx::query_scalar::<_, String>(
            r#"
            SELECT column_name
            FROM information_schema.columns
            WHERE table_schema = 'cadalytix_config'
              AND table_name = 'applied_migrations'
            "#,
        )
        .fetch_all(&mut **tx)
        .await
        .with_context(|| "Failed to read applied_migrations columns (PostgreSQL)")?;

        let colset: HashSet<String> = cols.into_iter().collect();
        let has_enhanced = colset.contains("checksum")
            && colset.contains("migration_group")
            && colset.contains("engine")
            && colset.contains("execution_time_ms")
            && colset.contains("applied_by");

        let engine = normalize_engine(&self.engine);

        if has_enhanced {
            sqlx::query(
                r#"
                INSERT INTO cadalytix_config.applied_migrations
                    (migration_name, checksum, migration_group, engine, execution_time_ms, applied_by)
                VALUES
                    ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (migration_name) DO NOTHING
                "#,
            )
            .bind(&migration.name)
            .bind(checksum)
            .bind(migration.migration_group.as_deref())
            .bind(engine.as_str())
            .bind(execution_time_ms)
            .bind(applied_by)
            .execute(&mut **tx)
            .await?;
        } else {
            // Minimal schema (migration_name, applied_at). Let the DB default `applied_at`.
            sqlx::query(
                r#"
                INSERT INTO cadalytix_config.applied_migrations (migration_name)
                VALUES ($1)
                ON CONFLICT (migration_name) DO NOTHING
                "#,
            )
            .bind(&migration.name)
            .execute(&mut **tx)
            .await?;
        }

        Ok(())
    }

    async fn record_applied_migration_sql_server(
        &self,
        client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
        migration: &MigrationEntry,
        checksum: &str,
        execution_time_ms: i32,
        applied_by: &str,
    ) -> Result<()> {
        use futures::TryStreamExt;
        use tiberius::QueryItem;

        // Check table exists (scope the stream so we can reuse `client` afterward).
        let table_exists = {
            let exists_query = Query::new(
                r#"
                SELECT COUNT(*)
                FROM INFORMATION_SCHEMA.TABLES
                WHERE TABLE_SCHEMA = 'cadalytix_config'
                  AND TABLE_NAME = 'applied_migrations'
                "#,
            );
            let mut stream = exists_query.query(client).await.with_context(|| {
                "Failed to check applied_migrations table existence (SQL Server)"
            })?;

            let mut table_exists = false;
            while let Some(item) = stream.try_next().await? {
                if let QueryItem::Row(row) = item {
                    let count: i32 = row
                        .get::<i32, _>(0)
                        .ok_or_else(|| anyhow::anyhow!("COUNT(*) is null"))?;
                    table_exists = count > 0;
                    break;
                }
            }

            table_exists
        };

        if !table_exists {
            return Ok(());
        }

        // Read column list to know whether enhanced metadata columns exist yet (added in migration 010).
        let colset: HashSet<String> = {
            let cols_query = Query::new(
                r#"
                SELECT COLUMN_NAME
                FROM INFORMATION_SCHEMA.COLUMNS
                WHERE TABLE_SCHEMA = 'cadalytix_config'
                  AND TABLE_NAME = 'applied_migrations'
                "#,
            );
            let mut stream = cols_query
                .query(client)
                .await
                .with_context(|| "Failed to read applied_migrations columns (SQL Server)")?;

            let mut colset = HashSet::new();
            while let Some(item) = stream.try_next().await? {
                if let QueryItem::Row(row) = item {
                    let col: &str = row
                        .get::<&str, _>(0)
                        .ok_or_else(|| anyhow::anyhow!("COLUMN_NAME is null"))?;
                    colset.insert(col.to_ascii_lowercase());
                }
            }

            colset
        };

        let has_enhanced = colset.contains("checksum")
            && colset.contains("migration_group")
            && colset.contains("engine")
            && colset.contains("execution_time_ms")
            && colset.contains("applied_by");

        let engine = normalize_engine(&self.engine);

        if has_enhanced {
            let insert_sql = r#"
                IF NOT EXISTS (
                    SELECT 1
                    FROM cadalytix_config.applied_migrations
                    WHERE migration_name = @P1
                )
                BEGIN
                    INSERT INTO cadalytix_config.applied_migrations
                        (migration_name, checksum, migration_group, engine, execution_time_ms, applied_by)
                    VALUES
                        (@P1, @P2, @P3, @P4, @P5, @P6)
                END
            "#;

            let mut q = Query::new(insert_sql);
            q.bind(migration.name.as_str());
            q.bind(checksum);
            q.bind(migration.migration_group.as_deref());
            q.bind(engine.as_str());
            q.bind(execution_time_ms);
            q.bind(applied_by);

            let mut stream = q
                .query(client)
                .await
                .with_context(|| "Failed to insert applied migration metadata (SQL Server)")?;
            while stream.try_next().await?.is_some() {}
        } else {
            let insert_sql = r#"
                IF NOT EXISTS (
                    SELECT 1
                    FROM cadalytix_config.applied_migrations
                    WHERE migration_name = @P1
                )
                BEGIN
                    INSERT INTO cadalytix_config.applied_migrations (migration_name)
                    VALUES (@P1)
                END
            "#;

            let mut q = Query::new(insert_sql);
            q.bind(migration.name.as_str());
            let mut stream = q
                .query(client)
                .await
                .with_context(|| "Failed to insert applied migration (SQL Server)")?;
            while stream.try_next().await?.is_some() {}
        }

        Ok(())
    }

    async fn backfill_applied_migration_metadata(
        &self,
        migrations: &[MigrationEntry],
    ) -> Result<()> {
        if migrations.is_empty() {
            return Ok(());
        }

        info!(
            "[PHASE: database] [STEP: apply_all_pending] Backfilling applied_migrations metadata for {} migrations",
            migrations.len()
        );

        match &self.connection {
            DatabaseConnection::Postgres(pool) => {
                let mut tx = pool.begin().await?;
                for m in migrations {
                    // Use "0ms" for backfill if we no longer have per-migration timing.
                    let exec_ms = 0;
                    self.record_applied_migration_postgres(
                        &mut tx,
                        m,
                        m.checksum.as_deref().unwrap_or(""),
                        exec_ms,
                        "INSTALLER",
                    )
                    .await?;
                }
                tx.commit().await?;
                Ok(())
            }
            DatabaseConnection::SqlServer(_) => {
                use futures::TryStreamExt;

                let client_arc = self
                    .connection
                    .as_sql_server()
                    .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
                let mut client = client_arc.lock().await;

                // Transaction for batch backfill (scope stream so the mutable borrow ends).
                {
                    let mut stream = client.simple_query("BEGIN TRANSACTION").await?;
                    while stream.try_next().await?.is_some() {}
                }

                let result: Result<()> = (async {
                    for m in migrations {
                        let exec_ms = 0;
                        self.record_applied_migration_sql_server(
                            &mut client,
                            m,
                            m.checksum.as_deref().unwrap_or(""),
                            exec_ms,
                            "INSTALLER",
                        )
                        .await?;
                    }
                    Ok(())
                })
                .await;

                match result {
                    Ok(()) => {
                        let mut stream = client.simple_query("COMMIT TRANSACTION").await?;
                        while stream.try_next().await?.is_some() {}
                        Ok(())
                    }
                    Err(e) => {
                        if let Ok(mut stream) = client.simple_query("ROLLBACK TRANSACTION").await {
                            let _ = stream.try_next().await;
                        }
                        Err(e)
                    }
                }
            }
        }
    }

    /// Apply all pending migrations
    pub async fn apply_all_pending(&self) -> Result<Vec<String>> {
        info!(
            "[PHASE: database] [STEP: apply_all_pending] Starting migration execution for {} {}",
            self.engine, self.engine_version
        );

        // Load manifest
        let manifest = self.load_manifest().await?;

        // Get applied migrations (names only)
        let applied_names: HashSet<String> = self.get_applied_migration_names().await?;

        // Determine pending migrations
        let pending: Vec<&MigrationEntry> = manifest
            .migrations
            .iter()
            .filter(|m| !applied_names.contains(&m.name))
            .collect();

        if pending.is_empty() {
            info!("[PHASE: database] [STEP: apply_all_pending] No pending migrations");
            return Ok(vec![]);
        }

        info!(
            "[PHASE: database] [STEP: apply_all_pending] Found {} pending migrations",
            pending.len()
        );

        // Apply each pending migration in order
        let mut applied_names = Vec::new();
        let mut applied_entries: Vec<MigrationEntry> = Vec::new();
        for migration in pending {
            self.apply_migration(migration)
                .await
                .with_context(|| format!("Failed to apply migration: {}", migration.name))?;
            applied_names.push(migration.name.clone());
            applied_entries.push(migration.clone());
        }

        // Best-effort: backfill full metadata (checksum/group/engine/timing/actor) once the
        // enhanced columns exist (migration 010). This also records migration 001 after 002 creates
        // the applied_migrations table.
        self.backfill_applied_migration_metadata(&applied_entries)
            .await?;

        info!(
            "[PHASE: database] [STEP: apply_all_pending] Successfully applied {} migrations",
            applied_names.len()
        );

        Ok(applied_names)
    }
}

fn normalize_engine(engine: &str) -> String {
    match engine.trim().to_ascii_lowercase().as_str() {
        "sql" | "sqlserver" | "mssql" => "sqlserver".to_string(),
        "postgres" | "postgresql" => "postgres".to_string(),
        other => other.to_string(),
    }
}

fn manifest_relative_path(path: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.split(['/', '\\']) {
        if !part.is_empty() {
            out.push(part);
        }
    }
    out
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Split a SQL Server migration script into batches separated by `GO` statements.
///
/// `GO` is not valid T-SQL; it's a client-side batch separator used by tools like SSMS/sqlcmd.
fn split_sql_server_batches(sql: &str) -> Vec<String> {
    let mut batches = Vec::new();
    let mut current = String::new();

    for line in sql.lines() {
        if line.trim().eq_ignore_ascii_case("GO") {
            if !current.trim().is_empty() {
                batches.push(current);
            }
            current = String::new();
            continue;
        }

        current.push_str(line);
        current.push('\n');
    }

    if !current.trim().is_empty() {
        batches.push(current);
    }

    batches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_sql_server_batches_splits_on_go_lines() {
        let sql = "SELECT 1;\nGO\nSELECT 2;\n  go  \nSELECT 3;\n";
        let batches = split_sql_server_batches(sql);
        assert_eq!(batches.len(), 3);
        assert!(batches[0].contains("SELECT 1"));
        assert!(batches[1].contains("SELECT 2"));
        assert!(batches[2].contains("SELECT 3"));
    }

    #[test]
    fn manifest_relative_path_handles_forward_slashes() {
        let p = manifest_relative_path("SQL/v2022/file.sql");
        assert!(p.to_string_lossy().contains("SQL"));
        assert!(p.to_string_lossy().contains("v2022"));
        assert!(p.to_string_lossy().contains("file.sql"));
    }
}
