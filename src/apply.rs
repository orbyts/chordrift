//! Gated, resumable execution of one phase from an approved Spotify plan.

use std::{collections::BTreeMap, fs, io::Cursor, path::PathBuf};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use image::{codecs::jpeg::JpegEncoder, imageops::FilterType};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, QueryBuilder, Row};
use storexa::Database;
use uuid::Uuid;

use crate::{
    ChordriftError, Result,
    providers::spotify::{self, MutationSession},
};

const APPLY_VERSION: &str = "spotify-apply-v4";
const READINESS_VERSION: &str = "spotify-apply-readiness-v5";
const PLANNER_VERSION: &str = "spotify-dry-run-v11";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Independently gated execution phases in a synchronization plan.
pub enum ApplyPhase {
    /// Create and populate canonical surfaces, then upload approved covers.
    Publish,
    /// Apply non-deferred managed drift and Neon-only exclusions.
    Reconcile,
    /// Clear verified intake entries and remove approved external relationships.
    Cleanup,
    /// Remove separately approved legacy playlist relationships.
    Retirement,
}

impl ApplyPhase {
    /// Returns the stable database representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Publish => "publish",
            Self::Reconcile => "reconcile",
            Self::Cleanup => "cleanup",
            Self::Retirement => "retirement",
        }
    }

    fn destructive(self) -> bool {
        matches!(self, Self::Cleanup | Self::Retirement)
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Summary of one completed or resumed phase execution.
pub struct ApplyReport {
    /// Durable apply execution identity.
    pub apply_run_id: Uuid,
    /// Immutable dry-run plan being executed.
    pub plan_id: Uuid,
    /// Exact ready assessment confirmed by the user.
    pub assessment_id: Uuid,
    /// Executed safety phase.
    pub phase: String,
    /// Current execution state.
    pub status: String,
    /// Planned operation count in the phase.
    pub operation_count: usize,
    /// Successfully executed or reconciled operations.
    pub succeeded_count: usize,
    /// Failed operations.
    pub failed_count: usize,
    /// Whether this execution resumed a durable prior run.
    pub resumed: bool,
    /// Original execution start time.
    pub started_at: DateTime<Utc>,
}

/// Immutable record that the approved Neon model exactly matched one observed
/// provider snapshot without a Chordrift provider write.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderStateAcceptance {
    /// Provider inventory observation accepted as current user intent.
    pub snapshot_id: Uuid,
    /// Approved proposal proven equal to that observation.
    pub proposal_generation_id: Uuid,
    /// Exact managed playlists included in the accepted checkpoint.
    pub playlist_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
/// Local validation and request estimate for one immutable publish plan.
pub struct PublishPreflightReport {
    /// Immutable plan inspected by the preflight.
    pub plan_id: Uuid,
    /// Playlist containers that the plan will create.
    pub playlist_creates: usize,
    /// Canonical playlists whose exact ordered membership will be written.
    pub populated_playlists: usize,
    /// Planned canonical track memberships.
    pub playlist_entries: usize,
    /// Replace/append requests needed for those memberships.
    pub playlist_item_writes: usize,
    /// Approved covers decoded, hash-checked, and converted successfully.
    pub artwork_uploads: usize,
    /// Largest converted base64 JPEG body.
    pub largest_artwork_bytes: usize,
    /// Estimated Spotify reads performed by the publish phase.
    pub estimated_spotify_reads: usize,
    /// Estimated Spotify writes performed by the publish phase.
    pub estimated_spotify_writes: usize,
}

/// Validates every local publish artifact and estimates requests without contacting Spotify.
pub async fn preflight_publish(
    database: &Database,
    account_label: &str,
    requested_plan: Option<Uuid>,
) -> Result<PublishPreflightReport> {
    let plan = sqlx::query(
        "SELECT run.id, run.planner_version, run.source_snapshot_id,
                (SELECT id FROM provider_inventory_observations latest
                 WHERE latest.provider_account_id = account.id
                 ORDER BY captured_at DESC, id DESC LIMIT 1) AS latest_snapshot_id
         FROM sync_runs run
         JOIN provider_accounts account ON account.id = run.provider_account_id
         WHERE account.provider = 'spotify' AND account.account_label = $1
           AND ($2::uuid IS NULL OR run.id = $2)
         ORDER BY run.started_at DESC, run.id DESC LIMIT 1",
    )
    .bind(account_label)
    .bind(requested_plan)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration("no matching Spotify plan exists"))?;
    let plan_id: Uuid = plan.try_get("id")?;
    if plan.try_get::<String, _>("planner_version")? != PLANNER_VERSION
        || plan.try_get::<Uuid, _>("source_snapshot_id")?
            != plan.try_get::<Uuid, _>("latest_snapshot_id")?
    {
        return Err(configuration("preflight requires a current v10 plan"));
    }

    let playlist_creates: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sync_operations
         WHERE sync_run_id = $1 AND phase = 'publish' AND operation_type = 'create_playlist'",
    )
    .bind(plan_id)
    .fetch_one(database.pool())
    .await?;
    let groups = sqlx::query(
        "WITH targets AS (
             SELECT DISTINCT playlist_id
             FROM sync_operations
             WHERE sync_run_id = $1 AND phase = 'publish'
               AND operation_type IN ('add_track', 'restore_track', 'reorder_playlist')
               AND playlist_id IS NOT NULL
         )
         SELECT target.playlist_id, count(membership.id)::bigint AS entries
         FROM targets target
         LEFT JOIN playlist_tracks membership ON membership.playlist_id = target.playlist_id
         GROUP BY target.playlist_id",
    )
    .bind(plan_id)
    .fetch_all(database.pool())
    .await?;
    let playlist_entries = groups.iter().try_fold(0usize, |total, row| {
        let entries: i64 = row.try_get("entries")?;
        usize::try_from(entries)
            .map(|entries| total + entries)
            .map_err(|_| configuration("playlist entry count exceeds limits"))
    })?;
    let playlist_item_writes = groups.iter().try_fold(0usize, |total, row| {
        let entries: i64 = row.try_get("entries")?;
        let entries = usize::try_from(entries)
            .map_err(|_| configuration("playlist entry count exceeds limits"))?;
        Ok::<usize, ChordriftError>(total + entries.div_ceil(100))
    })?;

    let artwork = sqlx::query(
        "SELECT payload FROM sync_operations
         WHERE sync_run_id = $1 AND phase = 'publish' AND operation_type = 'upload_artwork'
         ORDER BY operation_key",
    )
    .bind(plan_id)
    .fetch_all(database.pool())
    .await?;
    let mut largest_artwork_bytes = 0;
    for row in &artwork {
        let payload: Value = row.try_get("payload")?;
        let detail = payload
            .get("detail")
            .ok_or_else(|| configuration("artwork operation has no detail payload"))?;
        let path = PathBuf::from(detail_string(detail, "artifact_path")?);
        let encoded = spotify_jpeg(&path, detail_string(detail, "content_sha256")?)?;
        largest_artwork_bytes = largest_artwork_bytes.max(encoded.len());
    }
    let playlist_creates = usize::try_from(playlist_creates)
        .map_err(|_| configuration("playlist create count exceeds limits"))?;
    let estimated_spotify_reads = 1 + groups.len();
    let estimated_spotify_writes = playlist_creates + playlist_item_writes + artwork.len();
    Ok(PublishPreflightReport {
        plan_id,
        playlist_creates,
        populated_playlists: groups.len(),
        playlist_entries,
        playlist_item_writes,
        artwork_uploads: artwork.len(),
        largest_artwork_bytes,
        estimated_spotify_reads,
        estimated_spotify_writes,
    })
}

/// Exact durable approval for the retirement operations in one immutable plan.
#[derive(Clone, Debug, PartialEq)]
pub struct RetirementApproval {
    /// Approved plan.
    pub plan_id: Uuid,
    /// Number of retirement operations covered.
    pub operation_count: usize,
    /// Approval time.
    pub approved_at: DateTime<Utc>,
}

