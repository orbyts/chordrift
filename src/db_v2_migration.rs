//! Exact-confirmed rehearsal migration and read-only database-v2 verification.

use chrono::{DateTime, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result, db_reports};

const MIGRATION_VERSION: &str = "normalized-evidence-checkpoints-v1";

/// Deterministic description of the legacy rows eligible for v2 migration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationPlan {
    /// Selected account label.
    pub account_label: String,
    /// Exact confirmation digest.
    pub plan_sha256: String,
    /// Current provider snapshot restored after checkpoint construction.
    pub current_snapshot_id: Uuid,
    /// Legacy track listening events.
    pub legacy_events: i64,
    /// Active legacy track listening events.
    pub active_legacy_events: i64,
    /// Distinct historical provider track identities.
    pub historical_identities: i64,
    /// Legacy immutable archive manifests.
    pub archive_imports: i64,
    /// Event-bearing archive members whose paths are known.
    pub known_archive_source_files: i64,
    /// Distinct legacy provider snapshots needed by durable audit rows.
    pub checkpoint_source_snapshots: i64,
    /// Sync-plan rows to reference compact checkpoints.
    pub sync_plan_references: i64,
    /// Managed verification rows to reference compact checkpoints.
    pub verification_references: i64,
    /// External-cleanup approval rows to reference compact checkpoints.
    pub cleanup_references: i64,
    /// Re-evaluate audit rows to reference compact checkpoints.
    pub reevaluation_references: i64,
    /// Non-track events that this migration intentionally refuses to discard.
    pub unsupported_media_events: i64,
    /// Archive events missing immutable-import provenance.
    pub archive_events_missing_import: i64,
    /// Track events missing a historical provider identity.
    pub events_missing_provider_identity: i64,
    /// Whether apply is safe for the selected database state.
    pub applicable: bool,
    account_id: Uuid,
}

/// Exact parity checks after normalized evidence and checkpoint migration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationVerification {
    /// Selected account label.
    pub account_label: String,
    /// Legacy track-event count.
    pub legacy_events: i64,
    /// Normalized track-event count.
    pub normalized_events: i64,
    /// Legacy active-event count.
    pub active_legacy_events: i64,
    /// Normalized active-event count.
    pub active_normalized_events: i64,
    /// Legacy total listening duration.
    pub legacy_duration_ms: i64,
    /// Normalized total listening duration.
    pub normalized_duration_ms: i64,
    /// Legacy first listening timestamp.
    pub legacy_first_event_at: Option<DateTime<Utc>>,
    /// Normalized first listening timestamp.
    pub normalized_first_event_at: Option<DateTime<Utc>>,
    /// Legacy last listening timestamp.
    pub legacy_last_event_at: Option<DateTime<Utc>>,
    /// Normalized last listening timestamp.
    pub normalized_last_event_at: Option<DateTime<Utc>>,
    /// Legacy events with canonical assignments.
    pub legacy_matched_events: i64,
    /// Normalized events whose historical identity has a canonical assignment.
    pub normalized_matched_events: i64,
    /// Legacy historical identities with canonical assignments.
    pub legacy_matched_identities: i64,
    /// Normalized historical identities with canonical assignments.
    pub normalized_matched_identities: i64,
    /// Legacy historical identities without canonical assignments.
    pub legacy_unmatched_identities: i64,
    /// Normalized historical identities without canonical assignments.
    pub normalized_unmatched_identities: i64,
    /// Whether archive hashes and import counts match exactly.
    pub archive_manifests_match: bool,
    /// Sync plans still missing checkpoint references.
    pub plans_awaiting_checkpoints: i64,
    /// Managed verifications still missing checkpoint references.
    pub verifications_awaiting_checkpoints: i64,
    /// Cleanup approvals still missing checkpoint references.
    pub cleanups_awaiting_checkpoints: i64,
    /// Re-evaluate audit events still missing checkpoint references.
    pub reevaluations_awaiting_checkpoints: i64,
    /// Compact checkpoints retained for durable audit history.
    pub checkpoints: i64,
    /// Whether every migration invariant holds.
    pub verified: bool,
}

/// Result of an exact-confirmed migration apply.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationApply {
    /// Exact applied plan digest.
    pub plan_sha256: String,
    /// Independent post-apply verification.
    pub verification: MigrationVerification,
}

