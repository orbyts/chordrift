//! Approved Spin publication plans and provider-neutral fake execution proof.

use std::{collections::BTreeSet, fmt};

use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{
    ChordriftError,
    application::ApplicationInvocation,
    contract::{CONTRACT_VERSION, ClientError, Command, CommandRequest, ErrorCode},
    domain::{AccountOwnedId, ChordriftAccountId, ProviderConnectionId, SurfaceId},
};

/// Stable planner identity for immutable Spin publication plans.
pub const SPIN_PUBLICATION_PLANNER_VERSION: &str = "spin-publication-v1";

/// Extra account-owned target selected for an approved Spin publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SpinPublicationRequest {
    /// Renewable playlist surface that receives the Spin.
    pub surface_id: AccountOwnedId<SurfaceId>,
    /// Provider connection whose current checkpoint and target are used.
    pub provider_connection_id: ProviderConnectionId,
}

/// One exact operation in a Spin publication plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SpinPublicationOperation {
    /// Create the provider playlist target before adding tracks.
    CreatePlaylist {
        /// Provider-neutral surface name.
        name: String,
    },
    /// Add one and only one enumerated provider track.
    AddTrack {
        /// Opaque provider track identity.
        provider_track_id: String,
        /// One-based position in the approved Spin, retained as intent.
        spin_position: u16,
    },
}

/// Immutable approved plan linking one Spin, surface, and provider checkpoint.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SpinPublicationPlan {
    /// Durable publication-link identity.
    pub publication_id: Uuid,
    /// Existing synchronization-ledger plan identity.
    pub plan_id: Uuid,
    /// Owning Chordrift account.
    pub account_id: ChordriftAccountId,
    /// Approved immutable Spin.
    pub spin_id: Uuid,
    /// Account-owned target surface.
    pub surface_id: AccountOwnedId<SurfaceId>,
    /// Provider connection selected for publication.
    pub provider_connection_id: ProviderConnectionId,
    /// Provider namespace understood by an adapter.
    pub provider_namespace: String,
    /// Stable provider target key, real or planned.
    pub target_key: String,
    /// Immutable inventory checkpoint used for readiness.
    pub source_checkpoint_id: Uuid,
    /// Legacy snapshot linked by the shared synchronization ledger.
    pub source_snapshot_id: Uuid,
    /// Canonical hash of all planning inputs.
    pub input_hash: String,
    /// Exact ordered operations. No implicit desired membership exists.
    pub operations: Vec<SpinPublicationOperation>,
    /// Active surface exclusions omitted from this publication.
    pub excluded_tracks: usize,
    /// Whether an identical immutable plan was reused.
    pub reused: bool,
}

/// Provider observation used by readiness and verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationObservation {
    /// Current immutable checkpoint identity.
    pub checkpoint_id: Uuid,
    /// Whether the provider target currently exists.
    pub target_exists: bool,
    /// Exact current membership in provider order.
    pub tracks: Vec<String>,
}

/// Readiness evidence bound to one exact plan and provider observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationReadiness {
    /// Plan that was assessed.
    pub plan_id: Uuid,
    /// Exact plan input hash.
    pub input_hash: String,
    /// Checkpoint observed immediately before fake execution.
    pub checkpoint_id: Uuid,
    /// Membership that must be preserved by enumerated additions.
    pub baseline_tracks: Vec<String>,
}

/// Verified outcome of one fake-provider publication exercise.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationVerification {
    /// Exact plan verified.
    pub plan_id: Uuid,
    /// Number of provider creates executed.
    pub created_targets: usize,
    /// Number of explicit additions executed.
    pub added_tracks: usize,
    /// Number of already-present additions reused during replay.
    pub reused_tracks: usize,
    /// Final provider membership.
    pub final_tracks: Vec<String>,
}

/// Provider port used to prove publication without wiring a production adapter.
pub trait SpinPublicationProvider {
    /// Returns one exact target observation.
    fn observe(&self, target_key: &str) -> Result<PublicationObservation, ClientError>;

    /// Creates one empty target.
    fn create_playlist(&mut self, target_key: &str, name: &str) -> Result<(), ClientError>;