/// Approves every retirement operation in one exact, current plan.
pub async fn approve_retirement(
    database: &Database,
    account_label: &str,
    plan_id: Uuid,
    confirmation: Uuid,
) -> Result<RetirementApproval> {
    if plan_id != confirmation {
        return Err(configuration("--confirm must exactly match the plan ID"));
    }
    let row = sqlx::query(
        "SELECT account.id AS account_id, run.input_hash, run.planner_version,
                run.source_snapshot_id,
                (SELECT id FROM provider_inventory_observations latest
                 WHERE latest.provider_account_id = account.id
                 ORDER BY captured_at DESC, id DESC LIMIT 1) AS latest_snapshot_id,
                (SELECT count(*)::bigint FROM sync_operations operation
                 WHERE operation.sync_run_id = run.id AND operation.phase = 'retirement') AS operations
         FROM sync_runs run
         JOIN provider_accounts account ON account.id = run.provider_account_id
         WHERE run.id = $1 AND account.provider = 'spotify' AND account.account_label = $2",
    )
    .bind(plan_id)
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration("no matching Spotify plan exists"))?;
    if row.try_get::<String, _>("planner_version")? != PLANNER_VERSION
        || row.try_get::<Uuid, _>("source_snapshot_id")?
            != row.try_get::<Uuid, _>("latest_snapshot_id")?
    {
        return Err(configuration(
            "retirement approval requires a current v10 plan",
        ));
    }
    let count: i64 = row.try_get("operations")?;
    if count == 0 {
        return Err(configuration("the plan has no retirement operations"));
    }
    let approved_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO sync_retirement_approvals
         (provider_account_id, plan_id, plan_input_hash, operation_count)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (provider_account_id, plan_id) DO UPDATE
         SET plan_input_hash = EXCLUDED.plan_input_hash
         RETURNING approved_at",
    )
    .bind(row.try_get::<Uuid, _>("account_id")?)
    .bind(plan_id)
    .bind(row.try_get::<String, _>("input_hash")?)
    .bind(i32::try_from(count).map_err(|_| configuration("retirement count exceeds limits"))?)
    .fetch_one(database.pool())
    .await?;
    Ok(RetirementApproval {
        plan_id,
        operation_count: count as usize,
        approved_at,
    })
}

#[derive(Clone, Debug)]
struct Operation {
    id: Uuid,
    kind: String,
    playlist_id: Option<Uuid>,
    playlist_name: String,
    spotify_playlist_id: Option<String>,
    spotify_track_id: Option<String>,
    detail: Value,
    status: String,
}

enum PlaylistMembershipWrite<'operation> {
    ExactReorder,
    EnumeratedAdditions {
        reused: Vec<&'operation Operation>,
        missing: Vec<(&'operation Operation, String)>,
    },
}

fn playlist_membership_write<'operation>(
    live_items: &[String],
    desired: &[String],
    pending: &[&'operation Operation],
) -> Result<PlaylistMembershipWrite<'operation>> {
    let reorders = pending
        .iter()
        .filter(|operation| operation.kind == "reorder_playlist")
        .count();
    if reorders > 0 {
        if reorders != pending.len() {
            return Err(configuration(
                "a reorder phase cannot contain implicit membership changes",
            ));
        }
        let mut live_membership = live_items.to_vec();
        live_membership.sort();
        let mut desired_membership = desired.to_vec();
        desired_membership.sort();
        if live_membership != desired_membership {
            return Err(configuration(
                "a reorder requires identical current and desired membership",
            ));
        }
        return Ok(PlaylistMembershipWrite::ExactReorder);
    }

    let mut reused = Vec::new();
    let mut missing = Vec::new();
    for operation in pending {
        let spotify_track_id = operation
            .spotify_track_id
            .as_ref()
            .ok_or_else(|| configuration("planned track addition has no Spotify track ID"))?;
        if live_items.contains(spotify_track_id) {
            reused.push(*operation);
        } else {
            missing.push((*operation, spotify_track_id.clone()));
        }
    }
    Ok(PlaylistMembershipWrite::EnumeratedAdditions { reused, missing })
}

/// Executes one explicitly confirmed phase after revalidating every durable gate.
pub async fn execute(
    database: &Database,
    account_label: &str,
    assessment_id: Uuid,
    phase: ApplyPhase,
    confirmation: Uuid,
    allow_destructive: bool,
) -> Result<ApplyReport> {
    if confirmation != assessment_id {
        return Err(configuration(
            "--confirm must exactly match the apply-readiness assessment ID",
        ));
    }
    if phase.destructive() && !allow_destructive {
        return Err(configuration(
            "cleanup and retirement require --allow-destructive in addition to the exact confirmation",
        ));
    }

    let gate = load_gate(database, account_label, assessment_id, phase).await?;
    let session = spotify::mutation_session(account_label).await?;
    if session.account_id() != gate.provider_account_id {
        return Err(configuration(
            "Spotify credential identity does not match the planned Neon account",
        ));
    }
    let (apply_run_id, resumed, completed, started_at) =
        prepare_run(database, &gate, assessment_id, phase).await?;
    if completed {
        return report(database, apply_run_id, true, started_at).await;
    }
    let result = execute_phase(database, &session, apply_run_id, phase).await;
    match result {
        Ok(()) => {
            sqlx::query(
                "UPDATE sync_apply_runs SET status = 'awaiting_pull', finished_at = now(),
                   succeeded_count = (SELECT count(*) FROM sync_apply_operations
                                      WHERE apply_run_id = $1 AND status = 'succeeded'),
                   failed_count = 0, last_error = NULL WHERE id = $1",
            )
            .bind(apply_run_id)
            .execute(database.pool())
            .await?;
        }
        Err(error) => {
            sqlx::query(
                "UPDATE sync_apply_operations SET status = 'failed', last_error = $2
                 WHERE apply_run_id = $1 AND status = 'running'",
            )
            .bind(apply_run_id)
            .bind(error.to_string())
            .execute(database.pool())
            .await?;
            sqlx::query(
                "UPDATE sync_apply_runs SET status = 'failed', finished_at = now(),
                   succeeded_count = (SELECT count(*) FROM sync_apply_operations
                                      WHERE apply_run_id = $1 AND status = 'succeeded'),
                   failed_count = (SELECT count(*) FROM sync_apply_operations
                                   WHERE apply_run_id = $1 AND status = 'failed'),
                   last_error = $2 WHERE id = $1",
            )
            .bind(apply_run_id)
            .bind(error.to_string())
            .execute(database.pool())
            .await?;
            return Err(error);
        }
    }
    report(database, apply_run_id, resumed, started_at).await
}

/// Shows the latest or selected durable apply execution without contacting Spotify.
pub async fn show(
    database: &Database,
    account_label: &str,
    requested: Option<Uuid>,
) -> Result<ApplyReport> {
    let row = sqlx::query(
        "SELECT apply.id, apply.started_at
         FROM sync_apply_runs apply
         JOIN provider_accounts account ON account.id = apply.provider_account_id
         WHERE account.provider = 'spotify' AND account.account_label = $1
           AND ($2::uuid IS NULL OR apply.id = $2)
         ORDER BY apply.started_at DESC, apply.id DESC LIMIT 1",
    )
    .bind(account_label)
    .bind(requested)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration("no matching Spotify apply run exists"))?;
    report(
        database,
        row.try_get("id")?,
        false,
        row.try_get("started_at")?,
    )
    .await
}