fn hash_lines(lines: &[String]) -> String {
    let mut hasher = Sha256::new();
    for line in lines {
        hasher.update(line.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

/// Builds an exact, read-only migration plan for one provider account.
pub async fn plan(database: &Database, account_label: &str) -> Result<MigrationPlan> {
    let mut transaction = database.pool().begin().await?;
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *transaction)
        .await?;
    let account = sqlx::query(
        "SELECT id FROM provider_accounts WHERE account_label = $1 ORDER BY provider LIMIT 1",
    )
    .bind(account_label)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(format!("unknown account label `{account_label}`"))
    })?;
    let account_id: Uuid = account.try_get("id")?;

    let row = sqlx::query(
        "WITH durable_snapshots AS (
           SELECT source_snapshot_id AS id FROM sync_runs WHERE provider_account_id = $1
           UNION SELECT verified_snapshot_id FROM managed_playlist_verifications WHERE provider_account_id = $1
           UNION SELECT source_snapshot_id FROM external_playlist_cleanup_batches WHERE provider_account_id = $1
           UNION SELECT provider_snapshot_id FROM reevaluation_events WHERE provider_account_id = $1
         )
         SELECT
           (SELECT snapshot_id FROM provider_import_runs
             WHERE provider_account_id = $1 AND status = 'succeeded' AND snapshot_id IS NOT NULL
             ORDER BY finished_at DESC NULLS LAST, id DESC LIMIT 1) AS current_snapshot_id,
           (SELECT count(*) FROM listening_events WHERE provider_account_id = $1 AND media_type = 'track') AS legacy_events,
           (SELECT count(*) FROM listening_events WHERE provider_account_id = $1 AND media_type = 'track' AND superseded_at IS NULL) AS active_events,
           (SELECT count(DISTINCT (provider, provider_track_id)) FROM listening_events WHERE provider_account_id = $1 AND media_type = 'track') AS identities,
           (SELECT count(*) FROM spotify_archive_imports WHERE provider_account_id = $1) AS imports,
           (SELECT count(DISTINCT (source_import_id, source_file)) FROM listening_events
             WHERE provider_account_id = $1 AND media_type = 'track' AND source_kind = 'archive') AS source_files,
           (SELECT count(*) FROM durable_snapshots) AS checkpoint_snapshots,
           (SELECT count(*) FROM sync_runs WHERE provider_account_id = $1 AND source_snapshot_id IS NOT NULL) AS sync_refs,
           (SELECT count(*) FROM managed_playlist_verifications WHERE provider_account_id = $1 AND verified_snapshot_id IS NOT NULL) AS verification_refs,
           (SELECT count(*) FROM external_playlist_cleanup_batches WHERE provider_account_id = $1 AND source_snapshot_id IS NOT NULL) AS cleanup_refs,
           (SELECT count(*) FROM reevaluation_events WHERE provider_account_id = $1 AND provider_snapshot_id IS NOT NULL) AS reevaluation_refs,
           (SELECT count(*) FROM listening_events WHERE provider_account_id = $1 AND media_type <> 'track') AS unsupported_media,
           (SELECT count(*) FROM listening_events WHERE provider_account_id = $1 AND media_type = 'track' AND source_kind = 'archive' AND source_import_id IS NULL) AS missing_import,
           (SELECT count(*) FROM listening_events WHERE provider_account_id = $1 AND media_type = 'track' AND (provider_track_id IS NULL OR btrim(provider_track_id) = '')) AS missing_identity,
           (SELECT encode(sha256(convert_to(COALESCE(string_agg(
               id::text || ':' || provider || ':' || COALESCE(provider_track_id, '') || ':' ||
               played_at::text || ':' || COALESCE(ms_played::text, '') || ':' || source_occurrence::text,
               E'\\n' ORDER BY id), ''), 'UTF8')), 'hex')
              FROM listening_events WHERE provider_account_id = $1 AND media_type = 'track') AS event_fingerprint,
           (SELECT encode(sha256(convert_to(COALESCE(string_agg(
               archive_sha256 || ':' || archive_kind || ':' || events_imported::text,
               E'\\n' ORDER BY archive_sha256), ''), 'UTF8')), 'hex')
              FROM spotify_archive_imports WHERE provider_account_id = $1) AS archive_fingerprint,
           (SELECT encode(sha256(convert_to(COALESCE(string_agg(id::text, E'\\n' ORDER BY id), ''), 'UTF8')), 'hex') FROM durable_snapshots) AS snapshot_fingerprint",
    )
    .bind(account_id)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.rollback().await?;

    let current_snapshot_id: Uuid = row.try_get("current_snapshot_id")?;
    let legacy_events: i64 = row.try_get("legacy_events")?;
    let active_legacy_events: i64 = row.try_get("active_events")?;
    let historical_identities: i64 = row.try_get("identities")?;
    let archive_imports: i64 = row.try_get("imports")?;
    let known_archive_source_files: i64 = row.try_get("source_files")?;
    let checkpoint_source_snapshots: i64 = row.try_get("checkpoint_snapshots")?;
    let sync_plan_references: i64 = row.try_get("sync_refs")?;
    let verification_references: i64 = row.try_get("verification_refs")?;
    let cleanup_references: i64 = row.try_get("cleanup_refs")?;
    let reevaluation_references: i64 = row.try_get("reevaluation_refs")?;
    let unsupported_media_events: i64 = row.try_get("unsupported_media")?;
    let archive_events_missing_import: i64 = row.try_get("missing_import")?;
    let events_missing_provider_identity: i64 = row.try_get("missing_identity")?;
    let event_fingerprint: String = row.try_get("event_fingerprint")?;
    let archive_fingerprint: String = row.try_get("archive_fingerprint")?;
    let snapshot_fingerprint: String = row.try_get("snapshot_fingerprint")?;
    let applicable = unsupported_media_events == 0
        && archive_events_missing_import == 0
        && events_missing_provider_identity == 0;
    let plan_sha256 = hash_lines(&[
        MIGRATION_VERSION.to_owned(),
        account_id.to_string(),
        current_snapshot_id.to_string(),
        legacy_events.to_string(),
        active_legacy_events.to_string(),
        event_fingerprint,
        archive_fingerprint,
        snapshot_fingerprint,
    ]);

    Ok(MigrationPlan {
        account_label: account_label.to_owned(),
        plan_sha256,
        current_snapshot_id,
        legacy_events,
        active_legacy_events,
        historical_identities,
        archive_imports,
        known_archive_source_files,
        checkpoint_source_snapshots,
        sync_plan_references,
        verification_references,
        cleanup_references,
        reevaluation_references,
        unsupported_media_events,
        archive_events_missing_import,
        events_missing_provider_identity,
        applicable,
        account_id,
    })
}

