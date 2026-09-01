//! Durable hosted provider-operation worker.
//!
//! The API persists typed commands before this process claims them. Only this
//! trusted server process may lease encrypted provider credentials. It calls
//! Rust provider/database adapters directly and never invokes the Chordrift
//! CLI, a shell, arbitrary SQL supplied by a client, or a client-supplied URL.

use std::{env, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::Utc;
use sqlx::Row as _;
use storexa::Database;

use crate::{
    ChordriftError, Result, apply, apply_readiness, config,
    contract::{
        ClientError, Command, ErrorCode, MaintenanceDecision, MaintenanceProviderEffectKind,
        MaintenanceReviewId, MaintenanceSessionId, MaintenanceSessionState, OperationId, Progress,
        ProgressUnit, ResourceId,
    },
    db,
    durable_operations::{
        DurableOperationLease, DurableOperationQueue, PostgresDurableOperationStore,
    },
    maintenance::{MaintenanceDecisionProjection, MaintenanceWorkflow},
    maintenance_interpretation::PostgresMaintenanceInterpreter,
    maintenance_projection::{
        CanonicalMaintenanceProjector, IntakePlacementPolicy, attach_maintenance_provider_effects,
        intake_placement_policy, maintenance_provider_effects,
    },
    maintenance_store::{DurableMaintenanceAuthority, PostgresMaintenanceSessionStore},
    provider_vault::{
        PostgresProviderCredentialStore, ProviderCredentialIdentity, ProviderCredentialVault,
        ProviderVaultKeyring,
    },
    providers::spotify,
    service::AuthenticatedSubject,
};

const DEFAULT_POLL_MILLISECONDS: u64 = 750;
const LEASE_DURATION: Duration = Duration::from_secs(120);

/// Uses the encrypted hosted provider credential to readiness-check and apply
/// one already-persisted append-only publish plan. This operator boundary is
/// intentionally unavailable through public HTTP and accepts no provider URL,
/// SQL, or operation payload from a client.
pub async fn apply_reviewed_sync_plan_from_env(
    account_label: &str,
    plan_id: uuid::Uuid,
) -> Result<apply::ApplyReport> {
    let database = db::connect(config::database_config_from_env()?).await?;
    db::require_schema_through(&database, 51).await?;
    let row = sqlx::query(
        "SELECT account.id AS provider_account_id,
                account.provider_account_id AS stable_provider_id,
                account.chordrift_account_id,
                membership.product_subject_id
         FROM provider_accounts account
         JOIN chordrift_account_memberships membership
           ON membership.chordrift_account_id = account.chordrift_account_id
          AND membership.status = 'active' AND membership.role = 'owner'
         WHERE account.provider = 'spotify' AND account.account_label = $1",
    )
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration("hosted account owner or Spotify connection is missing"))?;
    let provider_account_id: uuid::Uuid = row.try_get("provider_account_id")?;
    let account_id: uuid::Uuid = row.try_get("chordrift_account_id")?;
    let subject_id: uuid::Uuid = row.try_get("product_subject_id")?;
    let stable_provider_id: String = row.try_get("stable_provider_id")?;
    let subject = AuthenticatedSubject {
        subject_id: ResourceId::from_uuid(subject_id),
        account_id: ResourceId::from_uuid(account_id),
    };
    let identity = ProviderCredentialIdentity::new(
        subject.account_id,
        ResourceId::from_uuid(provider_account_id),
        "spotify",
    )
    .map_err(|_| configuration("hosted provider credential identity is invalid"))?;
    let store = PostgresProviderCredentialStore::new(database.pool().clone());
    store
        .verify_schema()
        .await
        .map_err(|_| configuration("hosted provider credential schema is not ready"))?;
    let vault = ProviderCredentialVault::new(
        store,
        ProviderVaultKeyring::from_environment()
            .map_err(|_| configuration("hosted provider credential key is not ready"))?,
    );
    let lease = vault
        .lease(subject, &identity)
        .await
        .map_err(|_| configuration("hosted Spotify credential is unavailable"))?;
    let (spotify_session, rotated) =
        spotify::hosted_session(lease.refresh_token(), lease.scopes(), &stable_provider_id).await?;
    let auth_status = spotify::AuthStatus {
        account_label: account_label.to_owned(),
        account_id: spotify_session.profile.account_id.clone(),
        display_name: spotify_session.profile.display_name.clone(),
        scopes: spotify_session.scopes.clone(),
    };
    if let Some(rotated) = rotated.as_ref() {
        vault
            .rotate(subject, identity.clone(), rotated, Utc::now())
            .await
            .map_err(|_| configuration("hosted Spotify credential rotation failed"))?;
    }
    let existing_apply: Option<(uuid::Uuid, String)> = sqlx::query_as(
        "SELECT id, status FROM sync_apply_runs
         WHERE provider_account_id = $1 AND plan_id = $2 AND phase = 'publish'
         ORDER BY started_at DESC, id DESC LIMIT 1",
    )
    .bind(provider_account_id)
    .bind(plan_id)
    .fetch_optional(database.pool())
    .await?;
    if let Some((apply_run_id, status)) = existing_apply {
        if status == "succeeded" {
            return apply::show(&database, account_label, Some(apply_run_id)).await;
        }
        if status == "awaiting_pull" {
            spotify::import_hosted_fresh(account_label, &database, spotify_session).await?;
            let verified =
                apply::verify_pending_publications(&database, account_label, false).await?;
            if verified == 0 {
                return Err(configuration(
                    "hosted post-apply observation did not verify the reviewed plan",
                ));
            }
            return apply::show(&database, account_label, Some(apply_run_id)).await;
        }
    }
    let assessment =
        apply_readiness::assess(&database, account_label, Some(plan_id), Some(&auth_status))
            .await?;
    if assessment.status != "ready" {
        return Err(ChordriftError::Configuration(format!(
            "reviewed plan readiness is {}",
            assessment.status
        )));
    }
    let mutation_session = spotify::hosted_mutation_session(spotify_session)?;
    let report = apply::execute_with_session(
        &database,
        account_label,
        assessment.assessment_id,
        apply::ApplyPhase::Publish,
        assessment.assessment_id,
        false,
        &mutation_session,
    )
    .await?;
    if report.status == "awaiting_pull" {
        drop(mutation_session);
        let observation_lease = vault
            .lease(subject, &identity)
            .await
            .map_err(|_| configuration("hosted Spotify credential is unavailable"))?;
        let (observation_session, rotated) = spotify::hosted_session(
            observation_lease.refresh_token(),
            observation_lease.scopes(),
            &stable_provider_id,
        )
        .await?;
        if let Some(rotated) = rotated.as_ref() {
            vault
                .rotate(subject, identity, rotated, Utc::now())
                .await
                .map_err(|_| configuration("hosted Spotify credential rotation failed"))?;
        }
        spotify::import_hosted_fresh(account_label, &database, observation_session).await?;
        let verified = apply::verify_pending_publications(&database, account_label, false).await?;
        if verified == 0 {
            return Err(configuration(
                "hosted post-apply observation did not verify the reviewed plan",
            ));
        }
        return apply::show(&database, account_label, Some(report.apply_run_id)).await;
    }
    Ok(report)
}

