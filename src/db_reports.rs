//! Read-only database-v2 invariants, physical storage, and compaction planning.

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result, history};

/// One immutable Spotify archive import included in the invariant report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveInvariant {
    /// Content-addressed archive hash.
    pub sha256: String,
    /// Archive family.
    pub kind: String,
    /// Events retained from this import.
    pub events_imported: i64,
    /// Events linked to canonical identities when imported.
    pub events_matched: i64,
    /// Import timestamp.
    pub imported_at: DateTime<Utc>,
}

/// Reusable logical invariant report for one provider account.
#[derive(Clone, Debug, PartialEq)]
pub struct InvariantReport {
    /// Total configured provider accounts.
    pub provider_accounts: i64,
    /// Selected provider.
    pub provider: String,
    /// Selected local account label.
    pub account_label: String,
    /// Latest successful provider snapshot.
    pub snapshot_id: Uuid,
    /// Capture time of the latest successful provider snapshot.
    pub snapshot_captured_at: DateTime<Utc>,
    /// Current provider playlists.
    pub playlist_count: i64,
    /// Current ordered provider playlist memberships.
    pub playlist_memberships: i64,
    /// Deterministic fingerprint of playlist identity, position, and track identity.
    pub playlist_order_fingerprint: String,
    /// Distinct tracks in current provider playlists.
    pub unique_playlist_tracks: i64,
    /// Current saved tracks.
    pub saved_tracks: i64,
    /// Current saved albums.
    pub saved_albums: i64,
    /// Current saved-album track memberships.
    pub saved_album_tracks: i64,
    /// Latest approved canonical proposal.
    pub canonical_generation_id: Uuid,
    /// Canonical playlists in the approved proposal.
    pub canonical_playlists: i64,
    /// Ordered canonical membership rows.
    pub canonical_assignments: i64,
    /// Distinct canonically assigned tracks.
    pub unique_canonical_tracks: i64,
    /// Deterministic fingerprint of canonical concept, position, and track identity.
    pub canonical_fingerprint: String,
    /// Active reversible exclusions.
    pub active_exclusions: i64,
    /// Active Re-evaluate surfaces.
    pub reevaluate_surfaces: i64,
    /// Tracks currently present in the provider Re-evaluate queue.
    pub reevaluate_tracks: i64,
    /// Deterministic fingerprint of current Re-evaluate membership and order.
    pub reevaluate_fingerprint: String,
    /// Permanent listening-history invariants.
    pub history: history::HistorySummary,
    /// Exact content-addressed Spotify archive import records.
    pub archives: Vec<ArchiveInvariant>,
    /// Provider-verified apply runs.
    pub verified_apply_runs: i64,
    /// Latest provider-free synchronization plan.
    pub latest_plan_id: Uuid,
    /// Planner version for the latest plan.
    pub latest_planner_version: String,
    /// Operations in the latest convergence plan.
    pub latest_plan_operations: i64,
    /// Input hash of the latest convergence plan.
    pub latest_plan_input_hash: String,
}

/// Physical storage consumed by one ordinary table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableStorage {
    /// Schema-qualified table name.
    pub table: String,
    /// Main relation fork bytes.
    pub heap_bytes: i64,
    /// Table bytes including auxiliary forks and TOAST.
    pub table_bytes: i64,
    /// All index bytes.
    pub index_bytes: i64,
    /// Table, TOAST, and index bytes.
    pub total_bytes: i64,
}

/// Complete table-level physical storage report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageReport {
    /// Entire current database size reported by PostgreSQL.
    pub database_bytes: i64,
    /// Sum of main relation fork bytes.
    pub heap_bytes: i64,
    /// Sum of table bytes including auxiliary forks and TOAST.
    pub table_bytes: i64,
    /// Sum of index bytes.
    pub index_bytes: i64,
    /// Sum of total relation bytes.
    pub total_bytes: i64,
    /// Per-table detail, largest total relation first.
    pub tables: Vec<TableStorage>,
}

/// Non-mutating description of legacy retention and normalization effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactionPlan {
    /// Selected local account label.
    pub account_label: String,
    /// Provider snapshots currently retained.
    pub snapshots_total: i64,
    /// One latest materialized current-state snapshot.
    pub current_snapshots: i64,
    /// Older snapshots referenced by durable plans, verifications, or audit history.
    pub protected_historical_snapshots: i64,
    /// Older routine snapshots with no durable-history reference.
    pub redundant_routine_snapshots: i64,
    /// Playlist snapshot headers eligible for normalization with redundant snapshots.
    pub redundant_playlist_headers: i64,
    /// Complete ordered playlist bodies eligible for normalization.
    pub redundant_playlist_memberships: i64,
    /// Saved-track snapshot rows eligible for normalization.
    pub redundant_saved_tracks: i64,
    /// Saved-album snapshot rows eligible for normalization.
    pub redundant_saved_albums: i64,
    /// Saved-album membership rows eligible for normalization.
    pub redundant_saved_album_tracks: i64,
    /// Active normalized listening events that must remain permanent.
    pub listening_events: i64,
    /// Historical provider identities represented by those events.
    pub historical_identities: i64,
    /// Raw per-event JSON bytes recoverable from verified immutable archives.
    pub raw_event_json_bytes: i64,
    /// Snapshots referenced by immutable synchronization plans.
    pub plan_protected_snapshots: i64,
    /// Snapshots referenced by managed-playlist verification history.
    pub verification_protected_snapshots: i64,
    /// Snapshots referenced by embedding or signal generations.
    pub generation_protected_snapshots: i64,
    /// Snapshots referenced by external-bookmark audit history.
    pub bookmark_protected_snapshots: i64,
    /// Snapshots referenced by cleanup approval or Re-evaluate history.
    pub intent_audit_protected_snapshots: i64,
}