    /// Appends only the supplied explicit provider track identities.
    fn add_tracks(&mut self, target_key: &str, tracks: &[String]) -> Result<(), ClientError>;
}

/// V020-12 application error with a stable client-safe form.
#[derive(Debug)]
pub enum SpinPublicationError {
    /// Valid client-visible rejection.
    Client(ClientError),
    /// Database or serialization failure.
    Infrastructure(ChordriftError),
}

impl SpinPublicationError {
    /// Returns the client-safe error representation.
    #[must_use]
    pub fn client_error(&self) -> ClientError {
        match self {
            Self::Client(error) => *error,
            Self::Infrastructure(_) => ClientError::new(ErrorCode::Internal, true),
        }
    }
}

impl fmt::Display for SpinPublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.client_error().message())
    }
}

impl std::error::Error for SpinPublicationError {}

impl From<sqlx::Error> for SpinPublicationError {
    fn from(error: sqlx::Error) -> Self {
        Self::Infrastructure(ChordriftError::from(error))
    }
}

impl From<serde_json::Error> for SpinPublicationError {
    fn from(error: serde_json::Error) -> Self {
        Self::Infrastructure(ChordriftError::from(error))
    }
}

/// PostgreSQL-backed boundary that approves and plans without provider access.
pub struct SpinPublicationBoundary<'database> {
    database: &'database Database,
}

impl<'database> SpinPublicationBoundary<'database> {
    /// Creates a publication boundary over an existing database connection.
    #[must_use]
    pub const fn new(database: &'database Database) -> Self {
        Self { database }
    }