/// Verifies awaiting publication runs and current canonical state against the newest snapshot.
///
/// A run becomes successful only when every canonical playlist has the exact
/// approved ordered membership. The resulting immutable baseline enables later
/// user-removal detection and deferred cleanup gates.
pub async fn verify_pending_publications(
    database: &Database,
    account_label: &str,
    verify_current_publication: bool,
) -> Result<usize> {
    let account = sqlx::query(
        "SELECT account.id,
                EXISTS (
                    SELECT 1 FROM sync_apply_runs apply
                    WHERE apply.provider_account_id = account.id
                      AND apply.status = 'awaiting_pull'
                ) AS has_pending_verification
         FROM provider_accounts account
         WHERE account.provider = 'spotify' AND account.account_label = $1",
    )
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration("Spotify account is not imported"))?;
    let account_id: Uuid = account.try_get("id")?;
    if !verify_current_publication && !account.try_get::<bool, _>("has_pending_verification")? {
        return Ok(0);
    }
    let snapshot_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM provider_inventory_observations WHERE provider_account_id = $1
         ORDER BY captured_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let runs = sqlx::query(
        "SELECT apply.id, plan.proposal_generation_id
         FROM sync_apply_runs apply
         JOIN sync_runs plan ON plan.id = apply.plan_id
         WHERE apply.provider_account_id = $1 AND apply.phase = 'publish'
           AND apply.status = 'awaiting_pull'
         ORDER BY apply.started_at, apply.id",
    )
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    let mut verified = 0;
    for run in runs {
        let run_id: Uuid = run.try_get("id")?;
        let proposal_id: Uuid = run.try_get("proposal_generation_id")?;
        if verify_publish_operations(database, account_id, snapshot_id, run_id).await? {
            // Persist a complete managed baseline when the final proposal already
            // matches. A later reconcile phase may still be required, so complete
            // proposal equality is deliberately not a condition of this publish
            // receipt succeeding.
            let _ = verify_publication(database, account_id, snapshot_id, proposal_id).await?;
            sqlx::query(
                "UPDATE sync_apply_runs SET status = 'succeeded', finished_at = now(),
                 summary = summary || jsonb_build_object('verified_snapshot_id', $2::text)
                 WHERE id = $1",
            )
            .bind(run_id)
            .bind(snapshot_id)
            .execute(database.pool())
            .await?;
            verified += 1;
        }
    }
    let reconcile_runs: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM sync_apply_runs
         WHERE provider_account_id = $1 AND phase = 'reconcile'
           AND status = 'awaiting_pull'
         ORDER BY started_at, id",
    )
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    for run_id in reconcile_runs {
        let unverified: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint
             FROM sync_apply_operations execution
             JOIN sync_operations planned ON planned.id = execution.planned_operation_id
             LEFT JOIN provider_tracks planned_track
               ON planned_track.provider = 'spotify'
              AND planned_track.provider_track_id = planned.payload->>'spotify_track_id'
             WHERE execution.apply_run_id = $1 AND (
                 (planned.operation_type = 'exclude_track' AND NOT EXISTS (
                     SELECT 1 FROM excluded_tracks exclusion
                     WHERE exclusion.provider_account_id = $2
                       AND exclusion.track_id = planned_track.track_id
                       AND exclusion.restored_at IS NULL
                 )) OR
                 (planned.operation_type = 'remove_track' AND EXISTS (
                     SELECT 1
                     FROM provider_account_playlists account_playlist
                     JOIN provider_playlists playlist
                       ON playlist.id = account_playlist.provider_playlist_id
                     JOIN provider_observed_playlist_tracks membership
                       ON membership.provider_playlist_id = playlist.id
                      AND membership.snapshot_id = $3
                     JOIN provider_tracks track ON track.id = membership.provider_track_id
                     WHERE account_playlist.provider_account_id = $2
                       AND account_playlist.present_in_latest_snapshot
                       AND playlist.provider_playlist_id = planned.payload->>'spotify_playlist_id'
                       AND track.provider = 'spotify'
                       AND track.provider_track_id = planned.payload->>'spotify_track_id'
                 ))
             )",
        )
        .bind(run_id)
        .bind(account_id)
        .bind(snapshot_id)
        .fetch_one(database.pool())
        .await?;
        if unverified == 0 {
            sqlx::query(
                "UPDATE sync_apply_runs SET status = 'succeeded', finished_at = now(),
                 summary = summary || jsonb_build_object('verified_snapshot_id', $2::text)
                 WHERE id = $1",
            )
            .bind(run_id)
            .bind(snapshot_id)
            .execute(database.pool())
            .await?;
            verified += 1;
        }
    }
    if verify_current_publication {
        let current_proposal: Option<Uuid> = sqlx::query_scalar(
            "SELECT plan.proposal_generation_id
             FROM sync_apply_runs apply
             JOIN sync_runs plan ON plan.id = apply.plan_id
             WHERE apply.provider_account_id = $1 AND apply.phase = 'publish'
               AND apply.status = 'succeeded'
             ORDER BY apply.started_at DESC, apply.id DESC LIMIT 1",
        )
        .bind(account_id)
        .fetch_optional(database.pool())
        .await?;
        if let Some(proposal_id) = current_proposal {
            verify_publication(database, account_id, snapshot_id, proposal_id).await?;
        }
    }
    let destructive_runs: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM sync_apply_runs
         WHERE provider_account_id = $1 AND phase IN ('cleanup', 'retirement')
           AND status = 'awaiting_pull'
         ORDER BY started_at, id",
    )
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    for run_id in destructive_runs {
        let remaining: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint
             FROM sync_apply_operations execution
             JOIN sync_operations planned ON planned.id = execution.planned_operation_id
             WHERE execution.apply_run_id = $1 AND (
                 (planned.operation_type = 'remove_track' AND EXISTS (
                     SELECT 1
                     FROM provider_account_playlists account_playlist
                     JOIN provider_playlists playlist
                       ON playlist.id = account_playlist.provider_playlist_id
                     JOIN provider_observed_playlist_tracks membership
                       ON membership.provider_playlist_id = playlist.id
                      AND membership.snapshot_id = $3
                     JOIN provider_tracks track ON track.id = membership.provider_track_id
                     WHERE account_playlist.provider_account_id = $2
                       AND playlist.provider_playlist_id = planned.payload->>'spotify_playlist_id'
                       AND track.provider = 'spotify'
                       AND track.provider_track_id = planned.payload->>'spotify_track_id'
                 )) OR
                (planned.operation_type = 'remove_saved_track' AND EXISTS (
                     SELECT 1
                     FROM provider_observed_saved_tracks saved
                     JOIN provider_tracks track ON track.id = saved.provider_track_id
                     WHERE saved.snapshot_id = $3
                       AND track.provider = 'spotify'
                       AND track.provider_track_id = planned.payload->>'spotify_track_id'
                 )) OR
                 (planned.operation_type = 'remove_saved_album' AND EXISTS (
                     SELECT 1
                     FROM provider_observed_saved_albums saved
                     JOIN provider_albums album ON album.id = saved.provider_album_id
                     WHERE saved.snapshot_id = $3
                       AND album.provider = 'spotify'
                       AND album.provider_album_id = planned.payload #>> '{detail,spotify_album_id}'
                 )) OR
                 (planned.operation_type IN ('remove_external_playlist', 'archive_playlist')
                  AND EXISTS (
                     SELECT 1
                     FROM provider_account_playlists account_playlist
                     JOIN provider_playlists playlist
                       ON playlist.id = account_playlist.provider_playlist_id
                     WHERE account_playlist.provider_account_id = $2
                       AND account_playlist.present_in_latest_snapshot
                       AND playlist.provider_playlist_id = planned.payload->>'spotify_playlist_id'
                 ))
             )",
        )
        .bind(run_id)
        .bind(account_id)
        .bind(snapshot_id)
        .fetch_one(database.pool())
        .await?;
        if remaining == 0 {
            sqlx::query(
                "UPDATE sync_apply_runs SET status = 'succeeded', finished_at = now(),
                 summary = summary || jsonb_build_object('verified_snapshot_id', $2::text)
                 WHERE id = $1",
            )
            .bind(run_id)
            .bind(snapshot_id)
            .execute(database.pool())
            .await?;
            verified += 1;
        }
    }
    Ok(verified)
}