/// Provider-side work accepted by the durable hosted command boundary.
#[async_trait]
pub trait HostedProviderExecutor: Send + Sync {
    /// Reads and persists one complete provider observation.
    async fn observe(
        &self,
        subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
    ) -> std::result::Result<ResourceId, ClientError>;

    /// Starts a durable record-only maintenance interpretation.
    async fn start_maintenance(
        &self,
        _subject: AuthenticatedSubject,
        _operation_id: OperationId,
        _session_id: MaintenanceSessionId,
        _provider_connection_id: ResourceId,
    ) -> std::result::Result<ResourceId, ClientError> {
        Err(ClientError::new(ErrorCode::CapabilityUnavailable, false))
    }

    /// Rebases a durable session onto the newest already-observed state.
    async fn refresh_maintenance(
        &self,
        _subject: AuthenticatedSubject,
        _operation_id: OperationId,
        _session_id: MaintenanceSessionId,
        _expected_revision: u64,
    ) -> std::result::Result<ResourceId, ClientError> {
        Err(ClientError::new(ErrorCode::CapabilityUnavailable, false))
    }

    /// Persists explicit ambiguity decisions without provider effects.
    async fn resolve_maintenance(
        &self,
        _subject: AuthenticatedSubject,
        _operation_id: OperationId,
        _session_id: MaintenanceSessionId,
        _expected_revision: u64,
        _decisions: Vec<MaintenanceDecision>,
    ) -> std::result::Result<ResourceId, ClientError> {
        Err(ClientError::new(ErrorCode::CapabilityUnavailable, false))
    }

    /// Authorizes, executes, observes, and verifies one immutable review.
    async fn authorize_maintenance(
        &self,
        _subject: AuthenticatedSubject,
        _operation_id: OperationId,
        _session_id: MaintenanceSessionId,
        _expected_revision: u64,
        _review_id: MaintenanceReviewId,
    ) -> std::result::Result<ResourceId, ClientError> {
        Err(ClientError::new(ErrorCode::CapabilityUnavailable, false))
    }
}