/// Read-only readiness report for the additive database-v2 schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatabaseV2Status {
    /// Selected local account label.
    pub account_label: String,
    /// Latest successful legacy snapshot.
    pub legacy_snapshot_id: Uuid,
    /// Snapshot currently materialized by the v2 current-state pointer.
    pub current_source_snapshot_id: Option<Uuid>,
    /// Current v2 playlist pointers.
    pub current_playlists: i64,
    /// Current ordered v2 playlist memberships.
    pub current_playlist_tracks: i64,
    /// Immutable playlist bodies retained by content revision.
    pub playlist_revisions: i64,
    /// Whether current playlist identities and mutable headers match.
    pub current_playlist_headers_match: bool,
    /// Whether v1 and v2 current playlist identity and exact order match.
    pub current_playlist_order_matches: bool,
    /// Whether current saved-track identity and order match.
    pub current_saved_tracks_match: bool,
    /// Whether current saved-album identity and order match.
    pub current_saved_albums_match: bool,
    /// Active legacy listening events awaiting normalized migration.
    pub legacy_listening_events: i64,
    /// Normalized v2 listening events.
    pub normalized_listening_events: i64,
    /// Historical provider identities materialized in v2.
    pub historical_identities: i64,
    /// Legacy archive import manifests.
    pub legacy_archive_imports: i64,
    /// Provider-neutral v2 evidence import manifests.
    pub evidence_imports: i64,
    /// Compact provider checkpoints.
    pub checkpoints: i64,
    /// Immutable sync plans still depending on complete legacy snapshots.
    pub plans_awaiting_checkpoints: i64,
    /// Managed verifications still depending on complete legacy snapshots.
    pub verifications_awaiting_checkpoints: i64,
    /// Durable cleanup approvals still depending on complete legacy snapshots.
    pub cleanups_awaiting_checkpoints: i64,
    /// Re-evaluate audit events still depending on complete legacy snapshots.
    pub reevaluations_awaiting_checkpoints: i64,
    /// Whether every required cutover prerequisite is satisfied.
    pub ready_for_cutover: bool,
}