/// Accepts the newest provider observation as the durable managed baseline only
/// after proving exact ordered equality with the latest approved proposal.
///
/// This is the terminal step of record-only maintenance. It never contacts or
/// writes the provider and cannot bless a partially converged model.
pub async fn accept_current_provider_state(
    database: &Database,
    account_label: &str,
) -> Result<ProviderStateAcceptance> {
    let account_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM provider_accounts
         WHERE provider = 'spotify' AND account_label = $1",
    )
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration("Spotify account is not imported"))?;
    let snapshot_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM provider_inventory_observations
         WHERE provider_account_id = $1
         ORDER BY captured_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration("no provider observation exists"))?;
    let proposal_generation_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM playlist_generations
         WHERE provider_account_id = $1 AND status = 'approved'
         ORDER BY approved_at DESC NULLS LAST, created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration("no approved playlist model exists"))?;
    if !verify_publication(database, account_id, snapshot_id, proposal_generation_id).await? {
        return Err(configuration(
            "current provider state cannot be accepted until the approved playlist model has identical ordered membership",
        ));
    }
    // A direct provider-side Unlike is itself the newer user decision. Retire
    // any older explicit keep directive when the newest complete saved-track
    // inventory no longer contains that track. This is Neon-only convergence;
    // it neither calls the provider nor invents a cleanup operation.
    sqlx::query(
        "UPDATE playlist_track_directives directive
         SET superseded_at = now()
         FROM playlist_surfaces surface
         WHERE directive.surface_id = surface.id
           AND directive.chordrift_account_id = surface.chordrift_account_id
           AND directive.superseded_at IS NULL
           AND directive.directive = 'include'
           AND surface.stable_key = 'provider-saved-tracks:' || $1::text
           AND NOT EXISTS (
               SELECT 1
               FROM provider_current_inventories inventory
               JOIN provider_observed_saved_tracks saved
                 ON saved.snapshot_id = inventory.source_snapshot_id
               JOIN provider_tracks provider ON provider.id = saved.provider_track_id
               WHERE inventory.provider_account_id = $1
                 AND provider.track_id = directive.track_id
           )",
    )
    .bind(account_id)
    .execute(database.pool())
    .await?;
    let playlist_count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM managed_playlist_verifications
         WHERE provider_account_id = $1 AND proposal_generation_id = $2
           AND verified_snapshot_id = $3",
    )
    .bind(account_id)
    .bind(proposal_generation_id)
    .bind(snapshot_id)
    .fetch_one(database.pool())
    .await?;
    Ok(ProviderStateAcceptance {
        snapshot_id,
        proposal_generation_id,
        playlist_count: usize::try_from(playlist_count)
            .map_err(|_| configuration("accepted playlist count exceeds usize"))?,
    })
}

