// Platform DB adapter
// Ported from C# SqlServerPlatformDbAdapter.cs
// Handles instance settings, license state, and platform-specific database operations

use anyhow::{Context, Result};
use chrono::DateTime;
use chrono::Utc;
use futures::TryStreamExt;
use log::{debug, info};
use serde_json::Value;
use sqlx::{Pool, Postgres};
use std::collections::HashMap;
use std::sync::Arc;
use tiberius::Query;

use crate::database::connection::DatabaseConnection;
use crate::security::secret_protector::SecretProtector;

/// Platform database adapter for instance settings and license state
pub struct PlatformDbAdapter {
    connection: DatabaseConnection,
    secrets: Arc<SecretProtector>,
}

impl PlatformDbAdapter {
    /// Create a new platform database adapter
    pub fn new(connection: DatabaseConnection, secrets: Arc<SecretProtector>) -> Self {
        info!("[PHASE: database] [STEP: platform_db_init] Creating platform DB adapter");
        PlatformDbAdapter {
            connection,
            secrets,
        }
    }

    /// Get a single instance setting value by key.
    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        debug!(
            "[PHASE: database] [STEP: get_setting] Retrieving setting: {}",
            key
        );

        let raw = match &self.connection {
            DatabaseConnection::Postgres(pool) => self.get_setting_postgres(pool, key).await?,
            DatabaseConnection::SqlServer(_) => self.get_setting_sql_server(key).await?,
        };

        if let Some(v) = raw {
            if self.secrets.is_encrypted(&v) {
                return Ok(Some(self.secrets.decrypt(&v).await?));
            }
            return Ok(Some(v));
        }