/// Production Spotify/Neon executor using an encrypted credential vault.
pub struct SpotifyObservationExecutor {
    database: Database,
    vault: ProviderCredentialVault<PostgresProviderCredentialStore>,
    sessions: PostgresMaintenanceSessionStore,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReviewedAddition {
    track_id: uuid::Uuid,
    spotify_track_id: String,
    spotify_playlist_id: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ReviewedProviderWork {
    additions: Vec<ReviewedAddition>,
    saved_removals: Vec<(uuid::Uuid, String)>,
}

impl SpotifyObservationExecutor {
    /// Builds the executor after the caller verifies schemas 0049 through 0051.
    pub fn new(
        database: Database,
        vault: ProviderCredentialVault<PostgresProviderCredentialStore>,
    ) -> Self {
        let sessions = PostgresMaintenanceSessionStore::new(database.pool().clone());
        Self {
            database,
            vault,
            sessions,
        }
    }

    async fn hosted_mutation_session(
        &self,
        subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
    ) -> std::result::Result<spotify::MutationSession, ClientError> {
        let row = sqlx::query(
            "SELECT provider, provider_account_id FROM provider_accounts
              WHERE id = $1 AND chordrift_account_id = $2",
        )
        .bind(provider_connection_id.as_uuid())
        .bind(subject.account_id.as_uuid())
        .fetch_optional(self.database.pool())
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(|| ClientError::new(ErrorCode::PermissionDenied, false))?;
        let provider: String = row.try_get("provider").map_err(|_| unavailable())?;
        if provider != "spotify" {
            return Err(ClientError::new(ErrorCode::CapabilityUnavailable, false));
        }
        let stable_provider_id: String = row
            .try_get("provider_account_id")
            .map_err(|_| unavailable())?;
        let identity =
            ProviderCredentialIdentity::new(subject.account_id, provider_connection_id, provider)?;
        let lease = self.vault.lease(subject, &identity).await?;
        let (session, rotated) = match spotify::hosted_session(
            lease.refresh_token(),
            lease.scopes(),
            &stable_provider_id,
        )
        .await
        {
            Ok(value) => value,
            Err(error) if rejected_refresh_credential(&error) => {
                self.vault
                    .revoke(
                        subject,
                        &identity,
                        "provider rejected refresh credential",
                        Utc::now(),
                    )
                    .await?;
                return Err(ClientError::new(ErrorCode::AuthenticationRequired, false));
            }
            Err(error) => return Err(provider_error(error)),
        };
        if let Some(rotated) = rotated.as_ref() {
            self.vault
                .rotate(subject, identity, rotated, Utc::now())
                .await?;
        }
        spotify::hosted_mutation_session(session).map_err(provider_error)
    }

    async fn reviewed_provider_work(
        &self,
        provider_connection_id: ResourceId,
        view: &crate::contract::MaintenanceSessionView,
    ) -> std::result::Result<ReviewedProviderWork, ClientError> {
        let mut work = ReviewedProviderWork::default();
        for (kind, track_id, surface_name) in trusted_provider_effects(view)? {
            let spotify_id: String = sqlx::query_scalar(
                "SELECT provider_track_id FROM provider_tracks
                  WHERE provider = 'spotify' AND track_id = $1
                  ORDER BY id LIMIT 1",
            )
            .bind(track_id)
            .fetch_optional(self.database.pool())
            .await
            .map_err(|_| unavailable())?
            .ok_or_else(|| ClientError::new(ErrorCode::StateConflict, false))?;
            match kind {
                MaintenanceProviderEffectKind::AddTrack => {
                    let playlist_ids: Vec<String> = sqlx::query_scalar(
                        "SELECT spotify_playlist_id FROM current_spotify_playlists
                          WHERE provider_account_id = $1 AND lower(name) = lower($2)",
                    )
                    .bind(provider_connection_id.as_uuid())
                    .bind(&surface_name)
                    .fetch_all(self.database.pool())
                    .await
                    .map_err(|_| unavailable())?;
                    let [playlist_id] = playlist_ids.as_slice() else {
                        // A reviewed human name must resolve to exactly one
                        // current provider container. Missing or ambiguous
                        // destinations fail closed before any provider write.
                        return Err(ClientError::new(ErrorCode::StateConflict, false));
                    };
                    work.additions.push(ReviewedAddition {
                        track_id,
                        spotify_track_id: spotify_id,
                        spotify_playlist_id: playlist_id.clone(),
                    });
                }
                MaintenanceProviderEffectKind::UpdateSavedState => {
                    work.saved_removals.push((track_id, spotify_id));
                }
                _ => return Err(ClientError::new(ErrorCode::CapabilityUnavailable, false)),
            }
        }
        work.additions.sort_by(|left, right| {
            (&left.spotify_playlist_id, &left.spotify_track_id)
                .cmp(&(&right.spotify_playlist_id, &right.spotify_track_id))
        });
        work.additions.dedup_by(|left, right| {
            left.track_id == right.track_id && left.spotify_playlist_id == right.spotify_playlist_id
        });
        work.saved_removals
            .sort_by(|left, right| left.1.cmp(&right.1));
        work.saved_removals
            .dedup_by(|left, right| left.0 == right.0);
        Ok(work)
    }

    async fn playlist_addition_present(
        &self,
        provider_connection_id: ResourceId,
        addition: &ReviewedAddition,
    ) -> std::result::Result<bool, ClientError> {
        sqlx::query_scalar(
            "SELECT EXISTS (
               SELECT 1 FROM current_spotify_playlists playlist
               JOIN provider_observed_playlist_tracks membership
                 ON membership.snapshot_id = playlist.snapshot_id
                AND membership.provider_playlist_id = playlist.provider_playlist_id
               JOIN provider_tracks provider_track
                 ON provider_track.id = membership.provider_track_id
              WHERE playlist.provider_account_id = $1
                AND playlist.spotify_playlist_id = $2
                AND provider_track.track_id = $3)",
        )
        .bind(provider_connection_id.as_uuid())
        .bind(&addition.spotify_playlist_id)
        .bind(addition.track_id)
        .fetch_one(self.database.pool())
        .await
        .map_err(|_| unavailable())
    }

    async fn verify_playlist_additions(
        &self,
        provider_connection_id: ResourceId,
        additions: &[ReviewedAddition],
    ) -> std::result::Result<(), ClientError> {
        for addition in additions {
            if !self
                .playlist_addition_present(provider_connection_id, addition)
                .await?
            {
                return Err(ClientError::new(ErrorCode::StateConflict, true));
            }
        }
        Ok(())
    }

    async fn verify_saved_track_removals(
        &self,
        provider_connection_id: ResourceId,
        removals: &[(uuid::Uuid, String)],
    ) -> std::result::Result<(), ClientError> {
        for (track_id, _) in removals {
            let still_saved: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                   SELECT 1 FROM provider_current_inventories inventory
                   JOIN provider_saved_track_revision_tracks saved
                     ON saved.revision_id = inventory.saved_track_revision_id
                   JOIN provider_tracks provider_track
                     ON provider_track.id = saved.provider_track_id
                  WHERE inventory.provider_account_id = $1
                    AND provider_track.track_id = $2)",
            )
            .bind(provider_connection_id.as_uuid())
            .bind(track_id)
            .fetch_one(self.database.pool())
            .await
            .map_err(|_| unavailable())?;
            if still_saved {
                return Err(ClientError::new(ErrorCode::StateConflict, true));
            }
        }
        Ok(())
    }

    async fn current_provider_snapshot(
        &self,
        provider_connection_id: ResourceId,
    ) -> std::result::Result<ResourceId, ClientError> {
        let snapshot: uuid::Uuid = sqlx::query_scalar(
            "SELECT source_snapshot_id FROM provider_current_inventories
              WHERE provider_account_id = $1",
        )
        .bind(provider_connection_id.as_uuid())
        .fetch_optional(self.database.pool())
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(|| ClientError::new(ErrorCode::StateConflict, false))?;
        Ok(ResourceId::from_uuid(snapshot))
    }

    async fn operation_already_verified(
        &self,
        subject: AuthenticatedSubject,
        session_id: MaintenanceSessionId,
        operation_id: OperationId,
    ) -> std::result::Result<bool, ClientError> {
        sqlx::query_scalar(
            "SELECT EXISTS (
               SELECT 1 FROM maintenance_sessions session
               JOIN maintenance_session_events event
                 ON event.maintenance_session_id = session.id
              WHERE session.id = $1 AND session.chordrift_account_id = $2
                AND session.product_subject_id = $3
                AND event.source_operation_id = $4
                AND event.transition_name = 'verified')",
        )
        .bind(session_id.as_uuid())
        .bind(subject.account_id.as_uuid())
        .bind(subject.subject_id.as_uuid())
        .bind(operation_id.as_uuid())
        .fetch_one(self.database.pool())
        .await
        .map_err(|_| unavailable())
    }
}