async fn verify_publish_operations(
    database: &Database,
    account_id: Uuid,
    snapshot_id: Uuid,
    apply_run_id: Uuid,
) -> Result<bool> {
    let operations = operations(database, apply_run_id).await?;
    for operation in operations {
        if operation.status != "succeeded" {
            return Ok(false);
        }
        if operation.kind == "upload_artwork" {
            // Spotify does not expose a stable cover-content digest. A successful
            // provider response is the strongest available verification signal.
            continue;
        }
        let target = target_for(database, apply_run_id, &operation).await?;
        let observed_name: Option<String> = sqlx::query_scalar(
            "SELECT current.name
             FROM current_spotify_playlists current
             WHERE current.provider_account_id = $1
               AND current.spotify_playlist_id = $2",
        )
        .bind(account_id)
        .bind(&target)
        .fetch_optional(database.pool())
        .await?;
        let Some(observed_name) = observed_name else {
            return Ok(false);
        };
        match operation.kind.as_str() {
            "create_playlist" | "rename_playlist" => {
                if observed_name != operation.playlist_name {
                    return Ok(false);
                }
            }
            "add_track" | "restore_track" => {
                let spotify_track_id = operation.spotify_track_id.as_deref().ok_or_else(|| {
                    configuration("published track operation has no Spotify track ID")
                })?;
                let present: bool = sqlx::query_scalar(
                    "SELECT EXISTS (
                         SELECT 1
                         FROM provider_playlists playlist
                         JOIN provider_observed_playlist_tracks membership
                           ON membership.provider_playlist_id = playlist.id
                          AND membership.snapshot_id = $3
                         JOIN provider_tracks track ON track.id = membership.provider_track_id
                         WHERE playlist.provider = 'spotify'
                           AND playlist.provider_playlist_id = $1
                           AND track.provider = 'spotify'
                           AND track.provider_track_id = $2
                     )",
                )
                .bind(&target)
                .bind(spotify_track_id)
                .bind(snapshot_id)
                .fetch_one(database.pool())
                .await?;
                if !present {
                    return Ok(false);
                }
            }
            "reorder_playlist" => {
                let playlist_id = operation.playlist_id.ok_or_else(|| {
                    configuration("published reorder has no canonical playlist ID")
                })?;
                let desired: Vec<Uuid> = sqlx::query_scalar(
                    "SELECT track_id FROM playlist_tracks
                     WHERE playlist_id = $1 ORDER BY position",
                )
                .bind(playlist_id)
                .fetch_all(database.pool())
                .await?;
                let current: Vec<Uuid> = sqlx::query_scalar(
                    "SELECT track.track_id
                     FROM provider_account_playlists policy
                     JOIN provider_playlists playlist
                       ON playlist.id = policy.provider_playlist_id
                     JOIN provider_observed_playlist_tracks membership
                       ON membership.provider_playlist_id = playlist.id
                      AND membership.snapshot_id = $3
                     JOIN provider_tracks track ON track.id = membership.provider_track_id
                     WHERE policy.provider_account_id = $1
                       AND policy.present_in_latest_snapshot
                       AND playlist.provider = 'spotify'
                       AND playlist.provider_playlist_id = $2
                     ORDER BY membership.position",
                )
                .bind(account_id)
                .bind(&target)
                .bind(snapshot_id)
                .fetch_all(database.pool())
                .await?;
                if current != desired {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
}

async fn verify_publication(
    database: &Database,
    account_id: Uuid,
    snapshot_id: Uuid,
    proposal_id: Uuid,
) -> Result<bool> {
    type Desired = (Uuid, Vec<Uuid>);
    let desired_rows = sqlx::query(
        "SELECT playlist.concept_id, membership.position, membership.track_id
         FROM playlists playlist
         LEFT JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
         LEFT JOIN excluded_tracks exclusion
           ON exclusion.provider_account_id = $2
          AND exclusion.track_id = membership.track_id
          AND exclusion.restored_at IS NULL
         WHERE playlist.generation_id = $1 AND exclusion.id IS NULL
         ORDER BY playlist.concept_id, membership.position",
    )
    .bind(proposal_id)
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    let mut desired: BTreeMap<Uuid, Desired> = BTreeMap::new();
    for row in desired_rows {
        let concept: Uuid = row.try_get("concept_id")?;
        let entry = desired
            .entry(concept)
            .or_insert_with(|| (concept, Vec::new()));
        if let Some(track_id) = row.try_get::<Option<Uuid>, _>("track_id")? {
            entry.1.push(track_id);
        }
    }
    let current_rows = sqlx::query(
        "SELECT provider.id AS provider_playlist_id, provider.concept_id,
                membership.position, track.track_id
         FROM provider_account_playlists account_playlist
         JOIN provider_playlists provider ON provider.id = account_playlist.provider_playlist_id
         LEFT JOIN provider_observed_playlist_tracks membership
           ON membership.provider_playlist_id = provider.id AND membership.snapshot_id = $2
         LEFT JOIN provider_tracks track ON track.id = membership.provider_track_id
         WHERE account_playlist.provider_account_id = $1
           AND account_playlist.present_in_latest_snapshot
           AND provider.concept_id IS NOT NULL
           AND provider.concept_id IN (
               SELECT proposed.concept_id
               FROM playlists proposed
               WHERE proposed.generation_id = $3)
         ORDER BY provider.concept_id, membership.position",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .bind(proposal_id)
    .fetch_all(database.pool())
    .await?;
    let mut current: BTreeMap<Uuid, (Uuid, Vec<Uuid>)> = BTreeMap::new();
    for row in current_rows {
        let concept: Uuid = row.try_get("concept_id")?;
        let entry = current.entry(concept).or_insert_with(|| {
            (
                row.try_get("provider_playlist_id")
                    .expect("selected provider playlist ID"),
                Vec::new(),
            )
        });
        if let Some(track_id) = row.try_get::<Option<Uuid>, _>("track_id")? {
            entry.1.push(track_id);
        }
    }
    if desired.is_empty()
        || desired.len() != current.len()
        || desired.iter().any(|(concept, (_, tracks))| {
            current.get(concept).is_none_or(|value| &value.1 != tracks)
        })
    {
        return Ok(false);
    }
    let verification_rows = desired
        .iter()
        .map(|(concept, (_, tracks))| {
            let bytes = serde_json::to_vec(tracks)?;
            Ok((
                *concept,
                current[concept].0,
                format!("{:x}", Sha256::digest(bytes)),
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut tx = database.pool().begin().await?;
    let mut verification_insert = QueryBuilder::<Postgres>::new(
        "INSERT INTO managed_playlist_verifications
         (provider_account_id, provider_playlist_id, concept_id,
          proposal_generation_id, verified_snapshot_id, desired_state_hash) ",
    );
    verification_insert.push_values(
        &verification_rows,
        |mut row, (concept, provider_playlist_id, hash)| {
            row.push_bind(account_id)
                .push_bind(*provider_playlist_id)
                .push_bind(*concept)
                .push_bind(proposal_id)
                .push_bind(snapshot_id)
                .push_bind(hash);
        },
    );
    verification_insert.push(
        " ON CONFLICT (provider_account_id, provider_playlist_id, verified_snapshot_id)
          DO UPDATE SET desired_state_hash = EXCLUDED.desired_state_hash
          RETURNING id, concept_id",
    );
    let verification_ids = verification_insert
        .build()
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|row| Ok((row.try_get("concept_id")?, row.try_get("id")?)))
        .collect::<Result<BTreeMap<Uuid, Uuid>>>()?;
    let mut memberships = Vec::new();
    for (concept, (_, tracks)) in desired {
        let verification_id = verification_ids[&concept];
        for (position, track_id) in tracks.into_iter().enumerate() {
            memberships.push((
                verification_id,
                track_id,
                i32::try_from(position)
                    .map_err(|_| configuration("verified playlist position exceeds i32"))?,
            ));
        }
    }
    for chunk in memberships.chunks(10_000) {
        let mut membership_insert = QueryBuilder::<Postgres>::new(
            "INSERT INTO managed_playlist_verified_tracks
             (verification_id, track_id, position) ",
        );
        membership_insert.push_values(chunk, |mut row, (verification_id, track_id, position)| {
            row.push_bind(*verification_id)
                .push_bind(*track_id)
                .push_bind(*position);
        });
        membership_insert.push(
            " ON CONFLICT (verification_id, track_id) DO UPDATE
              SET position = EXCLUDED.position",
        );
        membership_insert.build().execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(true)
}

struct Gate {
    account_id: Uuid,
    provider_account_id: String,
    plan_id: Uuid,
    plan_hash: String,
}

async fn load_gate(
    database: &Database,
    account_label: &str,
    assessment_id: Uuid,
    phase: ApplyPhase,
) -> Result<Gate> {
    let row = sqlx::query(
        "SELECT account.id AS account_id, account.provider_account_id,
                assessment.sync_run_id AS plan_id, assessment.status AS readiness_status,
                assessment.assessment_version, run.planner_version, run.input_hash,
                run.source_snapshot_id, run.proposal_generation_id,
                (SELECT id FROM provider_inventory_observations latest
                 WHERE latest.provider_account_id = account.id
                 ORDER BY captured_at DESC, id DESC LIMIT 1) AS latest_snapshot_id
         FROM sync_readiness_assessments assessment
         JOIN provider_accounts account ON account.id = assessment.provider_account_id
         JOIN sync_runs run ON run.id = assessment.sync_run_id
         WHERE assessment.id = $1 AND account.provider = 'spotify'
           AND account.account_label = $2",
    )
    .bind(assessment_id)
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration("no matching readiness assessment exists for this account"))?;
    if row.try_get::<String, _>("readiness_status")? != "ready"
        || row.try_get::<String, _>("assessment_version")? != READINESS_VERSION
        || row.try_get::<String, _>("planner_version")? != PLANNER_VERSION
    {
        return Err(configuration(
            "apply requires a ready v0.1.2 assessment of a v10 plan",
        ));
    }
    if row.try_get::<Uuid, _>("source_snapshot_id")?
        != row.try_get::<Uuid, _>("latest_snapshot_id")?
    {
        return Err(configuration(
            "the assessed Spotify snapshot is stale; pull, plan, and assess again",
        ));
    }
    let plan_id: Uuid = row.try_get("plan_id")?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM sync_operations
         WHERE sync_run_id = $1 AND phase = $2",
    )
    .bind(plan_id)
    .bind(phase.as_str())
    .fetch_one(database.pool())
    .await?;
    if count == 0 {
        return Err(configuration(
            "the selected plan has no operations in this phase",
        ));
    }
    if phase.destructive() {
        let proposal_id: Uuid = row.try_get("proposal_generation_id")?;
        let snapshot_id: Uuid = row.try_get("latest_snapshot_id")?;
        let verification = sqlx::query(
            "SELECT
               (SELECT count(*)::bigint FROM playlists WHERE generation_id = $2) AS required,
               count(DISTINCT verification.concept_id)::bigint AS verified
             FROM managed_playlist_verifications verification
             WHERE verification.provider_account_id = $1
               AND verification.proposal_generation_id = $2
               AND verification.verified_snapshot_id = $3",
        )
        .bind(row.try_get::<Uuid, _>("account_id")?)
        .bind(proposal_id)
        .bind(snapshot_id)
        .fetch_one(database.pool())
        .await?;
        let required = verification.try_get::<i64, _>("required")?;
        let verified = verification.try_get::<i64, _>("verified")?;
        if required != verified
            && !verify_publication(
                database,
                row.try_get("account_id")?,
                snapshot_id,
                proposal_id,
            )
            .await?
        {
            return Err(configuration(
                "destructive phases require every canonical destination to be verified in the current pulled snapshot",
            ));
        }
    }
    if phase == ApplyPhase::Retirement {
        let approved: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sync_retirement_approvals
             WHERE provider_account_id = $1 AND plan_id = $2
               AND plan_input_hash = $3)",
        )
        .bind(row.try_get::<Uuid, _>("account_id")?)
        .bind(plan_id)
        .bind(row.try_get::<String, _>("input_hash")?)
        .fetch_one(database.pool())
        .await?;
        if !approved {
            return Err(configuration(
                "retirement requires `chordrift sync retirement-approve` for this exact plan",
            ));
        }
    }
    Ok(Gate {
        account_id: row.try_get("account_id")?,
        provider_account_id: row.try_get("provider_account_id")?,
        plan_id,
        plan_hash: row.try_get("input_hash")?,
    })
}

async fn prepare_run(
    database: &Database,
    gate: &Gate,
    assessment_id: Uuid,
    phase: ApplyPhase,
) -> Result<(Uuid, bool, bool, DateTime<Utc>)> {
    let input_hash = format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}:{assessment_id}:{}:{APPLY_VERSION}",
                gate.plan_hash,
                phase.as_str()
            )
            .as_bytes()
        )
    );
    if let Some(row) = sqlx::query(
        "SELECT id, status, started_at FROM sync_apply_runs
         WHERE provider_account_id = $1 AND plan_id = $2
           AND readiness_assessment_id = $3 AND apply_version = $4 AND phase = $5",
    )
    .bind(gate.account_id)
    .bind(gate.plan_id)
    .bind(assessment_id)
    .bind(APPLY_VERSION)
    .bind(phase.as_str())
    .fetch_optional(database.pool())
    .await?
    {
        let id: Uuid = row.try_get("id")?;
        let status: String = row.try_get("status")?;
        if status == "succeeded" {
            return Ok((id, true, true, row.try_get("started_at")?));
        }
        sqlx::query(
            "UPDATE sync_apply_runs SET status = 'running', finished_at = NULL,
             last_error = NULL WHERE id = $1",
        )
        .bind(id)
        .execute(database.pool())
        .await?;
        return Ok((id, true, false, row.try_get("started_at")?));
    }
    let mut tx = database.pool().begin().await?;
    let row = sqlx::query(
        "INSERT INTO sync_apply_runs
         (provider_account_id, plan_id, readiness_assessment_id, apply_version,
          phase, input_hash, operation_count)
         SELECT $1, $2, $3, $4, $5, $6, count(*)::integer
         FROM sync_operations WHERE sync_run_id = $2 AND phase = $5
         RETURNING id, started_at",
    )
    .bind(gate.account_id)
    .bind(gate.plan_id)
    .bind(assessment_id)
    .bind(APPLY_VERSION)
    .bind(phase.as_str())
    .bind(input_hash)
    .fetch_one(&mut *tx)
    .await?;
    let id: Uuid = row.try_get("id")?;
    sqlx::query(
        "INSERT INTO sync_apply_operations
         (apply_run_id, planned_operation_id, sequence, operation_key)
         SELECT $1, id, sequence, operation_key FROM sync_operations
         WHERE sync_run_id = $2 AND phase = $3 ORDER BY sequence",
    )
    .bind(id)
    .bind(gate.plan_id)
    .bind(phase.as_str())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((id, false, false, row.try_get("started_at")?))
}