fn sha256_lines(lines: impl IntoIterator<Item = String>) -> String {
    let mut hasher = Sha256::new();
    for line in lines {
        hasher.update(line.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

/// Builds a logical invariant report without changing database state.
pub async fn invariant_report(database: &Database, account_label: &str) -> Result<InvariantReport> {
    let account = sqlx::query(
        "SELECT id, provider FROM provider_accounts WHERE account_label = $1 ORDER BY provider LIMIT 1",
    )
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| ChordriftError::Configuration(format!("unknown account label `{account_label}`")))?;
    let account_id: Uuid = account.try_get("id")?;
    let provider: String = account.try_get("provider")?;
    let provider_accounts: i64 = sqlx::query_scalar("SELECT count(*) FROM provider_accounts")
        .fetch_one(database.pool())
        .await?;

    let snapshot = sqlx::query(
        "SELECT snapshot.id, snapshot.captured_at
         FROM provider_import_runs run
         JOIN provider_inventory_observations snapshot ON snapshot.id = run.snapshot_id
         WHERE run.provider_account_id = $1 AND run.status = 'succeeded'
         ORDER BY run.finished_at DESC NULLS LAST, run.id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration("account has no successful provider snapshot".to_owned())
    })?;
    let snapshot_id: Uuid = snapshot.try_get("id")?;
    let snapshot_captured_at: DateTime<Utc> = snapshot.try_get("captured_at")?;

    let provider_state = sqlx::query(
        "SELECT
           (SELECT count(*) FROM provider_observed_playlists WHERE snapshot_id = $1) AS playlists,
           count(*) AS memberships,
           count(DISTINCT track.provider_track_id) AS unique_tracks,
           (SELECT count(*) FROM provider_observed_saved_tracks WHERE snapshot_id = $1) AS saved_tracks,
           (SELECT count(*) FROM provider_observed_saved_albums WHERE snapshot_id = $1) AS saved_albums,
           (SELECT count(*) FROM provider_observed_saved_album_tracks WHERE snapshot_id = $1) AS saved_album_tracks
         FROM provider_observed_playlist_tracks member
         JOIN provider_tracks track ON track.id = member.provider_track_id
         WHERE member.snapshot_id = $1",
    )
    .bind(snapshot_id)
    .fetch_one(database.pool())
    .await?;
    let provider_order_rows = sqlx::query(
        "SELECT playlist.provider_playlist_id, member.position, track.provider_track_id
         FROM provider_observed_playlist_tracks member
         JOIN provider_playlists playlist ON playlist.id = member.provider_playlist_id
         JOIN provider_tracks track ON track.id = member.provider_track_id
         WHERE member.snapshot_id = $1
         ORDER BY playlist.provider_playlist_id COLLATE \"C\", member.position,
                  track.provider_track_id COLLATE \"C\"",
    )
    .bind(snapshot_id)
    .fetch_all(database.pool())
    .await?;
    let playlist_order_fingerprint = sha256_lines(provider_order_rows.into_iter().map(|row| {
        format!(
            "{}:{}:{}",
            row.get::<String, _>("provider_playlist_id"),
            row.get::<i32, _>("position"),
            row.get::<String, _>("provider_track_id")
        )
    }));

    let canonical = sqlx::query(
        "WITH generation AS (
           SELECT id FROM playlist_generations
           WHERE provider_account_id = $1 AND status = 'approved'
           ORDER BY approved_at DESC NULLS LAST, created_at DESC, id DESC LIMIT 1
         )
         SELECT generation.id AS generation_id,
           count(DISTINCT playlist.id) AS playlists,
           count(member.id) AS assignments,
           count(DISTINCT member.track_id) AS unique_tracks
         FROM generation
         JOIN playlists playlist ON playlist.generation_id = generation.id
         LEFT JOIN playlist_tracks member ON member.playlist_id = playlist.id
         GROUP BY generation.id",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration("account has no approved canonical generation".to_owned())
    })?;
    let canonical_generation_id: Uuid = canonical.try_get("generation_id")?;
    let canonical_order_rows = sqlx::query(
        "SELECT playlist.concept_id, member.position, member.track_id
         FROM playlists playlist
         JOIN playlist_tracks member ON member.playlist_id = playlist.id
         WHERE playlist.generation_id = $1
         ORDER BY playlist.concept_id, member.position, member.track_id",
    )
    .bind(canonical_generation_id)
    .fetch_all(database.pool())
    .await?;
    let canonical_fingerprint = sha256_lines(canonical_order_rows.into_iter().map(|row| {
        format!(
            "{}:{}:{}",
            row.get::<Uuid, _>("concept_id"),
            row.get::<i32, _>("position"),
            row.get::<Uuid, _>("track_id")
        )
    }));

    let intent = sqlx::query(
        "WITH reevaluate AS (
           SELECT surface.playlist_id, provider_playlist.id AS provider_playlist_id
           FROM routing_surfaces surface
           LEFT JOIN provider_playlists provider_playlist ON provider_playlist.playlist_id = surface.playlist_id
           WHERE surface.provider_account_id = $1 AND surface.active AND surface.purpose = 'reevaluate'
         )
         SELECT
           (SELECT count(*) FROM excluded_tracks WHERE provider_account_id = $1 AND restored_at IS NULL) AS exclusions,
           (SELECT count(*) FROM reevaluate) AS surfaces,
           count(member.provider_track_id) AS queue_tracks
         FROM reevaluate
         LEFT JOIN provider_observed_playlist_tracks member
           ON member.snapshot_id = $2 AND member.provider_playlist_id = reevaluate.provider_playlist_id",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .fetch_one(database.pool())
    .await?;
    let reevaluate_order_rows = sqlx::query(
        "SELECT member.position, track.provider_track_id
         FROM routing_surfaces surface
         JOIN provider_playlists playlist ON playlist.playlist_id = surface.playlist_id
         JOIN provider_observed_playlist_tracks member
           ON member.snapshot_id = $2 AND member.provider_playlist_id = playlist.id
         JOIN provider_tracks track ON track.id = member.provider_track_id
         WHERE surface.provider_account_id = $1 AND surface.active
           AND surface.purpose = 'reevaluate'
         ORDER BY member.position, track.provider_track_id COLLATE \"C\"",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .fetch_all(database.pool())
    .await?;
    let reevaluate_fingerprint = sha256_lines(reevaluate_order_rows.into_iter().map(|row| {
        format!(
            "{}:{}",
            row.get::<i32, _>("position"),
            row.get::<String, _>("provider_track_id")
        )
    }));

    let history = history::summary(database, account_label).await?;
    let archive_rows = sqlx::query(
        "SELECT archive_sha256, archive_kind, event_count AS events_imported,
                COALESCE((manifest->>'events_matched')::bigint, 0) AS events_matched,
                imported_at
         FROM listening_evidence_imports WHERE provider_account_id = $1
         ORDER BY imported_at, archive_sha256 COLLATE \"C\"",
    )
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    let archives = archive_rows
        .into_iter()
        .map(|row| {
            Ok(ArchiveInvariant {
                sha256: row.try_get("archive_sha256")?,
                kind: row.try_get("archive_kind")?,
                events_imported: row.try_get("events_imported")?,
                events_matched: row.try_get("events_matched")?,
                imported_at: row.try_get("imported_at")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let convergence = sqlx::query(
        "WITH latest AS (
           SELECT id, planner_version, input_hash
           FROM sync_runs
           WHERE provider_account_id = $1 AND mode = 'dry_run' AND status = 'planned'
           ORDER BY started_at DESC, id DESC LIMIT 1
         )
         SELECT latest.id, latest.planner_version, latest.input_hash,
                (SELECT count(*) FROM sync_operations WHERE sync_run_id = latest.id) AS operations,
                (SELECT count(*) FROM sync_apply_runs
                  WHERE provider_account_id = $1 AND status = 'succeeded') AS verified_apply_runs
         FROM latest",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| ChordriftError::Configuration("account has no convergence plan".to_owned()))?;

    Ok(InvariantReport {
        provider_accounts,
        provider,
        account_label: account_label.to_owned(),
        snapshot_id,
        snapshot_captured_at,
        playlist_count: provider_state.try_get("playlists")?,
        playlist_memberships: provider_state.try_get("memberships")?,
        playlist_order_fingerprint,
        unique_playlist_tracks: provider_state.try_get("unique_tracks")?,
        saved_tracks: provider_state.try_get("saved_tracks")?,
        saved_albums: provider_state.try_get("saved_albums")?,
        saved_album_tracks: provider_state.try_get("saved_album_tracks")?,
        canonical_generation_id,
        canonical_playlists: canonical.try_get("playlists")?,
        canonical_assignments: canonical.try_get("assignments")?,
        unique_canonical_tracks: canonical.try_get("unique_tracks")?,
        canonical_fingerprint,
        active_exclusions: intent.try_get("exclusions")?,
        reevaluate_surfaces: intent.try_get("surfaces")?,
        reevaluate_tracks: intent.try_get("queue_tracks")?,
        reevaluate_fingerprint,
        history,
        archives,
        verified_apply_runs: convergence.try_get("verified_apply_runs")?,
        latest_plan_id: convergence.try_get("id")?,
        latest_planner_version: convergence.try_get("planner_version")?,
        latest_plan_operations: convergence.try_get("operations")?,
        latest_plan_input_hash: convergence.try_get("input_hash")?,
    })
}

/// Reports physical storage for every user table without changing database state.
pub async fn storage_report(database: &Database) -> Result<StorageReport> {
    let database_bytes: i64 = sqlx::query_scalar("SELECT pg_database_size(current_database())")
        .fetch_one(database.pool())
        .await?;
    let rows = sqlx::query(
        "SELECT schemaname || '.' || relname AS table_name,
                pg_relation_size(relid)::bigint AS heap_bytes,
                pg_table_size(relid)::bigint AS table_bytes,
                pg_indexes_size(relid)::bigint AS index_bytes,
                pg_total_relation_size(relid)::bigint AS total_bytes
         FROM pg_stat_user_tables
         ORDER BY pg_total_relation_size(relid) DESC, schemaname, relname",
    )
    .fetch_all(database.pool())
    .await?;
    let tables = rows
        .into_iter()
        .map(|row| {
            Ok(TableStorage {
                table: row.try_get("table_name")?,
                heap_bytes: row.try_get("heap_bytes")?,
                table_bytes: row.try_get("table_bytes")?,
                index_bytes: row.try_get("index_bytes")?,
                total_bytes: row.try_get("total_bytes")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(StorageReport {
        database_bytes,
        heap_bytes: tables.iter().map(|row| row.heap_bytes).sum(),
        table_bytes: tables.iter().map(|row| row.table_bytes).sum(),
        index_bytes: tables.iter().map(|row| row.index_bytes).sum(),
        total_bytes: tables.iter().map(|row| row.total_bytes).sum(),
        tables,
    })
}

/// Plans retention and normalization effects inside a read-only transaction.
pub async fn compaction_plan(database: &Database, account_label: &str) -> Result<CompactionPlan> {
    let mut transaction = database.pool().begin().await?;
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *transaction)
        .await?;
    let account_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM provider_accounts WHERE account_label = $1 ORDER BY provider LIMIT 1",
    )
    .bind(account_label)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(format!("unknown account label `{account_label}`"))
    })?;

    let row = sqlx::query(
        "WITH current_snapshot AS (
           SELECT snapshot_id AS id FROM provider_import_runs
           WHERE provider_account_id = $1 AND status = 'succeeded'
           ORDER BY finished_at DESC NULLS LAST, id DESC LIMIT 1
         ), durable_snapshot AS (
           SELECT source_snapshot_id AS id FROM embedding_generations WHERE provider_account_id = $1
           UNION SELECT source_snapshot_id FROM signal_generations WHERE provider_account_id = $1
           UNION SELECT source_snapshot_id FROM sync_runs WHERE provider_account_id = $1
           UNION SELECT source_snapshot_id FROM external_playlist_cleanup_batches WHERE provider_account_id = $1
           UNION SELECT verified_snapshot_id FROM managed_playlist_verifications WHERE provider_account_id = $1
           UNION SELECT provider_snapshot_id FROM reevaluation_events WHERE provider_account_id = $1
           UNION SELECT bookmark.snapshot_id
             FROM external_playlist_bookmark_snapshots bookmark
             JOIN provider_library_snapshots snapshot ON snapshot.id = bookmark.snapshot_id
            WHERE snapshot.provider_account_id = $1
         ), redundant AS (
           SELECT snapshot.id FROM provider_library_snapshots snapshot
           WHERE snapshot.provider_account_id = $1
             AND snapshot.id NOT IN (SELECT id FROM current_snapshot)
             AND snapshot.id NOT IN (SELECT id FROM durable_snapshot)
         )
         SELECT
           (SELECT count(*) FROM provider_library_snapshots WHERE provider_account_id = $1) AS snapshots_total,
           (SELECT count(*) FROM current_snapshot) AS current_snapshots,
           (SELECT count(*) FROM durable_snapshot WHERE id NOT IN (SELECT id FROM current_snapshot)) AS protected_snapshots,
           (SELECT count(*) FROM redundant) AS redundant_snapshots,
           (SELECT count(*) FROM provider_playlist_snapshots WHERE snapshot_id IN (SELECT id FROM redundant)) AS redundant_playlist_headers,
           (SELECT count(*) FROM provider_playlist_tracks WHERE snapshot_id IN (SELECT id FROM redundant)) AS redundant_playlist_memberships,
           (SELECT count(*) FROM provider_saved_tracks WHERE snapshot_id IN (SELECT id FROM redundant)) AS redundant_saved_tracks,
           (SELECT count(*) FROM provider_saved_albums WHERE snapshot_id IN (SELECT id FROM redundant)) AS redundant_saved_albums,
           (SELECT count(*) FROM provider_saved_album_tracks WHERE snapshot_id IN (SELECT id FROM redundant)) AS redundant_saved_album_tracks,
           (SELECT count(*) FROM listening_events WHERE provider_account_id = $1 AND media_type = 'track' AND superseded_at IS NULL) AS listening_events,
           (SELECT count(DISTINCT provider_track_id) FROM listening_events WHERE provider_account_id = $1 AND media_type = 'track' AND superseded_at IS NULL) AS historical_identities,
           (SELECT COALESCE(sum(pg_column_size(raw_metadata)), 0)::bigint FROM listening_events WHERE provider_account_id = $1 AND media_type = 'track' AND superseded_at IS NULL) AS raw_event_json_bytes,
           (SELECT count(DISTINCT source_snapshot_id) FROM sync_runs WHERE provider_account_id = $1) AS plan_protected,
           (SELECT count(DISTINCT verified_snapshot_id) FROM managed_playlist_verifications WHERE provider_account_id = $1) AS verification_protected,
           (SELECT count(DISTINCT source_snapshot_id) FROM (
              SELECT source_snapshot_id FROM embedding_generations WHERE provider_account_id = $1
              UNION ALL SELECT source_snapshot_id FROM signal_generations WHERE provider_account_id = $1
            ) generation) AS generation_protected,
           (SELECT count(DISTINCT bookmark.snapshot_id)
              FROM external_playlist_bookmark_snapshots bookmark
              JOIN provider_library_snapshots snapshot ON snapshot.id = bookmark.snapshot_id
             WHERE snapshot.provider_account_id = $1) AS bookmark_protected,
           (SELECT count(DISTINCT id) FROM (
              SELECT source_snapshot_id AS id FROM external_playlist_cleanup_batches WHERE provider_account_id = $1
              UNION ALL SELECT provider_snapshot_id FROM reevaluation_events WHERE provider_account_id = $1
            ) intent_audit) AS intent_audit_protected",
    )
    .bind(account_id)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.rollback().await?;

    Ok(CompactionPlan {
        account_label: account_label.to_owned(),
        snapshots_total: row.try_get("snapshots_total")?,
        current_snapshots: row.try_get("current_snapshots")?,
        protected_historical_snapshots: row.try_get("protected_snapshots")?,
        redundant_routine_snapshots: row.try_get("redundant_snapshots")?,
        redundant_playlist_headers: row.try_get("redundant_playlist_headers")?,
        redundant_playlist_memberships: row.try_get("redundant_playlist_memberships")?,
        redundant_saved_tracks: row.try_get("redundant_saved_tracks")?,
        redundant_saved_albums: row.try_get("redundant_saved_albums")?,
        redundant_saved_album_tracks: row.try_get("redundant_saved_album_tracks")?,
        listening_events: row.try_get("listening_events")?,
        historical_identities: row.try_get("historical_identities")?,
        raw_event_json_bytes: row.try_get("raw_event_json_bytes")?,
        plan_protected_snapshots: row.try_get("plan_protected")?,
        verification_protected_snapshots: row.try_get("verification_protected")?,
        generation_protected_snapshots: row.try_get("generation_protected")?,
        bookmark_protected_snapshots: row.try_get("bookmark_protected")?,
        intent_audit_protected_snapshots: row.try_get("intent_audit_protected")?,
    })
}

/// Reports additive v2 materialization and intentionally unsatisfied cutover gates.
pub async fn database_v2_status(
    database: &Database,
    account_label: &str,
) -> Result<DatabaseV2Status> {
    let account_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM provider_accounts WHERE account_label = $1 ORDER BY provider LIMIT 1",
    )
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(format!("unknown account label `{account_label}`"))
    })?;
    let legacy_snapshot_id: Uuid = sqlx::query_scalar(
        "SELECT snapshot_id FROM provider_import_runs
         WHERE provider_account_id = $1 AND status = 'succeeded'
           AND snapshot_id IS NOT NULL
         ORDER BY finished_at DESC NULLS LAST, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration("account has no successful snapshot".to_owned())
    })?;

    let current = sqlx::query(
        "SELECT source_snapshot_id,
                (SELECT count(*) FROM provider_current_playlists
                  WHERE provider_account_id = $1) AS playlists,
                (SELECT count(*)
                   FROM provider_current_playlists current_playlist
                   JOIN provider_playlist_revision_tracks member
                     ON member.revision_id = current_playlist.revision_id
                  WHERE current_playlist.provider_account_id = $1) AS playlist_tracks,
                (SELECT count(DISTINCT revision_id)
                   FROM provider_current_playlists
                  WHERE provider_account_id = $1) AS revisions
         FROM provider_current_inventories WHERE provider_account_id = $1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?;

    let legacy_playlist_rows = sqlx::query(
        "SELECT playlist.provider_playlist_id, member.position, track.provider_track_id
         FROM provider_observed_playlist_tracks member
         JOIN provider_playlists playlist ON playlist.id = member.provider_playlist_id
         JOIN provider_tracks track ON track.id = member.provider_track_id
         WHERE member.snapshot_id = $1
         ORDER BY playlist.provider_playlist_id COLLATE \"C\", member.position,
                  track.provider_track_id COLLATE \"C\"",
    )
    .bind(legacy_snapshot_id)
    .fetch_all(database.pool())
    .await?;
    let current_playlist_rows = sqlx::query(
        "SELECT playlist.provider_playlist_id, member.position, track.provider_track_id
         FROM provider_current_playlists current_playlist
         JOIN provider_playlists playlist ON playlist.id = current_playlist.provider_playlist_id
         JOIN provider_playlist_revision_tracks member
           ON member.revision_id = current_playlist.revision_id
         JOIN provider_tracks track ON track.id = member.provider_track_id
         WHERE current_playlist.provider_account_id = $1
         ORDER BY playlist.provider_playlist_id COLLATE \"C\", member.position,
                  track.provider_track_id COLLATE \"C\"",
    )
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    let ordered_fingerprint = |rows: Vec<sqlx::postgres::PgRow>| {
        sha256_lines(rows.into_iter().map(|row| {
            format!(
                "{}:{}:{}",
                row.get::<String, _>("provider_playlist_id"),
                row.get::<i32, _>("position"),
                row.get::<String, _>("provider_track_id")
            )
        }))
    };
    let current_playlist_order_matches =
        ordered_fingerprint(legacy_playlist_rows) == ordered_fingerprint(current_playlist_rows);

    let current_playlist_headers_match: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS (
           (SELECT playlist.provider_playlist_id, observed.name, observed.description,
                   observed.public, observed.collaborative,
                   observed.provider_snapshot_id, observed.total_items, observed.metadata
              FROM provider_observed_playlists observed
              JOIN provider_playlists playlist ON playlist.id = observed.provider_playlist_id
             WHERE observed.snapshot_id = $2
            EXCEPT
            SELECT playlist.provider_playlist_id, current.name, current.description,
                   current.public, current.collaborative,
                   current.provider_revision, current.reported_item_count, current.metadata
              FROM provider_current_playlists current
              JOIN provider_playlists playlist ON playlist.id = current.provider_playlist_id
             WHERE current.provider_account_id = $1)
           UNION ALL
           (SELECT playlist.provider_playlist_id, current.name, current.description,
                   current.public, current.collaborative,
                   current.provider_revision, current.reported_item_count, current.metadata
              FROM provider_current_playlists current
              JOIN provider_playlists playlist ON playlist.id = current.provider_playlist_id
             WHERE current.provider_account_id = $1
            EXCEPT
            SELECT playlist.provider_playlist_id, observed.name, observed.description,
                   observed.public, observed.collaborative,
                   observed.provider_snapshot_id, observed.total_items, observed.metadata
              FROM provider_observed_playlists observed
              JOIN provider_playlists playlist ON playlist.id = observed.provider_playlist_id
             WHERE observed.snapshot_id = $2)
         )",
    )
    .bind(account_id)
    .bind(legacy_snapshot_id)
    .fetch_one(database.pool())
    .await?;

    let saved_match = sqlx::query(
        "SELECT
           NOT EXISTS (
             (SELECT saved.position, track.provider, track.provider_track_id, saved.saved_at
                FROM provider_observed_saved_tracks saved
                JOIN provider_tracks track ON track.id = saved.provider_track_id
               WHERE saved.snapshot_id = $2
              EXCEPT
              SELECT saved.position, track.provider, track.provider_track_id, saved.saved_at
                FROM provider_current_inventories inventory
                JOIN provider_saved_track_revision_tracks saved
                  ON saved.revision_id = inventory.saved_track_revision_id
                JOIN provider_tracks track ON track.id = saved.provider_track_id
               WHERE inventory.provider_account_id = $1)
             UNION ALL
             (SELECT saved.position, track.provider, track.provider_track_id, saved.saved_at
                FROM provider_current_inventories inventory
                JOIN provider_saved_track_revision_tracks saved
                  ON saved.revision_id = inventory.saved_track_revision_id
                JOIN provider_tracks track ON track.id = saved.provider_track_id
               WHERE inventory.provider_account_id = $1
              EXCEPT
              SELECT saved.position, track.provider, track.provider_track_id, saved.saved_at
                FROM provider_observed_saved_tracks saved
                JOIN provider_tracks track ON track.id = saved.provider_track_id
               WHERE saved.snapshot_id = $2)
           ) AS tracks_match,
           NOT EXISTS (
             (SELECT album.position, provider_album.provider,
                     provider_album.provider_album_id, album.saved_at
                FROM provider_observed_saved_albums album
                JOIN provider_albums provider_album ON provider_album.id = album.provider_album_id
               WHERE album.snapshot_id = $2
              EXCEPT
              SELECT album.position, provider_album.provider,
                     provider_album.provider_album_id, album.saved_at
                FROM provider_current_inventories inventory
                JOIN provider_saved_album_revision_albums album
                  ON album.revision_id = inventory.saved_album_revision_id
                JOIN provider_albums provider_album ON provider_album.id = album.provider_album_id
               WHERE inventory.provider_account_id = $1)
             UNION ALL
             (SELECT album.position, provider_album.provider,
                     provider_album.provider_album_id, album.saved_at
                FROM provider_current_inventories inventory
                JOIN provider_saved_album_revision_albums album
                  ON album.revision_id = inventory.saved_album_revision_id
                JOIN provider_albums provider_album ON provider_album.id = album.provider_album_id
               WHERE inventory.provider_account_id = $1
              EXCEPT
              SELECT album.position, provider_album.provider,
                     provider_album.provider_album_id, album.saved_at
                FROM provider_observed_saved_albums album
                JOIN provider_albums provider_album ON provider_album.id = album.provider_album_id
               WHERE album.snapshot_id = $2)
           ) AND NOT EXISTS (
             (SELECT provider_album.provider, provider_album.provider_album_id,
                     track.position, provider_track.provider, provider_track.provider_track_id
                FROM provider_observed_saved_album_tracks track
                JOIN provider_albums provider_album ON provider_album.id = track.provider_album_id
                JOIN provider_tracks provider_track ON provider_track.id = track.provider_track_id
               WHERE track.snapshot_id = $2
              EXCEPT
              SELECT provider_album.provider, provider_album.provider_album_id,
                     track.position, provider_track.provider, provider_track.provider_track_id
                FROM provider_current_inventories inventory
                JOIN provider_saved_album_revision_tracks track
                  ON track.revision_id = inventory.saved_album_revision_id
                JOIN provider_albums provider_album ON provider_album.id = track.provider_album_id
                JOIN provider_tracks provider_track ON provider_track.id = track.provider_track_id
               WHERE inventory.provider_account_id = $1)
             UNION ALL
             (SELECT provider_album.provider, provider_album.provider_album_id,
                     track.position, provider_track.provider, provider_track.provider_track_id
                FROM provider_current_inventories inventory
                JOIN provider_saved_album_revision_tracks track
                  ON track.revision_id = inventory.saved_album_revision_id
                JOIN provider_albums provider_album ON provider_album.id = track.provider_album_id
                JOIN provider_tracks provider_track ON provider_track.id = track.provider_track_id
               WHERE inventory.provider_account_id = $1
              EXCEPT
              SELECT provider_album.provider, provider_album.provider_album_id,
                     track.position, provider_track.provider, provider_track.provider_track_id
                FROM provider_observed_saved_album_tracks track
                JOIN provider_albums provider_album ON provider_album.id = track.provider_album_id
                JOIN provider_tracks provider_track ON provider_track.id = track.provider_track_id
               WHERE track.snapshot_id = $2)
           ) AS albums_match",
    )
    .bind(account_id)
    .bind(legacy_snapshot_id)
    .fetch_one(database.pool())
    .await?;

    let legacy_evidence_available: bool = sqlx::query_scalar(
        "SELECT to_regclass('public.listening_events') IS NOT NULL
             AND to_regclass('public.spotify_archive_imports') IS NOT NULL",
    )
    .fetch_one(database.pool())
    .await?;
    let gates = if legacy_evidence_available {
        sqlx::query(
            "SELECT
               (SELECT count(*) FROM listening_events
                 WHERE provider_account_id = $1 AND media_type = 'track'
                   AND superseded_at IS NULL) AS legacy_events,
               (SELECT count(*) FROM normalized_listening_events
                 WHERE provider_account_id = $1 AND superseded_at IS NULL) AS normalized_events,
               (SELECT count(*) FROM historical_provider_track_identities) AS identities,
               (SELECT count(*) FROM spotify_archive_imports
                 WHERE provider_account_id = $1) AS legacy_imports,
               (SELECT count(*) FROM listening_evidence_imports
                 WHERE provider_account_id = $1) AS evidence_imports,
               (SELECT count(*) FROM provider_inventory_checkpoints
                 WHERE provider_account_id = $1 AND released_at IS NULL) AS checkpoints,
               (SELECT count(*) FROM sync_runs
                 WHERE provider_account_id = $1 AND source_snapshot_id IS NOT NULL
                   AND provider_checkpoint_id IS NULL) AS plans_awaiting,
               (SELECT count(*) FROM managed_playlist_verifications
                 WHERE provider_account_id = $1 AND verified_snapshot_id IS NOT NULL
                   AND provider_checkpoint_id IS NULL) AS verifications_awaiting,
               (SELECT count(*) FROM external_playlist_cleanup_batches
                 WHERE provider_account_id = $1 AND source_snapshot_id IS NOT NULL
                   AND provider_checkpoint_id IS NULL) AS cleanups_awaiting,
               (SELECT count(*) FROM reevaluation_events
                 WHERE provider_account_id = $1 AND provider_snapshot_id IS NOT NULL
                   AND provider_checkpoint_id IS NULL) AS reevaluations_awaiting",
        )
        .bind(account_id)
        .fetch_one(database.pool())
        .await?
    } else {
        sqlx::query(
            "SELECT
               (SELECT count(*) FROM normalized_listening_events
                 WHERE provider_account_id = $1 AND superseded_at IS NULL) AS legacy_events,
               (SELECT count(*) FROM normalized_listening_events
                 WHERE provider_account_id = $1 AND superseded_at IS NULL) AS normalized_events,
               (SELECT count(*) FROM historical_provider_track_identities) AS identities,
               (SELECT count(*) FROM listening_evidence_imports
                 WHERE provider_account_id = $1) AS legacy_imports,
               (SELECT count(*) FROM listening_evidence_imports
                 WHERE provider_account_id = $1) AS evidence_imports,
               (SELECT count(*) FROM provider_inventory_checkpoints
                 WHERE provider_account_id = $1 AND released_at IS NULL) AS checkpoints,
               0::bigint AS plans_awaiting,
               0::bigint AS verifications_awaiting,
               0::bigint AS cleanups_awaiting,
               0::bigint AS reevaluations_awaiting",
        )
        .bind(account_id)
        .fetch_one(database.pool())
        .await?
    };

    let current_source_snapshot_id = current
        .as_ref()
        .map(|row| row.try_get("source_snapshot_id"))
        .transpose()?;
    let current_playlists = current
        .as_ref()
        .map_or(Ok(0), |row| row.try_get("playlists"))?;
    let current_playlist_tracks = current
        .as_ref()
        .map_or(Ok(0), |row| row.try_get("playlist_tracks"))?;
    let playlist_revisions = current
        .as_ref()
        .map_or(Ok(0), |row| row.try_get("revisions"))?;
    let current_saved_tracks_match: bool = saved_match.try_get("tracks_match")?;
    let current_saved_albums_match: bool = saved_match.try_get("albums_match")?;
    let legacy_listening_events: i64 = gates.try_get("legacy_events")?;
    let normalized_listening_events: i64 = gates.try_get("normalized_events")?;
    let legacy_archive_imports: i64 = gates.try_get("legacy_imports")?;
    let evidence_imports: i64 = gates.try_get("evidence_imports")?;
    let plans_awaiting_checkpoints: i64 = gates.try_get("plans_awaiting")?;
    let verifications_awaiting_checkpoints: i64 = gates.try_get("verifications_awaiting")?;
    let cleanups_awaiting_checkpoints: i64 = gates.try_get("cleanups_awaiting")?;
    let reevaluations_awaiting_checkpoints: i64 = gates.try_get("reevaluations_awaiting")?;
    let ready_for_cutover = current_source_snapshot_id == Some(legacy_snapshot_id)
        && current_playlist_order_matches
        && current_playlist_headers_match
        && current_saved_tracks_match
        && current_saved_albums_match
        && normalized_listening_events == legacy_listening_events
        && evidence_imports == legacy_archive_imports
        && plans_awaiting_checkpoints == 0
        && verifications_awaiting_checkpoints == 0
        && cleanups_awaiting_checkpoints == 0
        && reevaluations_awaiting_checkpoints == 0;

    Ok(DatabaseV2Status {
        account_label: account_label.to_owned(),
        legacy_snapshot_id,
        current_source_snapshot_id,
        current_playlists,
        current_playlist_tracks,
        playlist_revisions,
        current_playlist_headers_match,
        current_playlist_order_matches,
        current_saved_tracks_match,
        current_saved_albums_match,
        legacy_listening_events,
        normalized_listening_events,
        historical_identities: gates.try_get("identities")?,
        legacy_archive_imports,
        evidence_imports,
        checkpoints: gates.try_get("checkpoints")?,
        plans_awaiting_checkpoints,
        verifications_awaiting_checkpoints,
        cleanups_awaiting_checkpoints,
        reevaluations_awaiting_checkpoints,
        ready_for_cutover,
    })
}

#[cfg(test)]
mod tests {
    use super::sha256_lines;

    #[test]
    fn invariant_fingerprints_are_stable_and_order_sensitive() {
        assert_eq!(
            sha256_lines(Vec::new()),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_lines([
                "playlist:0:track-a".to_owned(),
                "playlist:1:track-b".to_owned()
            ]),
            sha256_lines([
                "playlist:0:track-a".to_owned(),
                "playlist:1:track-b".to_owned()
            ])
        );
        assert_ne!(
            sha256_lines([
                "playlist:0:track-a".to_owned(),
                "playlist:1:track-b".to_owned()
            ]),
            sha256_lines([
                "playlist:1:track-b".to_owned(),
                "playlist:0:track-a".to_owned()
            ])
        );
    }
}