fn trusted_provider_effects(
    view: &crate::contract::MaintenanceSessionView,
) -> std::result::Result<Vec<(MaintenanceProviderEffectKind, uuid::Uuid, String)>, ClientError> {
    let expected = maintenance_provider_effects(view.provider_snapshot_id, &view.observed_changes);
    if expected.provider_effects != view.provider_effects || expected.review_id != view.review_id {
        return Err(ClientError::new(ErrorCode::StateConflict, false));
    }
    let mut effects = Vec::new();
    for effect in &view.provider_effects {
        if !matches!(
            effect.kind,
            MaintenanceProviderEffectKind::AddTrack
                | MaintenanceProviderEffectKind::UpdateSavedState
        ) {
            return Err(ClientError::new(ErrorCode::CapabilityUnavailable, false));
        }
        let track = effect
            .track
            .as_ref()
            .ok_or_else(|| ClientError::new(ErrorCode::InvalidRequest, false))?;
        let surface = effect
            .surface
            .as_ref()
            .ok_or_else(|| ClientError::new(ErrorCode::InvalidRequest, false))?;
        effects.push((effect.kind, track.track_id.as_uuid(), surface.name.clone()));
    }
    Ok(effects)
}

#[async_trait]
impl HostedProviderExecutor for SpotifyObservationExecutor {
    async fn observe(
        &self,
        subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
    ) -> std::result::Result<ResourceId, ClientError> {
        let row = sqlx::query(
            "SELECT account.account_label, account.provider,
                    account.provider_account_id
               FROM provider_accounts account
              WHERE account.id = $1 AND account.chordrift_account_id = $2",
        )
        .bind(provider_connection_id.as_uuid())
        .bind(subject.account_id.as_uuid())
        .fetch_optional(self.database.pool())
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(|| ClientError::new(ErrorCode::PermissionDenied, false))?;
        let provider: String = row.try_get("provider").map_err(|_| unavailable())?;
        if provider != "spotify" {
            return Err(ClientError::new(ErrorCode::CapabilityUnavailable, false));
        }
        let account_label: String = row.try_get("account_label").map_err(|_| unavailable())?;
        let stable_provider_id: String = row
            .try_get("provider_account_id")
            .map_err(|_| unavailable())?;
        let identity =
            ProviderCredentialIdentity::new(subject.account_id, provider_connection_id, provider)?;
        let lease = self.vault.lease(subject, &identity).await?;
        let (session, rotated) = match spotify::hosted_session(
            lease.refresh_token(),
            lease.scopes(),
            &stable_provider_id,
        )
        .await
        {
            Ok(value) => value,
            Err(error) if rejected_refresh_credential(&error) => {
                self.vault
                    .revoke(
                        subject,
                        &identity,
                        "provider rejected refresh credential",
                        Utc::now(),
                    )
                    .await?;
                return Err(ClientError::new(ErrorCode::AuthenticationRequired, false));
            }
            Err(error) => return Err(provider_error(error)),
        };
        if let Some(rotated) = rotated.as_ref() {
            self.vault
                .rotate(subject, identity, rotated, Utc::now())
                .await?;
        }
        let report = spotify::import_hosted(&account_label, &self.database, session)
            .await
            .map_err(provider_error)?;
        Ok(ResourceId::from_uuid(report.snapshot_id))
    }