    /// Approves one immutable Spin and creates or reuses its immutable plan.
    pub async fn approve_and_plan(
        &self,
        account_id: ChordriftAccountId,
        command: &CommandRequest,
        request: SpinPublicationRequest,
    ) -> Result<SpinPublicationPlan, SpinPublicationError> {
        let spin_id = validate_request(account_id, command, request)?;
        let mut transaction = self.database.pool().begin().await?;

        let spin = sqlx::query(
            "SELECT spin.status, spin.input_fingerprint, revision.recipe_id,
                    revision.state AS recipe_state
               FROM playlist_spins spin
               JOIN playlist_recipe_revisions revision
                 ON revision.chordrift_account_id = spin.chordrift_account_id
                AND revision.id = spin.recipe_revision_id
              WHERE spin.chordrift_account_id = $1 AND spin.id = $2
              FOR UPDATE OF spin",
        )
        .bind(account_id.as_uuid())
        .bind(spin_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| client_error(ErrorCode::ResourceNotFound))?;
        let spin_status: String = spin.try_get("status")?;
        if spin_status == "superseded" {
            return Err(client_error(ErrorCode::StateConflict));
        }
        if spin.try_get::<String, _>("recipe_state")? != "approved" {
            return Err(client_error(ErrorCode::StateConflict));
        }
        let recipe_id: Uuid = spin.try_get("recipe_id")?;

        let surface_id = request.surface_id.into_resource_id().as_uuid();
        let surface = sqlx::query(
            "SELECT name, authority, purpose, refresh_policy, active, recipe_id
               FROM playlist_surfaces
              WHERE chordrift_account_id = $1 AND id = $2",
        )
        .bind(account_id.as_uuid())
        .bind(surface_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| client_error(ErrorCode::ResourceNotFound))?;
        let surface_recipe: Option<Uuid> = surface.try_get("recipe_id")?;
        if !surface.try_get::<bool, _>("active")?
            || surface.try_get::<String, _>("purpose")? != "renewable_experience"
            || !matches!(
                surface.try_get::<String, _>("authority")?.as_str(),
                "chordrift" | "collaborative"
            )
            || !matches!(
                surface.try_get::<String, _>("refresh_policy")?.as_str(),
                "manual_spin" | "scheduled"
            )
            || surface_recipe != Some(recipe_id)
        {
            return Err(client_error(ErrorCode::StateConflict));
        }
        let surface_name: String = surface.try_get("name")?;

        let provider_connection_id = request.provider_connection_id.as_uuid();
        let provider_namespace: String = sqlx::query_scalar(
            "SELECT provider FROM provider_accounts
              WHERE chordrift_account_id = $1 AND id = $2",
        )
        .bind(account_id.as_uuid())
        .bind(provider_connection_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| client_error(ErrorCode::PermissionDenied))?;
        let checkpoint = sqlx::query(
            "SELECT id, source_snapshot_id
               FROM provider_inventory_checkpoints
              WHERE provider_account_id = $1 AND source_snapshot_id IS NOT NULL
              ORDER BY captured_at DESC, id DESC LIMIT 1",
        )
        .bind(provider_connection_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| client_error(ErrorCode::StateConflict))?;
        let source_checkpoint_id: Uuid = checkpoint.try_get("id")?;
        let source_snapshot_id: Uuid = checkpoint.try_get("source_snapshot_id")?;

        let link = sqlx::query(
            "SELECT link.state, link.provider_playlist_id,
                    COALESCE(link.provider_playlist_key, provider.provider_playlist_id)
                        AS target_key
               FROM playlist_surface_provider_links link
               LEFT JOIN provider_playlists provider ON provider.id = link.provider_playlist_id
              WHERE link.chordrift_account_id = $1 AND link.surface_id = $2
                AND link.provider_account_id = $3
                AND link.state IN ('planned', 'observed', 'active')
              ORDER BY CASE link.state WHEN 'active' THEN 0 WHEN 'observed' THEN 1 ELSE 2 END,
                       link.first_linked_at DESC, link.id DESC LIMIT 1",
        )
        .bind(account_id.as_uuid())
        .bind(surface_id)
        .bind(provider_connection_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| client_error(ErrorCode::StateConflict))?;
        let provider_playlist_id: Option<Uuid> = link.try_get("provider_playlist_id")?;
        let target_key: Option<String> = link.try_get("target_key")?;
        let create_target = target_key.is_none();
        let target_key = target_key.unwrap_or_else(|| format!("chordrift-surface-{surface_id}"));

        let live_tracks: Vec<String> = if let Some(provider_playlist_id) = provider_playlist_id {
            sqlx::query_scalar(
                "SELECT provider.provider_track_id
                   FROM provider_inventory_checkpoint_playlists playlist
                   JOIN provider_playlist_revision_tracks membership
                     ON membership.revision_id = playlist.revision_id
                   JOIN provider_tracks provider ON provider.id = membership.provider_track_id
                  WHERE playlist.checkpoint_id = $1
                    AND playlist.provider_playlist_id = $2
                  ORDER BY membership.position",
            )
            .bind(source_checkpoint_id)
            .bind(provider_playlist_id)
            .fetch_all(&mut *transaction)
            .await?
        } else {
            Vec::new()
        };

        let track_rows = sqlx::query(
            "SELECT membership.position, membership.track_id,
                    ARRAY(
                        SELECT provider.provider_track_id
                          FROM provider_tracks provider
                         WHERE provider.track_id = membership.track_id
                           AND provider.provider = $4
                         ORDER BY provider.provider_track_id
                    ) AS provider_track_ids,
                    EXISTS (
                        SELECT 1 FROM playlist_track_directives directive
                         WHERE directive.chordrift_account_id = $1
                           AND directive.surface_id = $3
                           AND directive.track_id = membership.track_id
                           AND directive.directive = 'exclude'
                           AND directive.superseded_at IS NULL
                    ) AS excluded
               FROM playlist_spin_tracks membership
              WHERE membership.spin_id = $2
              ORDER BY membership.position",
        )
        .bind(account_id.as_uuid())
        .bind(spin_id)
        .bind(surface_id)
        .bind(&provider_namespace)
        .fetch_all(&mut *transaction)
        .await?;
        let mut desired = Vec::with_capacity(track_rows.len());
        let mut excluded_tracks = 0usize;
        for row in track_rows {
            if row.try_get::<bool, _>("excluded")? {
                excluded_tracks += 1;
                continue;
            }
            let provider_track_ids: Vec<String> = row.try_get("provider_track_ids")?;
            let [provider_track_id] = provider_track_ids.as_slice() else {
                return Err(client_error(ErrorCode::CapabilityUnavailable));
            };
            desired.push((
                u16::try_from(row.try_get::<i32, _>("position")? + 1)
                    .map_err(|_| client_error(ErrorCode::StateConflict))?,
                provider_track_id.clone(),
            ));
        }
        let additions = enumerated_additions(&desired, &live_tracks, &BTreeSet::new());
        let mut operations = Vec::with_capacity(additions.len() + usize::from(create_target));
        if create_target {
            operations.push(SpinPublicationOperation::CreatePlaylist {
                name: surface_name.clone(),
            });
        }
        operations.extend(
            additions
                .into_iter()
                .map(
                    |(spin_position, provider_track_id)| SpinPublicationOperation::AddTrack {
                        provider_track_id,
                        spin_position,
                    },
                ),
        );

        let input = json!({
            "planner_version": SPIN_PUBLICATION_PLANNER_VERSION,
            "plan_origin": "spin_publication",
            "account_id": account_id,
            "spin_id": spin_id,
            "spin_fingerprint": spin.try_get::<String, _>("input_fingerprint")?,
            "surface_id": surface_id,
            "provider_connection_id": provider_connection_id,
            "provider_namespace": provider_namespace,
            "target_key": target_key,
            "source_checkpoint_id": source_checkpoint_id,
            "source_snapshot_id": source_snapshot_id,
            "live_tracks": live_tracks,
            "excluded_tracks": excluded_tracks,
            "operations": operations,
        });
        let input_hash = hex_sha256(&serde_json::to_vec(&input)?);

        if let Some(plan_id) = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM sync_runs
              WHERE provider_account_id = $1 AND provider = $2
                AND planner_version = $3 AND input_hash = $4",
        )
        .bind(provider_connection_id)
        .bind(&provider_namespace)
        .bind(SPIN_PUBLICATION_PLANNER_VERSION)
        .bind(&input_hash)
        .fetch_optional(&mut *transaction)
        .await?
        {
            transaction.commit().await?;
            return self.load(account_id, plan_id, true).await;
        }