/// Applies one exact-confirmed migration plan to the selected database only.
pub async fn apply(
    database: &Database,
    account_label: &str,
    confirmation: &str,
) -> Result<MigrationApply> {
    let migration_plan = plan(database, account_label).await?;
    if confirmation != migration_plan.plan_sha256 {
        return Err(ChordriftError::Configuration(
            "migration confirmation does not match the current read-only plan".to_owned(),
        ));
    }
    if !migration_plan.applicable {
        return Err(ChordriftError::Configuration(
            "migration plan contains unsupported or incomplete legacy evidence".to_owned(),
        ));
    }

    let mut transaction = database.pool().begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('chordrift-db-v2-migration:' || $1::text))")
        .bind(migration_plan.account_id)
        .execute(&mut *transaction)
        .await?;

    sqlx::query(
        "INSERT INTO listening_evidence_imports
           (id, provider_account_id, provider, archive_kind, archive_sha256,
            parser_version, source_filename, source_file_count, event_count,
            first_event_at, last_event_at, manifest, imported_at)
         SELECT legacy.id, legacy.provider_account_id, account.provider,
                legacy.archive_kind, legacy.archive_sha256, 'legacy-history-v1',
                legacy.source_filename, legacy.source_files, legacy.events_imported,
                legacy.first_event_at, legacy.last_event_at,
                jsonb_build_object(
                    'legacy_import_id', legacy.id,
                    'events_seen', legacy.events_seen,
                    'events_matched', legacy.events_matched,
                    'events_ignored', legacy.events_ignored,
                    'legacy_metadata', legacy.metadata,
                    'member_hashes', 'unavailable; containing archive hash verified'
                ), legacy.imported_at
           FROM spotify_archive_imports legacy
           JOIN provider_accounts account ON account.id = legacy.provider_account_id
          WHERE legacy.provider_account_id = $1
         ON CONFLICT (provider_account_id, provider, archive_sha256) DO NOTHING",
    )
    .bind(migration_plan.account_id)
    .execute(&mut *transaction)
    .await?;

    sqlx::query(
        "INSERT INTO listening_evidence_source_files
           (import_id, source_path, content_sha256, event_count, hash_status)
         SELECT event.source_import_id, event.source_file, NULL, count(*),
                'archive_manifest_only'
           FROM listening_events event
          WHERE event.provider_account_id = $1 AND event.media_type = 'track'
            AND event.source_kind = 'archive' AND event.source_import_id IS NOT NULL
            AND event.source_file IS NOT NULL
          GROUP BY event.source_import_id, event.source_file
         ON CONFLICT (import_id, source_path) DO NOTHING",
    )
    .bind(migration_plan.account_id)
    .execute(&mut *transaction)
    .await?;

    sqlx::query(
        "WITH observations AS (
           SELECT provider, provider_track_id,
                  min(played_at) AS first_observed_at,
                  max(played_at) AS last_observed_at
             FROM listening_events
            WHERE provider_account_id = $1 AND media_type = 'track'
            GROUP BY provider, provider_track_id
         ), latest AS (
           SELECT DISTINCT ON (provider, provider_track_id)
                  provider, provider_track_id, track_id,
                  raw_metadata->>'track_name' AS track_name,
                  raw_metadata->>'artist_name' AS artist_name,
                  raw_metadata->>'album_name' AS album_name
             FROM listening_events
            WHERE provider_account_id = $1 AND media_type = 'track'
            ORDER BY provider, provider_track_id,
                     (track_id IS NOT NULL) DESC, played_at DESC, id DESC
         )
         INSERT INTO historical_provider_track_identities
           (provider, provider_track_id, canonical_track_id, track_name,
            artist_name, album_name, first_observed_at, last_observed_at)
         SELECT observations.provider, observations.provider_track_id,
                latest.track_id, latest.track_name, latest.artist_name,
                latest.album_name, observations.first_observed_at,
                observations.last_observed_at
           FROM observations JOIN latest USING (provider, provider_track_id)
         ON CONFLICT (provider, provider_track_id) DO UPDATE SET
           canonical_track_id = EXCLUDED.canonical_track_id,
           track_name = EXCLUDED.track_name,
           artist_name = EXCLUDED.artist_name,
           album_name = EXCLUDED.album_name,
           first_observed_at = LEAST(historical_provider_track_identities.first_observed_at,
                                     EXCLUDED.first_observed_at),
           last_observed_at = GREATEST(historical_provider_track_identities.last_observed_at,
                                      EXCLUDED.last_observed_at)",
    )
    .bind(migration_plan.account_id)
    .execute(&mut *transaction)
    .await?;

    sqlx::query(
        "INSERT INTO normalized_listening_events
           (id, provider_account_id, historical_identity_id, source_import_id,
            source_file_id, source_kind, source_event_id, source_occurrence,
            played_at, ms_played, skipped, completed, completion_reason,
            context_uri, context_type, superseded_at, provider_extensions)
         SELECT event.id, event.provider_account_id, identity.id,
                event.source_import_id, source_file.id, event.source_kind,
                event.source_event_id, event.source_occurrence, event.played_at,
                event.ms_played, event.skipped,
                CASE WHEN event.raw_metadata ? 'reason_end'
                     THEN event.raw_metadata->>'reason_end' = 'trackdone' END,
                event.raw_metadata->>'reason_end', event.context_uri,
                event.raw_metadata->>'context_type', event.superseded_at,
                '{}'::jsonb
           FROM listening_events event
           JOIN historical_provider_track_identities identity
             ON identity.provider = event.provider
            AND identity.provider_track_id = event.provider_track_id
           LEFT JOIN listening_evidence_source_files source_file
             ON source_file.import_id = event.source_import_id
            AND source_file.source_path = event.source_file
          WHERE event.provider_account_id = $1 AND event.media_type = 'track'
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(migration_plan.account_id)
    .execute(&mut *transaction)
    .await?;

    sqlx::query(
        "CREATE TEMP TABLE chordrift_v2_checkpoint_map ON COMMIT DROP AS
         SELECT id AS snapshot_id, NULL::uuid AS checkpoint_id
           FROM (
             SELECT source_snapshot_id AS id FROM sync_runs WHERE provider_account_id = $1
             UNION SELECT verified_snapshot_id FROM managed_playlist_verifications WHERE provider_account_id = $1
             UNION SELECT source_snapshot_id FROM external_playlist_cleanup_batches WHERE provider_account_id = $1
             UNION SELECT provider_snapshot_id FROM reevaluation_events WHERE provider_account_id = $1
           ) snapshots",
    )
    .bind(migration_plan.account_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "UPDATE chordrift_v2_checkpoint_map
            SET checkpoint_id = materialize_provider_checkpoint_v2($1, snapshot_id)",
    )
    .bind(migration_plan.account_id)
    .execute(&mut *transaction)
    .await?;
    for statement in [
        "UPDATE sync_runs row SET provider_checkpoint_id = map.checkpoint_id FROM chordrift_v2_checkpoint_map map WHERE row.provider_account_id = $1 AND row.source_snapshot_id = map.snapshot_id",
        "UPDATE managed_playlist_verifications row SET provider_checkpoint_id = map.checkpoint_id FROM chordrift_v2_checkpoint_map map WHERE row.provider_account_id = $1 AND row.verified_snapshot_id = map.snapshot_id",
        "UPDATE external_playlist_cleanup_batches row SET provider_checkpoint_id = map.checkpoint_id FROM chordrift_v2_checkpoint_map map WHERE row.provider_account_id = $1 AND row.source_snapshot_id = map.snapshot_id",
        "UPDATE reevaluation_events row SET provider_checkpoint_id = map.checkpoint_id FROM chordrift_v2_checkpoint_map map WHERE row.provider_account_id = $1 AND row.provider_snapshot_id = map.snapshot_id",
    ] {
        sqlx::query(statement)
            .bind(migration_plan.account_id)
            .execute(&mut *transaction)
            .await?;
    }
    sqlx::query("SELECT materialize_provider_current_state_v2($1, $2)")
        .bind(migration_plan.account_id)
        .bind(migration_plan.current_snapshot_id)
        .execute(&mut *transaction)
        .await?;

    let counts = sqlx::query(
        "SELECT
           (SELECT count(*) FROM normalized_listening_events WHERE provider_account_id = $1) AS events,
           (SELECT count(*) FROM listening_evidence_imports WHERE provider_account_id = $1) AS imports,
           (SELECT count(*) FROM provider_inventory_checkpoints WHERE provider_account_id = $1 AND released_at IS NULL) AS checkpoints",
    )
    .bind(migration_plan.account_id)
    .fetch_one(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO database_v2_migration_runs
           (provider_account_id, plan_sha256, migration_version,
            legacy_event_count, normalized_event_count,
            evidence_import_count, checkpoint_count)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (provider_account_id, plan_sha256) DO UPDATE SET
           normalized_event_count = EXCLUDED.normalized_event_count,
           evidence_import_count = EXCLUDED.evidence_import_count,
           checkpoint_count = EXCLUDED.checkpoint_count",
    )
    .bind(migration_plan.account_id)
    .bind(&migration_plan.plan_sha256)
    .bind(MIGRATION_VERSION)
    .bind(migration_plan.legacy_events)
    .bind(counts.try_get::<i64, _>("events")?)
    .bind(
        i32::try_from(counts.try_get::<i64, _>("imports")?).map_err(|_| {
            ChordriftError::Configuration("evidence import count exceeds integer range".to_owned())
        })?,
    )
    .bind(
        i32::try_from(counts.try_get::<i64, _>("checkpoints")?).map_err(|_| {
            ChordriftError::Configuration("checkpoint count exceeds integer range".to_owned())
        })?,
    )
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    let verification = verify(database, account_label).await?;
    if verification.verified {
        sqlx::query(
            "UPDATE database_v2_migration_runs
                SET verified_at = now(), verification = $3
              WHERE provider_account_id = $1 AND plan_sha256 = $2",
        )
        .bind(migration_plan.account_id)
        .bind(&migration_plan.plan_sha256)
        .bind(json!({
            "verified": true,
            "events": verification.normalized_events,
            "duration_ms": verification.normalized_duration_ms,
            "checkpoints": verification.checkpoints,
        }))
        .execute(database.pool())
        .await?;
    }

    Ok(MigrationApply {
        plan_sha256: migration_plan.plan_sha256,
        verification,
    })
}

