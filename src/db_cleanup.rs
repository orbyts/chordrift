//! Exact-confirmed database-v1 storage cleanup after database-v2 cutover.

use chrono::{DateTime, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{AssertSqlSafe, Row};
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result, db_reports};

const CLEANUP_VERSION: &str = "database-v2-clean-runtime-v1";

/// Exact, provider-free cleanup proposal for one database.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupPlan {
    /// Selected account label.
    pub account_label: String,
    /// Exact confirmation digest.
    pub plan_sha256: String,
    /// Fingerprint of all durable logical invariants.
    pub invariant_sha256: String,
    /// Lightweight provider observation headers retained.
    pub observations_retained: i64,
    /// Duplicated provider body rows removed.
    pub legacy_provider_rows_removed: i64,
    /// Legacy event rows removed after normalized parity.
    pub legacy_listening_events_removed: i64,
    /// Legacy archive-manifest rows removed after normalized parity.
    pub legacy_archive_imports_removed: i64,
    /// Normalized events retained permanently.
    pub normalized_listening_events_retained: i64,
    /// Provider-neutral evidence imports retained.
    pub evidence_imports_retained: i64,
}

/// Post-cleanup schema and invariant verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupVerification {
    /// Exact plan that was applied.
    pub plan_sha256: String,
    /// Durable invariant fingerprint after cleanup.
    pub invariant_sha256: String,
    /// Whether all old physical table names are absent.
    pub legacy_tables_absent: bool,
    /// Whether every transient provider-import table is empty.
    pub import_staging_empty: bool,
    /// Normalized events retained.
    pub normalized_listening_events: i64,
    /// Evidence imports retained.
    pub evidence_imports: i64,
    /// Verification timestamp from the cleanup receipt.
    pub verified_at: DateTime<Utc>,
    /// Whether every cleanup gate passed.
    pub verified: bool,
}