    async fn start_maintenance(
        &self,
        subject: AuthenticatedSubject,
        operation_id: OperationId,
        session_id: MaintenanceSessionId,
        provider_connection_id: ResourceId,
    ) -> std::result::Result<ResourceId, ClientError> {
        self.observe(subject, provider_connection_id).await?;
        let projection = attach_maintenance_provider_effects(
            PostgresMaintenanceInterpreter::new(&self.database)
                .project(subject, provider_connection_id)
                .await?,
        );
        let projected_view = MaintenanceWorkflow::new(session_id, projection.clone())
            .map_err(|error| error.client_error())?
            .view();
        CanonicalMaintenanceProjector::new(&self.database)
            .project(subject, provider_connection_id, &projected_view)
            .await?;
        DurableMaintenanceAuthority::new(self.sessions.clone())
            .start(
                subject,
                provider_connection_id,
                session_id,
                projection,
                Some(operation_id),
                Utc::now(),
            )
            .await?;
        Ok(ResourceId::from_uuid(session_id.as_uuid()))
    }

    async fn refresh_maintenance(
        &self,
        subject: AuthenticatedSubject,
        operation_id: OperationId,
        session_id: MaintenanceSessionId,
        expected_revision: u64,
    ) -> std::result::Result<ResourceId, ClientError> {
        let current = self.sessions.load(subject, session_id).await?;
        self.observe(subject, current.provider_connection_id)
            .await?;
        // A provider snapshot is only one input to the maintenance projection.
        // Canonical intent can change through another client or an approved
        // import while Spotify itself remains unchanged. Always rebuild the
        // projection so stale recorded decisions converge against both stores.
        let projection = attach_maintenance_provider_effects(
            PostgresMaintenanceInterpreter::new(&self.database)
                .project(subject, current.provider_connection_id)
                .await?,
        );
        let mut projected_workflow = MaintenanceWorkflow::from_view(current.view.clone())
            .map_err(|error| error.client_error())?;
        let projected_view = projected_workflow
            .rebase(expected_revision, projection.clone())
            .map_err(|error| error.client_error())?;
        CanonicalMaintenanceProjector::new(&self.database)
            .project(subject, current.provider_connection_id, &projected_view)
            .await?;
        DurableMaintenanceAuthority::new(self.sessions.clone())
            .refresh(
                subject,
                session_id,
                expected_revision,
                projection,
                Some(operation_id),
                Utc::now(),
            )
            .await?;
        Ok(ResourceId::from_uuid(session_id.as_uuid()))
    }

    async fn resolve_maintenance(
        &self,
        subject: AuthenticatedSubject,
        operation_id: OperationId,
        session_id: MaintenanceSessionId,
        expected_revision: u64,
        decisions: Vec<MaintenanceDecision>,
    ) -> std::result::Result<ResourceId, ClientError> {
        let current = self.sessions.load(subject, session_id).await?;
        let mut projected_workflow = MaintenanceWorkflow::from_view(current.view.clone())
            .map_err(|error| error.client_error())?;
        let preliminary_view = projected_workflow
            .resolve(
                expected_revision,
                decisions.clone(),
                MaintenanceDecisionProjection {
                    provider_effects: Vec::new(),
                    review_id: None,
                },
            )
            .map_err(|error| error.client_error())?;
        let decision_projection = maintenance_provider_effects(
            preliminary_view.provider_snapshot_id,
            &preliminary_view.observed_changes,
        );
        let mut projected_workflow = MaintenanceWorkflow::from_view(current.view.clone())
            .map_err(|error| error.client_error())?;
        let projected_view = projected_workflow
            .resolve(
                expected_revision,
                decisions.clone(),
                decision_projection.clone(),
            )
            .map_err(|error| error.client_error())?;
        CanonicalMaintenanceProjector::new(&self.database)
            .project(subject, current.provider_connection_id, &projected_view)
            .await?;
        DurableMaintenanceAuthority::new(self.sessions.clone())
            .resolve(
                subject,
                session_id,
                expected_revision,
                decisions,
                decision_projection,
                Some(operation_id),
                Utc::now(),
            )
            .await?;
        Ok(ResourceId::from_uuid(session_id.as_uuid()))
    }