/// Verifies normalized evidence and checkpoint parity without changing rows.
pub async fn verify(database: &Database, account_label: &str) -> Result<MigrationVerification> {
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
        "SELECT
           (SELECT count(*) FROM listening_events WHERE provider_account_id = $1 AND media_type = 'track') AS legacy_events,
           (SELECT count(*) FROM normalized_listening_events WHERE provider_account_id = $1) AS normalized_events,
           (SELECT count(*) FROM listening_events WHERE provider_account_id = $1 AND media_type = 'track' AND superseded_at IS NULL) AS active_legacy,
           (SELECT count(*) FROM normalized_listening_events WHERE provider_account_id = $1 AND superseded_at IS NULL) AS active_normalized,
           (SELECT COALESCE(sum(ms_played), 0)::bigint FROM listening_events WHERE provider_account_id = $1 AND media_type = 'track') AS legacy_duration,
           (SELECT COALESCE(sum(ms_played), 0)::bigint FROM normalized_listening_events WHERE provider_account_id = $1) AS normalized_duration,
           (SELECT min(played_at) FROM listening_events WHERE provider_account_id = $1 AND media_type = 'track') AS legacy_first,
           (SELECT min(played_at) FROM normalized_listening_events WHERE provider_account_id = $1) AS normalized_first,
           (SELECT max(played_at) FROM listening_events WHERE provider_account_id = $1 AND media_type = 'track') AS legacy_last,
           (SELECT max(played_at) FROM normalized_listening_events WHERE provider_account_id = $1) AS normalized_last,
           (SELECT count(*) FROM listening_events WHERE provider_account_id = $1 AND media_type = 'track' AND track_id IS NOT NULL) AS legacy_matched_events,
           (SELECT count(*) FROM normalized_listening_events event JOIN historical_provider_track_identities identity ON identity.id = event.historical_identity_id WHERE event.provider_account_id = $1 AND identity.canonical_track_id IS NOT NULL) AS normalized_matched_events,
           (SELECT count(DISTINCT (provider, provider_track_id)) FROM listening_events WHERE provider_account_id = $1 AND media_type = 'track' AND track_id IS NOT NULL) AS legacy_matched_identities,
           (SELECT count(DISTINCT event.historical_identity_id)
              FROM normalized_listening_events event
              JOIN historical_provider_track_identities identity
                ON identity.id = event.historical_identity_id
             WHERE event.provider_account_id = $1
               AND identity.canonical_track_id IS NOT NULL) AS normalized_matched_identities,
           (SELECT count(DISTINCT (provider, provider_track_id)) FROM listening_events WHERE provider_account_id = $1 AND media_type = 'track' AND track_id IS NULL) AS legacy_unmatched_identities,
           (SELECT count(DISTINCT event.historical_identity_id)
              FROM normalized_listening_events event
              JOIN historical_provider_track_identities identity
                ON identity.id = event.historical_identity_id
             WHERE event.provider_account_id = $1
               AND identity.canonical_track_id IS NULL) AS normalized_unmatched_identities,
           NOT EXISTS (
             (SELECT archive_sha256, archive_kind, events_imported FROM spotify_archive_imports WHERE provider_account_id = $1
              EXCEPT SELECT archive_sha256, archive_kind, event_count FROM listening_evidence_imports WHERE provider_account_id = $1)
             UNION ALL
             (SELECT archive_sha256, archive_kind, event_count FROM listening_evidence_imports WHERE provider_account_id = $1
              EXCEPT SELECT archive_sha256, archive_kind, events_imported FROM spotify_archive_imports WHERE provider_account_id = $1)
           ) AS archives_match,
           (SELECT count(*) FROM sync_runs WHERE provider_account_id = $1 AND source_snapshot_id IS NOT NULL AND provider_checkpoint_id IS NULL) AS plans_awaiting,
           (SELECT count(*) FROM managed_playlist_verifications WHERE provider_account_id = $1 AND verified_snapshot_id IS NOT NULL AND provider_checkpoint_id IS NULL) AS verifications_awaiting,
           (SELECT count(*) FROM external_playlist_cleanup_batches WHERE provider_account_id = $1 AND source_snapshot_id IS NOT NULL AND provider_checkpoint_id IS NULL) AS cleanups_awaiting,
           (SELECT count(*) FROM reevaluation_events WHERE provider_account_id = $1 AND provider_snapshot_id IS NOT NULL AND provider_checkpoint_id IS NULL) AS reevaluations_awaiting,
           (SELECT count(*) FROM provider_inventory_checkpoints WHERE provider_account_id = $1 AND released_at IS NULL) AS checkpoints",
    )
    .bind(account_id)
    .fetch_one(&mut *transaction)
    .await?;
    transaction.rollback().await?;

    let mut report = MigrationVerification {
        account_label: account_label.to_owned(),
        legacy_events: row.try_get("legacy_events")?,
        normalized_events: row.try_get("normalized_events")?,
        active_legacy_events: row.try_get("active_legacy")?,
        active_normalized_events: row.try_get("active_normalized")?,
        legacy_duration_ms: row.try_get("legacy_duration")?,
        normalized_duration_ms: row.try_get("normalized_duration")?,
        legacy_first_event_at: row.try_get("legacy_first")?,
        normalized_first_event_at: row.try_get("normalized_first")?,
        legacy_last_event_at: row.try_get("legacy_last")?,
        normalized_last_event_at: row.try_get("normalized_last")?,
        legacy_matched_events: row.try_get("legacy_matched_events")?,
        normalized_matched_events: row.try_get("normalized_matched_events")?,
        legacy_matched_identities: row.try_get("legacy_matched_identities")?,
        normalized_matched_identities: row.try_get("normalized_matched_identities")?,
        legacy_unmatched_identities: row.try_get("legacy_unmatched_identities")?,
        normalized_unmatched_identities: row.try_get("normalized_unmatched_identities")?,
        archive_manifests_match: row.try_get("archives_match")?,
        plans_awaiting_checkpoints: row.try_get("plans_awaiting")?,
        verifications_awaiting_checkpoints: row.try_get("verifications_awaiting")?,
        cleanups_awaiting_checkpoints: row.try_get("cleanups_awaiting")?,
        reevaluations_awaiting_checkpoints: row.try_get("reevaluations_awaiting")?,
        checkpoints: row.try_get("checkpoints")?,
        verified: false,
    };
    report.verified = report.legacy_events == report.normalized_events
        && report.active_legacy_events == report.active_normalized_events
        && report.legacy_duration_ms == report.normalized_duration_ms
        && report.legacy_first_event_at == report.normalized_first_event_at
        && report.legacy_last_event_at == report.normalized_last_event_at
        && report.legacy_matched_events == report.normalized_matched_events
        && report.legacy_matched_identities == report.normalized_matched_identities
        && report.legacy_unmatched_identities == report.normalized_unmatched_identities
        && report.archive_manifests_match
        && report.plans_awaiting_checkpoints == 0
        && report.verifications_awaiting_checkpoints == 0
        && report.cleanups_awaiting_checkpoints == 0
        && report.reevaluations_awaiting_checkpoints == 0;
    Ok(report)
}

/// Produces a deterministic, non-applying production cutover plan.
pub async fn cutover_plan(
    database: &Database,
    account_label: &str,
) -> Result<(String, MigrationVerification, bool)> {
    let migration_plan = plan(database, account_label).await?;
    let verification = verify(database, account_label).await?;
    let v2_status = db_reports::database_v2_status(database, account_label).await?;
    let cutover_sha256 = hash_lines(&[
        "database-v2-production-cutover-v1".to_owned(),
        migration_plan.plan_sha256,
        verification.verified.to_string(),
        v2_status.ready_for_cutover.to_string(),
        v2_status.legacy_snapshot_id.to_string(),
        verification.normalized_events.to_string(),
        verification.checkpoints.to_string(),
    ]);
    Ok((cutover_sha256, verification, v2_status.ready_for_cutover))
}

#[cfg(test)]
mod tests {
    use super::hash_lines;

    #[test]
    fn confirmation_hash_is_order_sensitive() {
        assert_ne!(
            hash_lines(&["a".to_owned(), "b".to_owned()]),
            hash_lines(&["b".to_owned(), "a".to_owned()])
        );
    }
}