/// Builds an exact cleanup plan without changing database state.
pub async fn plan(database: &Database, account_label: &str) -> Result<CleanupPlan> {
    let account_id = account_id(database, account_label).await?;
    ensure_pre_cleanup_schema(database).await?;
    let status = db_reports::database_v2_status(database, account_label).await?;
    if !status.ready_for_cutover {
        return Err(ChordriftError::Configuration(
            "database-v2 parity gates are not all satisfied; cleanup is refused".to_owned(),
        ));
    }
    let invariant = db_reports::invariant_report(database, account_label).await?;
    let invariant_sha256 = invariant_sha256(&invariant);
    let row = sqlx::query(
        "SELECT
           (SELECT count(*) FROM provider_library_snapshots
             WHERE provider_account_id = $1) AS observations,
           ((SELECT count(*) FROM provider_playlist_snapshots
              WHERE snapshot_id IN (SELECT id FROM provider_library_snapshots
                                      WHERE provider_account_id = $1)) +
            (SELECT count(*) FROM provider_playlist_tracks
              WHERE snapshot_id IN (SELECT id FROM provider_library_snapshots
                                      WHERE provider_account_id = $1)) +
            (SELECT count(*) FROM provider_saved_tracks
              WHERE snapshot_id IN (SELECT id FROM provider_library_snapshots
                                      WHERE provider_account_id = $1)) +
            (SELECT count(*) FROM provider_saved_albums
              WHERE snapshot_id IN (SELECT id FROM provider_library_snapshots
                                      WHERE provider_account_id = $1)) +
            (SELECT count(*) FROM provider_saved_album_tracks
              WHERE snapshot_id IN (SELECT id FROM provider_library_snapshots
                                      WHERE provider_account_id = $1)))::bigint AS provider_rows,
           (SELECT count(*) FROM listening_events
             WHERE provider_account_id = $1) AS legacy_events,
           (SELECT count(*) FROM spotify_archive_imports
             WHERE provider_account_id = $1) AS legacy_imports,
           (SELECT count(*) FROM normalized_listening_events
             WHERE provider_account_id = $1) AS normalized_events,
           (SELECT count(*) FROM listening_evidence_imports
             WHERE provider_account_id = $1) AS evidence_imports",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let observations_retained = row.try_get("observations")?;
    let legacy_provider_rows_removed = row.try_get("provider_rows")?;
    let legacy_listening_events_removed = row.try_get("legacy_events")?;
    let legacy_archive_imports_removed = row.try_get("legacy_imports")?;
    let normalized_listening_events_retained = row.try_get("normalized_events")?;
    let evidence_imports_retained = row.try_get("evidence_imports")?;
    if legacy_listening_events_removed != normalized_listening_events_retained
        || legacy_archive_imports_removed != evidence_imports_retained
    {
        return Err(ChordriftError::Configuration(
            "legacy and database-v2 listening evidence counts differ; cleanup is refused"
                .to_owned(),
        ));
    }
    let plan_sha256 = digest(&format!(
        "{CLEANUP_VERSION}\0{account_id}\0{invariant_sha256}\0{observations_retained}\0\
         {legacy_provider_rows_removed}\0{legacy_listening_events_removed}\0\
         {legacy_archive_imports_removed}\0{normalized_listening_events_retained}\0\
         {evidence_imports_retained}"
    ));
    Ok(CleanupPlan {
        account_label: account_label.to_owned(),
        plan_sha256,
        invariant_sha256,
        observations_retained,
        legacy_provider_rows_removed,
        legacy_listening_events_removed,
        legacy_archive_imports_removed,
        normalized_listening_events_retained,
        evidence_imports_retained,
    })
}

/// Applies only the exact cleanup plan supplied by the operator.
pub async fn apply(
    database: &Database,
    account_label: &str,
    confirmation: &str,
) -> Result<CleanupVerification> {
    let cleanup = plan(database, account_label).await?;
    if confirmation != cleanup.plan_sha256 {
        return Err(ChordriftError::Configuration(format!(
            "cleanup confirmation did not match; rerun `chordrift db compact cleanup plan --account {account_label}`"
        )));
    }
    let account_id = account_id(database, account_label).await?;
    let mut transaction = database.pool().begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('chordrift-database-v2-cleanup'))")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("SELECT id FROM provider_accounts WHERE id = $1 FOR UPDATE")
        .bind(account_id)
        .execute(&mut *transaction)
        .await?;
    let locked_counts = sqlx::query(
        "SELECT
           (SELECT count(*) FROM provider_library_snapshots
             WHERE provider_account_id = $1) AS observations,
           ((SELECT count(*) FROM provider_playlist_snapshots
              WHERE snapshot_id IN (SELECT id FROM provider_library_snapshots
                                      WHERE provider_account_id = $1)) +
            (SELECT count(*) FROM provider_playlist_tracks
              WHERE snapshot_id IN (SELECT id FROM provider_library_snapshots
                                      WHERE provider_account_id = $1)) +
            (SELECT count(*) FROM provider_saved_tracks
              WHERE snapshot_id IN (SELECT id FROM provider_library_snapshots
                                      WHERE provider_account_id = $1)) +
            (SELECT count(*) FROM provider_saved_albums
              WHERE snapshot_id IN (SELECT id FROM provider_library_snapshots
                                      WHERE provider_account_id = $1)) +
            (SELECT count(*) FROM provider_saved_album_tracks
              WHERE snapshot_id IN (SELECT id FROM provider_library_snapshots
                                      WHERE provider_account_id = $1)))::bigint AS provider_rows,
           (SELECT count(*) FROM listening_events
             WHERE provider_account_id = $1) AS legacy_events,
           (SELECT count(*) FROM spotify_archive_imports
             WHERE provider_account_id = $1) AS legacy_imports,
           (SELECT count(*) FROM normalized_listening_events
             WHERE provider_account_id = $1) AS normalized_events,
           (SELECT count(*) FROM listening_evidence_imports
             WHERE provider_account_id = $1) AS evidence_imports",
    )
    .bind(account_id)
    .fetch_one(&mut *transaction)
    .await?;
    if locked_counts.try_get::<i64, _>("observations")? != cleanup.observations_retained
        || locked_counts.try_get::<i64, _>("provider_rows")? != cleanup.legacy_provider_rows_removed
        || locked_counts.try_get::<i64, _>("legacy_events")?
            != cleanup.legacy_listening_events_removed
        || locked_counts.try_get::<i64, _>("legacy_imports")?
            != cleanup.legacy_archive_imports_removed
        || locked_counts.try_get::<i64, _>("normalized_events")?
            != cleanup.normalized_listening_events_retained
        || locked_counts.try_get::<i64, _>("evidence_imports")? != cleanup.evidence_imports_retained
    {
        return Err(ChordriftError::Configuration(
            "database changed after cleanup planning; no cleanup was applied".to_owned(),
        ));
    }
    let materializer_definition: String = sqlx::query_scalar(
        "SELECT pg_get_functiondef(
            'materialize_provider_current_state_v2_collation_legacy(uuid,uuid)'::regprocedure
         )",
    )
    .fetch_one(&mut *transaction)
    .await?;

    sqlx::query("DROP TRIGGER listening_events_v2_dual_write ON listening_events")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DROP TRIGGER spotify_archive_imports_v2_dual_write ON spotify_archive_imports")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DROP FUNCTION sync_listening_event_v2()")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DROP FUNCTION sync_spotify_archive_import_v2()")
        .execute(&mut *transaction)
        .await?;

    for statement in [
        "DROP VIEW provider_inventory_import_playlist_tracks",
        "DROP VIEW provider_inventory_import_playlists",
        "DROP VIEW provider_inventory_import_saved_album_tracks",
        "DROP VIEW provider_inventory_import_saved_albums",
        "DROP VIEW provider_inventory_import_saved_tracks",
        "DROP VIEW provider_inventory_observations",
    ] {
        sqlx::query(statement).execute(&mut *transaction).await?;
    }

    sqlx::query(
        "TRUNCATE provider_playlist_tracks, provider_playlist_snapshots,
                  provider_saved_album_tracks, provider_saved_albums,
                  provider_saved_tracks",
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query("ALTER TABLE provider_library_snapshots RENAME TO provider_inventory_observations")
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "ALTER TABLE provider_playlist_snapshots RENAME TO provider_inventory_import_playlists",
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "ALTER TABLE provider_playlist_tracks RENAME TO provider_inventory_import_playlist_tracks",
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "ALTER TABLE provider_saved_tracks RENAME TO provider_inventory_import_saved_tracks",
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "ALTER TABLE provider_saved_albums RENAME TO provider_inventory_import_saved_albums",
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query("ALTER TABLE provider_saved_album_tracks RENAME TO provider_inventory_import_saved_album_tracks")
        .execute(&mut *transaction)
        .await?;

    // PostgreSQL tracks table dependencies in parsed SQL functions, but the
    // legacy materializer is PL/pgSQL and its statement text must be rewritten
    // explicitly when the physical staging tables receive their v2 names. The
    // source is pg_get_functiondef for this fixed internal function, never user
    // input, and every replacement target is a static identifier.
    let materializer_definition = materializer_definition
        .replace(
            "provider_saved_album_tracks",
            "provider_inventory_import_saved_album_tracks",
        )
        .replace(
            "provider_playlist_snapshots",
            "provider_inventory_import_playlists",
        )
        .replace(
            "provider_playlist_tracks",
            "provider_inventory_import_playlist_tracks",
        )
        .replace(
            "provider_saved_tracks",
            "provider_inventory_import_saved_tracks",
        )
        .replace(
            "provider_saved_albums",
            "provider_inventory_import_saved_albums",
        )
        .replace(
            "provider_library_snapshots",
            "provider_inventory_observations",
        );
    sqlx::raw_sql(AssertSqlSafe(materializer_definition))
        .execute(&mut *transaction)
        .await?;

    sqlx::query("DROP TABLE listening_events")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DROP TABLE spotify_archive_imports")
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "INSERT INTO database_v2_cleanup_runs
         (provider_account_id, plan_sha256, cleanup_version,
          legacy_snapshot_count, legacy_provider_row_count,
          legacy_listening_event_count, legacy_archive_import_count,
          invariant_sha256)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(account_id)
    .bind(&cleanup.plan_sha256)
    .bind(CLEANUP_VERSION)
    .bind(cleanup.observations_retained)
    .bind(cleanup.legacy_provider_rows_removed)
    .bind(cleanup.legacy_listening_events_removed)
    .bind(cleanup.legacy_archive_imports_removed)
    .bind(&cleanup.invariant_sha256)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    verify(database, account_label).await
}