    async fn authorize_maintenance(
        &self,
        subject: AuthenticatedSubject,
        operation_id: OperationId,
        session_id: MaintenanceSessionId,
        expected_revision: u64,
        review_id: MaintenanceReviewId,
    ) -> std::result::Result<ResourceId, ClientError> {
        let authority = DurableMaintenanceAuthority::new(self.sessions.clone());
        let mut current = self.sessions.load(subject, session_id).await?;
        if self
            .operation_already_verified(subject, session_id, operation_id)
            .await?
        {
            return Ok(ResourceId::from_uuid(session_id.as_uuid()));
        }
        if current.view.state == MaintenanceSessionState::ReadyForAuthorization {
            if self
                .current_provider_snapshot(current.provider_connection_id)
                .await?
                != current.view.provider_snapshot_id
            {
                return Err(ClientError::new(ErrorCode::StateConflict, false));
            }
            current.view = authority
                .authorize(
                    subject,
                    session_id,
                    expected_revision,
                    review_id,
                    Some(operation_id),
                    Utc::now(),
                )
                .await?;
        } else if current.view.review_id != Some(review_id) {
            return Err(ClientError::new(ErrorCode::StateConflict, false));
        }
        let work = self
            .reviewed_provider_work(current.provider_connection_id, &current.view)
            .await?;
        if work.additions.is_empty() && work.saved_removals.is_empty() {
            return Err(ClientError::new(ErrorCode::InvalidRequest, false));
        }
        if !work.additions.is_empty() && !work.saved_removals.is_empty() {
            // Intake cleanup is deliberately a later reviewed stage. Never
            // combine it with placement until placement has been observed.
            return Err(ClientError::new(ErrorCode::StateConflict, false));
        }
        if current.view.state == MaintenanceSessionState::Authorized {
            current.view = authority
                .mark_execution_state(
                    subject,
                    session_id,
                    current.view.revision,
                    MaintenanceSessionState::Applying,
                    Some(operation_id),
                    Utc::now(),
                )
                .await?;
        }
        if current.view.state == MaintenanceSessionState::Applying {
            // A fresh read makes operation replay idempotent after an
            // interruption between a provider write and durable completion.
            self.observe(subject, current.provider_connection_id)
                .await?;
            let session = self
                .hosted_mutation_session(subject, current.provider_connection_id)
                .await?;
            for addition in &work.additions {
                if !self
                    .playlist_addition_present(current.provider_connection_id, addition)
                    .await?
                {
                    session
                        .add_items(
                            &addition.spotify_playlist_id,
                            std::slice::from_ref(&addition.spotify_track_id),
                            match intake_placement_policy() {
                                IntakePlacementPolicy::Top => Some(0),
                            },
                        )
                        .await
                        .map_err(provider_error)?;
                }
            }
            for chunk in work.saved_removals.chunks(40) {
                let spotify_ids = chunk
                    .iter()
                    .map(|(_, spotify_id)| spotify_id.clone())
                    .collect::<Vec<_>>();
                session
                    .remove_library_tracks(&spotify_ids)
                    .await
                    .map_err(provider_error)?;
            }
            current.view = authority
                .mark_execution_state(
                    subject,
                    session_id,
                    current.view.revision,
                    MaintenanceSessionState::Verifying,
                    Some(operation_id),
                    Utc::now(),
                )
                .await?;
        }
        if current.view.state == MaintenanceSessionState::Verifying {
            self.observe(subject, current.provider_connection_id)
                .await?;
            self.verify_playlist_additions(current.provider_connection_id, &work.additions)
                .await?;
            self.verify_saved_track_removals(current.provider_connection_id, &work.saved_removals)
                .await?;
            let projection = attach_maintenance_provider_effects(
                PostgresMaintenanceInterpreter::new(&self.database)
                    .project(subject, current.provider_connection_id)
                    .await?,
            );
            let projected_view = MaintenanceWorkflow::new(session_id, projection.clone())
                .map_err(|error| error.client_error())?
                .view();
            CanonicalMaintenanceProjector::new(&self.database)
                .project(subject, current.provider_connection_id, &projected_view)
                .await?;
            authority
                .complete_verification(
                    subject,
                    session_id,
                    current.view.revision,
                    projection,
                    Some(operation_id),
                    Utc::now(),
                )
                .await?;
        }
        Ok(ResourceId::from_uuid(session_id.as_uuid()))
    }
}

/// Runs the separate provider worker until process shutdown.
pub async fn run_from_env() -> Result<()> {
    let worker_name = required("CHORDRIFT_WORKER_NAME")?;
    let database = db::connect(config::database_config_from_env()?).await?;
    db::require_schema_through(&database, 51).await?;
    let pool = database.pool().clone();
    let credential_store = PostgresProviderCredentialStore::new(pool.clone());
    credential_store
        .verify_schema()
        .await
        .map_err(|_| configuration("hosted provider credential schema is not ready"))?;
    let keyring = ProviderVaultKeyring::from_environment()
        .map_err(|_| configuration("hosted provider credential key is not ready"))?;
    let operation_store = Arc::new(PostgresDurableOperationStore::new(pool));
    operation_store
        .verify_schema()
        .await
        .map_err(|_| configuration("durable operation schema is not ready"))?;
    PostgresMaintenanceSessionStore::new(database.pool().clone())
        .verify_schema()
        .await
        .map_err(|_| configuration("durable maintenance schema is not ready"))?;
    let queue = DurableOperationQueue::new(operation_store);
    let executor = SpotifyObservationExecutor::new(
        database,
        ProviderCredentialVault::new(credential_store, keyring),
    );
    let mut shutdown = Box::pin(shutdown_signal());
    loop {
        tokio::select! {
            _ = &mut shutdown => return Ok(()),
            outcome = run_once(&queue, &executor, &worker_name) => {
                if outcome.map_err(|_| configuration("durable worker queue is unavailable"))? {
                    continue;
                }
                tokio::time::sleep(Duration::from_millis(DEFAULT_POLL_MILLISECONDS)).await;
            }
        }
    }
}

/// Claims and executes at most one durable command. Returns whether work was claimed.
pub async fn run_once<S, E>(
    queue: &DurableOperationQueue<S>,
    executor: &E,
    worker_name: &str,
) -> std::result::Result<bool, ClientError>
where
    S: crate::durable_operations::DurableOperationStore,
    E: HostedProviderExecutor,
{
    let Some(lease) = queue.claim_next(worker_name, LEASE_DURATION).await? else {
        return Ok(false);
    };
    worker_log("claimed", worker_name, &lease, None);
    if queue.cancellation_requested(&lease).await? {
        queue.acknowledge_cancellation(&lease).await?;
        worker_log("cancelled", worker_name, &lease, None);
        return Ok(true);
    }
    queue
        .record_progress(
            &lease,
            Progress::new(
                progress_phase(&lease.request.command),
                0,
                Some(1),
                ProgressUnit::Steps,
            )
            .expect("static worker progress is valid"),
        )
        .await?;
    let mut work = Box::pin(dispatch(executor, &lease));
    let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
    heartbeat.tick().await;
    let outcome = loop {
        tokio::select! {
            result = &mut work => break result,
            _ = heartbeat.tick() => {
                if queue.cancellation_requested(&lease).await? {
                    drop(work);
                    queue.acknowledge_cancellation(&lease).await?;
                    worker_log("cancelled", worker_name, &lease, None);
                    return Ok(true);
                }
                queue.renew_lease(&lease, LEASE_DURATION).await?;
                worker_log("lease_renewed", worker_name, &lease, None);
            }
        }
    };
    match outcome {
        Ok(result_id) => {
            queue.complete(&lease, Some(result_id)).await?;
            worker_log("completed", worker_name, &lease, None);
        }
        Err(error) => {
            worker_log("failed", worker_name, &lease, Some(error.code));
            queue.fail(&lease, error).await?;
        }
    }
    Ok(true)
}