        sqlx::query(
            "UPDATE playlist_spins SET status = 'approved', approved_at = COALESCE(approved_at, now())
              WHERE chordrift_account_id = $1 AND id = $2",
        )
        .bind(account_id.as_uuid())
        .bind(spin_id)
        .execute(&mut *transaction)
        .await?;
        let summary = json!({
            "origin": "spin_publication",
            "operations": operations.len(),
            "creates": usize::from(create_target),
            "renames": 0,
            "reorders": 0,
            "additions": operations.iter().filter(|operation| matches!(operation, SpinPublicationOperation::AddTrack { .. })).count(),
            "restorations": 0,
            "artwork_uploads": 0,
            "exclusions": 0,
            "removals": 0,
            "retirements": 0,
            "external_cleanups": 0,
            "deferred": operations.len(),
            "excluded_tracks": excluded_tracks,
            "spotify_writes": false,
        });
        let plan_id: Uuid = sqlx::query_scalar(
            "INSERT INTO sync_runs
                 (provider, mode, status, desired_state_hash, summary,
                  provider_account_id, source_snapshot_id, provider_checkpoint_id,
                  planner_version, input_hash, preconditions, finished_at)
             VALUES ($1, 'dry_run', 'planned', $2, $3, $4, $5, $6, $7, $2,
                     $8, now()) RETURNING id",
        )
        .bind(&provider_namespace)
        .bind(&input_hash)
        .bind(summary)
        .bind(provider_connection_id)
        .bind(source_snapshot_id)
        .bind(source_checkpoint_id)
        .bind(SPIN_PUBLICATION_PLANNER_VERSION)
        .bind(json!({
            "plan_origin": "spin_publication",
            "requires_approved_spin": spin_id,
            "requires_current_checkpoint": source_checkpoint_id,
            "enumerated_additions_only": true,
            "preserve_unenumerated_live_membership": true,
            "production_provider_adapter": false,
        }))
        .fetch_one(&mut *transaction)
        .await?;
        for (sequence, operation) in operations.iter().enumerate() {
            let (operation_type, payload) = match operation {
                SpinPublicationOperation::CreatePlaylist { name } => (
                    "create_playlist",
                    json!({
                        "playlist_name": surface_name,
                        "spotify_playlist_id": Value::Null,
                        "detail": {"surface_id": surface_id, "target_key": target_key, "name": name}
                    }),
                ),
                SpinPublicationOperation::AddTrack {
                    provider_track_id,
                    spin_position,
                } => (
                    "add_track",
                    json!({
                        "playlist_name": surface_name,
                        "spotify_playlist_id": target_key,
                        "spotify_track_id": provider_track_id,
                        "detail": {
                            "surface_id": surface_id,
                            "target_key": target_key,
                            "provider_track_id": provider_track_id,
                            "spin_position": spin_position,
                            "enumerated_addition": true
                        }
                    }),
                ),
            };
            sqlx::query(
                "INSERT INTO sync_operations
                     (sync_run_id, operation_type, operation_key, phase, sequence,
                      payload, safety)
                 VALUES ($1, $2, $3, 'publish', $4, $5, $6)",
            )
            .bind(plan_id)
            .bind(operation_type)
            .bind(format!("spin-publication:{sequence:05}"))
            .bind(i32::try_from(sequence).map_err(|_| client_error(ErrorCode::StateConflict))?)
            .bind(payload)
            .bind(json!({
                "plan_origin": "spin_publication",
                "preserve_unenumerated_live_membership": true,
                "active_surface_exclusions_checked": true,
            }))
            .execute(&mut *transaction)
            .await?;
        }
        let publication_id: Uuid = sqlx::query_scalar(
            "INSERT INTO playlist_spin_publications
                 (chordrift_account_id, spin_id, surface_id, provider_account_id,
                  sync_run_id, status, approved_at)
             VALUES ($1, $2, $3, $4, $5, 'approved', now()) RETURNING id",
        )
        .bind(account_id.as_uuid())
        .bind(spin_id)
        .bind(surface_id)
        .bind(provider_connection_id)
        .bind(plan_id)
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;

