//! Immutable, read-only synchronization planning against provider snapshots.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, QueryBuilder, Row};
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result};

const PROVIDER: &str = "spotify";
const PLANNER_VERSION: &str = "spotify-dry-run-v10";
const PLAN_ORIGIN: PlanOrigin = PlanOrigin::Maintenance;
const INTAKE_SURFACES: [(&str, &str, Option<&str>); 4] = [
    (
        "Inbox",
        "Strong recent personal discoveries awaiting verified Chordrift placement.",
        None,
    ),
    (
        "From Friends",
        "Explicit recommendations from friends awaiting verified Chordrift placement.",
        Some("recommendation"),
    ),
    (
        "Liked from Radio",
        "Radio or autoplay discoveries awaiting verified Chordrift placement.",
        Some("discovery"),
    ),
    (
        "From Prompts",
        "Intentional discoveries carried forward from Spotify prompt-generated playlists.",
        Some("prompted"),
    ),
];

/// Business origin of an immutable synchronization plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanOrigin {
    /// Ordinary library maintenance, intake, and convergence work.
    Maintenance,
    /// Publication of one approved immutable Spin.
    SpinPublication,
}

impl PlanOrigin {
    /// Stable machine-readable plan-origin label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Maintenance => "maintenance",
            Self::SpinPublication => "spin_publication",
        }
    }
}

/// Summary of an immutable synchronization plan.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanReport {
    /// Persisted dry-run identifier.
    pub plan_id: Uuid,
    /// Explicit business path that created the plan.
    pub origin: PlanOrigin,
    /// Approved proposal used as desired state.
    pub proposal_generation_id: Option<Uuid>,
    /// Immutable Spotify snapshot used as observed state.
    pub source_snapshot_id: Uuid,
    /// Whether an identical plan already existed.
    pub reused: bool,
    /// Stable hash of all plan inputs and operations.
    pub input_hash: String,
    /// Total operation count.
    pub operation_count: usize,
    /// Playlist creations.
    pub creates: usize,
    /// Playlist renames.
    pub renames: usize,
    /// Exact-order replacements where membership is already correct.
    pub reorders: usize,
    /// Track additions, excluding explicit restores.
    pub additions: usize,
    /// Explicit Excluded Tracks restores.
    pub restorations: usize,
    /// Approved cover uploads.
    pub artwork_uploads: usize,
    /// New Excluded Tracks entries inferred from verified managed state.
    pub exclusions: usize,
    /// Provider-drift or inbox removals.
    pub removals: usize,
    /// Legacy playlist retirements.
    pub retirements: usize,
    /// Approved external provider-library relationships to remove.
    pub external_cleanups: usize,
    /// Operations that require post-publication verification.
    pub deferred: usize,
    /// When the plan was persisted.
    pub created_at: DateTime<Utc>,
}

/// One inspectable operation from a dry-run plan.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedOperation {
    /// Stable execution order.
    pub sequence: i32,
    /// Safety phase.
    pub phase: String,
    /// Provider-neutral operation kind.
    pub operation_type: String,
    /// Idempotency key within the plan.
    pub operation_key: String,
    /// Human-readable playlist name at planning time.
    pub playlist_name: String,
    /// Spotify playlist ID when the playlist already exists.
    pub spotify_playlist_id: Option<String>,
    /// Spotify track ID for membership operations.
    pub spotify_track_id: Option<String>,
    /// Machine-readable operation payload.
    pub payload: Value,
    /// Machine-readable safety gates.
    pub safety: Value,
}

#[derive(Clone, Debug, Serialize)]
struct PlanOperationInput {
    phase: String,
    operation_type: String,
    operation_key: String,
    playlist_id: Option<Uuid>,
    provider_playlist_id: Option<Uuid>,
    playlist_name: String,
    spotify_playlist_id: Option<String>,
    spotify_track_id: Option<String>,
    payload: Value,
    safety: Value,
}

#[derive(Clone, Debug)]
struct DesiredPlaylist {
    playlist_id: Uuid,
    concept_id: Uuid,
    stable_key: String,
    name: String,
    description: String,
    tracks: Vec<DesiredTrack>,
}

#[derive(Clone, Debug)]
struct DesiredTrack {
    canonical_id: Uuid,
    spotify_id: String,
    position: i32,
    restored: bool,
}

#[derive(Clone, Debug)]
struct CurrentPlaylist {
    playlist_id: Uuid,
    provider_playlist_id: Uuid,
    spotify_id: String,
    name: String,
    provider_snapshot_id: Option<String>,
    tracks: Vec<CurrentTrack>,
    verified_tracks: BTreeSet<Uuid>,
}