fn worker_log(
    event: &str,
    worker_name: &str,
    lease: &DurableOperationLease,
    error_code: Option<ErrorCode>,
) {
    eprintln!(
        "{}",
        serde_json::json!({
            "event": format!("worker_{event}"),
            "worker": worker_name,
            "request_id": lease.request.request_id,
            "operation_id": lease.operation_id,
            "phase": progress_phase(&lease.request.command),
            "attempt": lease.attempt,
            "max_attempts": lease.max_attempts,
            "error_code": error_code,
        })
    );
}

async fn dispatch<E: HostedProviderExecutor>(
    executor: &E,
    lease: &DurableOperationLease,
) -> std::result::Result<ResourceId, ClientError> {
    match &lease.request.command {
        Command::ObserveProvider {
            provider_connection_id,
        } => {
            executor
                .observe(lease.subject, *provider_connection_id)
                .await
        }
        Command::StartMaintenance {
            session_id,
            provider_connection_id,
        } => {
            executor
                .start_maintenance(
                    lease.subject,
                    lease.operation_id,
                    *session_id,
                    *provider_connection_id,
                )
                .await
        }
        Command::RefreshMaintenance {
            session_id,
            expected_revision,
        } => {
            executor
                .refresh_maintenance(
                    lease.subject,
                    lease.operation_id,
                    *session_id,
                    *expected_revision,
                )
                .await
        }
        Command::ResolveMaintenance {
            session_id,
            expected_revision,
            decisions,
        } => {
            executor
                .resolve_maintenance(
                    lease.subject,
                    lease.operation_id,
                    *session_id,
                    *expected_revision,
                    decisions.clone(),
                )
                .await
        }
        Command::AuthorizeMaintenance {
            session_id,
            expected_revision,
            review_id,
        } => {
            executor
                .authorize_maintenance(
                    lease.subject,
                    lease.operation_id,
                    *session_id,
                    *expected_revision,
                    *review_id,
                )
                .await
        }
        _ => Err(ClientError::new(ErrorCode::InvalidRequest, false)),
    }
}

fn progress_phase(command: &Command) -> &'static str {
    match command {
        Command::ObserveProvider { .. } => "observe_provider",
        Command::StartMaintenance { .. } => "start_maintenance",
        Command::RefreshMaintenance { .. } => "refresh_maintenance",
        Command::ResolveMaintenance { .. } => "resolve_maintenance",
        Command::AuthorizeMaintenance { .. } => "authorize_maintenance",
        _ => "unsupported",
    }
}

async fn shutdown_signal() {
    let interrupt = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install termination handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = interrupt => {}, _ = terminate => {} }
}

fn provider_error(error: ChordriftError) -> ClientError {
    match error {
        ChordriftError::Configuration(_) => ClientError::new(ErrorCode::StateConflict, false),
        _ => unavailable(),
    }
}

fn rejected_refresh_credential(error: &ChordriftError) -> bool {
    let ChordriftError::SpotifyApi { status, message } = error else {
        return false;
    };
    if !matches!(status, 400 | 401) {
        return false;
    }
    let message = message.to_ascii_lowercase();
    message.contains("invalid_grant")
        || message.contains("refresh token")
        || message.contains("revoked")
}

fn unavailable() -> ClientError {
    ClientError::new(ErrorCode::DependencyUnavailable, true)
}

fn configuration(message: &str) -> ChordriftError {
    ChordriftError::Configuration(message.to_owned())
}