async fn execute_phase(
    database: &Database,
    session: &MutationSession,
    apply_run_id: Uuid,
    phase: ApplyPhase,
) -> Result<()> {
    let operations = operations(database, apply_run_id).await?;
    match phase {
        ApplyPhase::Publish => execute_publish(database, session, apply_run_id, operations).await,
        ApplyPhase::Reconcile => {
            execute_reconcile(database, session, apply_run_id, operations).await
        }
        ApplyPhase::Cleanup | ApplyPhase::Retirement => {
            execute_destructive(database, session, apply_run_id, operations).await
        }
    }
}

async fn execute_publish(
    database: &Database,
    session: &MutationSession,
    run_id: Uuid,
    initial_operations: Vec<Operation>,
) -> Result<()> {
    let live = session.playlists().await?;
    for operation in initial_operations.iter().filter(|op| {
        op.status != "succeeded"
            && matches!(op.kind.as_str(), "create_playlist" | "rename_playlist")
    }) {
        mark_running(database, run_id, operation).await?;
        let target = if operation.kind == "create_playlist" {
            if let Some(existing) = resolved_target(database, run_id, operation.playlist_id).await?
            {
                existing
            } else {
                let description = detail_string(&operation.detail, "description")?;
                let matches: Vec<_> = live
                    .iter()
                    .filter(|playlist| {
                        playlist.owner.id == session.user_id()
                            && playlist.name == operation.playlist_name
                            && playlist.description.as_deref().unwrap_or("") == description
                    })
                    .collect();
                let playlist = match matches.as_slice() {
                    [existing] => (*existing).clone(),
                    [] => {
                        session
                            .create_playlist(
                                &operation.playlist_name,
                                description,
                                operation
                                    .detail
                                    .get("public")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false),
                            )
                            .await?
                    }
                    _ => {
                        return Err(configuration(
                            "multiple owned playlists match a pending create",
                        ));
                    }
                };
                persist_target(
                    database,
                    run_id,
                    operation,
                    &playlist.id,
                    playlist.snapshot_id.as_deref(),
                )
                .await?;
                playlist.id
            }
        } else {
            let target = target_for(database, run_id, operation).await?;
            session
                .update_playlist(&target, &operation.playlist_name, None)
                .await?;
            target
        };
        mark_succeeded(database, run_id, operation, &target, json!({})).await?;
    }

    let current_operations = operations(database, run_id).await?;
    let mut additions: BTreeMap<String, Vec<&Operation>> = BTreeMap::new();
    for operation in current_operations.iter().filter(|op| {
        op.status != "succeeded"
            && matches!(
                op.kind.as_str(),
                "add_track" | "restore_track" | "reorder_playlist"
            )
    }) {
        let target = target_for(database, run_id, operation).await?;
        additions.entry(target).or_default().push(operation);
    }
    for (target, mut pending) in additions {
        pending.sort_by_key(|operation| {
            operation
                .detail
                .get("position")
                .and_then(Value::as_i64)
                .unwrap_or(i64::MAX)
        });
        let live_items = session.playlist_items(&target).await?;
        let playlist_id = pending
            .first()
            .and_then(|operation| operation.playlist_id)
            .ok_or_else(|| configuration("planned canonical addition has no playlist identity"))?;
        let desired: Vec<String> = sqlx::query_scalar(
            "SELECT provider.provider_track_id
             FROM playlist_tracks membership
             JOIN provider_tracks provider ON provider.track_id = membership.track_id
              AND provider.provider = 'spotify'
             JOIN sync_apply_runs run ON run.id = $2
             LEFT JOIN excluded_tracks exclusion
               ON exclusion.provider_account_id = run.provider_account_id
              AND exclusion.track_id = membership.track_id
              AND exclusion.restored_at IS NULL
             WHERE membership.playlist_id = $1 AND exclusion.id IS NULL
             ORDER BY membership.position",
        )
        .bind(playlist_id)
        .bind(run_id)
        .fetch_all(database.pool())
        .await?;
        if live_items == desired {
            for operation in &pending {
                mark_succeeded(
                    database,
                    run_id,
                    operation,
                    &target,
                    json!({"reused_exact_live_membership": true}),
                )
                .await?;
            }
            continue;
        }
        match playlist_membership_write(&live_items, &desired, &pending)? {
            PlaylistMembershipWrite::ExactReorder => {
                for operation in &pending {
                    mark_running(database, run_id, operation).await?;
                }
                let first = desired.len().min(100);
                let mut snapshot = session.replace_items(&target, &desired[..first]).await?;
                for chunk in desired[first..].chunks(100) {
                    snapshot = session.add_items(&target, chunk, None).await?;
                }
                for operation in &pending {
                    mark_succeeded(
                        database,
                        run_id,
                        operation,
                        &target,
                        json!({"snapshot_id": snapshot, "exact_order_replaced": true}),
                    )
                    .await?;
                }
            }
            PlaylistMembershipWrite::EnumeratedAdditions { reused, missing } => {
                for operation in reused {
                    mark_succeeded(
                        database,
                        run_id,
                        operation,
                        &target,
                        json!({"reused_live_membership": true}),
                    )
                    .await?;
                }
                for chunk in missing.chunks(100) {
                    for (operation, _) in chunk {
                        mark_running(database, run_id, operation).await?;
                    }
                    let spotify_track_ids = chunk
                        .iter()
                        .map(|(_, spotify_track_id)| spotify_track_id.clone())
                        .collect::<Vec<_>>();
                    let snapshot = session.add_items(&target, &spotify_track_ids, None).await?;
                    for (operation, _) in chunk {
                        mark_succeeded(
                            database,
                            run_id,
                            operation,
                            &target,
                            json!({"snapshot_id": snapshot, "enumerated_addition": true}),
                        )
                        .await?;
                    }
                }
            }
        }
    }

    let artwork_operations = operations(database, run_id).await?;
    for operation in artwork_operations
        .iter()
        .filter(|op| op.status != "succeeded" && op.kind == "upload_artwork")
    {
        let target = target_for(database, run_id, operation).await?;
        mark_running(database, run_id, operation).await?;
        let path = PathBuf::from(detail_string(&operation.detail, "artifact_path")?);
        let encoded = spotify_jpeg(&path, detail_string(&operation.detail, "content_sha256")?)?;
        session.upload_cover(&target, &encoded).await?;
        mark_succeeded(
            database,
            run_id,
            operation,
            &target,
            json!({"source_sha256": operation.detail.get("content_sha256"), "jpeg_base64_bytes": encoded.len()}),
        )
        .await?;
    }
    Ok(())
}

