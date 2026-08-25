use std::{collections::HashSet, time::Duration};

use sqlx::Row;
use storexa::{Database, DatabaseConfig, Migrator};

use crate::Result;

/// Chordrift's strictly ordered, application-owned, embedded migrations.
///
/// The catalog includes routing-inbox policy and its verified-clear constraint.
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Read-only diagnostics for the canonical database.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatabaseStatus {
    /// PostgreSQL server version reported by the connected database.
    pub server_version: String,
    /// End-to-end health-query latency.
    pub latency: Duration,
    /// Number of embedded migrations available in this Chordrift build.
    pub available_migrations: usize,
    /// Number of available migrations already applied successfully.
    pub applied_migrations: usize,
    /// Number of available migrations not yet applied.
    pub pending_migrations: usize,
    /// Number of recorded unsuccessful migration attempts.
    pub failed_migrations: usize,
}

/// Connects to Chordrift's database through Storexa.
pub async fn connect(config: DatabaseConfig) -> Result<Database> {
    Database::connect(config).await.map_err(Into::into)
}

/// Applies every pending embedded Chordrift migration through Storexa.
pub async fn migrate(database: &Database) -> Result<storexa::MigrationReport> {
    database.run_migrations(&MIGRATOR).await.map_err(Into::into)
}

/// Inspects connectivity and migration state without changing the database.
pub async fn status(database: &Database) -> Result<DatabaseStatus> {
    let health = database.health().await?;
    let available_versions: HashSet<i64> =
        MIGRATOR.iter().map(|migration| migration.version).collect();
    let available_migrations = available_versions.len();

    let migration_table: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('_sqlx_migrations')::text")
            .fetch_one(database.pool())
            .await?;

    let (applied_versions, failed_migrations) = if migration_table.is_some() {
        let rows = sqlx::query("SELECT version, success FROM _sqlx_migrations")
            .fetch_all(database.pool())
            .await?;
        let mut applied = HashSet::new();
        let mut failed = 0;

        for row in rows {
            let version: i64 = row.try_get("version")?;
            let success: bool = row.try_get("success")?;
            if success {
                applied.insert(version);
            } else {
                failed += 1;
            }
        }
        (applied, failed)
    } else {
        (HashSet::new(), 0)
    };

    let applied_migrations = available_versions.intersection(&applied_versions).count();
    let pending_migrations = available_migrations.saturating_sub(applied_migrations);

    Ok(DatabaseStatus {
        server_version: health.server_version,
        latency: health.latency,
        available_migrations,
        applied_migrations,
        pending_migrations,
        failed_migrations,
    })
}
