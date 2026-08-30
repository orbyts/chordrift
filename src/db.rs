use std::{collections::HashSet, time::Duration};

use sqlx::Row;
use storexa::{Database, DatabaseConfig, Migrator};

use crate::{ChordriftError, Result};

/// Chordrift's strictly ordered, application-owned, embedded schema and
/// rehearsal-migration support migrations.
///
/// The catalog includes the provider-native Re-evaluate queue, database-v2
/// storage, and the additive provider-neutral product-domain foundation.
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

/// Requires every embedded migration up to and including one version while
/// allowing newer feature-specific migrations to remain unapplied.
pub async fn require_schema_through(database: &Database, required_version: i64) -> Result<()> {
    let required: Vec<i64> = MIGRATOR
        .iter()
        .filter(|migration| migration.version <= required_version)
        .map(|migration| migration.version)
        .collect();
    let applied: Vec<i64> = sqlx::query_scalar(
        "SELECT version FROM _sqlx_migrations
         WHERE success = TRUE AND version <= $1",
    )
    .bind(required_version)
    .fetch_all(database.pool())
    .await?;
    let applied: HashSet<i64> = applied.into_iter().collect();
    if schema_versions_satisfy(&required, &applied) {
        return Ok(());
    }
    Err(ChordriftError::Configuration(format!(
        "database schema required through migration {required_version:04}; run `chordrift db migrate` against the selected deployment"
    )))
}

fn schema_versions_satisfy(required: &[i64], applied: &HashSet<i64>) -> bool {
    required.iter().all(|version| applied.contains(version))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::schema_versions_satisfy;

    #[test]
    fn feature_specific_newer_migration_may_remain_pending() {
        let required = (1_i64..=47).collect::<Vec<_>>();
        let applied = (1_i64..=47).collect::<HashSet<_>>();
        assert!(schema_versions_satisfy(&required, &applied));

        let missing = (1_i64..=46).collect::<HashSet<_>>();
        assert!(!schema_versions_satisfy(&required, &missing));
    }
}