async fn execute_reconcile(
    database: &Database,
    session: &MutationSession,
    run_id: Uuid,
    operations: Vec<Operation>,
) -> Result<()> {
    for operation in operations
        .iter()
        .filter(|op| op.status != "succeeded" && op.kind == "exclude_track")
    {
        mark_running(database, run_id, operation).await?;
        sqlx::query(
            "INSERT INTO excluded_tracks
                 (provider_account_id, track_id, source_provider,
                  source_provider_playlist_id, previous_concept_id, excluded_at,
                  exclusion_reason)
                 SELECT run.provider_account_id, provider_track.track_id, 'spotify',
                        planned.provider_playlist_id,
                        (planned.payload->'detail'->>'previous_concept_id')::uuid,
                        now(), 'removed_from_verified_managed_playlist'
                 FROM sync_apply_runs run
                 JOIN sync_operations planned ON planned.id = $2
                 JOIN provider_tracks provider_track
                   ON provider_track.provider = 'spotify'
                  AND provider_track.provider_track_id = planned.payload->>'spotify_track_id'
                 WHERE run.id = $1
                 ON CONFLICT (provider_account_id, track_id) WHERE restored_at IS NULL DO NOTHING",
        )
        .bind(run_id)
        .bind(operation.id)
        .execute(database.pool())
        .await?;
        mark_succeeded(
            database,
            run_id,
            operation,
            "neon",
            json!({"neon_only": true}),
        )
        .await?;
    }
    let mut removals: BTreeMap<String, Vec<&Operation>> = BTreeMap::new();
    for operation in operations
        .iter()
        .filter(|op| op.status != "succeeded" && op.kind == "remove_track")
    {
        let target = target_for(database, run_id, operation).await?;
        mark_running(database, run_id, operation).await?;
        removals.entry(target).or_default().push(operation);
    }
    for (target, pending) in removals {
        let mut expected = pending.first().and_then(|operation| {
            detail_optional_string(&operation.detail, "expected_snapshot_id").map(str::to_owned)
        });
        for chunk in pending.chunks(100) {
            let tracks = chunk
                .iter()
                .map(|operation| {
                    operation
                        .spotify_track_id
                        .clone()
                        .ok_or_else(|| configuration("planned removal has no Spotify track ID"))
                })
                .collect::<Result<Vec<_>>>()?;
            let snapshot = session
                .remove_items(&target, &tracks, expected.as_deref())
                .await?;
            for operation in chunk {
                mark_succeeded(
                    database,
                    run_id,
                    operation,
                    &target,
                    json!({"snapshot_id": snapshot}),
                )
                .await?;
            }
            expected = Some(snapshot);
        }
    }
    Ok(())
}

async fn execute_destructive(
    database: &Database,
    session: &MutationSession,
    run_id: Uuid,
    operations: Vec<Operation>,
) -> Result<()> {
    let track_removals = operations
        .iter()
        .filter(|operation| operation.kind == "remove_track")
        .cloned()
        .collect();
    execute_reconcile(database, session, run_id, track_removals).await?;
    let saved_track_removals = operations
        .iter()
        .filter(|operation| {
            operation.status != "succeeded" && operation.kind == "remove_saved_track"
        })
        .collect::<Vec<_>>();
    for chunk in saved_track_removals.chunks(40) {
        let targets = chunk
            .iter()
            .map(|operation| {
                operation.spotify_track_id.clone().ok_or_else(|| {
                    configuration("planned saved-track removal has no Spotify track ID")
                })
            })
            .collect::<Result<Vec<_>>>()?;
        for operation in chunk {
            mark_running(database, run_id, operation).await?;
        }
        session.remove_library_tracks(&targets).await?;
        for (operation, target) in chunk.iter().zip(&targets) {
            mark_succeeded(
                database,
                run_id,
                operation,
                target,
                json!({"library_surface": "saved_tracks"}),
            )
            .await?;
        }
    }
    let saved_album_removals = operations
        .iter()
        .filter(|operation| {
            operation.status != "succeeded" && operation.kind == "remove_saved_album"
        })
        .collect::<Vec<_>>();
    for chunk in saved_album_removals.chunks(40) {
        let targets = chunk
            .iter()
            .map(|operation| {
                detail_string(&operation.detail, "spotify_album_id").map(str::to_owned)
            })
            .collect::<Result<Vec<_>>>()?;
        for operation in chunk {
            mark_running(database, run_id, operation).await?;
        }
        session.remove_library_albums(&targets).await?;
        for (operation, target) in chunk.iter().zip(&targets) {
            mark_succeeded(
                database,
                run_id,
                operation,
                target,
                json!({"library_surface": "saved_albums", "container_only": true}),
            )
            .await?;
        }
    }
    let relationships = operations
        .iter()
        .filter(|operation| {
            operation.status != "succeeded"
                && matches!(
                    operation.kind.as_str(),
                    "remove_external_playlist" | "archive_playlist"
                )
        })
        .collect::<Vec<_>>();
    for chunk in relationships.chunks(40) {
        let targets = chunk
            .iter()
            .map(|operation| {
                operation.spotify_playlist_id.clone().ok_or_else(|| {
                    configuration("planned library removal has no Spotify playlist ID")
                })
            })
            .collect::<Result<Vec<_>>>()?;
        for operation in chunk {
            mark_running(database, run_id, operation).await?;
        }
        session.remove_library_playlists(&targets).await?;
        for (operation, target) in chunk.iter().zip(&targets) {
            mark_succeeded(
                database,
                run_id,
                operation,
                target,
                json!({"relationship_only": true}),
            )
            .await?;
        }
    }
    Ok(())
}

async fn operations(database: &Database, run_id: Uuid) -> Result<Vec<Operation>> {
    let rows = sqlx::query(
        "SELECT planned.id, planned.sequence, planned.operation_type,
                planned.operation_key, planned.playlist_id, planned.payload,
                execution.status
         FROM sync_apply_operations execution
         JOIN sync_operations planned ON planned.id = execution.planned_operation_id
         WHERE execution.apply_run_id = $1 ORDER BY planned.sequence",
    )
    .bind(run_id)
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            let payload: Value = row.try_get("payload")?;
            Ok(Operation {
                id: row.try_get("id")?,
                kind: row.try_get("operation_type")?,
                playlist_id: row.try_get("playlist_id")?,
                playlist_name: payload
                    .get("playlist_name")
                    .and_then(Value::as_str)
                    .unwrap_or("-")
                    .to_owned(),
                spotify_playlist_id: payload
                    .get("spotify_playlist_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                spotify_track_id: payload
                    .get("spotify_track_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                detail: payload.get("detail").cloned().unwrap_or_else(|| json!({})),
                status: row.try_get("status")?,
            })
        })
        .collect()
}