/// Verifies the latest cleanup receipt without provider access.
pub async fn verify(database: &Database, account_label: &str) -> Result<CleanupVerification> {
    let account_id = account_id(database, account_label).await?;
    let receipt = sqlx::query(
        "SELECT plan_sha256, invariant_sha256,
                legacy_listening_event_count, legacy_archive_import_count
         FROM database_v2_cleanup_runs
         WHERE provider_account_id = $1
         ORDER BY applied_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| ChordriftError::Configuration("database has no cleanup receipt".to_owned()))?;
    let plan_sha256: String = receipt.try_get("plan_sha256")?;
    let expected_invariant: String = receipt.try_get("invariant_sha256")?;
    let expected_events: i64 = receipt.try_get("legacy_listening_event_count")?;
    let expected_imports: i64 = receipt.try_get("legacy_archive_import_count")?;
    let invariant = db_reports::invariant_report(database, account_label).await?;
    let invariant_sha256 = invariant_sha256(&invariant);
    let schema = sqlx::query(
        "SELECT
           to_regclass('public.provider_library_snapshots') IS NULL
             AND to_regclass('public.provider_playlist_snapshots') IS NULL
             AND to_regclass('public.provider_playlist_tracks') IS NULL
             AND to_regclass('public.provider_saved_tracks') IS NULL
             AND to_regclass('public.provider_saved_albums') IS NULL
             AND to_regclass('public.provider_saved_album_tracks') IS NULL
             AND to_regclass('public.listening_events') IS NULL
             AND to_regclass('public.spotify_archive_imports') IS NULL AS legacy_absent,
           ((SELECT count(*) FROM provider_inventory_import_playlists) +
            (SELECT count(*) FROM provider_inventory_import_playlist_tracks) +
            (SELECT count(*) FROM provider_inventory_import_saved_tracks) +
            (SELECT count(*) FROM provider_inventory_import_saved_albums) +
            (SELECT count(*) FROM provider_inventory_import_saved_album_tracks)) = 0
             AS staging_empty,
           (SELECT count(*) FROM normalized_listening_events
             WHERE provider_account_id = $1) AS events,
           (SELECT count(*) FROM listening_evidence_imports
             WHERE provider_account_id = $1) AS imports",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let legacy_tables_absent = schema.try_get("legacy_absent")?;
    let import_staging_empty = schema.try_get("staging_empty")?;
    let normalized_listening_events = schema.try_get("events")?;
    let evidence_imports = schema.try_get("imports")?;
    let verified = legacy_tables_absent
        && import_staging_empty
        && invariant_sha256 == expected_invariant
        && normalized_listening_events == expected_events
        && evidence_imports == expected_imports;
    let verified_at = Utc::now();
    sqlx::query(
        "UPDATE database_v2_cleanup_runs
         SET verified_at = $2, verification = $3
         WHERE provider_account_id = $1 AND plan_sha256 = $4",
    )
    .bind(account_id)
    .bind(verified_at)
    .bind(json!({
        "verified": verified,
        "legacy_tables_absent": legacy_tables_absent,
        "import_staging_empty": import_staging_empty,
        "invariant_sha256": invariant_sha256,
        "normalized_listening_events": normalized_listening_events,
        "evidence_imports": evidence_imports,
    }))
    .bind(&plan_sha256)
    .execute(database.pool())
    .await?;
    Ok(CleanupVerification {
        plan_sha256,
        invariant_sha256,
        legacy_tables_absent,
        import_staging_empty,
        normalized_listening_events,
        evidence_imports,
        verified_at,
        verified,
    })
}

async fn account_id(database: &Database, account_label: &str) -> Result<Uuid> {
    sqlx::query_scalar(
        "SELECT id FROM provider_accounts
         WHERE account_label = $1 ORDER BY provider LIMIT 1",
    )
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(format!("unknown account label `{account_label}`"))
    })
}