#[derive(Clone, Debug)]
struct CurrentTrack {
    canonical_id: Uuid,
    spotify_id: String,
    position: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Summary {
    creates: usize,
    renames: usize,
    #[serde(default)]
    reorders: usize,
    additions: usize,
    restorations: usize,
    #[serde(default)]
    artwork_uploads: usize,
    exclusions: usize,
    removals: usize,
    retirements: usize,
    #[serde(default)]
    external_cleanups: usize,
    deferred: usize,
}

/// Builds or reuses an immutable, read-only Spotify synchronization plan.
pub async fn create(
    database: &Database,
    account_label: &str,
    proposal_generation_id: Option<Uuid>,
) -> Result<PlanReport> {
    let account_id = account_id(database, account_label).await?;
    let proposal_id = approved_proposal(database, account_id, proposal_generation_id).await?;
    let snapshot_id = latest_snapshot(database, account_id).await?;
    validate_proposal(database, proposal_id).await?;

    let desired = desired_playlists(database, account_id, proposal_id).await?;
    let current = current_managed_playlists(database, account_id, snapshot_id).await?;
    let reevaluating = current_reevaluate_tracks(database, account_id, snapshot_id).await?;
    let mut operations = playlist_diff(&desired, &current, &reevaluating);
    operations.extend(canonical_retirement_operations(&desired, &current));
    operations.extend(artwork_operations(database, account_id, proposal_id, &current).await?);
    operations.extend(intake_surface_operations(database, account_id, snapshot_id).await?);
    operations
        .extend(routing_surface_operations(database, account_id, snapshot_id, proposal_id).await?);
    operations.extend(cleanup_operations(database, account_id, snapshot_id, proposal_id).await?);
    operations.extend(album_retirement_operations(database, account_id, snapshot_id).await?);
    operations.extend(external_cleanup_operations(database, account_id).await?);
    operations.sort_by(|left, right| {
        phase_rank(&left.phase)
            .cmp(&phase_rank(&right.phase))
            .then_with(|| left.playlist_name.cmp(&right.playlist_name))
            .then_with(|| {
                operation_rank(&left.operation_type).cmp(&operation_rank(&right.operation_type))
            })
            .then_with(|| operation_position(left).cmp(&operation_position(right)))
            .then_with(|| left.operation_key.cmp(&right.operation_key))
    });

    persist_operations(
        database,
        account_id,
        proposal_id,
        snapshot_id,
        operations,
        "full",
    )
    .await
}

/// Builds a one-cover immutable update plan for an approved playlist artifact.
pub async fn create_artwork_update(
    database: &Database,
    account_label: &str,
    playlist: &str,
) -> Result<PlanReport> {
    let selector = playlist.trim();
    if selector.is_empty() {
        return Err(ChordriftError::Configuration(
            "artwork playlist selector cannot be empty".to_owned(),
        ));
    }
    let account_id = account_id(database, account_label).await?;
    let proposal_id = approved_proposal(database, account_id, None).await?;
    let snapshot_id = latest_snapshot(database, account_id).await?;
    validate_proposal(database, proposal_id).await?;
    let current = current_managed_playlists(database, account_id, snapshot_id).await?;
    let mut operations = artwork_operations(database, account_id, proposal_id, &current)
        .await?
        .into_iter()
        .filter(|operation| {
            operation.playlist_name.eq_ignore_ascii_case(selector)
                || operation
                    .payload
                    .get("stable_key")
                    .and_then(Value::as_str)
                    .is_some_and(|key| key.eq_ignore_ascii_case(selector))
        })
        .collect::<Vec<_>>();
    if operations.is_empty() {
        return Err(ChordriftError::Configuration(format!(
            "no pending approved artwork update matches playlist or stable key {selector:?}"
        )));
    }
    if operations.len() != 1 {
        return Err(ChordriftError::Configuration(format!(
            "artwork selector {selector:?} is ambiguous"
        )));
    }
    let operation = &operations[0];
    if operation.spotify_playlist_id.is_none() {
        return Err(ChordriftError::Configuration(format!(
            "playlist {:?} has no current Spotify target",
            operation.playlist_name
        )));
    }
    operations[0].safety = json!({
        "destructive": false,
        "requires_approved_artwork": true,
        "focused_artwork_update": true
    });
    persist_operations(
        database,
        account_id,
        proposal_id,
        snapshot_id,
        operations,
        &format!("artwork:{selector}"),
    )
    .await
}

async fn persist_operations(
    database: &Database,
    account_id: Uuid,
    proposal_id: Uuid,
    snapshot_id: Uuid,
    operations: Vec<PlanOperationInput>,
    scope: &str,
) -> Result<PlanReport> {
    let input = json!({
        "planner_version": PLANNER_VERSION,
        "plan_origin": PLAN_ORIGIN.as_str(),
        "scope": scope,
        "provider": PROVIDER,
        "account_id": account_id,
        "source_snapshot_id": snapshot_id,
        "proposal_generation_id": proposal_id,
        "operations": operations,
    });
    let input_hash = hex_sha256(&serde_json::to_vec(&input)?);
    if let Some(report) = existing_plan(database, account_id, &input_hash).await? {
        return Ok(PlanReport {
            reused: true,
            ..report
        });
    }

    let summary = summarize(&operations);
    let mut transaction = database.pool().begin().await?;
    let plan_row = sqlx::query(
        "INSERT INTO sync_runs
         (provider, mode, status, desired_state_hash, summary, finished_at,
          provider_account_id, source_snapshot_id, proposal_generation_id,
          planner_version, input_hash, preconditions)
         VALUES ($1, 'dry_run', 'planned', $2, $3, now(), $4, $5, $6, $7, $2, $8)
         RETURNING id, started_at",
    )
    .bind(PROVIDER)
    .bind(&input_hash)
    .bind(serde_json::to_value(&summary)?)
    .bind(account_id)
    .bind(snapshot_id)
    .bind(proposal_id)
    .bind(PLANNER_VERSION)
    .bind(json!({
        "spotify_writes": false,
        "plan_origin": PLAN_ORIGIN.as_str(),
        "requires_current_snapshot": snapshot_id,
        "requires_approved_proposal": proposal_id,
        "retirement_requires_separate_approval": true
        ,"external_cleanup_requires_approved_batch": true
    }))
    .fetch_one(&mut *transaction)
    .await?;
    let plan_id: Uuid = plan_row.try_get("id")?;
    let created_at: DateTime<Utc> = plan_row.try_get("started_at")?;
    if !operations.is_empty() {
        let mut insert: QueryBuilder<Postgres> = QueryBuilder::new(
            "INSERT INTO sync_operations
             (sync_run_id, playlist_id, provider_playlist_id, operation_type,
              operation_key, payload, phase, sequence, safety) ",
        );
        insert.push_values(
            operations.iter().enumerate(),
            |mut row, (index, operation)| {
                let sequence = i32::try_from(index).expect("validated operation count fits i32");
                row.push_bind(plan_id)
                    .push_bind(operation.playlist_id)
                    .push_bind(operation.provider_playlist_id)
                    .push_bind(&operation.operation_type)
                    .push_bind(&operation.operation_key)
                    .push_bind(json!({
                        "playlist_name": operation.playlist_name,
                        "spotify_playlist_id": operation.spotify_playlist_id,
                        "spotify_track_id": operation.spotify_track_id,
                        "detail": operation.payload
                    }))
                    .push_bind(&operation.phase)
                    .push_bind(sequence)
                    .push_bind(&operation.safety);
            },
        );
        insert.build().execute(&mut *transaction).await?;
    }
    transaction.commit().await?;

    Ok(report(
        plan_id,
        PLAN_ORIGIN,
        Some(proposal_id),
        snapshot_id,
        false,
        input_hash,
        operations.len(),
        summary,
        created_at,
    ))
}

async fn artwork_operations(
    database: &Database,
    account_id: Uuid,
    proposal_id: Uuid,
    current: &BTreeMap<Uuid, CurrentPlaylist>,
) -> Result<Vec<PlanOperationInput>> {
    let rows = sqlx::query(
        "SELECT artifact.playlist_id, artifact.target_kind, playlist.concept_id,
                artifact.stable_key, artifact.playlist_name, artifact.artifact_path,
                artifact.content_sha256, artifact.batch_id,
                intake.provider_playlist_row_id AS intake_provider_playlist_row_id,
                intake.spotify_playlist_id AS intake_spotify_playlist_id
         FROM playlist_artwork_batches batch
         JOIN playlist_artwork_artifacts artifact ON artifact.batch_id = batch.id
         LEFT JOIN playlists playlist ON playlist.id = artifact.playlist_id
         LEFT JOIN LATERAL (
             SELECT provider.id AS provider_playlist_row_id,
                    provider.provider_playlist_id AS spotify_playlist_id
             FROM current_spotify_playlists current
             JOIN provider_playlists provider ON provider.id = current.provider_playlist_id
             WHERE current.provider_account_id = $1
               AND current.signal_class = 'intake'
               AND lower(current.name) = lower(artifact.playlist_name)
             ORDER BY provider.id LIMIT 1
         ) intake ON artifact.target_kind = 'intake'
         WHERE batch.id = (
             SELECT latest.id
             FROM playlist_artwork_batches latest
             WHERE latest.provider_account_id = $1
               AND latest.proposal_generation_id = $2
               AND latest.state = 'approved'
             ORDER BY latest.approved_at DESC, latest.created_at DESC, latest.id DESC
             LIMIT 1
         )
         ORDER BY lower(artifact.playlist_name), artifact.playlist_id",
    )
    .bind(account_id)
    .bind(proposal_id)
    .fetch_all(database.pool())
    .await?;
    let mut operations = Vec::new();
    for row in rows {
        let target_kind: String = row.try_get("target_kind")?;
        let concept_id: Option<Uuid> = row.try_get("concept_id")?;
        let observed = concept_id.and_then(|id| current.get(&id));
        let stable_key: String = row.try_get("stable_key")?;
        let spotify_playlist_id = if target_kind == "intake" {
            row.try_get("intake_spotify_playlist_id")?
        } else {
            observed.map(|value| value.spotify_id.clone())
        };
        let operation_key = format!(
            "artwork:{stable_key}:{}",
            row.try_get::<String, _>("content_sha256")?
        );
        if let Some(spotify_id) = &spotify_playlist_id
            && artwork_already_uploaded(database, account_id, &operation_key, spotify_id).await?
        {
            continue;
        }
        operations.push(PlanOperationInput {
            phase: "publish".to_owned(),
            operation_type: "upload_artwork".to_owned(),
            operation_key,
            playlist_id: row.try_get("playlist_id")?,
            provider_playlist_id: if target_kind == "intake" {
                row.try_get("intake_provider_playlist_row_id")?
            } else {
                observed.map(|value| value.provider_playlist_id)
            },
            playlist_name: row.try_get("playlist_name")?,
            spotify_playlist_id,
            spotify_track_id: None,
            payload: json!({
                "artifact_path": row.try_get::<String, _>("artifact_path")?,
                "content_sha256": row.try_get::<String, _>("content_sha256")?,
                "artwork_batch_id": row.try_get::<Uuid, _>("batch_id")?,
                "stable_key": stable_key,
                "target_kind": target_kind,
            }),
            safety: json!({"destructive": false, "requires_approved_artwork": true}),
        });
    }
    Ok(operations)
}

async fn artwork_already_uploaded(
    database: &Database,
    account_id: Uuid,
    operation_key: &str,
    spotify_playlist_id: &str,
) -> Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM sync_apply_operations execution
             JOIN sync_apply_runs apply ON apply.id = execution.apply_run_id
             JOIN sync_operations planned ON planned.id = execution.planned_operation_id
             WHERE apply.provider_account_id = $1
               AND execution.status = 'succeeded'
               AND planned.operation_key = $2
               AND execution.resolved_spotify_playlist_id = $3
         )",
    )
    .bind(account_id)
    .bind(operation_key)
    .bind(spotify_playlist_id)
    .fetch_one(database.pool())
    .await
    .map_err(Into::into)
}