        Ok(SpinPublicationPlan {
            publication_id,
            plan_id,
            account_id,
            spin_id,
            surface_id: request.surface_id,
            provider_connection_id: request.provider_connection_id,
            provider_namespace,
            target_key,
            source_checkpoint_id,
            source_snapshot_id,
            input_hash,
            operations,
            excluded_tracks,
            reused: false,
        })
    }

    async fn load(
        &self,
        account_id: ChordriftAccountId,
        plan_id: Uuid,
        reused: bool,
    ) -> Result<SpinPublicationPlan, SpinPublicationError> {
        let row = sqlx::query(
            "SELECT publication.id AS publication_id, publication.spin_id,
                    publication.surface_id, publication.provider_account_id,
                    run.provider, run.source_snapshot_id, run.provider_checkpoint_id,
                    run.input_hash, run.summary,
                    COALESCE(link.provider_playlist_key, provider.provider_playlist_id,
                             'chordrift-surface-' || publication.surface_id::text) AS target_key
               FROM playlist_spin_publications publication
               JOIN sync_runs run ON run.id = publication.sync_run_id
               LEFT JOIN playlist_surface_provider_links link
                 ON link.surface_id = publication.surface_id
                AND link.provider_account_id = publication.provider_account_id
                AND link.state IN ('planned', 'observed', 'active')
               LEFT JOIN provider_playlists provider ON provider.id = link.provider_playlist_id
              WHERE publication.chordrift_account_id = $1
                AND publication.sync_run_id = $2
                AND run.preconditions ->> 'plan_origin' = 'spin_publication'
              ORDER BY link.first_linked_at DESC NULLS LAST LIMIT 1",
        )
        .bind(account_id.as_uuid())
        .bind(plan_id)
        .fetch_optional(self.database.pool())
        .await?
        .ok_or_else(|| client_error(ErrorCode::ResourceNotFound))?;
        let operation_rows = sqlx::query(
            "SELECT operation_type, payload FROM sync_operations
              WHERE sync_run_id = $1 ORDER BY sequence",
        )
        .bind(plan_id)
        .fetch_all(self.database.pool())
        .await?;
        let mut operations = Vec::with_capacity(operation_rows.len());
        for operation in operation_rows {
            let payload: Value = operation.try_get("payload")?;
            let detail = payload
                .get("detail")
                .ok_or_else(|| client_error(ErrorCode::StateConflict))?;
            match operation.try_get::<String, _>("operation_type")?.as_str() {
                "create_playlist" => operations.push(SpinPublicationOperation::CreatePlaylist {
                    name: json_string(detail, "name")?,
                }),
                "add_track" => operations.push(SpinPublicationOperation::AddTrack {
                    provider_track_id: json_string(detail, "provider_track_id")?,
                    spin_position: u16::try_from(
                        detail
                            .get("spin_position")
                            .and_then(Value::as_u64)
                            .ok_or_else(|| client_error(ErrorCode::StateConflict))?,
                    )
                    .map_err(|_| client_error(ErrorCode::StateConflict))?,
                }),
                _ => return Err(client_error(ErrorCode::StateConflict)),
            }
        }
        let surface_id: Uuid = row.try_get("surface_id")?;
        let summary: Value = row.try_get("summary")?;
        Ok(SpinPublicationPlan {
            publication_id: row.try_get("publication_id")?,
            plan_id,
            account_id,
            spin_id: row.try_get("spin_id")?,
            surface_id: AccountOwnedId::new(account_id, SurfaceId::from_uuid(surface_id)),
            provider_connection_id: ProviderConnectionId::from_uuid(
                row.try_get("provider_account_id")?,
            ),
            provider_namespace: row.try_get("provider")?,
            target_key: row.try_get("target_key")?,
            source_checkpoint_id: row.try_get("provider_checkpoint_id")?,
            source_snapshot_id: row.try_get("source_snapshot_id")?,
            input_hash: row.try_get("input_hash")?,
            operations,
            excluded_tracks: summary
                .get("excluded_tracks")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| client_error(ErrorCode::StateConflict))?,
            reused,
        })
    }

    /// Wraps approval and planning for the shared application facade.
    #[must_use]
    pub const fn invocation<'request>(
        &'request self,
        account_id: ChordriftAccountId,
        command: &'request CommandRequest,
        request: SpinPublicationRequest,
    ) -> SpinPublicationInvocation<'request, 'database> {
        SpinPublicationInvocation {
            boundary: self,
            account_id,
            command,
            request,
        }
    }
}