async fn ensure_pre_cleanup_schema(database: &Database) -> Result<()> {
    let available: bool = sqlx::query_scalar(
        "SELECT to_regclass('public.provider_library_snapshots') IS NOT NULL
             AND to_regclass('public.listening_events') IS NOT NULL
             AND to_regclass('public.database_v2_cleanup_runs') IS NOT NULL",
    )
    .fetch_one(database.pool())
    .await?;
    if !available {
        return Err(ChordriftError::Configuration(
            "cleanup is already applied or migration 0044 is not installed".to_owned(),
        ));
    }
    Ok(())
}

fn invariant_sha256(report: &db_reports::InvariantReport) -> String {
    let archives = report
        .archives
        .iter()
        .map(|archive| {
            format!(
                "{}:{}:{}:{}",
                archive.sha256, archive.kind, archive.events_imported, archive.events_matched
            )
        })
        .collect::<Vec<_>>()
        .join("|");
    digest(&format!(
        "{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
        report.provider_accounts,
        report.provider,
        report.snapshot_id,
        report.playlist_count,
        report.playlist_memberships,
        report.playlist_order_fingerprint,
        report.saved_tracks,
        report.saved_albums,
        report.saved_album_tracks,
        report.canonical_fingerprint,
        report.active_exclusions,
        report.reevaluate_fingerprint,
        report.history.events,
        report.history.unique_tracks,
        report.history.total_ms_played,
        report
            .history
            .first_event_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        report
            .history
            .last_event_at
            .map(|value| value.to_rfc3339())
            .unwrap_or_default(),
        report.verified_apply_runs,
        archives,
    ))
}

fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