async fn resolved_target(
    database: &Database,
    run_id: Uuid,
    playlist_id: Option<Uuid>,
) -> Result<Option<String>> {
    let Some(playlist_id) = playlist_id else {
        return Ok(None);
    };
    sqlx::query_scalar(
        "SELECT spotify_playlist_id FROM sync_apply_playlist_targets
         WHERE apply_run_id = $1 AND playlist_id = $2",
    )
    .bind(run_id)
    .bind(playlist_id)
    .fetch_optional(database.pool())
    .await
    .map_err(Into::into)
}

async fn target_for(database: &Database, run_id: Uuid, operation: &Operation) -> Result<String> {
    if let Some(id) = &operation.spotify_playlist_id {
        return Ok(id.clone());
    }
    if let Some(target) = resolved_target(database, run_id, operation.playlist_id).await? {
        return Ok(target);
    }
    sqlx::query_scalar(
        "SELECT spotify_playlist_id FROM sync_apply_playlist_targets
         WHERE apply_run_id = $1 AND lower(playlist_name) = lower($2)
         ORDER BY resolved_at DESC LIMIT 1",
    )
    .bind(run_id)
    .bind(&operation.playlist_name)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration("a prior create operation has not resolved this playlist"))
}

async fn persist_target(
    database: &Database,
    run_id: Uuid,
    operation: &Operation,
    spotify_id: &str,
    snapshot_id: Option<&str>,
) -> Result<()> {
    let concept_id = operation
        .detail
        .get("concept_id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    sqlx::query(
        "INSERT INTO sync_apply_playlist_targets
         (apply_run_id, playlist_id, concept_id, playlist_name,
          spotify_playlist_id, provider_snapshot_id)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (apply_run_id, spotify_playlist_id) DO NOTHING",
    )
    .bind(run_id)
    .bind(operation.playlist_id)
    .bind(concept_id)
    .bind(&operation.playlist_name)
    .bind(spotify_id)
    .bind(snapshot_id)
    .execute(database.pool())
    .await?;
    Ok(())
}

async fn mark_running(database: &Database, run_id: Uuid, operation: &Operation) -> Result<()> {
    sqlx::query(
        "UPDATE sync_apply_operations SET status = 'running', attempt_count = attempt_count + 1,
         started_at = now(), last_error = NULL WHERE apply_run_id = $1 AND planned_operation_id = $2",
    )
    .bind(run_id).bind(operation.id).execute(database.pool()).await?;
    Ok(())
}

async fn mark_succeeded(
    database: &Database,
    run_id: Uuid,
    operation: &Operation,
    target: &str,
    response: Value,
) -> Result<()> {
    sqlx::query(
        "UPDATE sync_apply_operations SET status = 'succeeded',
         resolved_spotify_playlist_id = $3, provider_response = $4,
         executed_at = now(), last_error = NULL
         WHERE apply_run_id = $1 AND planned_operation_id = $2",
    )
    .bind(run_id)
    .bind(operation.id)
    .bind(target)
    .bind(response)
    .execute(database.pool())
    .await?;
    Ok(())
}

async fn report(
    database: &Database,
    id: Uuid,
    resumed: bool,
    started_at: DateTime<Utc>,
) -> Result<ApplyReport> {
    let row = sqlx::query(
        "SELECT plan_id, readiness_assessment_id, phase, status, operation_count,
                succeeded_count, failed_count FROM sync_apply_runs WHERE id = $1",
    )
    .bind(id)
    .fetch_one(database.pool())
    .await?;
    Ok(ApplyReport {
        apply_run_id: id,
        plan_id: row.try_get("plan_id")?,
        assessment_id: row.try_get("readiness_assessment_id")?,
        phase: row.try_get("phase")?,
        status: row.try_get("status")?,
        operation_count: row.try_get::<i32, _>("operation_count")? as usize,
        succeeded_count: row.try_get::<i32, _>("succeeded_count")? as usize,
        failed_count: row.try_get::<i32, _>("failed_count")? as usize,
        resumed,
        started_at,
    })
}

fn spotify_jpeg(path: &PathBuf, expected_sha256: &str) -> Result<String> {
    let source = fs::read(path).map_err(|error| {
        configuration(format!(
            "cannot read approved artwork {}: {error}",
            path.display()
        ))
    })?;
    let actual = format!("{:x}", Sha256::digest(&source));
    if actual != expected_sha256 {
        return Err(configuration(format!(
            "approved artwork hash changed for {}",
            path.display()
        )));
    }
    let image = image::load_from_memory(&source)
        .map_err(|error| {
            configuration(format!(
                "cannot decode approved artwork {}: {error}",
                path.display()
            ))
        })?
        .resize(640, 640, FilterType::Lanczos3)
        .to_rgb8();
    for quality in [88, 80, 72, 64, 56, 48] {
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(Cursor::new(&mut bytes), quality)
            .encode_image(&image)
            .map_err(|error| configuration(format!("cannot encode Spotify artwork: {error}")))?;
        let encoded = STANDARD.encode(bytes);
        if encoded.len() <= 256 * 1024 {
            return Ok(encoded);
        }
    }
    Err(configuration(
        "approved artwork cannot fit Spotify's 256 KB base64 limit",
    ))
}

fn detail_string<'a>(detail: &'a Value, key: &str) -> Result<&'a str> {
    detail
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| configuration(format!("planned operation is missing {key}")))
}

fn detail_optional_string<'a>(detail: &'a Value, key: &str) -> Option<&'a str> {
    detail.get(key).and_then(Value::as_str)
}

fn configuration(message: impl Into<String>) -> ChordriftError {
    ChordriftError::Configuration(message.into())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use super::{ApplyPhase, Operation, PlaylistMembershipWrite, playlist_membership_write};

    fn addition(spotify_track_id: &str) -> Operation {
        Operation {
            id: Uuid::new_v4(),
            kind: "add_track".to_owned(),
            playlist_id: Some(Uuid::new_v4()),
            playlist_name: "Fixture".to_owned(),
            spotify_playlist_id: Some("playlist".to_owned()),
            spotify_track_id: Some(spotify_track_id.to_owned()),
            detail: json!({"position": 1}),
            status: "pending".to_owned(),
        }
    }

    #[test]
    fn destructive_phases_require_the_extra_gate() {
        assert!(!ApplyPhase::Publish.destructive());
        assert!(!ApplyPhase::Reconcile.destructive());
        assert!(ApplyPhase::Cleanup.destructive());
        assert!(ApplyPhase::Retirement.destructive());
    }

    #[test]
    fn ordinary_addition_never_replaces_unrelated_live_membership() {
        let operation = addition("new-track");
        let plan = playlist_membership_write(
            &["manual-live-track".to_owned()],
            &["new-track".to_owned()],
            &[&operation],
        )
        .expect("ordinary addition is valid");

        let PlaylistMembershipWrite::EnumeratedAdditions { reused, missing } = plan else {
            panic!("ordinary additions must never select exact replacement");
        };
        assert!(reused.is_empty());
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].1, "new-track");
    }

    #[test]
    fn ordinary_addition_cannot_restore_an_unenumerated_manual_removal() {
        let operation = addition("explicit-new-track");
        let plan = playlist_membership_write(
            &[],
            &[
                "manually-removed-track".to_owned(),
                "explicit-new-track".to_owned(),
            ],
            &[&operation],
        )
        .expect("ordinary addition is valid");

        let PlaylistMembershipWrite::EnumeratedAdditions { missing, .. } = plan else {
            panic!("ordinary additions must remain enumerated");
        };
        assert_eq!(
            missing
                .iter()
                .map(|(_, spotify_id)| spotify_id.as_str())
                .collect::<Vec<_>>(),
            vec!["explicit-new-track"]
        );
    }
}