/// Facade adapter for approved Spin publication planning.
pub struct SpinPublicationInvocation<'request, 'database> {
    boundary: &'request SpinPublicationBoundary<'database>,
    account_id: ChordriftAccountId,
    command: &'request CommandRequest,
    request: SpinPublicationRequest,
}

impl ApplicationInvocation for SpinPublicationInvocation<'_, '_> {
    type Output = Result<SpinPublicationPlan, SpinPublicationError>;

    async fn execute(self) -> crate::Result<Self::Output> {
        Ok(self
            .boundary
            .approve_and_plan(self.account_id, self.command, self.request)
            .await)
    }
}

/// Assesses one exact plan against a provider observation without writing.
pub fn assess_publication(
    plan: &SpinPublicationPlan,
    observation: &PublicationObservation,
) -> Result<PublicationReadiness, SpinPublicationError> {
    if observation.checkpoint_id != plan.source_checkpoint_id {
        return Err(client_error(ErrorCode::StateConflict));
    }
    let creates = plan
        .operations
        .iter()
        .filter(|operation| matches!(operation, SpinPublicationOperation::CreatePlaylist { .. }))
        .count();
    if creates > 1 || (creates == 0 && !observation.target_exists) {
        return Err(client_error(ErrorCode::StateConflict));
    }
    Ok(PublicationReadiness {
        plan_id: plan.plan_id,
        input_hash: plan.input_hash.clone(),
        checkpoint_id: observation.checkpoint_id,
        baseline_tracks: observation.tracks.clone(),
    })
}

