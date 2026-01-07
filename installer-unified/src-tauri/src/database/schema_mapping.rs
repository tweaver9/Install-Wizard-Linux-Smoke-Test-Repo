// Schema mapping repository
// Ported from C# Cadalytix.Data.SqlServer/SchemaMappingRepository.cs

use anyhow::{Context, Result};
use sqlx::{Pool, Postgres};
use std::collections::HashMap;

use crate::database::connection::DatabaseConnection;

/// Get schema mappings for a given source name.
/// Returns map of canonical_field -> source_column.
pub async fn get_mappings(
    connection: &DatabaseConnection,
    source_name: &str,
) -> Result<HashMap<String, String>> {
    match connection {
        DatabaseConnection::Postgres(pool) => get_mappings_postgres(pool, source_name).await,
        DatabaseConnection::SqlServer(_) => get_mappings_sql_server(connection, source_name).await,
    }
}

async fn get_mappings_postgres(
    pool: &Pool<Postgres>,
    source_name: &str,
) -> Result<HashMap<String, String>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT canonical_field, source_column
        FROM cadalytix_config.schema_mapping
        WHERE source_name = $1
        ORDER BY canonical_field
        "#,
    )
    .bind(source_name)
    .fetch_all(pool)
    .await
    .with_context(|| "Failed to query schema mappings (PostgreSQL)")?;

    Ok(rows.into_iter().collect())
}

async fn get_mappings_sql_server(
    connection: &DatabaseConnection,
    source_name: &str,
) -> Result<HashMap<String, String>> {
    use futures::TryStreamExt;
    use tiberius::{Query, QueryItem};

    let client_arc = connection
        .as_sql_server()
        .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
    let mut client = client_arc.lock().await;

    let mut query = Query::new(
        r#"
        SELECT canonical_field, source_column
        FROM cadalytix_config.schema_mapping
        WHERE source_name = @P1
        ORDER BY canonical_field
        "#,
    );
    query.bind(source_name);

    let mut stream = query.query(&mut *client).await?;

    let mut out = HashMap::new();
    while let Some(item) = stream.try_next().await? {
        if let QueryItem::Row(row) = item {
            let canonical = row.get::<&str, _>(0).unwrap_or("").to_string();
            let source = row.get::<&str, _>(1).unwrap_or("").to_string();
            if !canonical.is_empty() {
                out.insert(canonical, source);
            }
        }
    }

    Ok(out)
}

/// Upsert a single mapping.
pub async fn upsert_mapping(
    connection: &DatabaseConnection,
    source_name: &str,
    canonical_field: &str,
    source_column: &str,
) -> Result<()> {
    match connection {
        DatabaseConnection::Postgres(pool) => {
            upsert_mapping_postgres(pool, source_name, canonical_field, source_column).await
        }
        DatabaseConnection::SqlServer(_) => {
            upsert_mapping_sql_server(connection, source_name, canonical_field, source_column).await
        }
    }
}

/// Upsert a single mapping using owned values.
///
/// This avoids non-`'static` borrows in spawned installation tasks.
pub async fn upsert_mapping_owned(
    connection: DatabaseConnection,
    source_name: String,
    canonical_field: String,
    source_column: String,
) -> Result<()> {
    match connection {
        DatabaseConnection::Postgres(pool) => {
            // Same SQL as `upsert_mapping_postgres`, but with owned bind values.
            sqlx::query(
                r#"
                INSERT INTO cadalytix_config.schema_mapping (source_name, canonical_field, source_column, is_required, transform, notes, created_at, updated_at)
                VALUES ($1, $2, $3, false, NULL, NULL, (CURRENT_TIMESTAMP AT TIME ZONE 'UTC'), (CURRENT_TIMESTAMP AT TIME ZONE 'UTC'))
                ON CONFLICT (source_name, canonical_field) DO UPDATE
                SET source_column = EXCLUDED.source_column,
                    updated_at = (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')
                "#,
            )
            .bind(source_name)
            .bind(canonical_field)
            .bind(source_column)
            .execute(&pool)
            .await
            .with_context(|| "Failed to upsert schema mapping (PostgreSQL)")?;
            Ok(())
        }
        DatabaseConnection::SqlServer(conn) => {
            // Rewrap so we can call the existing SQL Server implementation.
            let connection = DatabaseConnection::SqlServer(conn);
            upsert_mapping_sql_server(&connection, &source_name, &canonical_field, &source_column)
                .await
        }
    }
}

async fn upsert_mapping_postgres(
    pool: &Pool<Postgres>,
    source_name: &str,
    canonical_field: &str,
    source_column: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO cadalytix_config.schema_mapping (source_name, canonical_field, source_column, is_required, transform, notes, created_at, updated_at)
        VALUES ($1, $2, $3, false, NULL, NULL, (CURRENT_TIMESTAMP AT TIME ZONE 'UTC'), (CURRENT_TIMESTAMP AT TIME ZONE 'UTC'))
        ON CONFLICT (source_name, canonical_field) DO UPDATE
        SET source_column = EXCLUDED.source_column,
            updated_at = (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')
        "#,
    )
    .bind(source_name)
    .bind(canonical_field)
    .bind(source_column)
    .execute(pool)
    .await
    .with_context(|| "Failed to upsert schema mapping (PostgreSQL)")?;

    Ok(())
}

async fn upsert_mapping_sql_server(
    connection: &DatabaseConnection,
    source_name: &str,
    canonical_field: &str,
    source_column: &str,
) -> Result<()> {
    use futures::TryStreamExt;
    use tiberius::Query;

    let client_arc = connection
        .as_sql_server()
        .ok_or_else(|| anyhow::anyhow!("Not a SQL Server connection"))?;
    let mut client = client_arc.lock().await;

    let sql = r#"
        MERGE INTO cadalytix_config.schema_mapping AS target
        USING (SELECT @P1 AS source_name, @P2 AS canonical_field) AS source
        ON target.source_name = source.source_name AND target.canonical_field = source.canonical_field
        WHEN MATCHED THEN
            UPDATE SET
                source_column = @P3,
                updated_at = SYSUTCDATETIME()
        WHEN NOT MATCHED THEN
            INSERT (source_name, canonical_field, source_column, is_required, transform, notes, created_at, updated_at)
            VALUES (@P1, @P2, @P3, 0, NULL, NULL, SYSUTCDATETIME(), SYSUTCDATETIME());
    "#;

    let mut query = Query::new(sql);
    query.bind(source_name);
    query.bind(canonical_field);
    query.bind(source_column);

    let mut stream = query.query(&mut *client).await?;
    while stream.try_next().await?.is_some() {}
    Ok(())
}