async fn intake_surface_operations(
    database: &Database,
    account_id: Uuid,
    snapshot_id: Uuid,
) -> Result<Vec<PlanOperationInput>> {
    let current_names: Vec<String> = sqlx::query_scalar(
        "SELECT snapshot.name
         FROM provider_account_playlists account_playlist
         JOIN provider_observed_playlists snapshot
           ON snapshot.provider_playlist_id = account_playlist.provider_playlist_id
          AND snapshot.snapshot_id = $2
         WHERE account_playlist.provider_account_id = $1
           AND account_playlist.present_in_latest_snapshot",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .fetch_all(database.pool())
    .await?;
    let normalized: BTreeSet<String> = current_names
        .into_iter()
        .map(|name| name.trim().to_lowercase())
        .collect();
    Ok(INTAKE_SURFACES
        .iter()
        .filter(|(name, _, _)| !normalized.contains(&name.to_lowercase()))
        .map(
            |(name, description, behavioral_signal)| PlanOperationInput {
                phase: "publish".to_owned(),
                operation_type: "create_playlist".to_owned(),
                operation_key: format!("create-intake:{}", name.to_lowercase().replace(' ', "-")),
                playlist_id: None,
                provider_playlist_id: None,
                playlist_name: (*name).to_owned(),
                spotify_playlist_id: None,
                spotify_track_id: None,
                payload: json!({
                    "description": description,
                    "public": false,
                    "surface": "intake",
                    "role": "inbox",
                    "drift_policy": "provider_wins",
                    "signal_class": "intake",
                    "behavioral_signal": behavioral_signal,
                    "clear_policy": "after_verified_assignment"
                }),
                safety: json!({"destructive": false}),
            },
        )
        .collect())
}

async fn routing_surface_operations(
    database: &Database,
    account_id: Uuid,
    snapshot_id: Uuid,
    proposal_id: Uuid,
) -> Result<Vec<PlanOperationInput>> {
    let routes = sqlx::query(
        "SELECT route.playlist_id, route.stable_key, playlist.name,
                COALESCE(playlist.description, '') AS description,
                route.artwork_path, route.artwork_sha256, route.active, route.purpose,
                provider.id AS provider_playlist_row_id,
                provider.provider_playlist_id AS spotify_playlist_id,
                snapshot.name AS current_name,
                snapshot.provider_snapshot_id
         FROM routing_surfaces route
         JOIN playlists playlist ON playlist.id = route.playlist_id
         LEFT JOIN provider_playlists provider
           ON provider.playlist_id = route.playlist_id AND provider.provider = 'spotify'
         LEFT JOIN provider_observed_playlists snapshot
           ON snapshot.provider_playlist_id = provider.id AND snapshot.snapshot_id = $2
         WHERE route.provider_account_id = $1
         ORDER BY lower(playlist.name), route.playlist_id",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .fetch_all(database.pool())
    .await?;
    let mut operations = Vec::new();
    for route in routes {
        let playlist_id: Uuid = route.try_get("playlist_id")?;
        let stable_key: String = route.try_get("stable_key")?;
        let name: String = route.try_get("name")?;
        let description: String = route.try_get("description")?;
        let provider_playlist_id: Option<Uuid> = route.try_get("provider_playlist_row_id")?;
        let spotify_playlist_id: Option<String> = route.try_get("spotify_playlist_id")?;
        let active: bool = route.try_get("active")?;
        let purpose: String = route.try_get("purpose")?;
        let present = route
            .try_get::<Option<String>, _>("current_name")?
            .is_some();

        if !active {
            if present && purpose == "legacy_route" {
                operations.push(PlanOperationInput {
                    phase: "retirement".to_owned(),
                    operation_type: "archive_playlist".to_owned(),
                    operation_key: format!("retire:{stable_key}"),
                    playlist_id: Some(playlist_id),
                    provider_playlist_id,
                    playlist_name: name,
                    spotify_playlist_id,
                    spotify_track_id: None,
                    payload: json!({
                        "expected_snapshot_id": route.try_get::<Option<String>, _>("provider_snapshot_id")?,
                        "surface": "legacy_routing",
                        "replacement": "Re-evaluate",
                        "container_only": true,
                        "inventory_retained": true
                    }),
                    safety: json!({
                        "destructive": true,
                        "deferred": true,
                        "requires_snapshot_match": true,
                        "tracks_preserved_in_approved_proposal": true
                    }),
                });
            }
            continue;
        }

        if !present {
            operations.push(PlanOperationInput {
                phase: "publish".to_owned(),
                operation_type: "create_playlist".to_owned(),
                operation_key: format!("create:{stable_key}"),
                playlist_id: Some(playlist_id),
                provider_playlist_id: None,
                playlist_name: name.clone(),
                spotify_playlist_id: None,
                spotify_track_id: None,
                payload: json!({
                    "description": description,
                    "public": false,
                    "stable_key": stable_key,
                    "surface": "routing",
                    "role": "inbox",
                    "drift_policy": "provider_wins",
                    "signal_class": "routing",
                    "clear_policy": "after_verified_assignment"
                }),
                safety: json!({"destructive": false}),
            });
        } else if route
            .try_get::<Option<String>, _>("current_name")?
            .as_deref()
            != Some(&name)
        {
            operations.push(PlanOperationInput {
                phase: "publish".to_owned(),
                operation_type: "rename_playlist".to_owned(),
                operation_key: format!("rename:{stable_key}:{name}"),
                playlist_id: Some(playlist_id),
                provider_playlist_id,
                playlist_name: name.clone(),
                spotify_playlist_id: spotify_playlist_id.clone(),
                spotify_track_id: None,
                payload: json!({
                    "from": route.try_get::<Option<String>, _>("current_name")?,
                    "to": name,
                    "description": description,
                    "expected_snapshot_id": route.try_get::<Option<String>, _>("provider_snapshot_id")?
                }),
                safety: json!({"destructive": false, "requires_snapshot_match": true}),
            });
        }

        let desired_rows = sqlx::query(
            "SELECT membership.track_id, membership.position, provider.provider_track_id
             FROM playlist_tracks membership
             JOIN provider_tracks provider ON provider.track_id = membership.track_id
                  AND provider.provider = 'spotify'
             WHERE membership.playlist_id = $1
             ORDER BY membership.position",
        )
        .bind(playlist_id)
        .fetch_all(database.pool())
        .await?;
        let desired = desired_rows
            .into_iter()
            .map(|row| {
                Ok((
                    row.try_get::<Uuid, _>("track_id")?,
                    row.try_get::<i32, _>("position")?,
                    row.try_get::<String, _>("provider_track_id")?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let current: Vec<Uuid> = if let Some(provider_row_id) = provider_playlist_id {
            sqlx::query_scalar(
                "SELECT track.track_id
                 FROM provider_observed_playlist_tracks membership
                 JOIN provider_tracks track ON track.id = membership.provider_track_id
                 WHERE membership.snapshot_id = $1
                   AND membership.provider_playlist_id = $2
                 ORDER BY membership.position",
            )
            .bind(snapshot_id)
            .bind(provider_row_id)
            .fetch_all(database.pool())
            .await?
        } else {
            Vec::new()
        };
        let current_set = current.iter().copied().collect::<BTreeSet<_>>();
        let desired_ids = desired.iter().map(|row| row.0).collect::<Vec<_>>();
        let desired_set = desired_ids.iter().copied().collect::<BTreeSet<_>>();
        if present && current_set == desired_set && current != desired_ids {
            operations.push(PlanOperationInput {
                phase: "publish".to_owned(),
                operation_type: "reorder_playlist".to_owned(),
                operation_key: format!("reorder:{stable_key}"),
                playlist_id: Some(playlist_id),
                provider_playlist_id,
                playlist_name: name.clone(),
                spotify_playlist_id: spotify_playlist_id.clone(),
                spotify_track_id: None,
                payload: json!({
                    "track_count": desired.len(),
                    "surface": "routing",
                    "expected_snapshot_id": route.try_get::<Option<String>, _>("provider_snapshot_id")?
                }),
                safety: json!({
                    "destructive": false,
                    "membership_unchanged": true,
                    "exact_order_replacement": true
                }),
            });
        }
        for (track_id, position, spotify_track_id) in &desired {
            if !current_set.contains(track_id) {
                operations.push(PlanOperationInput {
                    phase: "publish".to_owned(),
                    operation_type: "add_track".to_owned(),
                    operation_key: format!("add:{stable_key}:{spotify_track_id}"),
                    playlist_id: Some(playlist_id),
                    provider_playlist_id,
                    playlist_name: name.clone(),
                    spotify_playlist_id: spotify_playlist_id.clone(),
                    spotify_track_id: Some(spotify_track_id.clone()),
                    payload: json!({
                        "position": position,
                        "surface": "routing",
                        "canonical_track_id": track_id,
                        "spotify_track_id": spotify_track_id
                    }),
                    safety: json!({"destructive": false, "preserves_track": true}),
                });
            }
        }

        if purpose == "reevaluate" && present {
            let resolved = sqlx::query(
                "SELECT desired.track_id, provider_track.provider_track_id
                 FROM playlist_tracks desired
                 JOIN provider_tracks provider_track
                   ON provider_track.track_id = desired.track_id
                  AND provider_track.provider = 'spotify'
                 JOIN LATERAL (
                     SELECT COALESCE(
                                NULLIF(event.metadata->>'residency_started_at', '')::timestamptz,
                                event.observed_at
                            ) AS observed_at,
                            NULLIF(event.metadata->>'previous_concept_id', '')::uuid
                                AS previous_concept_id
                     FROM reevaluation_events event
                     WHERE event.provider_account_id = $1
                       AND event.playlist_id = $2
                       AND event.track_id = desired.track_id
                       AND event.event_type = 'entered'
                       AND event.observed_at > COALESCE((
                           SELECT max(left_event.observed_at)
                           FROM reevaluation_events left_event
                           WHERE left_event.provider_account_id = event.provider_account_id
                             AND left_event.playlist_id = event.playlist_id
                             AND left_event.track_id = event.track_id
                             AND left_event.event_type = 'left'
                       ), '-infinity'::timestamptz)
                     ORDER BY event.observed_at, event.id LIMIT 1
                 ) entry ON TRUE
                 JOIN track_playlist_assignment_revisions revision
                   ON revision.provider_account_id = $1
                  AND revision.track_id = desired.track_id
                  AND revision.superseded_at IS NULL
                  AND revision.decision = 'assign'
                  AND revision.created_at > entry.observed_at
                 JOIN playlists proposed
                   ON proposed.generation_id = $3
                  AND proposed.concept_id = revision.destination_concept_id
                 JOIN playlist_tracks placed
                   ON placed.playlist_id = proposed.id
                  AND placed.track_id = desired.track_id
                 WHERE desired.playlist_id = $2
                   AND (entry.previous_concept_id IS NULL
                        OR revision.destination_concept_id <> entry.previous_concept_id)
                 ORDER BY desired.position",
            )
            .bind(account_id)
            .bind(playlist_id)
            .bind(proposal_id)
            .fetch_all(database.pool())
            .await?;
            for row in resolved {
                let track_id: Uuid = row.try_get("track_id")?;
                let spotify_track_id: String = row.try_get("provider_track_id")?;
                operations.push(PlanOperationInput {
                    phase: "cleanup".to_owned(),
                    operation_type: "remove_track".to_owned(),
                    operation_key: format!("resolve:{stable_key}:{spotify_track_id}"),
                    playlist_id: Some(playlist_id),
                    provider_playlist_id,
                    playlist_name: name.clone(),
                    spotify_playlist_id: spotify_playlist_id.clone(),
                    spotify_track_id: Some(spotify_track_id.clone()),
                    payload: json!({
                        "canonical_track_id": track_id,
                        "spotify_track_id": spotify_track_id,
                        "reason": "verified_reevaluation_assignment",
                        "expected_snapshot_id": route.try_get::<Option<String>, _>("provider_snapshot_id")?
                    }),
                    safety: json!({
                        "destructive": true,
                        "deferred": true,
                        "requires_snapshot_match": true,
                        "requires_new_destination_in_approved_proposal": true,
                        "removes_holding_queue_membership_only": true
                    }),
                });
            }
        }

        let artwork_sha256: String = route.try_get("artwork_sha256")?;
        let artwork_key = format!("artwork:{stable_key}:{artwork_sha256}");
        let already_uploaded = match &spotify_playlist_id {
            Some(spotify_id) => {
                artwork_already_uploaded(database, account_id, &artwork_key, spotify_id).await?
            }
            None => false,
        };
        if !already_uploaded {
            operations.push(PlanOperationInput {
                phase: "publish".to_owned(),
                operation_type: "upload_artwork".to_owned(),
                operation_key: artwork_key,
                playlist_id: Some(playlist_id),
                provider_playlist_id,
                playlist_name: name,
                spotify_playlist_id,
                spotify_track_id: None,
                payload: json!({
                    "artifact_path": route.try_get::<String, _>("artwork_path")?,
                    "content_sha256": artwork_sha256,
                    "stable_key": stable_key,
                    "target_kind": "routing"
                }),
                safety: json!({"destructive": false, "requires_approved_artwork": true}),
            });
        }
    }
    Ok(operations)
}

/// Returns a persisted plan and all of its exact operations.
pub async fn show(
    database: &Database,
    account_label: &str,
    plan_id: Option<Uuid>,
) -> Result<(PlanReport, bool, Vec<PlannedOperation>)> {
    let account_id = account_id(database, account_label).await?;
    let row = sqlx::query(
        "SELECT id, proposal_generation_id, source_snapshot_id, input_hash, summary,
                planner_version, preconditions, started_at,
                source_snapshot_id = (SELECT id FROM provider_inventory_observations
                    WHERE provider_account_id = $1 ORDER BY captured_at DESC, id DESC LIMIT 1)
                    AS snapshot_current
         FROM sync_runs
         WHERE provider_account_id = $1 AND mode = 'dry_run'
           AND ($2::uuid IS NULL OR id = $2)
         ORDER BY started_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .bind(plan_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| ChordriftError::Configuration("no dry-run sync plan exists".to_owned()))?;
    let selected_plan_id: Uuid = row.try_get("id")?;
    let origin = stored_plan_origin(
        &row.try_get::<String, _>("planner_version")?,
        &row.try_get::<Value, _>("preconditions")?,
    )?;
    let summary: Summary = serde_json::from_value(row.try_get("summary")?)?;
    let count: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM sync_operations WHERE sync_run_id = $1")
            .bind(selected_plan_id)
            .fetch_one(database.pool())
            .await?;
    let report = report(
        selected_plan_id,
        origin,
        row.try_get("proposal_generation_id")?,
        row.try_get("source_snapshot_id")?,
        true,
        row.try_get("input_hash")?,
        usize::try_from(count).map_err(|_| {
            ChordriftError::Configuration("invalid sync operation count".to_owned())
        })?,
        summary,
        row.try_get("started_at")?,
    );
    let rows = sqlx::query(
        "SELECT operation.sequence, operation.phase, operation.operation_type,
                operation.operation_key, operation.payload, operation.safety,
                provider.provider_playlist_id AS fallback_spotify_playlist_id
         FROM sync_operations operation
         LEFT JOIN provider_playlists provider ON provider.id = operation.provider_playlist_id
         WHERE operation.sync_run_id = $1 ORDER BY operation.sequence",
    )
    .bind(selected_plan_id)
    .fetch_all(database.pool())
    .await?;
    let operations = rows
        .into_iter()
        .map(|row| {
            let payload: Value = row.try_get("payload")?;
            Ok(PlannedOperation {
                sequence: row.try_get("sequence")?,
                phase: row.try_get("phase")?,
                operation_type: row.try_get("operation_type")?,
                operation_key: row.try_get("operation_key")?,
                playlist_name: json_string(&payload, "playlist_name"),
                spotify_playlist_id: payload
                    .get("spotify_playlist_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or(row.try_get("fallback_spotify_playlist_id")?),
                spotify_track_id: payload
                    .get("spotify_track_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                payload: payload.get("detail").cloned().unwrap_or_else(|| json!({})),
                safety: row.try_get("safety")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((report, row.try_get("snapshot_current")?, operations))
}

fn playlist_diff(
    desired: &[DesiredPlaylist],
    current: &BTreeMap<Uuid, CurrentPlaylist>,
    reevaluating: &BTreeSet<Uuid>,
) -> Vec<PlanOperationInput> {
    let mut operations = Vec::new();
    for playlist in desired {
        let observed = current.get(&playlist.concept_id);
        if observed.is_none() {
            operations.push(PlanOperationInput {
                phase: "publish".to_owned(),
                operation_type: "create_playlist".to_owned(),
                operation_key: format!("create:{}", playlist.stable_key),
                playlist_id: Some(playlist.playlist_id),
                provider_playlist_id: None,
                playlist_name: playlist.name.clone(),
                spotify_playlist_id: None,
                spotify_track_id: None,
                payload: json!({"description": playlist.description, "public": false,
                    "stable_key": playlist.stable_key, "concept_id": playlist.concept_id}),
                safety: json!({"destructive": false}),
            });
        }
        if let Some(observed) = observed
            && observed.name != playlist.name
        {
            operations.push(PlanOperationInput {
                phase: "publish".to_owned(),
                operation_type: "rename_playlist".to_owned(),
                operation_key: format!("rename:{}:{}", playlist.stable_key, playlist.name),
                playlist_id: Some(playlist.playlist_id),
                provider_playlist_id: Some(observed.provider_playlist_id),
                playlist_name: playlist.name.clone(),
                spotify_playlist_id: Some(observed.spotify_id.clone()),
                spotify_track_id: None,
                payload: json!({"from": observed.name, "to": playlist.name,
                    "expected_snapshot_id": observed.provider_snapshot_id}),
                safety: json!({"destructive": false, "requires_snapshot_match": true}),
            });
        }
        let current_tracks: BTreeSet<Uuid> = observed
            .map(|value| {
                value
                    .tracks
                    .iter()
                    .map(|track| track.canonical_id)
                    .collect()
            })
            .unwrap_or_default();
        let desired_tracks: BTreeSet<Uuid> = playlist
            .tracks
            .iter()
            .map(|track| track.canonical_id)
            .collect();
        if let Some(observed) = observed {
            let current_order = observed
                .tracks
                .iter()
                .map(|track| track.canonical_id)
                .collect::<Vec<_>>();
            let desired_order = playlist
                .tracks
                .iter()
                .map(|track| track.canonical_id)
                .collect::<Vec<_>>();
            if current_tracks == desired_tracks && current_order != desired_order {
                operations.push(PlanOperationInput {
                    phase: "publish".to_owned(),
                    operation_type: "reorder_playlist".to_owned(),
                    operation_key: format!("reorder:{}", playlist.stable_key),
                    playlist_id: Some(playlist.playlist_id),
                    provider_playlist_id: Some(observed.provider_playlist_id),
                    playlist_name: playlist.name.clone(),
                    spotify_playlist_id: Some(observed.spotify_id.clone()),
                    spotify_track_id: None,
                    payload: json!({"concept_id": playlist.concept_id,
                        "track_count": desired_order.len(),
                        "expected_snapshot_id": observed.provider_snapshot_id}),
                    safety: json!({"destructive": false,
                        "membership_unchanged": true,
                        "exact_order_replacement": true}),
                });
            }
        }
        for track in &playlist.tracks {
            if !current_tracks.contains(&track.canonical_id) {
                let removed_from_verified_destination = observed
                    .is_some_and(|value| value.verified_tracks.contains(&track.canonical_id));
                if reevaluating.contains(&track.canonical_id) && removed_from_verified_destination {
                    continue;
                }
                if let Some(observed) = observed
                    && removed_from_verified_destination
                    && !track.restored
                {
                    operations.push(PlanOperationInput {
                        phase: "reconcile".to_owned(),
                        operation_type: "exclude_track".to_owned(),
                        operation_key: format!(
                            "exclude:{}:{}",
                            playlist.stable_key, track.spotify_id
                        ),
                        playlist_id: Some(playlist.playlist_id),
                        provider_playlist_id: Some(observed.provider_playlist_id),
                        playlist_name: playlist.name.clone(),
                        spotify_playlist_id: Some(observed.spotify_id.clone()),
                        spotify_track_id: Some(track.spotify_id.clone()),
                        payload: json!({"previous_concept_id": playlist.concept_id,
                            "reason": "removed_from_verified_managed_playlist"}),
                        safety: json!({"destructive": false, "neon_only": true,
                            "requires_verified_managed_baseline": true}),
                    });
                    continue;
                }
                let kind = if track.restored {
                    "restore_track"
                } else {
                    "add_track"
                };
                operations.push(PlanOperationInput {
                    phase: "publish".to_owned(),
                    operation_type: kind.to_owned(),
                    operation_key: format!("{kind}:{}:{}", playlist.stable_key, track.spotify_id),
                    playlist_id: Some(playlist.playlist_id),
                    provider_playlist_id: observed.map(|value| value.provider_playlist_id),
                    playlist_name: playlist.name.clone(),
                    spotify_playlist_id: observed.map(|value| value.spotify_id.clone()),
                    spotify_track_id: Some(track.spotify_id.clone()),
                    payload: json!({"position": track.position, "concept_id": playlist.concept_id,
                        "reason": if track.restored { "excluded_track_restoration" } else { "approved_assignment" }}),
                    safety: json!({"destructive": false}),
                });
            }
        }
        if let Some(observed) = observed {
            for track in &observed.tracks {
                if !desired_tracks.contains(&track.canonical_id) {
                    operations.push(PlanOperationInput {
                        phase: "reconcile".to_owned(),
                        operation_type: "remove_track".to_owned(),
                        operation_key: format!(
                            "remove-drift:{}:{}:{}",
                            playlist.stable_key, track.spotify_id, track.position
                        ),
                        playlist_id: Some(playlist.playlist_id),
                        provider_playlist_id: Some(observed.provider_playlist_id),
                        playlist_name: playlist.name.clone(),
                        spotify_playlist_id: Some(observed.spotify_id.clone()),
                        spotify_track_id: Some(track.spotify_id.clone()),
                        payload: json!({"position": track.position, "reason": "managed_provider_drift",
                            "expected_snapshot_id": observed.provider_snapshot_id}),
                        safety: json!({"destructive": true, "creates_exclusion": false,
                            "requires_snapshot_match": true}),
                    });
                }
            }
        }
    }
    operations
}

fn canonical_retirement_operations(
    desired: &[DesiredPlaylist],
    current: &BTreeMap<Uuid, CurrentPlaylist>,
) -> Vec<PlanOperationInput> {
    let retained = desired
        .iter()
        .map(|playlist| playlist.concept_id)
        .collect::<BTreeSet<_>>();
    current
        .iter()
        .filter(|(concept_id, _)| !retained.contains(concept_id))
        .map(|(concept_id, playlist)| PlanOperationInput {
            phase: "retirement".to_owned(),
            operation_type: "archive_playlist".to_owned(),
            operation_key: format!("retire-canonical:{}", playlist.spotify_id),
            playlist_id: Some(playlist.playlist_id),
            provider_playlist_id: Some(playlist.provider_playlist_id),
            playlist_name: playlist.name.clone(),
            spotify_playlist_id: Some(playlist.spotify_id.clone()),
            spotify_track_id: None,
            payload: json!({
                "concept_id": concept_id,
                "expected_snapshot_id": playlist.provider_snapshot_id,
                "container_only": true,
                "inventory_retained": true,
                "reason": "concept_absent_from_complete_approved_proposal"
            }),
            safety: json!({
                "destructive": true,
                "deferred": true,
                "requires_separate_approval": true,
                "requires_snapshot_match": true,
                "requires_complete_approved_proposal": true,
                "immutable_inventory_retained": true
            }),
        })
        .collect()
}

async fn current_reevaluate_tracks(
    database: &Database,
    account_id: Uuid,
    snapshot_id: Uuid,
) -> Result<BTreeSet<Uuid>> {
    let rows = sqlx::query_scalar(
        "SELECT DISTINCT provider_track.track_id
         FROM routing_surfaces route
         JOIN provider_playlists provider
           ON provider.playlist_id = route.playlist_id
          AND provider.provider = 'spotify'
         JOIN provider_observed_playlist_tracks membership
           ON membership.provider_playlist_id = provider.id
          AND membership.snapshot_id = $2
         JOIN provider_tracks provider_track
           ON provider_track.id = membership.provider_track_id
         WHERE route.provider_account_id = $1
           AND route.active AND route.purpose = 'reevaluate'",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .fetch_all(database.pool())
    .await?;
    Ok(rows.into_iter().collect())
}

async fn cleanup_operations(
    database: &Database,
    account_id: Uuid,
    snapshot_id: Uuid,
    proposal_id: Uuid,
) -> Result<Vec<PlanOperationInput>> {
    let rows = sqlx::query(
        "WITH proposed AS (
             SELECT DISTINCT membership.track_id
             FROM playlists playlist
             JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
             WHERE playlist.generation_id = $3)
         SELECT account_playlist.signal_class, account_playlist.clear_policy,
                provider.id AS provider_playlist_row_id,
                provider.provider_playlist_id, snapshot.name,
                snapshot.provider_snapshot_id,
                membership.position, track.track_id, track.provider_track_id,
                (proposed.track_id IS NOT NULL) AS assigned,
                (exclusion.id IS NOT NULL) AS excluded
         FROM provider_account_playlists account_playlist
         JOIN provider_playlists provider ON provider.id = account_playlist.provider_playlist_id
         JOIN provider_observed_playlists snapshot
           ON snapshot.provider_playlist_id = provider.id AND snapshot.snapshot_id = $2
         LEFT JOIN provider_observed_playlist_tracks membership
           ON membership.provider_playlist_id = provider.id AND membership.snapshot_id = $2
         LEFT JOIN provider_tracks track ON track.id = membership.provider_track_id
         LEFT JOIN proposed ON proposed.track_id = track.track_id
         LEFT JOIN excluded_tracks exclusion
           ON exclusion.provider_account_id = $1 AND exclusion.track_id = track.track_id
          AND exclusion.restored_at IS NULL
         WHERE account_playlist.provider_account_id = $1
           AND account_playlist.present_in_latest_snapshot
           AND account_playlist.signal_class IN
               ('semantic_legacy', 'intake', 'ignored', 'transport')
         ORDER BY lower(snapshot.name), provider.provider_playlist_id, membership.position",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .bind(proposal_id)
    .fetch_all(database.pool())
    .await?;
    type Source = (
        Uuid,
        String,
        String,
        Option<String>,
        String,
        String,
        Vec<(i32, Uuid, String, bool, bool)>,
    );
    let mut sources: BTreeMap<Uuid, Source> = BTreeMap::new();
    for row in rows {
        let id: Uuid = row.try_get("provider_playlist_row_id")?;
        let source = sources.entry(id).or_insert_with(|| {
            (
                id,
                row.try_get("provider_playlist_id")
                    .expect("selected Spotify ID"),
                row.try_get("name").expect("selected playlist name"),
                row.try_get("provider_snapshot_id")
                    .expect("selected snapshot signature"),
                row.try_get("signal_class").expect("selected signal class"),
                row.try_get("clear_policy").expect("selected clear policy"),
                Vec::new(),
            )
        });
        let position: Option<i32> = row.try_get("position")?;
        if let Some(position) = position {
            source.6.push((
                position,
                row.try_get("track_id")?,
                row.try_get("provider_track_id")?,
                row.try_get("assigned")?,
                row.try_get("excluded")?,
            ));
        }
    }
    let mut operations = Vec::new();
    for (provider_row_id, spotify_id, name, signature, class, clear_policy, tracks) in
        sources.into_values()
    {
        let all_resolved = tracks.iter().all(|track| track.3 || track.4);
        let assigned_tracks = tracks.iter().filter(|track| track.3).count();
        let excluded_tracks = tracks.iter().filter(|track| track.4).count();
        if class != "intake" {
            operations.push(PlanOperationInput {
                phase: "retirement".to_owned(),
                operation_type: "archive_playlist".to_owned(),
                operation_key: format!("retire:{spotify_id}"),
                playlist_id: None,
                provider_playlist_id: Some(provider_row_id),
                playlist_name: name,
                spotify_playlist_id: Some(spotify_id),
                spotify_track_id: None,
                payload: json!({"track_count": tracks.len(), "assigned_tracks": assigned_tracks,
                    "excluded_tracks": excluded_tracks, "source_class": class,
                    "expected_snapshot_id": signature, "container_only": true}),
                safety: json!({"destructive": true, "deferred": true,
                    "requires_separate_approval": true,
                    "requires_published_and_verified_destinations": assigned_tracks > 0,
                    "requires_durable_exclusions": excluded_tracks > 0,
                    "track_disposition_complete": all_resolved}),
            });
        } else {
            if clear_policy == "after_verified_assignment" {
                for (position, _, track_id, assigned, excluded) in tracks {
                    if assigned || excluded {
                        operations.push(PlanOperationInput {
                            phase: "cleanup".to_owned(),
                            operation_type: "remove_track".to_owned(),
                            operation_key: format!("consume:{spotify_id}:{track_id}:{position}"),
                            playlist_id: None,
                            provider_playlist_id: Some(provider_row_id),
                            playlist_name: name.clone(),
                            spotify_playlist_id: Some(spotify_id.clone()),
                            spotify_track_id: Some(track_id),
                            payload: json!({"position": position, "reason": "consumed_intake",
                                "expected_snapshot_id": signature}),
                            safety: json!({"destructive": true, "deferred": true,
                                "creates_exclusion": false, "resolved_by_exclusion": excluded,
                                "requires_published_and_verified_destination": assigned,
                                "requires_durable_exclusion": excluded,
                                "requires_snapshot_match": true}),
                        });
                    }
                }
            }
        }
    }
    let saved_track_policy: String = sqlx::query_scalar(
        "SELECT COALESCE((SELECT saved_track_clear_policy
                          FROM provider_account_library_policies
                          WHERE provider_account_id = $1), 'preserve')",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    if saved_track_policy == "after_verified_assignment" {
        let saved = sqlx::query(
            "WITH proposed AS (
                 SELECT DISTINCT membership.track_id
                 FROM playlists playlist
                 JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
                 WHERE playlist.generation_id = $3
             )
             SELECT saved.position, provider.provider_track_id,
                    proposed.track_id IS NOT NULL AS assigned,
                    exclusion.id IS NOT NULL AS excluded
             FROM provider_observed_saved_tracks saved
             JOIN provider_tracks provider ON provider.id = saved.provider_track_id
             LEFT JOIN proposed ON proposed.track_id = provider.track_id
             LEFT JOIN excluded_tracks exclusion
               ON exclusion.provider_account_id = $1
              AND exclusion.track_id = provider.track_id
              AND exclusion.restored_at IS NULL
             WHERE saved.snapshot_id = $2
             ORDER BY saved.position",
        )
        .bind(account_id)
        .bind(snapshot_id)
        .bind(proposal_id)
        .fetch_all(database.pool())
        .await?;
        for row in saved {
            let assigned: bool = row.try_get("assigned")?;
            let excluded: bool = row.try_get("excluded")?;
            if !assigned && !excluded {
                continue;
            }
            let spotify_track_id: String = row.try_get("provider_track_id")?;
            let position: i32 = row.try_get("position")?;
            operations.push(PlanOperationInput {
                phase: "cleanup".to_owned(),
                operation_type: "remove_saved_track".to_owned(),
                operation_key: format!("consume-liked:{spotify_track_id}"),
                playlist_id: None,
                provider_playlist_id: None,
                playlist_name: "Liked Songs".to_owned(),
                spotify_playlist_id: None,
                spotify_track_id: Some(spotify_track_id),
                payload: json!({"position": position, "reason": "consumed_saved_track",
                    "source_snapshot_id": snapshot_id}),
                safety: json!({"destructive": true, "deferred": true,
                    "creates_exclusion": false, "resolved_by_exclusion": excluded,
                    "requires_published_and_verified_destination": assigned,
                    "requires_durable_exclusion": excluded}),
            });
        }
    }
    Ok(operations)
}

async fn external_cleanup_operations(
    database: &Database,
    account_id: Uuid,
) -> Result<Vec<PlanOperationInput>> {
    let batch_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT batch.id
         FROM external_playlist_cleanup_batches batch
         WHERE batch.provider_account_id = $1 AND batch.state = 'approved'
           AND NOT EXISTS (
             SELECT 1
             FROM external_playlist_cleanup_items item
             LEFT JOIN external_playlist_bookmarks bookmark
               ON bookmark.id = item.bookmark_id
              AND bookmark.provider_account_id = $1
              AND bookmark.present_in_provider_library
             WHERE item.batch_id = batch.id
               AND (bookmark.id IS NULL
                 OR bookmark.provider_playlist_id <> item.provider_playlist_id
                 OR bookmark.provider_snapshot_id IS DISTINCT FROM item.provider_snapshot_id
                 OR bookmark.name <> item.name
                 OR bookmark.owner_provider_id <> item.owner_provider_id
                 OR bookmark.content_status <> item.content_status
                 OR bookmark.item_count <> item.item_count))
           AND NOT EXISTS (
             SELECT 1
             FROM external_playlist_bookmarks bookmark
             LEFT JOIN external_playlist_cleanup_items item
               ON item.batch_id = batch.id AND item.bookmark_id = bookmark.id
             WHERE bookmark.provider_account_id = $1
               AND bookmark.provider = 'spotify'
               AND bookmark.present_in_provider_library
               AND item.bookmark_id IS NULL)
         ORDER BY batch.approved_at DESC, batch.id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?;
    let Some(batch_id) = batch_id else {
        return Ok(Vec::new());
    };
    let rows = sqlx::query(
        "SELECT item.provider_playlist_id, item.provider_snapshot_id,
                item.name, item.owner_provider_id, item.content_status,
                item.item_count
         FROM external_playlist_cleanup_items item
         WHERE item.batch_id = $1
         ORDER BY lower(item.name), item.provider_playlist_id",
    )
    .bind(batch_id)
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            let spotify_id: String = row.try_get("provider_playlist_id")?;
            let name: String = row.try_get("name")?;
            Ok(PlanOperationInput {
                phase: "cleanup".to_owned(),
                operation_type: "remove_external_playlist".to_owned(),
                operation_key: format!("remove-external:{spotify_id}"),
                playlist_id: None,
                provider_playlist_id: None,
                playlist_name: name,
                spotify_playlist_id: Some(spotify_id),
                spotify_track_id: None,
                payload: json!({
                    "cleanup_batch_id": batch_id,
                    "owner_provider_id": row.try_get::<String, _>("owner_provider_id")?,
                    "content_status": row.try_get::<String, _>("content_status")?,
                    "item_count": row.try_get::<i32, _>("item_count")?,
                    "expected_snapshot_id": row.try_get::<Option<String>, _>("provider_snapshot_id")?,
                    "relationship_only": true
                }),
                safety: json!({
                    "destructive": true,
                    "deferred": true,
                    "requires_separate_approval": true,
                    "requires_cleanup_batch": batch_id,
                    "requires_bookmark_preserved": true,
                    "requires_snapshot_match": true,
                    "removes_library_relationship_only": true,
                    "source_owner_playlist_unchanged": true
                }),
            })
        })
        .collect()
}

async fn album_retirement_operations(
    database: &Database,
    account_id: Uuid,
    snapshot_id: Uuid,
) -> Result<Vec<PlanOperationInput>> {
    let policy: String = sqlx::query_scalar(
        "SELECT COALESCE((SELECT saved_album_policy
                          FROM provider_account_library_policies
                          WHERE provider_account_id = $1), 'preserve')",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    if policy != "archive_only" {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        "SELECT provider.id AS provider_album_row_id, provider.provider_album_id,
                album.title, saved.position, saved.saved_at,
                count(membership.*)::bigint AS track_count
         FROM provider_observed_saved_albums saved
         JOIN provider_albums provider ON provider.id = saved.provider_album_id
         JOIN albums album ON album.id = provider.album_id
         LEFT JOIN provider_observed_saved_album_tracks membership
           ON membership.snapshot_id = saved.snapshot_id
          AND membership.provider_album_id = saved.provider_album_id
         WHERE saved.snapshot_id = $1
         GROUP BY provider.id, provider.provider_album_id, album.title,
                  saved.position, saved.saved_at
         ORDER BY saved.position",
    )
    .bind(snapshot_id)
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            let spotify_id: String = row.try_get("provider_album_id")?;
            let title: String = row.try_get("title")?;
            Ok(PlanOperationInput {
                phase: "retirement".to_owned(),
                operation_type: "remove_saved_album".to_owned(),
                operation_key: format!("retire-album:{spotify_id}"),
                playlist_id: None,
                provider_playlist_id: None,
                playlist_name: title,
                spotify_playlist_id: None,
                spotify_track_id: None,
                payload: json!({
                    "spotify_album_id": spotify_id,
                    "provider_album_row_id": row.try_get::<Uuid, _>("provider_album_row_id")?,
                    "position": row.try_get::<i32, _>("position")?,
                    "saved_at": row.try_get::<Option<DateTime<Utc>>, _>("saved_at")?,
                    "track_count": row.try_get::<i64, _>("track_count")?,
                    "source_snapshot_id": snapshot_id,
                    "container_only": true,
                    "inventory_retained": true
                }),
                safety: json!({
                    "destructive": true,
                    "deferred": true,
                    "requires_separate_approval": true,
                    "requires_snapshot_match": true,
                    "album_tracks_unchanged": true,
                    "immutable_inventory_retained": true
                }),
            })
        })
        .collect()
}

async fn desired_playlists(
    database: &Database,
    account_id: Uuid,
    proposal_id: Uuid,
) -> Result<Vec<DesiredPlaylist>> {
    let rows = sqlx::query(
        "SELECT playlist.id AS playlist_id, playlist.concept_id, concept.stable_key,
                revision.name, revision.description, membership.position,
                membership.track_id,
                min(provider_track.provider_track_id) AS spotify_track_id,
                bool_or(exclusion.restored_at IS NOT NULL) AS restored
         FROM playlists playlist
         JOIN playlist_concepts concept ON concept.id = playlist.concept_id
         JOIN playlist_name_revisions revision
           ON revision.playlist_id = playlist.id AND revision.selected
         JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
         JOIN provider_tracks provider_track
           ON provider_track.track_id = membership.track_id AND provider_track.provider = 'spotify'
         LEFT JOIN excluded_tracks active
           ON active.provider_account_id = $1 AND active.track_id = membership.track_id
          AND active.restored_at IS NULL
         LEFT JOIN excluded_tracks exclusion
           ON exclusion.provider_account_id = $1 AND exclusion.track_id = membership.track_id
          AND exclusion.restored_at IS NOT NULL
         WHERE playlist.generation_id = $2 AND active.id IS NULL
         GROUP BY playlist.id, playlist.concept_id, concept.stable_key,
                  revision.name, revision.description, membership.position,
                  membership.track_id
         ORDER BY lower(revision.name), playlist.id, membership.position",
    )
    .bind(account_id)
    .bind(proposal_id)
    .fetch_all(database.pool())
    .await?;
    let mut playlists: BTreeMap<Uuid, DesiredPlaylist> = BTreeMap::new();
    for row in rows {
        let id: Uuid = row.try_get("playlist_id")?;
        let playlist = playlists.entry(id).or_insert_with(|| DesiredPlaylist {
            playlist_id: id,
            concept_id: row.try_get("concept_id").expect("selected concept ID"),
            stable_key: row.try_get("stable_key").expect("selected stable key"),
            name: row.try_get("name").expect("selected name"),
            description: row.try_get("description").expect("selected description"),
            tracks: Vec::new(),
        });
        playlist.tracks.push(DesiredTrack {
            canonical_id: row.try_get("track_id")?,
            spotify_id: row.try_get("spotify_track_id")?,
            position: row.try_get("position")?,
            restored: row.try_get("restored")?,
        });
    }
    Ok(playlists.into_values().collect())
}

async fn current_managed_playlists(
    database: &Database,
    account_id: Uuid,
    snapshot_id: Uuid,
) -> Result<BTreeMap<Uuid, CurrentPlaylist>> {
    let rows = sqlx::query(
        "SELECT provider.id AS provider_playlist_row_id, provider.playlist_id,
                provider.concept_id,
                provider.provider_playlist_id, snapshot.name, snapshot.provider_snapshot_id,
                membership.position, track.track_id, track.provider_track_id
         FROM provider_account_playlists account_playlist
         JOIN provider_playlists provider ON provider.id = account_playlist.provider_playlist_id
         JOIN provider_observed_playlists snapshot
           ON snapshot.provider_playlist_id = provider.id AND snapshot.snapshot_id = $2
         LEFT JOIN provider_observed_playlist_tracks membership
           ON membership.provider_playlist_id = provider.id AND membership.snapshot_id = $2
         LEFT JOIN provider_tracks track ON track.id = membership.provider_track_id
         WHERE account_playlist.provider_account_id = $1
           AND account_playlist.present_in_latest_snapshot
           AND account_playlist.role = 'managed'
           AND account_playlist.drift_policy = 'neon_wins'
           AND provider.concept_id IS NOT NULL
         ORDER BY provider.concept_id, membership.position",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .fetch_all(database.pool())
    .await?;
    let mut playlists = BTreeMap::new();
    for row in rows {
        let concept_id: Uuid = row.try_get("concept_id")?;
        let playlist = playlists
            .entry(concept_id)
            .or_insert_with(|| CurrentPlaylist {
                playlist_id: row
                    .try_get("playlist_id")
                    .expect("selected playlist row ID"),
                provider_playlist_id: row
                    .try_get("provider_playlist_row_id")
                    .expect("selected provider playlist row ID"),
                spotify_id: row
                    .try_get("provider_playlist_id")
                    .expect("selected Spotify ID"),
                name: row.try_get("name").expect("selected name"),
                provider_snapshot_id: row
                    .try_get("provider_snapshot_id")
                    .expect("selected signature"),
                tracks: Vec::new(),
                verified_tracks: BTreeSet::new(),
            });
        let position: Option<i32> = row.try_get("position")?;
        if let Some(position) = position {
            playlist.tracks.push(CurrentTrack {
                canonical_id: row.try_get("track_id")?,
                spotify_id: row.try_get("provider_track_id")?,
                position,
            });
        }
    }
    let verified_rows = sqlx::query(
        "SELECT verification.provider_playlist_id, membership.track_id
         FROM managed_playlist_verifications verification
         JOIN managed_playlist_verified_tracks membership
           ON membership.verification_id = verification.id
         WHERE verification.provider_account_id = $1
           AND verification.id = (
               SELECT latest.id FROM managed_playlist_verifications latest
               WHERE latest.provider_account_id = verification.provider_account_id
                 AND latest.provider_playlist_id = verification.provider_playlist_id
               ORDER BY latest.verified_at DESC, latest.id DESC LIMIT 1)",
    )
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    for row in verified_rows {
        let provider_playlist_id: Uuid = row.try_get("provider_playlist_id")?;
        if let Some(playlist) = playlists
            .values_mut()
            .find(|playlist| playlist.provider_playlist_id == provider_playlist_id)
        {
            playlist.verified_tracks.insert(row.try_get("track_id")?);
        }
    }
    Ok(playlists)
}

async fn validate_proposal(database: &Database, proposal_id: Uuid) -> Result<()> {
    let row = sqlx::query(
        "SELECT status, coverage_complete, required_track_count, represented_track_count,
                count(DISTINCT playlist.id)::bigint AS playlists,
                count(DISTINCT revision.playlist_id)::bigint AS named
         FROM playlist_generations generation
         LEFT JOIN playlists playlist ON playlist.generation_id = generation.id
         LEFT JOIN playlist_name_revisions revision
           ON revision.playlist_id = playlist.id AND revision.selected
         WHERE generation.id = $1 GROUP BY generation.id",
    )
    .bind(proposal_id)
    .fetch_one(database.pool())
    .await?;
    let valid = row.try_get::<String, _>("status")? == "approved"
        && row.try_get::<bool, _>("coverage_complete")?
        && row.try_get::<i32, _>("required_track_count")?
            == row.try_get::<i32, _>("represented_track_count")?
        && row.try_get::<i64, _>("playlists")? == row.try_get::<i64, _>("named")?;
    if !valid {
        return Err(ChordriftError::Configuration(
            "sync planning requires a fully named, coverage-complete approved proposal".to_owned(),
        ));
    }
    let duplicate_assignments: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM (
             SELECT membership.track_id
             FROM playlists playlist
             JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
             WHERE playlist.generation_id = $1
             GROUP BY membership.track_id HAVING count(*) > 1
         ) duplicate",
    )
    .bind(proposal_id)
    .fetch_one(database.pool())
    .await?;
    if duplicate_assignments != 0 {
        return Err(ChordriftError::Configuration(
            "approved proposal assigns tracks to more than one canonical playlist".to_owned(),
        ));
    }
    Ok(())
}

async fn approved_proposal(
    database: &Database,
    account_id: Uuid,
    requested: Option<Uuid>,
) -> Result<Uuid> {
    sqlx::query_scalar(
        "SELECT id FROM playlist_generations
         WHERE provider_account_id = $1 AND status = 'approved'
           AND ($2::uuid IS NULL OR id = $2)
         ORDER BY approved_at DESC NULLS LAST, created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .bind(requested)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(
            "no matching approved proposal exists; approve one before planning".to_owned(),
        )
    })
}

async fn account_id(database: &Database, account_label: &str) -> Result<Uuid> {
    sqlx::query_scalar(
        "SELECT id FROM provider_accounts WHERE provider = $1 AND account_label = $2",
    )
    .bind(PROVIDER)
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| ChordriftError::Configuration("Spotify account is not imported".to_owned()))
}

async fn latest_snapshot(database: &Database, account_id: Uuid) -> Result<Uuid> {
    sqlx::query_scalar(
        "SELECT id FROM provider_inventory_observations WHERE provider_account_id = $1
         ORDER BY captured_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration("Spotify account has no imported snapshot".to_owned())
    })
}

async fn existing_plan(
    database: &Database,
    account_id: Uuid,
    input_hash: &str,
) -> Result<Option<PlanReport>> {
    let row = sqlx::query(
        "SELECT id, proposal_generation_id, source_snapshot_id, input_hash, summary,
                planner_version, preconditions, started_at,
                (SELECT count(*)::bigint FROM sync_operations operation
                 WHERE operation.sync_run_id = sync_runs.id) AS operation_count
         FROM sync_runs WHERE provider_account_id = $1 AND provider = $2
           AND mode = 'dry_run' AND planner_version = $3 AND input_hash = $4",
    )
    .bind(account_id)
    .bind(PROVIDER)
    .bind(PLANNER_VERSION)
    .bind(input_hash)
    .fetch_optional(database.pool())
    .await?;
    row.map(|row| {
        let summary: Summary = serde_json::from_value(row.try_get("summary")?)?;
        Ok(report(
            row.try_get("id")?,
            stored_plan_origin(
                &row.try_get::<String, _>("planner_version")?,
                &row.try_get::<Value, _>("preconditions")?,
            )?,
            row.try_get("proposal_generation_id")?,
            row.try_get("source_snapshot_id")?,
            true,
            row.try_get("input_hash")?,
            usize::try_from(row.try_get::<i64, _>("operation_count")?).map_err(|_| {
                ChordriftError::Configuration("invalid sync operation count".to_owned())
            })?,
            summary,
            row.try_get("started_at")?,
        ))
    })
    .transpose()
}

fn summarize(operations: &[PlanOperationInput]) -> Summary {
    Summary {
        creates: count_kind(operations, "create_playlist"),
        renames: count_kind(operations, "rename_playlist"),
        reorders: count_kind(operations, "reorder_playlist"),
        additions: count_kind(operations, "add_track"),
        restorations: count_kind(operations, "restore_track"),
        artwork_uploads: count_kind(operations, "upload_artwork"),
        exclusions: count_kind(operations, "exclude_track"),
        removals: count_kind(operations, "remove_track")
            + count_kind(operations, "remove_saved_track"),
        retirements: count_kind(operations, "archive_playlist")
            + count_kind(operations, "remove_saved_album"),
        external_cleanups: count_kind(operations, "remove_external_playlist"),
        deferred: operations
            .iter()
            .filter(|operation| operation.safety.get("deferred") == Some(&Value::Bool(true)))
            .count(),
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    plan_id: Uuid,
    origin: PlanOrigin,
    proposal_generation_id: Option<Uuid>,
    source_snapshot_id: Uuid,
    reused: bool,
    input_hash: String,
    operation_count: usize,
    summary: Summary,
    created_at: DateTime<Utc>,
) -> PlanReport {
    PlanReport {
        plan_id,
        origin,
        proposal_generation_id,
        source_snapshot_id,
        reused,
        input_hash,
        operation_count,
        creates: summary.creates,
        renames: summary.renames,
        reorders: summary.reorders,
        additions: summary.additions,
        restorations: summary.restorations,
        artwork_uploads: summary.artwork_uploads,
        exclusions: summary.exclusions,
        removals: summary.removals,
        retirements: summary.retirements,
        external_cleanups: summary.external_cleanups,
        deferred: summary.deferred,
        created_at,
    }
}

fn stored_plan_origin(planner_version: &str, preconditions: &Value) -> Result<PlanOrigin> {
    match preconditions.get("plan_origin").and_then(Value::as_str) {
        Some("maintenance") => Ok(PlanOrigin::Maintenance),
        Some("spin_publication") => Ok(PlanOrigin::SpinPublication),
        Some(origin) => Err(ChordriftError::Configuration(format!(
            "maintenance workflow refuses plan origin {origin:?}"
        ))),
        None if planner_version == PLANNER_VERSION => Ok(PlanOrigin::Maintenance),
        None => Err(ChordriftError::Configuration(
            "sync plan has no recognized business origin".to_owned(),
        )),
    }
}

fn count_kind(operations: &[PlanOperationInput], kind: &str) -> usize {
    operations
        .iter()
        .filter(|operation| operation.operation_type == kind)
        .count()
}

fn phase_rank(phase: &str) -> u8 {
    match phase {
        "publish" => 0,
        "reconcile" => 1,
        "cleanup" => 2,
        "retirement" => 3,
        _ => 4,
    }
}

fn operation_rank(operation_type: &str) -> u8 {
    match operation_type {
        "create_playlist" => 0,
        "rename_playlist" => 1,
        "add_track" | "restore_track" => 2,
        "reorder_playlist" => 3,
        "upload_artwork" => 4,
        "exclude_track" => 5,
        "remove_track" | "remove_saved_track" => 6,
        "remove_external_playlist" => 7,
        "archive_playlist" | "remove_saved_album" => 8,
        _ => 8,
    }
}

fn operation_position(operation: &PlanOperationInput) -> i64 {
    operation
        .payload
        .get("position")
        .and_then(Value::as_i64)
        .unwrap_or(-1)
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn json_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("-")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        CurrentPlaylist, DesiredPlaylist, DesiredTrack, PlanOperationInput,
        canonical_retirement_operations, count_kind, hex_sha256, operation_position,
        operation_rank, phase_rank, playlist_diff, stored_plan_origin, summarize,
    };
    use serde_json::json;
    use std::collections::{BTreeMap, BTreeSet};
    use uuid::Uuid;

    fn operation(kind: &str, phase: &str, deferred: bool) -> PlanOperationInput {
        PlanOperationInput {
            phase: phase.to_owned(),
            operation_type: kind.to_owned(),
            operation_key: format!("{kind}:key"),
            playlist_id: None,
            provider_playlist_id: None,
            playlist_name: "Test".to_owned(),
            spotify_playlist_id: None,
            spotify_track_id: None,
            payload: json!({}),
            safety: json!({"deferred": deferred}),
        }
    }

    #[test]
    fn summarizes_restores_separately_from_additions() {
        let operations = vec![
            operation("add_track", "publish", false),
            operation("restore_track", "publish", false),
            operation("remove_external_playlist", "cleanup", true),
            operation("archive_playlist", "retirement", true),
            operation("remove_saved_album", "retirement", true),
        ];
        let summary = summarize(&operations);
        assert_eq!(summary.additions, 1);
        assert_eq!(summary.restorations, 1);
        assert_eq!(summary.exclusions, 0);
        assert_eq!(summary.retirements, 2);
        assert_eq!(summary.external_cleanups, 1);
        assert_eq!(summary.deferred, 3);
        assert_eq!(count_kind(&operations, "remove_track"), 0);
    }

    #[test]
    fn phases_put_retirement_last() {
        assert!(phase_rank("publish") < phase_rank("reconcile"));
        assert!(phase_rank("reconcile") < phase_rank("cleanup"));
        assert!(phase_rank("cleanup") < phase_rank("retirement"));
    }

    #[test]
    fn plan_reader_exposes_spin_publication_origin() {
        let origin = stored_plan_origin(
            "spin-publication-v1",
            &json!({"plan_origin": "spin_publication"}),
        )
        .expect("general plan inspection recognizes the explicit origin");
        assert_eq!(origin, super::PlanOrigin::SpinPublication);
    }

    #[test]
    fn absent_managed_concept_becomes_an_explicit_retirement() {
        let concept_id = Uuid::new_v4();
        let current = CurrentPlaylist {
            playlist_id: Uuid::new_v4(),
            provider_playlist_id: Uuid::new_v4(),
            spotify_id: "spotify-playlist".to_owned(),
            name: "Retired concept".to_owned(),
            provider_snapshot_id: Some("snapshot".to_owned()),
            tracks: Vec::new(),
            verified_tracks: BTreeSet::new(),
        };

        let operations =
            canonical_retirement_operations(&[], &BTreeMap::from([(concept_id, current)]));

        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].phase, "retirement");
        assert_eq!(operations[0].operation_type, "archive_playlist");
        assert_eq!(operations[0].payload["concept_id"], concept_id.to_string());
        assert_eq!(
            operations[0].payload["reason"],
            "concept_absent_from_complete_approved_proposal"
        );
    }

    #[test]
    fn creation_precedes_position_ordered_additions() {
        let create = operation("create_playlist", "publish", false);
        let mut first = operation("add_track", "publish", false);
        first.payload = json!({"position": 1});
        let mut second = operation("add_track", "publish", false);
        second.payload = json!({"position": 2});
        assert!(operation_rank(&create.operation_type) < operation_rank(&first.operation_type));
        assert!(operation_position(&first) < operation_position(&second));
    }

    #[test]
    fn verified_user_removal_becomes_exclusion_not_readdition() {
        let concept_id = Uuid::new_v4();
        let track_id = Uuid::new_v4();
        let desired = DesiredPlaylist {
            playlist_id: Uuid::new_v4(),
            concept_id,
            stable_key: "playlist-test".to_owned(),
            name: "Test".to_owned(),
            description: "Test".to_owned(),
            tracks: vec![DesiredTrack {
                canonical_id: track_id,
                spotify_id: "spotify-track".to_owned(),
                position: 0,
                restored: false,
            }],
        };
        let current = CurrentPlaylist {
            playlist_id: Uuid::new_v4(),
            provider_playlist_id: Uuid::new_v4(),
            spotify_id: "spotify-playlist".to_owned(),
            name: "Test".to_owned(),
            provider_snapshot_id: Some("snapshot".to_owned()),
            tracks: Vec::new(),
            verified_tracks: BTreeSet::from([track_id]),
        };
        let operations = playlist_diff(
            &[desired],
            &BTreeMap::from([(concept_id, current)]),
            &BTreeSet::new(),
        );
        assert_eq!(count_kind(&operations, "exclude_track"), 1);
        assert_eq!(count_kind(&operations, "add_track"), 0);
    }

    #[test]
    fn verified_user_removal_in_reevaluate_is_not_excluded_or_readded() {
        let concept_id = Uuid::new_v4();
        let track_id = Uuid::new_v4();
        let desired = DesiredPlaylist {
            playlist_id: Uuid::new_v4(),
            concept_id,
            stable_key: "playlist-test".to_owned(),
            name: "Test".to_owned(),
            description: "Test".to_owned(),
            tracks: vec![DesiredTrack {
                canonical_id: track_id,
                spotify_id: "spotify-track".to_owned(),
                position: 0,
                restored: false,
            }],
        };
        let current = CurrentPlaylist {
            playlist_id: Uuid::new_v4(),
            provider_playlist_id: Uuid::new_v4(),
            spotify_id: "spotify-playlist".to_owned(),
            name: "Test".to_owned(),
            provider_snapshot_id: Some("snapshot".to_owned()),
            tracks: Vec::new(),
            verified_tracks: BTreeSet::from([track_id]),
        };
        let operations = playlist_diff(
            &[desired],
            &BTreeMap::from([(concept_id, current)]),
            &BTreeSet::from([track_id]),
        );
        assert_eq!(count_kind(&operations, "exclude_track"), 0);
        assert_eq!(count_kind(&operations, "add_track"), 0);
    }

    #[test]
    fn sha256_is_stable() {
        assert_eq!(
            hex_sha256(b"chordrift"),
            "d8c5f8026d25d28d1bc7431eec6b9d247a3711d2f19562ec6639d9d96fb0bbd0"
        );
    }
}