/// Executes and verifies only through an explicitly supplied provider port.
///
/// No production provider implements this port in V020-12.
pub fn apply_and_verify<P: SpinPublicationProvider>(
    plan: &SpinPublicationPlan,
    readiness: &PublicationReadiness,
    provider: &mut P,
) -> Result<PublicationVerification, SpinPublicationError> {
    if readiness.plan_id != plan.plan_id
        || readiness.input_hash != plan.input_hash
        || readiness.checkpoint_id != plan.source_checkpoint_id
    {
        return Err(client_error(ErrorCode::StateConflict));
    }
    let current = provider
        .observe(&plan.target_key)
        .map_err(SpinPublicationError::Client)?;
    if current.checkpoint_id != readiness.checkpoint_id
        || current.tracks != readiness.baseline_tracks
    {
        return Err(client_error(ErrorCode::StateConflict));
    }

    let mut created_targets = 0usize;
    let mut added_tracks = 0usize;
    let mut reused_tracks = 0usize;
    for operation in &plan.operations {
        match operation {
            SpinPublicationOperation::CreatePlaylist { name } => {
                let observed = provider
                    .observe(&plan.target_key)
                    .map_err(SpinPublicationError::Client)?;
                if !observed.target_exists {
                    provider
                        .create_playlist(&plan.target_key, name)
                        .map_err(SpinPublicationError::Client)?;
                    created_targets += 1;
                }
            }
            SpinPublicationOperation::AddTrack {
                provider_track_id, ..
            } => {
                let observed = provider
                    .observe(&plan.target_key)
                    .map_err(SpinPublicationError::Client)?;
                if observed.tracks.contains(provider_track_id) {
                    reused_tracks += 1;
                } else {
                    provider
                        .add_tracks(&plan.target_key, std::slice::from_ref(provider_track_id))
                        .map_err(SpinPublicationError::Client)?;
                    added_tracks += 1;
                }
            }
        }
    }
    let final_state = provider
        .observe(&plan.target_key)
        .map_err(SpinPublicationError::Client)?;
    if !readiness
        .baseline_tracks
        .iter()
        .all(|track| final_state.tracks.contains(track))
        || !plan.operations.iter().all(|operation| match operation {
            SpinPublicationOperation::CreatePlaylist { .. } => final_state.target_exists,
            SpinPublicationOperation::AddTrack {
                provider_track_id, ..
            } => final_state.tracks.contains(provider_track_id),
        })
    {
        return Err(client_error(ErrorCode::StateConflict));
    }
    Ok(PublicationVerification {
        plan_id: plan.plan_id,
        created_targets,
        added_tracks,
        reused_tracks,
        final_tracks: final_state.tracks,
    })
}

fn validate_request(
    account_id: ChordriftAccountId,
    command: &CommandRequest,
    request: SpinPublicationRequest,
) -> Result<Uuid, SpinPublicationError> {
    if command.contract_version != CONTRACT_VERSION {
        return Err(client_error(ErrorCode::IncompatibleContract));
    }
    if request.surface_id.account_id() != account_id {
        return Err(client_error(ErrorCode::PermissionDenied));
    }
    let Command::ApprovePublication { spin_id } = command.command else {
        return Err(client_error(ErrorCode::InvalidRequest));
    };
    Ok(spin_id.as_uuid())
}

fn enumerated_additions(
    desired: &[(u16, String)],
    live: &[String],
    excluded: &BTreeSet<String>,
) -> Vec<(u16, String)> {
    desired
        .iter()
        .filter(|(_, track)| !live.contains(track) && !excluded.contains(track))
        .cloned()
        .collect()
}