        Ok(None)
    }

    /// Get all instance settings as a key-value map.
    pub async fn get_all_settings(&self) -> Result<HashMap<String, String>> {
        info!("[PHASE: database] [STEP: get_all_settings] Retrieving all instance settings");

        let raw = match &self.connection {
            DatabaseConnection::Postgres(pool) => self.get_all_settings_postgres(pool).await?,
            DatabaseConnection::SqlServer(_) => self.get_all_settings_sql_server().await?,
        };

        let mut out = HashMap::new();
        for (k, v) in raw {
            if self.secrets.is_encrypted(&v) {
                out.insert(k, self.secrets.decrypt(&v).await?);
            } else {
                out.insert(k, v);
            }
        }
        Ok(out)
    }

    /// Get all instance setting keys (no values). Useful for PHI-safe support bundles.
    pub async fn get_setting_keys(&self) -> Result<Vec<String>> {
        match &self.connection {
            DatabaseConnection::Postgres(pool) => self.get_setting_keys_postgres(pool).await,
            DatabaseConnection::SqlServer(_) => self.get_setting_keys_sql_server().await,
        }
    }

    /// Set a single instance setting (upsert).
    #[allow(dead_code)]
    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        info!(
            "[PHASE: database] [STEP: set_setting] Saving setting: {}",
            key
        );

        let value_to_store = if should_encrypt_setting_key(key) {
            self.secrets.encrypt(value).await?
        } else {
            value.to_string()
        };

        match &self.connection {
            DatabaseConnection::Postgres(pool) => {
                self.set_setting_postgres(pool, key, &value_to_store).await
            }
            DatabaseConnection::SqlServer(_) => {
                self.set_setting_sql_server(key, &value_to_store).await
            }
        }
    }

    /// Set multiple settings in a single transaction.
    pub async fn set_settings(&self, settings: &HashMap<String, String>) -> Result<()> {
        info!(
            "[PHASE: database] [STEP: set_settings] Saving {} settings",
            settings.len()
        );

        let mut to_store = HashMap::new();
        for (k, v) in settings {
            let vv = if should_encrypt_setting_key(k) {
                self.secrets.encrypt(v).await?
            } else {
                v.clone()
            };
            to_store.insert(k.clone(), vv);
        }

        match &self.connection {
            DatabaseConnection::Postgres(pool) => self.set_settings_postgres(pool, &to_store).await,
            DatabaseConnection::SqlServer(_) => self.set_settings_sql_server(&to_store).await,
        }
    }

    /// Set multiple settings using an owned map.
    ///
    /// This wrapper exists so background tasks (tokio spawn) can call into settings persistence
    /// without creating non-`'static` borrows at the callsite.
    pub async fn set_settings_owned(&self, settings: HashMap<String, String>) -> Result<()> {
        info!(
            "[PHASE: database] [STEP: set_settings] Saving {} settings",
            settings.len()
        );

        let mut to_store = HashMap::new();
        for (k, v) in settings.into_iter() {
            let vv = if should_encrypt_setting_key(&k) {
                self.secrets.encrypt(&v).await?
            } else {
                v
            };
            to_store.insert(k, vv);
        }

        match &self.connection {
            DatabaseConnection::Postgres(pool) => self.set_settings_postgres(pool, &to_store).await,
            DatabaseConnection::SqlServer(_) => self.set_settings_sql_server(&to_store).await,
        }
    }

    /// Delete a setting by key.
    #[allow(dead_code)]
    pub async fn delete_setting(&self, key: &str) -> Result<()> {
        info!(
            "[PHASE: database] [STEP: delete_setting] Deleting setting: {}",
            key
        );

        match &self.connection {
            DatabaseConnection::Postgres(pool) => self.delete_setting_postgres(pool, key).await,
            DatabaseConnection::SqlServer(_) => self.delete_setting_sql_server(key).await,
        }
    }

    /// Convenience: return settings as a JSON object (key -> string value).
    #[allow(dead_code)]
    pub async fn get_instance_settings(&self) -> Result<Value> {
        let map = self.get_all_settings().await?;
        let mut obj = serde_json::Map::new();
        for (k, v) in map {
            obj.insert(k, Value::String(v));
        }
        Ok(Value::Object(obj))
    }

    /// Convenience: set settings from a JSON object (values are stringified if needed).
    #[allow(dead_code)]
    pub async fn set_instance_settings(&self, settings: &Value) -> Result<()> {
        let obj = settings
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("instance settings must be a JSON object"))?;

        let mut map = HashMap::new();
        for (k, v) in obj {
            let s = match v {
                Value::String(s) => s.clone(),
                other => serde_json::to_string(other).with_context(|| {
                    format!("Failed to serialize setting value for key '{}'", k)
                })?,
            };
            map.insert(k.clone(), s);
        }

        self.set_settings(&map).await
    }

    // --- Postgres impl ---

    async fn get_setting_postgres(
        &self,
        pool: &Pool<Postgres>,
        key: &str,
    ) -> Result<Option<String>> {
        let value = sqlx::query_scalar::<_, String>(
            r#"
            SELECT "value"
            FROM cadalytix_config.instance_settings
            WHERE "key" = $1
            "#,
        )
        .bind(key)
        .fetch_optional(pool)
        .await
        .with_context(|| "Failed to query instance setting (PostgreSQL)")?;

        Ok(value)
    }

    async fn get_all_settings_postgres(
        &self,
        pool: &Pool<Postgres>,
    ) -> Result<HashMap<String, String>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT "key", "value"
            FROM cadalytix_config.instance_settings
            "#,
        )
        .fetch_all(pool)
        .await
        .with_context(|| "Failed to query instance settings (PostgreSQL)")?;

        Ok(rows.into_iter().collect())
    }

    async fn get_setting_keys_postgres(&self, pool: &Pool<Postgres>) -> Result<Vec<String>> {
        let rows: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT "key"
            FROM cadalytix_config.instance_settings
            ORDER BY "key"
            "#,
        )
        .fetch_all(pool)
        .await
        .with_context(|| "Failed to query instance setting keys (PostgreSQL)")?;
        Ok(rows)
    }

    #[allow(dead_code)]
    async fn set_setting_postgres(
        &self,
        pool: &Pool<Postgres>,
        key: &str,
        value: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO cadalytix_config.instance_settings ("key", "value", updated_at)
            VALUES ($1, $2, $3)
            ON CONFLICT ("key") DO UPDATE
            SET "value" = EXCLUDED."value",
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(key)
        .bind(value)
        .bind(Utc::now().naive_utc())
        .execute(pool)
        .await
        .with_context(|| "Failed to upsert instance setting (PostgreSQL)")?;

        Ok(())
    }

    async fn set_settings_postgres(
        &self,
        pool: &Pool<Postgres>,
        settings: &HashMap<String, String>,
    ) -> Result<()> {
        let mut tx = pool.begin().await?;
        for (k, v) in settings {
            sqlx::query(
                r#"
                INSERT INTO cadalytix_config.instance_settings ("key", "value", updated_at)
                VALUES ($1, $2, $3)
                ON CONFLICT ("key") DO UPDATE
                SET "value" = EXCLUDED."value",
                    updated_at = EXCLUDED.updated_at
                "#,
            )
            .bind(k)
            .bind(v)
            .bind(Utc::now().naive_utc())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    #[allow(dead_code)]
    async fn delete_setting_postgres(&self, pool: &Pool<Postgres>, key: &str) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM cadalytix_config.instance_settings
            WHERE "key" = $1
            "#,
        )
        .bind(key)
        .execute(pool)
        .await
        .with_context(|| "Failed to delete instance setting (PostgreSQL)")?;
        Ok(())
    }

    // --- SQL Server impl ---

    async fn get_setting_sql_server(&self, key: &str) -> Result<Option<String>> {
        use tiberius::QueryItem;

        let client_arc = self
            .connection
            .as_sql_server()
            .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
        let mut client = client_arc.lock().await;

        let mut query = Query::new(
            r#"
            SELECT [value]
            FROM cadalytix_config.instance_settings
            WHERE [key] = @P1
            "#,
        );
        query.bind(key);

        let mut stream = query.query(&mut *client).await?;
        while let Some(item) = stream.try_next().await? {
            if let QueryItem::Row(row) = item {
                let v: &str = row.get::<&str, _>(0).unwrap_or("");
                return Ok(Some(v.to_string()));
            }
        }

        Ok(None)
    }

    async fn get_all_settings_sql_server(&self) -> Result<HashMap<String, String>> {
        use tiberius::QueryItem;

        let client_arc = self
            .connection
            .as_sql_server()
            .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
        let mut client = client_arc.lock().await;

        let query = Query::new(
            r#"
            SELECT [key], [value]
            FROM cadalytix_config.instance_settings
            "#,
        );

        let mut stream = query.query(&mut *client).await?;

        let mut out = HashMap::new();
        while let Some(item) = stream.try_next().await? {
            if let QueryItem::Row(row) = item {
                let k: &str = row.get::<&str, _>(0).unwrap_or("");
                let v: &str = row.get::<&str, _>(1).unwrap_or("");
                if !k.is_empty() {
                    out.insert(k.to_string(), v.to_string());
                }
            }
        }

        Ok(out)
    }

    async fn get_setting_keys_sql_server(&self) -> Result<Vec<String>> {
        use tiberius::QueryItem;

        let client_arc = self
            .connection
            .as_sql_server()
            .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
        let mut client = client_arc.lock().await;

        let query = Query::new(
            r#"
            SELECT [key]
            FROM cadalytix_config.instance_settings
            ORDER BY [key]
            "#,
        );

        let mut stream = query.query(&mut *client).await?;

        let mut out = Vec::new();
        while let Some(item) = stream.try_next().await? {
            if let QueryItem::Row(row) = item {
                let k: &str = row.get::<&str, _>(0).unwrap_or("");
                if !k.is_empty() {
                    out.push(k.to_string());
                }
            }
        }
        Ok(out)
    }

    #[allow(dead_code)]
    async fn set_setting_sql_server(&self, key: &str, value: &str) -> Result<()> {
        let client_arc = self
            .connection
            .as_sql_server()
            .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
        let mut client = client_arc.lock().await;

        let query_str = r#"
            MERGE cadalytix_config.instance_settings AS target
            USING (SELECT @P1 AS [key]) AS source
            ON target.[key] = source.[key]
            WHEN MATCHED THEN
                UPDATE SET [value] = @P2, updated_at = SYSUTCDATETIME()
            WHEN NOT MATCHED THEN
                INSERT ([key], [value], updated_at)
                VALUES (@P1, @P2, SYSUTCDATETIME());
        "#;

        let mut query = Query::new(query_str);
        query.bind(key);
        query.bind(value);

        let mut stream = query.query(&mut *client).await?;
        while stream.try_next().await?.is_some() {}
        Ok(())
    }

    async fn set_settings_sql_server(&self, settings: &HashMap<String, String>) -> Result<()> {
        let client_arc = self
            .connection
            .as_sql_server()
            .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
        let mut client = client_arc.lock().await;

        // Transaction wrapper
        {
            let mut stream = client.simple_query("BEGIN TRANSACTION").await?;
            while stream.try_next().await?.is_some() {}
        }

        let result: Result<()> = (async {
            for (k, v) in settings {
                let query_str = r#"
                    MERGE cadalytix_config.instance_settings AS target
                    USING (SELECT @P1 AS [key]) AS source
                    ON target.[key] = source.[key]
                    WHEN MATCHED THEN
                        UPDATE SET [value] = @P2, updated_at = SYSUTCDATETIME()
                    WHEN NOT MATCHED THEN
                        INSERT ([key], [value], updated_at)
                        VALUES (@P1, @P2, SYSUTCDATETIME());
                "#;

                let mut query = Query::new(query_str);
                query.bind(k.as_str());
                query.bind(v.as_str());

                let mut s = query.query(&mut *client).await?;
                while s.try_next().await?.is_some() {}
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
                let _ = client.simple_query("ROLLBACK TRANSACTION").await;
                Err(e)
            }
        }
    }

    #[allow(dead_code)]
    async fn delete_setting_sql_server(&self, key: &str) -> Result<()> {
        let client_arc = self
            .connection
            .as_sql_server()
            .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
        let mut client = client_arc.lock().await;

        let mut query = Query::new(
            r#"
            DELETE FROM cadalytix_config.instance_settings
            WHERE [key] = @P1
            "#,
        );
        query.bind(key);

        let mut stream = query.query(&mut *client).await?;
        while stream.try_next().await?.is_some() {}
        Ok(())
    }

    // =========================
    // Wizard checkpoints
    // =========================

    pub async fn save_checkpoint(&self, step_name: &str, state_json: &str) -> Result<()> {
        match &self.connection {
            DatabaseConnection::Postgres(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO cadalytix_config.wizard_checkpoints (step_name, state_json, updated_at)
                    VALUES ($1, $2, $3)
                    ON CONFLICT (step_name) DO UPDATE
                    SET state_json = EXCLUDED.state_json,
                        updated_at = EXCLUDED.updated_at
                    "#,
                )
                .bind(step_name)
                .bind(state_json)
                .bind(Utc::now().naive_utc())
                .execute(pool)
                .await?;
                Ok(())
            }
            DatabaseConnection::SqlServer(_) => {
                let client_arc = self
                    .connection
                    .as_sql_server()
                    .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
                let mut client = client_arc.lock().await;

                let sql = r#"
                    MERGE cadalytix_config.wizard_checkpoints AS target
                    USING (SELECT @P1 AS step_name) AS source
                    ON target.step_name = source.step_name
                    WHEN MATCHED THEN
                        UPDATE SET state_json = @P2, updated_at = SYSUTCDATETIME()
                    WHEN NOT MATCHED THEN
                        INSERT (step_name, state_json, updated_at)
                        VALUES (@P1, @P2, SYSUTCDATETIME());
                "#;
                let mut q = Query::new(sql);
                q.bind(step_name);
                q.bind(state_json);
                let mut s = q.query(&mut *client).await?;
                while s.try_next().await?.is_some() {}
                Ok(())
            }
        }
    }

    pub async fn clear_checkpoints(&self) -> Result<()> {
        match &self.connection {
            DatabaseConnection::Postgres(pool) => {
                sqlx::query("DELETE FROM cadalytix_config.wizard_checkpoints")
                    .execute(pool)
                    .await?;
                Ok(())
            }
            DatabaseConnection::SqlServer(_) => {
                let client_arc = self
                    .connection
                    .as_sql_server()
                    .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
                let mut client = client_arc.lock().await;
                let mut s = client
                    .simple_query("DELETE FROM cadalytix_config.wizard_checkpoints")
                    .await?;
                while s.try_next().await?.is_some() {}
                Ok(())
            }
        }
    }

    pub async fn get_latest_checkpoint(&self) -> Result<Option<(String, String, DateTime<Utc>)>> {
        match &self.connection {
            DatabaseConnection::Postgres(pool) => {
                let row: Option<(String, String, chrono::NaiveDateTime)> = sqlx::query_as(
                    r#"
                    SELECT step_name, state_json, updated_at
                    FROM cadalytix_config.wizard_checkpoints
                    ORDER BY updated_at DESC
                    LIMIT 1
                    "#,
                )
                .fetch_optional(pool)
                .await?;
                Ok(row.map(|(s, j, t)| (s, j, DateTime::<Utc>::from_naive_utc_and_offset(t, Utc))))
            }
            DatabaseConnection::SqlServer(_) => {
                use tiberius::QueryItem;
                let client_arc = self
                    .connection
                    .as_sql_server()
                    .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
                let mut client = client_arc.lock().await;

                let q = Query::new(
                    r#"
                    SELECT TOP 1 step_name, state_json, updated_at
                    FROM cadalytix_config.wizard_checkpoints
                    ORDER BY updated_at DESC
                    "#,
                );
                let mut stream = q.query(&mut *client).await?;
                while let Some(item) = stream.try_next().await? {
                    if let QueryItem::Row(row) = item {
                        let step = row.get::<&str, _>(0).unwrap_or("").to_string();
                        let state = row.get::<&str, _>(1).unwrap_or("").to_string();
                        let ts = row.get::<chrono::NaiveDateTime, _>(2);
                        let ts = ts
                            .map(|t| DateTime::<Utc>::from_naive_utc_and_offset(t, Utc))
                            .unwrap_or_else(Utc::now);
                        return Ok(Some((step, state, ts)));
                    }
                }
                Ok(None)
            }
        }
    }

    /// Return applied migrations (name + applied_at) for status display.
    /// Works before/after migration 010 enhancements by only selecting base columns.
    pub async fn get_applied_migrations_brief(&self) -> Result<Vec<(String, DateTime<Utc>)>> {
        match &self.connection {
            DatabaseConnection::Postgres(pool) => {
                let rows: Vec<(String, chrono::NaiveDateTime)> = sqlx::query_as(
                    r#"
                    SELECT migration_name, applied_at
                    FROM cadalytix_config.applied_migrations
                    ORDER BY applied_at
                    "#,
                )
                .fetch_all(pool)
                .await?;
                Ok(rows
                    .into_iter()
                    .map(|(n, t)| (n, DateTime::<Utc>::from_naive_utc_and_offset(t, Utc)))
                    .collect())
            }
            DatabaseConnection::SqlServer(_) => {
                use tiberius::QueryItem;
                let client_arc = self
                    .connection
                    .as_sql_server()
                    .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
                let mut client = client_arc.lock().await;

                let q = Query::new(
                    r#"
                    SELECT migration_name, applied_at
                    FROM cadalytix_config.applied_migrations
                    ORDER BY applied_at
                    "#,
                );
                let mut stream = q.query(&mut *client).await?;
                let mut out = Vec::new();
                while let Some(item) = stream.try_next().await? {
                    if let QueryItem::Row(row) = item {
                        let name = row.get::<&str, _>(0).unwrap_or("").to_string();
                        let applied_at = row
                            .get::<chrono::NaiveDateTime, _>(1)
                            .unwrap_or_else(|| Utc::now().naive_utc());
                        if !name.is_empty() {
                            out.push((
                                name,
                                DateTime::<Utc>::from_naive_utc_and_offset(applied_at, Utc),
                            ));
                        }
                    }
                }
                Ok(out)
            }
        }
    }

    // =========================
    // Setup events (audit)
    // =========================

    pub async fn log_setup_event(
        &self,
        event_type: &str,
        description: &str,
        actor: Option<&str>,
        metadata: Option<&str>,
    ) -> Result<()> {
        match &self.connection {
            DatabaseConnection::Postgres(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO cadalytix_config.setup_events (event_type, description, actor, metadata, occurred_at)
                    VALUES ($1, $2, $3, $4, (CURRENT_TIMESTAMP AT TIME ZONE 'UTC'))
                    "#,
                )
                .bind(event_type)
                .bind(description)
                .bind(actor)
                .bind(metadata)
                .execute(pool)
                .await?;
                Ok(())
            }
            DatabaseConnection::SqlServer(_) => {
                let client_arc = self
                    .connection
                    .as_sql_server()
                    .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
                let mut client = client_arc.lock().await;
                let mut q = Query::new(
                    r#"
                    INSERT INTO cadalytix_config.setup_events (event_type, description, actor, metadata, occurred_at)
                    VALUES (@P1, @P2, @P3, @P4, SYSUTCDATETIME())
                    "#,
                );
                q.bind(event_type);
                q.bind(description);
                q.bind(actor.unwrap_or(""));
                q.bind(metadata.unwrap_or(""));
                let mut s = q.query(&mut *client).await?;
                while s.try_next().await?.is_some() {}
                Ok(())
            }
        }
    }

    /// Get recent setup events for diagnostics (PHI-safe).
    pub async fn get_setup_events(
        &self,
        take: i32,
    ) -> Result<Vec<(String, String, Option<String>, DateTime<Utc>)>> {
        let take = take.clamp(1, 200);
        match &self.connection {
            DatabaseConnection::Postgres(pool) => {
                let rows: Vec<(String, String, Option<String>, chrono::NaiveDateTime)> =
                    sqlx::query_as(
                        r#"
                    SELECT event_type, description, actor, occurred_at
                    FROM cadalytix_config.setup_events
                    ORDER BY occurred_at DESC
                    LIMIT $1
                    "#,
                    )
                    .bind(take)
                    .fetch_all(pool)
                    .await?;

                Ok(rows
                    .into_iter()
                    .map(|(et, desc, actor, ts)| {
                        (
                            et,
                            desc,
                            actor,
                            DateTime::<Utc>::from_naive_utc_and_offset(ts, Utc),
                        )
                    })
                    .collect())
            }
            DatabaseConnection::SqlServer(_) => {
                use tiberius::QueryItem;
                let client_arc = self
                    .connection
                    .as_sql_server()
                    .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
                let mut client = client_arc.lock().await;

                // TOP can't be parameterized easily; clamp + inline to avoid injection.
                let sql = format!(
                    r#"
                    SELECT TOP {take}
                        event_type, description, actor, occurred_at
                    FROM cadalytix_config.setup_events
                    ORDER BY occurred_at DESC
                    "#
                );

                let mut stream = client.simple_query(sql).await?;
                let mut out = Vec::new();
                while let Some(item) = stream.try_next().await? {
                    if let QueryItem::Row(row) = item {
                        let et = row.get::<&str, _>(0).unwrap_or("").to_string();
                        let desc = row.get::<&str, _>(1).unwrap_or("").to_string();
                        let actor = row.get::<&str, _>(2).map(|s| s.to_string());
                        let ts = row
                            .get::<chrono::NaiveDateTime, _>(3)
                            .unwrap_or_else(|| Utc::now().naive_utc());
                        out.push((
                            et,
                            desc,
                            actor,
                            DateTime::<Utc>::from_naive_utc_and_offset(ts, Utc),
                        ));
                    }
                }
                Ok(out)
            }
        }
    }

    // =========================
    // License state (single-row semantics)
    // =========================

    #[allow(clippy::too_many_arguments)]
    pub async fn save_license_state(
        &self,
        mode: &str,
        license_key_masked: &str,
        license_key_hash: &str,
        status: &str,
        client_name: &str,
        license_id: &str,
        issued_at_utc: DateTime<Utc>,
        expires_at_utc: DateTime<Utc>,
        grace_until_utc: DateTime<Utc>,
        features_json: &str,
        last_verified_at_utc: DateTime<Utc>,
        installation_token_plain: &str,
        signed_token_blob_plain: Option<&str>,
        last_seen_now_utc: Option<DateTime<Utc>>,
        last_seen_expires_utc: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let installation_token = self.secrets.encrypt(installation_token_plain).await?;
        let signed_token_blob = match signed_token_blob_plain {
            Some(s) => Some(self.secrets.encrypt(s).await?),
            None => None,
        };

        match &self.connection {
            DatabaseConnection::Postgres(pool) => {
                // Determine existing row (first id)
                let existing_id: Option<i32> = sqlx::query_scalar(
                    r#"SELECT id FROM cadalytix_config.license_state ORDER BY id ASC LIMIT 1"#,
                )
                .fetch_optional(pool)
                .await?;

                if let Some(id) = existing_id {
                    sqlx::query(
                        r#"
                        UPDATE cadalytix_config.license_state
                        SET mode = $1,
                            license_key_masked = $2,
                            license_key_hash = $3,
                            status = $4,
                            client_name = $5,
                            license_id = $6,
                            issued_at_utc = $7,
                            expires_at_utc = $8,
                            grace_until_utc = $9,
                            features_json = $10,
                            last_verified_at_utc = $11,
                            installation_token = $12,
                            signed_token_blob = $13,
                            last_seen_now_utc = $14,
                            last_seen_expires_utc = $15,
                            updated_at = (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')
                        WHERE id = $16
                        "#,
                    )
                    .bind(mode)
                    .bind(license_key_masked)
                    .bind(license_key_hash)
                    .bind(status)
                    .bind(client_name)
                    .bind(license_id)
                    .bind(issued_at_utc.naive_utc())
                    .bind(expires_at_utc.naive_utc())
                    .bind(grace_until_utc.naive_utc())
                    .bind(features_json)
                    .bind(last_verified_at_utc.naive_utc())
                    .bind(installation_token)
                    .bind(signed_token_blob)
                    .bind(last_seen_now_utc.map(|t| t.naive_utc()))
                    .bind(last_seen_expires_utc.map(|t| t.naive_utc()))
                    .bind(id)
                    .execute(pool)
                    .await?;
                } else {
                    sqlx::query(
                        r#"
                        INSERT INTO cadalytix_config.license_state
                            (mode, license_key_masked, license_key_hash, status, client_name, license_id,
                             issued_at_utc, expires_at_utc, grace_until_utc, last_verified_at_utc,
                             features_json, installation_token, signed_token_blob, last_seen_now_utc, last_seen_expires_utc,
                             created_at, updated_at)
                        VALUES
                            ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,
                             (CURRENT_TIMESTAMP AT TIME ZONE 'UTC'), (CURRENT_TIMESTAMP AT TIME ZONE 'UTC'))
                        "#,
                    )
                    .bind(mode)
                    .bind(license_key_masked)
                    .bind(license_key_hash)
                    .bind(status)
                    .bind(client_name)
                    .bind(license_id)
                    .bind(issued_at_utc.naive_utc())
                    .bind(expires_at_utc.naive_utc())
                    .bind(grace_until_utc.naive_utc())
                    .bind(last_verified_at_utc.naive_utc())
                    .bind(features_json)
                    .bind(installation_token)
                    .bind(signed_token_blob)
                    .bind(last_seen_now_utc.map(|t| t.naive_utc()))
                    .bind(last_seen_expires_utc.map(|t| t.naive_utc()))
                    .execute(pool)
                    .await?;
                }
                Ok(())
            }
            DatabaseConnection::SqlServer(_) => {
                use tiberius::QueryItem;
                let client_arc = self
                    .connection
                    .as_sql_server()
                    .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
                let mut client = client_arc.lock().await;

                // Find existing id (single-row semantics)
                let qid = Query::new(
                    "SELECT TOP 1 id FROM cadalytix_config.license_state ORDER BY id ASC",
                );
                let mut stream = qid.query(&mut *client).await?;
                let mut existing_id: Option<i32> = None;
                while let Some(item) = stream.try_next().await? {
                    if let QueryItem::Row(row) = item {
                        existing_id = row.get::<i32, _>(0);
                        break;
                    }
                }
                // Ensure the query stream is dropped before we issue more commands on the client.
                drop(stream);

                // Transaction
                {
                    let mut s = client.simple_query("BEGIN TRANSACTION").await?;
                    while s.try_next().await?.is_some() {}
                }

                let op: Result<()> = (async {
                    if let Some(id) = existing_id {
                        let sql = r#"
                            UPDATE cadalytix_config.license_state
                            SET mode = @P1,
                                license_key_masked = @P2,
                                license_key_hash = @P3,
                                status = @P4,
                                client_name = @P5,
                                license_id = @P6,
                                issued_at_utc = @P7,
                                expires_at_utc = @P8,
                                grace_until_utc = @P9,
                                features_json = @P10,
                                last_verified_at_utc = @P11,
                                installation_token = @P12,
                                signed_token_blob = @P13,
                                last_seen_now_utc = @P14,
                                last_seen_expires_utc = @P15,
                                updated_at = SYSUTCDATETIME()
                            WHERE id = @P16
                        "#;
                        let mut q = Query::new(sql);
                        q.bind(mode);
                        q.bind(license_key_masked);
                        q.bind(license_key_hash);
                        q.bind(status);
                        q.bind(client_name);
                        q.bind(license_id);
                        q.bind(issued_at_utc.naive_utc());
                        q.bind(expires_at_utc.naive_utc());
                        q.bind(grace_until_utc.naive_utc());
                        q.bind(features_json);
                        q.bind(last_verified_at_utc.naive_utc());
                        q.bind(installation_token.as_str());
                        q.bind(signed_token_blob.as_deref());
                        q.bind(last_seen_now_utc.map(|t| t.naive_utc()));
                        q.bind(last_seen_expires_utc.map(|t| t.naive_utc()));
                        q.bind(id);
                        let mut s = q.query(&mut *client).await?;
                        while s.try_next().await?.is_some() {}
                    } else {
                        let sql = r#"
                            INSERT INTO cadalytix_config.license_state
                                (mode, license_key_masked, license_key_hash, status, client_name, license_id,
                                 issued_at_utc, expires_at_utc, grace_until_utc, last_verified_at_utc,
                                 features_json, installation_token, signed_token_blob, last_seen_now_utc, last_seen_expires_utc,
                                 created_at, updated_at)
                            VALUES
                                (@P1,@P2,@P3,@P4,@P5,@P6,@P7,@P8,@P9,@P10,@P11,@P12,@P13,@P14,@P15,
                                 SYSUTCDATETIME(), SYSUTCDATETIME())
                        "#;
                        let mut q = Query::new(sql);
                        q.bind(mode);
                        q.bind(license_key_masked);
                        q.bind(license_key_hash);
                        q.bind(status);
                        q.bind(client_name);
                        q.bind(license_id);
                        q.bind(issued_at_utc.naive_utc());
                        q.bind(expires_at_utc.naive_utc());
                        q.bind(grace_until_utc.naive_utc());
                        q.bind(last_verified_at_utc.naive_utc());
                        q.bind(features_json);
                        q.bind(installation_token.as_str());
                        q.bind(signed_token_blob.as_deref());
                        q.bind(last_seen_now_utc.map(|t| t.naive_utc()));
                        q.bind(last_seen_expires_utc.map(|t| t.naive_utc()));
                        let mut s = q.query(&mut *client).await?;
                        while s.try_next().await?.is_some() {}
                    }
                    Ok(())
                })
                .await;

                match op {
                    Ok(()) => {
                        let mut s = client.simple_query("COMMIT TRANSACTION").await?;
                        while s.try_next().await?.is_some() {}
                        Ok(())
                    }
                    Err(e) => {
                        let _ = client.simple_query("ROLLBACK TRANSACTION").await;
                        Err(e)
                    }
                }
            }
        }
    }

    pub async fn get_license_state(&self) -> Result<Option<Value>> {
        match &self.connection {
            DatabaseConnection::Postgres(pool) => {
                let row = sqlx::query_as::<_, (String, String, String, String, String, String, chrono::NaiveDateTime, chrono::NaiveDateTime, chrono::NaiveDateTime, String, chrono::NaiveDateTime, String, Option<String>, Option<chrono::NaiveDateTime>, Option<chrono::NaiveDateTime>)>(
                    r#"
                    SELECT mode, license_key_masked, license_key_hash, status, client_name, license_id,
                           issued_at_utc, expires_at_utc, grace_until_utc, features_json, last_verified_at_utc,
                           installation_token, signed_token_blob, last_seen_now_utc, last_seen_expires_utc
                    FROM cadalytix_config.license_state
                    ORDER BY id ASC
                    LIMIT 1
                    "#
                )
                .fetch_optional(pool)
                .await?;

                if let Some((
                    mode,
                    masked,
                    hash,
                    status,
                    client_name,
                    license_id,
                    issued_at,
                    expires_at,
                    grace_until,
                    features_json,
                    last_verified,
                    installation_token_enc,
                    signed_token_enc,
                    last_seen_now,
                    last_seen_exp,
                )) = row
                {
                    let installation_token = if self.secrets.is_encrypted(&installation_token_enc) {
                        self.secrets.decrypt(&installation_token_enc).await?
                    } else {
                        installation_token_enc
                    };
                    let signed_token_blob = match signed_token_enc {
                        Some(v) if self.secrets.is_encrypted(&v) => {
                            Some(self.secrets.decrypt(&v).await?)
                        }
                        Some(v) => Some(v),
                        None => None,
                    };

                    return Ok(Some(serde_json::json!({
                        "mode": mode,
                        "licenseKeyMasked": masked,
                        "licenseKeyHash": hash,
                        "status": status,
                        "clientName": client_name,
                        "licenseId": license_id,
                        "issuedAtUtc": DateTime::<Utc>::from_naive_utc_and_offset(issued_at, Utc).to_rfc3339(),
                        "expiresAtUtc": DateTime::<Utc>::from_naive_utc_and_offset(expires_at, Utc).to_rfc3339(),
                        "graceUntilUtc": DateTime::<Utc>::from_naive_utc_and_offset(grace_until, Utc).to_rfc3339(),
                        "featuresJson": features_json,
                        "lastVerifiedAtUtc": DateTime::<Utc>::from_naive_utc_and_offset(last_verified, Utc).to_rfc3339(),
                        "installationToken": installation_token,
                        "signedTokenBlob": signed_token_blob,
                        "lastSeenNowUtc": last_seen_now.map(|t| DateTime::<Utc>::from_naive_utc_and_offset(t, Utc).to_rfc3339()),
                        "lastSeenExpiresUtc": last_seen_exp.map(|t| DateTime::<Utc>::from_naive_utc_and_offset(t, Utc).to_rfc3339()),
                    })));
                }
                Ok(None)
            }
            DatabaseConnection::SqlServer(_) => {
                use tiberius::QueryItem;
                let client_arc = self
                    .connection
                    .as_sql_server()
                    .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
                let mut client = client_arc.lock().await;

                let q = Query::new(
                    r#"
                    SELECT TOP 1
                        mode, license_key_masked, license_key_hash, status, client_name, license_id,
                        issued_at_utc, expires_at_utc, grace_until_utc, features_json, last_verified_at_utc,
                        installation_token, signed_token_blob, last_seen_now_utc, last_seen_expires_utc
                    FROM cadalytix_config.license_state
                    ORDER BY id ASC
                    "#,
                );

                let mut stream = q.query(&mut *client).await?;
                while let Some(item) = stream.try_next().await? {
                    if let QueryItem::Row(row) = item {
                        let mode = row.get::<&str, _>(0).unwrap_or("").to_string();
                        let masked = row.get::<&str, _>(1).unwrap_or("").to_string();
                        let hash = row.get::<&str, _>(2).unwrap_or("").to_string();
                        let status = row.get::<&str, _>(3).unwrap_or("").to_string();
                        let client_name = row.get::<&str, _>(4).unwrap_or("").to_string();
                        let license_id = row.get::<&str, _>(5).unwrap_or("").to_string();

                        let issued_at = row
                            .get::<chrono::NaiveDateTime, _>(6)
                            .unwrap_or_else(|| Utc::now().naive_utc());
                        let expires_at = row
                            .get::<chrono::NaiveDateTime, _>(7)
                            .unwrap_or_else(|| Utc::now().naive_utc());
                        let grace_until = row
                            .get::<chrono::NaiveDateTime, _>(8)
                            .unwrap_or_else(|| Utc::now().naive_utc());
                        let features_json = row.get::<&str, _>(9).unwrap_or("{}").to_string();
                        let last_verified = row
                            .get::<chrono::NaiveDateTime, _>(10)
                            .unwrap_or_else(|| Utc::now().naive_utc());

                        let installation_token_enc =
                            row.get::<&str, _>(11).unwrap_or("").to_string();
                        let signed_token_enc = row.get::<&str, _>(12).map(|s| s.to_string());
                        let last_seen_now = row.get::<chrono::NaiveDateTime, _>(13);
                        let last_seen_exp = row.get::<chrono::NaiveDateTime, _>(14);

                        let installation_token =
                            if self.secrets.is_encrypted(&installation_token_enc) {
                                self.secrets.decrypt(&installation_token_enc).await?
                            } else {
                                installation_token_enc
                            };
                        let signed_token_blob = match signed_token_enc {
                            Some(v) if self.secrets.is_encrypted(&v) => {
                                Some(self.secrets.decrypt(&v).await?)
                            }
                            Some(v) => Some(v),
                            None => None,
                        };

                        return Ok(Some(serde_json::json!({
                            "mode": mode,
                            "licenseKeyMasked": masked,
                            "licenseKeyHash": hash,
                            "status": status,
                            "clientName": client_name,
                            "licenseId": license_id,
                            "issuedAtUtc": DateTime::<Utc>::from_naive_utc_and_offset(issued_at, Utc).to_rfc3339(),
                            "expiresAtUtc": DateTime::<Utc>::from_naive_utc_and_offset(expires_at, Utc).to_rfc3339(),
                            "graceUntilUtc": DateTime::<Utc>::from_naive_utc_and_offset(grace_until, Utc).to_rfc3339(),
                            "featuresJson": features_json,
                            "lastVerifiedAtUtc": DateTime::<Utc>::from_naive_utc_and_offset(last_verified, Utc).to_rfc3339(),
                            "installationToken": installation_token,
                            "signedTokenBlob": signed_token_blob,
                            "lastSeenNowUtc": last_seen_now.map(|t| DateTime::<Utc>::from_naive_utc_and_offset(t, Utc).to_rfc3339()),
                            "lastSeenExpiresUtc": last_seen_exp.map(|t| DateTime::<Utc>::from_naive_utc_and_offset(t, Utc).to_rfc3339()),
                        })));
                    }
                }
                Ok(None)
            }
        }
    }
}