fn required(name: &str) -> Result<String> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| configuration(&format!("{name} is required")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::CancellationId;
    use crate::contract::{
        CONTRACT_VERSION, CommandRequest, IdempotencyKey, OperationId, RequestId,
    };

    #[test]
    fn only_terminal_refresh_rejections_expire_the_vault_credential() {
        assert!(rejected_refresh_credential(&ChordriftError::SpotifyApi {
            status: 400,
            message: "invalid_grant".to_owned(),
        }));
        assert!(rejected_refresh_credential(&ChordriftError::SpotifyApi {
            status: 400,
            message: "Refresh token revoked".to_owned(),
        }));
        assert!(!rejected_refresh_credential(&ChordriftError::SpotifyApi {
            status: 429,
            message: "rate limited".to_owned(),
        }));
        assert!(!rejected_refresh_credential(
            &ChordriftError::Configuration("not a provider rejection".to_owned(),)
        ));
    }
    use crate::durable_operations::DurableOperationLease;
    use crate::{
        contract::{
            MaintenanceChangeId, MaintenanceChangeKind, MaintenanceChangeView,
            MaintenanceResolution, MaintenanceSessionView, MaintenanceSurfaceView,
            MaintenanceTrackView,
        },
        maintenance_projection::maintenance_provider_effects,
    };

    struct FakeExecutor;

    #[async_trait]
    impl HostedProviderExecutor for FakeExecutor {
        async fn observe(
            &self,
            _subject: AuthenticatedSubject,
            provider_connection_id: ResourceId,
        ) -> std::result::Result<ResourceId, ClientError> {
            Ok(provider_connection_id)
        }

        async fn start_maintenance(
            &self,
            _subject: AuthenticatedSubject,
            _operation_id: OperationId,
            session_id: MaintenanceSessionId,
            _provider_connection_id: ResourceId,
        ) -> std::result::Result<ResourceId, ClientError> {
            Ok(ResourceId::from_uuid(session_id.as_uuid()))
        }

        async fn refresh_maintenance(
            &self,
            _subject: AuthenticatedSubject,
            _operation_id: OperationId,
            session_id: MaintenanceSessionId,
            _expected_revision: u64,
        ) -> std::result::Result<ResourceId, ClientError> {
            Ok(ResourceId::from_uuid(session_id.as_uuid()))
        }

        async fn resolve_maintenance(
            &self,
            _subject: AuthenticatedSubject,
            _operation_id: OperationId,
            session_id: MaintenanceSessionId,
            _expected_revision: u64,
            _decisions: Vec<MaintenanceDecision>,
        ) -> std::result::Result<ResourceId, ClientError> {
            Ok(ResourceId::from_uuid(session_id.as_uuid()))
        }

        async fn authorize_maintenance(
            &self,
            _subject: AuthenticatedSubject,
            _operation_id: OperationId,
            session_id: MaintenanceSessionId,
            _expected_revision: u64,
            _review_id: MaintenanceReviewId,
        ) -> std::result::Result<ResourceId, ClientError> {
            Ok(ResourceId::from_uuid(session_id.as_uuid()))
        }
    }

    fn lease(command: Command) -> DurableOperationLease {
        DurableOperationLease {
            operation_id: OperationId::new(),
            lease_id: ResourceId::new(),
            subject: AuthenticatedSubject {
                subject_id: ResourceId::new(),
                account_id: ResourceId::new(),
            },
            request: CommandRequest {
                contract_version: CONTRACT_VERSION,
                request_id: RequestId::new(),
                idempotency_key: IdempotencyKey::new(),
                command,
            },
            attempt: 1,
            max_attempts: 3,
            lease_expires_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn worker_dispatches_only_typed_observation() {
        let provider_connection_id = ResourceId::new();
        let result = dispatch(
            &FakeExecutor,
            &lease(Command::ObserveProvider {
                provider_connection_id,
            }),
        )
        .await
        .expect("typed observation is accepted");
        assert_eq!(result, provider_connection_id);

        let cancellation = Command::CancelOperation(crate::contract::CancellationRequest {
            operation_id: OperationId::new(),
            cancellation_id: CancellationId::new(),
        });
        assert_eq!(
            dispatch(&FakeExecutor, &lease(cancellation))
                .await
                .expect_err("worker rejects unsupported command")
                .code,
            ErrorCode::InvalidRequest
        );
    }

    #[tokio::test]
    async fn worker_dispatches_typed_maintenance_commands() {
        let session_id = MaintenanceSessionId::new();
        for command in [
            Command::StartMaintenance {
                session_id,
                provider_connection_id: ResourceId::new(),
            },
            Command::RefreshMaintenance {
                session_id,
                expected_revision: 1,
            },
            Command::ResolveMaintenance {
                session_id,
                expected_revision: 1,
                decisions: Vec::new(),
            },
            Command::AuthorizeMaintenance {
                session_id,
                expected_revision: 2,
                review_id: MaintenanceReviewId::new(),
            },
        ] {
            assert_eq!(
                dispatch(&FakeExecutor, &lease(command)).await.unwrap(),
                ResourceId::from_uuid(session_id.as_uuid())
            );
        }
    }

    #[test]
    fn provider_execution_accepts_only_the_server_rederived_exact_review() {
        let snapshot = ResourceId::new();
        let track = MaintenanceTrackView {
            track_id: ResourceId::new(),
            title: "Saved fixture".to_owned(),
            artists: Vec::new(),
        };
        let liked = MaintenanceSurfaceView {
            surface_id: ResourceId::new(),
            name: "Liked Songs".to_owned(),
        };
        let changes = vec![MaintenanceChangeView {
            change_id: MaintenanceChangeId::new(),
            kind: MaintenanceChangeKind::SavedState,
            track: Some(track.clone()),
            previous_surface: None,
            current_surface: Some(liked.clone()),
            summary: "Choose saved state".to_owned(),
            resolution: Some(MaintenanceResolution::ConsumeIntake { source: liked }),
            recommended_resolution: None,
            recommendation_reason: None,
        }];
        let exact = maintenance_provider_effects(snapshot, &changes);
        let mut view = MaintenanceSessionView {
            session_id: MaintenanceSessionId::new(),
            revision: 2,
            provider_snapshot_id: snapshot,
            state: MaintenanceSessionState::ReadyForAuthorization,
            observed_changes: changes,
            provider_effects: exact.provider_effects,
            review_id: exact.review_id,
            allowed_actions: vec![crate::contract::MaintenanceAllowedAction::Authorize],
        };
        assert_eq!(
            trusted_provider_effects(&view).unwrap(),
            vec![(
                MaintenanceProviderEffectKind::UpdateSavedState,
                track.track_id.as_uuid(),
                "Liked Songs".to_owned()
            )]
        );
        view.provider_effects[0]
            .summary
            .push_str(" plus anything else");
        assert_eq!(
            trusted_provider_effects(&view).unwrap_err().code,
            ErrorCode::StateConflict
        );
    }
}