fn json_string(value: &Value, key: &str) -> Result<String, SpinPublicationError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| client_error(ErrorCode::StateConflict))
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn client_error(code: ErrorCode) -> SpinPublicationError {
    SpinPublicationError::Client(ClientError::new(code, false))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    struct FakePublicationProvider {
        checkpoint_id: Uuid,
        targets: BTreeMap<String, Vec<String>>,
        additions: Vec<Vec<String>>,
    }

    impl FakePublicationProvider {
        fn existing(checkpoint_id: Uuid, target: &str, tracks: &[&str]) -> Self {
            Self {
                checkpoint_id,
                targets: BTreeMap::from([(
                    target.to_owned(),
                    tracks.iter().map(|track| (*track).to_owned()).collect(),
                )]),
                additions: Vec::new(),
            }
        }
    }

    impl SpinPublicationProvider for FakePublicationProvider {
        fn observe(&self, target_key: &str) -> Result<PublicationObservation, ClientError> {
            Ok(PublicationObservation {
                checkpoint_id: self.checkpoint_id,
                target_exists: self.targets.contains_key(target_key),
                tracks: self.targets.get(target_key).cloned().unwrap_or_default(),
            })
        }

        fn create_playlist(&mut self, target_key: &str, _name: &str) -> Result<(), ClientError> {
            if self
                .targets
                .insert(target_key.to_owned(), Vec::new())
                .is_some()
            {
                return Err(ClientError::new(ErrorCode::StateConflict, false));
            }
            Ok(())
        }

        fn add_tracks(&mut self, target_key: &str, tracks: &[String]) -> Result<(), ClientError> {
            self.targets
                .get_mut(target_key)
                .ok_or_else(|| ClientError::new(ErrorCode::ResourceNotFound, false))?
                .extend_from_slice(tracks);
            self.additions.push(tracks.to_vec());
            Ok(())
        }
    }

    fn plan(checkpoint_id: Uuid, additions: &[&str]) -> SpinPublicationPlan {
        let account_id = ChordriftAccountId::from_uuid(Uuid::from_u128(1));
        SpinPublicationPlan {
            publication_id: Uuid::from_u128(2),
            plan_id: Uuid::from_u128(3),
            account_id,
            spin_id: Uuid::from_u128(4),
            surface_id: AccountOwnedId::new(account_id, SurfaceId::from_uuid(Uuid::from_u128(5))),
            provider_connection_id: ProviderConnectionId::from_uuid(Uuid::from_u128(6)),
            provider_namespace: "spotify".to_owned(),
            target_key: "target".to_owned(),
            source_checkpoint_id: checkpoint_id,
            source_snapshot_id: Uuid::from_u128(7),
            input_hash: "a".repeat(64),
            operations: additions
                .iter()
                .enumerate()
                .map(|(position, track)| SpinPublicationOperation::AddTrack {
                    provider_track_id: (*track).to_owned(),
                    spin_position: u16::try_from(position + 1).expect("fixture position fits"),
                })
                .collect(),
            excluded_tracks: 0,
            reused: false,
        }
    }

    #[test]
    fn fake_publication_preserves_unrelated_live_membership_and_replays() {
        let checkpoint_id = Uuid::new_v4();
        let plan = plan(checkpoint_id, &["explicit-new"]);
        let mut provider =
            FakePublicationProvider::existing(checkpoint_id, "target", &["user-added"]);
        let readiness = assess_publication(
            &plan,
            &provider
                .observe("target")
                .expect("fake observation succeeds"),
        )
        .expect("plan is ready");
        let first =
            apply_and_verify(&plan, &readiness, &mut provider).expect("fake publication verifies");
        assert_eq!(first.final_tracks, ["user-added", "explicit-new"]);
        assert_eq!(provider.additions, [vec!["explicit-new".to_owned()]]);

        let replay_readiness = assess_publication(
            &plan,
            &provider
                .observe("target")
                .expect("fake observation succeeds"),
        )
        .expect("replay plan is ready against its new baseline");
        let replay = apply_and_verify(&plan, &replay_readiness, &mut provider)
            .expect("fake replay verifies");
        assert_eq!(replay.added_tracks, 0);
        assert_eq!(replay.reused_tracks, 1);
        assert_eq!(provider.additions.len(), 1);
    }

    #[test]
    fn excluded_manual_removal_is_never_an_implicit_addition() {
        let desired = vec![(1, "manually-removed".to_owned()), (2, "new".to_owned())];
        let additions = enumerated_additions(
            &desired,
            &[],
            &BTreeSet::from(["manually-removed".to_owned()]),
        );
        assert_eq!(additions, [(2, "new".to_owned())]);
    }

    #[test]
    fn readiness_rejects_a_new_provider_checkpoint() {
        let plan = plan(Uuid::from_u128(10), &["new"]);
        let error = assess_publication(
            &plan,
            &PublicationObservation {
                checkpoint_id: Uuid::from_u128(11),
                target_exists: true,
                tracks: Vec::new(),
            },
        )
        .expect_err("stale plan is rejected");
        assert_eq!(error.client_error().code, ErrorCode::StateConflict);
    }
}