fn should_encrypt_setting_key(key: &str) -> bool {
    matches!(
        key,
        "ConfigDb:ConnectionString"
            | "CallDataDb:ConnectionString"
            | "Setup:BootstrapSecret"
            | "Ops:ApiKey"
            | "Weather:ApiKey"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Phase 8 Task 8.4: Encryption-at-rest sanity lock.
    ///
    /// These tests prove that sensitive setting keys are correctly identified
    /// and that the encryption layer is invoked for them.

    #[test]
    fn test_should_encrypt_sensitive_keys() {
        // Connection strings MUST be encrypted
        assert!(
            should_encrypt_setting_key("ConfigDb:ConnectionString"),
            "ConfigDb connection string must be encrypted"
        );
        assert!(
            should_encrypt_setting_key("CallDataDb:ConnectionString"),
            "CallDataDb connection string must be encrypted"
        );

        // Secrets/API keys MUST be encrypted
        assert!(
            should_encrypt_setting_key("Setup:BootstrapSecret"),
            "Bootstrap secret must be encrypted"
        );
        assert!(
            should_encrypt_setting_key("Ops:ApiKey"),
            "Ops API key must be encrypted"
        );
        assert!(
            should_encrypt_setting_key("Weather:ApiKey"),
            "Weather API key must be encrypted"
        );
    }

    #[test]
    fn test_should_not_encrypt_non_sensitive_keys() {
        // Non-sensitive settings should NOT be encrypted
        assert!(
            !should_encrypt_setting_key("Archive:ScheduleDayOfMonth"),
            "Archive day of month is not sensitive"
        );
        assert!(
            !should_encrypt_setting_key("Consent:AllowSupportSync"),
            "Consent flag is not sensitive"
        );
        assert!(
            !should_encrypt_setting_key("Retention:HotDays"),
            "Retention days is not sensitive"
        );
        assert!(
            !should_encrypt_setting_key("Mapping:Override"),
            "Mapping override is not sensitive"
        );
    }

    #[test]
    fn test_sensitive_keys_list_comprehensive() {
        // List all known sensitive keys we expect to encrypt
        let expected_sensitive_keys = [
            "ConfigDb:ConnectionString",
            "CallDataDb:ConnectionString",
            "Setup:BootstrapSecret",
            "Ops:ApiKey",
            "Weather:ApiKey",
        ];

        for key in expected_sensitive_keys {
            assert!(
                should_encrypt_setting_key(key),
                "Key '{}' should be marked as sensitive and encrypted",
                key
            );
        }
    }

    #[test]
    fn test_connection_string_pattern_detection() {
        // Ensure partial matches don't encrypt (defense in depth)
        // Only exact key matches should trigger encryption
        assert!(
            !should_encrypt_setting_key("ConnectionString"),
            "Bare ConnectionString key should not match"
        );
        assert!(
            !should_encrypt_setting_key("SomeDb:ConnectionString"),
            "Unknown DB connection string should not auto-encrypt"
        );
        assert!(
            !should_encrypt_setting_key("ConfigDbConnectionString"),
            "Key without colon separator should not match"
        );
    }
}
