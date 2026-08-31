use std::{cell::Cell, collections::BTreeMap, future, num::NonZeroU16, sync::Arc, time::Duration};

use chordrift::{
    application::ApplicationFacade,
    apply, config,
    contract::{
        CONTRACT_VERSION, CancellationOutcome, CancellationRequest, ClientError, Command,
        CommandRequest, ErrorCode, IdempotencyKey, MaintenanceChangeId, MaintenanceChangeKind,
        MaintenanceChangeView, MaintenanceProviderEffectKind, MaintenanceProviderEffectView,
        MaintenanceResolution, MaintenanceReviewId, MaintenanceSessionId, MaintenanceSessionState,
        MaintenanceSurfaceView, MaintenanceTrackView, OperationState, Progress, ProgressUnit,
        Query, QueryRequest, RequestId, ResourceId,
    },
    db, db_reports,
    domain::{
        AccountContext, AccountOwnedId, AllocationWeight, CanonicalArtistId, CanonicalTrackId,
        CapabilityStatus, ChordriftAccountId, CollectionId, EvidenceCapabilities,
        EvidenceCapability, FamiliarityCadence, GuardrailKind, OrderingNarrative,
        ProviderAccountId, ProviderCapabilities, ProviderCapability, ProviderConnectionId,
        ProviderConnectionIdentity, ProviderNamespace, RecipeId, RecipeRevisionId,
        RecipeRevisionIdentity, RecipeSection, RecipeSource, RecipeV1, SourceAllocation,
        SourceLane,
    },
    durable_operations::{
        DurableOperationQueue, OperationRetryPolicy, PostgresDurableOperationStore,
    },
    identity::{
        NewProductSession, PostgresProductIdentityStore, ProductIdentityStore,
        VerifiedExternalIdentity,
    },
    intake::{self, IntakeState},
    maintenance::{MaintenanceProjection, MaintenanceWorkflow},
    maintenance_projection::CanonicalMaintenanceProjector,
    maintenance_store::{
        DurableMaintenanceAuthority, MaintenanceTransition, PostgresMaintenanceSessionStore,
    },
    onboarding::{
        ContentFingerprint, OnboardingEvidence, OnboardingInputs, OnboardingInventory,
        OnboardingProviderReader, OnboardingReadSelection, OnboardingSessionBoundary,
    },
    onboarding_audit::{
        AuditEvidenceBasis, AuditLimitation, EnrichedAuditBoundary, EnrichedAuditEvidenceBasis,
        EnrichedAuditLimitation, InventoryOnlyAuditBoundary, StarterCollectionBasis,
        StarterProposalConfidence, StrengthenedConclusionKind, inventory_findings_fingerprint,
    },
    product_rehearsal::{CollectionReviewBoundary, RecipeReviewBoundary},
    proposals,
    provider_vault::{
        PostgresProviderCredentialStore, ProviderCredentialIdentity, ProviderCredentialVault,
        ProviderRefreshCredential, ProviderVaultKeyring,
    },
    recipe_execution::{
        CandidateEligibility, RecipeCandidate, RecipeExecutionRequest, RecipeExecutor,
        SelectionBudgets,
    },
    service::{AuthenticatedSubject, ServiceClock},
    spin_preview::{SpinPreviewBoundary, SpinPreviewInput},
    spin_publication::{SpinPublicationBoundary, SpinPublicationRequest},
    sync_plan::{self, PlanOrigin},
    tracks as track_ops,
};
use chrono::{DateTime, TimeDelta, Utc};
use serde_json::json;
use storexa::{Database, DatabaseConfig, PostgresProvider};
use uuid::Uuid;

struct FakeOnboardingReader {
    connection: ProviderConnectionIdentity,
    checkpoint_id: Uuid,
    reads: Cell<u8>,
}

impl OnboardingProviderReader for FakeOnboardingReader {
    fn read_onboarding_inputs(
        &self,
        context: &AccountContext,
        selection: OnboardingReadSelection,
    ) -> impl Future<Output = Result<OnboardingInputs, chordrift::contract::ClientError>> {
        let result = if context.provider_connection() != &self.connection {
            Err(chordrift::contract::ClientError::new(
                chordrift::contract::ErrorCode::PermissionDenied,
                false,
            ))
        } else {
            self.reads.set(self.reads.get() + 1);
            let evidence = if selection.include_extended_history {
                vec![OnboardingEvidence {
                    capability: EvidenceCapability::ExtendedPlaybackHistory,
                    content_fingerprint: ContentFingerprint::new("e".repeat(64))
                        .expect("fixture fingerprint is valid"),
                    record_count: 7,
                }]
            } else {
                Vec::new()
            };
            Ok(OnboardingInputs::new(
                OnboardingInventory {
                    checkpoint_id: ResourceId::from_uuid(self.checkpoint_id),
                    state_fingerprint: ContentFingerprint::new("d".repeat(64))
                        .expect("fixture fingerprint is valid"),
                    item_count: 4,
                },
                evidence,
            )
            .expect("fixture evidence is canonical"))
        };
        future::ready(result)
    }
}

fn onboarding_request(account_id: Uuid, include_extended_history: bool) -> CommandRequest {
    CommandRequest {
        contract_version: CONTRACT_VERSION,
        request_id: Default::default(),
        idempotency_key: IdempotencyKey::new(),
        command: Command::CreateOnboardingSession {
            account_id: ResourceId::from_uuid(account_id),
            include_extended_history,
        },
    }
}

fn onboarding_audit_request(session_id: Uuid) -> QueryRequest {
    QueryRequest {
        contract_version: CONTRACT_VERSION,
        request_id: Default::default(),
        query: Query::OnboardingAudit {
            session_id: ResourceId::from_uuid(session_id),
        },
    }
}

async fn seed_inventory_checkpoint(
    database: &Database,
    provider_account_id: Uuid,
) -> chordrift::Result<Uuid> {
    let source_snapshot_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_library_snapshots
         (provider, source, provider_account_id)
         VALUES ('spotify', 'v02012-fixture', $1) RETURNING id",
    )
    .bind(provider_account_id)
    .fetch_one(database.pool())
    .await?;
    let mut provider_track_ids = Vec::new();
    for (provider_track_id, title) in [
        ("track-a", "Track A"),
        ("track-b", "Track B"),
        ("track-c", "Track C"),
        ("track-d", "Track D"),
    ] {
        let track_id: Uuid = sqlx::query_scalar(
            "INSERT INTO tracks (title, normalized_title) VALUES ($1, lower($1)) RETURNING id",
        )
        .bind(title)
        .fetch_one(database.pool())
        .await?;
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO provider_tracks (track_id, provider, provider_track_id)
             VALUES ($1, 'spotify', $2) RETURNING id",
        )
        .bind(track_id)
        .bind(provider_track_id)
        .fetch_one(database.pool())
        .await?;
        provider_track_ids.push(id);
    }

    let mut provider_playlists = Vec::new();
    for (index, name, reported, membership) in [
        (0_u8, "Alpha", 3_i32, vec![0_usize, 1, 1]),
        (1_u8, "Beta", 3_i32, vec![1_usize, 2]),
    ] {
        let playlist_id: Uuid = sqlx::query_scalar(
            "INSERT INTO playlists (name, kind) VALUES ($1, 'historical') RETURNING id",
        )
        .bind(name)
        .fetch_one(database.pool())
        .await?;
        let provider_playlist_id: Uuid = sqlx::query_scalar(
            "INSERT INTO provider_playlists
             (playlist_id, provider, provider_playlist_id)
             VALUES ($1, 'spotify', $2) RETURNING id",
        )
        .bind(playlist_id)
        .bind(format!("playlist-{index}"))
        .fetch_one(database.pool())
        .await?;
        sqlx::query(
            "INSERT INTO provider_account_playlists
             (provider_account_id, provider_playlist_id)
             VALUES ($1, $2)",
        )
        .bind(provider_account_id)
        .bind(provider_playlist_id)
        .execute(database.pool())
        .await?;
        let revision_id: Uuid = sqlx::query_scalar(
            "INSERT INTO provider_playlist_revisions
             (provider_playlist_id, content_sha256, item_count)
             VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(provider_playlist_id)
        .bind(if index == 0 {
            "1".repeat(64)
        } else {
            "2".repeat(64)
        })
        .bind(reported)
        .fetch_one(database.pool())
        .await?;
        for (position, track_index) in membership.into_iter().enumerate() {
            sqlx::query(
                "INSERT INTO provider_playlist_revision_tracks
                 (revision_id, provider_track_id, position)
                 VALUES ($1, $2, $3)",
            )
            .bind(revision_id)
            .bind(provider_track_ids[track_index])
            .bind(i32::try_from(position).expect("fixture position fits i32"))
            .execute(database.pool())
            .await?;
        }
        provider_playlists.push((provider_playlist_id, revision_id, name));
    }

    let saved_track_revision_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_saved_track_revisions
         (provider_account_id, content_sha256, item_count)
         VALUES ($1, $2, 2) RETURNING id",
    )
    .bind(provider_account_id)
    .bind("3".repeat(64))
    .fetch_one(database.pool())
    .await?;
    for (position, track_index) in [0_usize, 3].into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO provider_saved_track_revision_tracks
             (revision_id, provider_track_id, position)
             VALUES ($1, $2, $3)",
        )
        .bind(saved_track_revision_id)
        .bind(provider_track_ids[track_index])
        .bind(i32::try_from(position).expect("fixture position fits i32"))
        .execute(database.pool())
        .await?;
    }
    let saved_album_revision_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_saved_album_revisions
         (provider_account_id, content_sha256, album_count, track_count)
         VALUES ($1, $2, 0, 0) RETURNING id",
    )
    .bind(provider_account_id)
    .bind("4".repeat(64))
    .fetch_one(database.pool())
    .await?;

    let checkpoint_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_inventory_checkpoints
         (provider_account_id, provider, checkpoint_kind, label, state_sha256,
          source_snapshot_id, captured_at)
         VALUES ($1, 'spotify', 'named_baseline', 'V020-07 fixture', $2, $3, now())
         RETURNING id",
    )
    .bind(provider_account_id)
    .bind("d".repeat(64))
    .bind(source_snapshot_id)
    .fetch_one(database.pool())
    .await?;
    for (provider_playlist_id, revision_id, name) in provider_playlists {
        sqlx::query(
            "INSERT INTO provider_inventory_checkpoint_playlists
             (checkpoint_id, provider_playlist_id, revision_id, name,
              public, collaborative)
             VALUES ($1, $2, $3, $4, FALSE, FALSE)",
        )
        .bind(checkpoint_id)
        .bind(provider_playlist_id)
        .bind(revision_id)
        .bind(name)
        .execute(database.pool())
        .await?;
    }
    sqlx::query(
        "INSERT INTO provider_inventory_checkpoint_saved_surfaces
         (checkpoint_id, saved_track_revision_id, saved_album_revision_id)
         VALUES ($1, $2, $3)",
    )
    .bind(checkpoint_id)
    .bind(saved_track_revision_id)
    .bind(saved_album_revision_id)
    .execute(database.pool())
    .await?;
    Ok(checkpoint_id)
}

async fn seed_extended_history(
    database: &Database,
    provider_account_id: Uuid,
) -> chordrift::Result<()> {
    let import_id: Uuid = sqlx::query_scalar(
        "INSERT INTO listening_evidence_imports
         (provider_account_id, provider, archive_kind, archive_sha256,
          parser_version, source_filename, source_file_count, event_count,
          first_event_at, last_event_at)
         VALUES ($1, 'spotify', 'extended_streaming_history', $2,
                 'v02008-fixture', 'fixture.zip', 1, 7,
                 TIMESTAMPTZ '2019-01-01 00:00:00Z',
                 TIMESTAMPTZ '2022-01-01 00:00:00Z')
         RETURNING id",
    )
    .bind(provider_account_id)
    .bind("e".repeat(64))
    .fetch_one(database.pool())
    .await?;
    let mut identities = Vec::new();
    for provider_track_id in ["track-a", "track-b", "track-d", "track-z"] {
        let identity_id: Uuid = sqlx::query_scalar(
            "INSERT INTO historical_provider_track_identities
             (provider, provider_track_id, track_name, artist_name,
              first_observed_at, last_observed_at)
             VALUES ('spotify', $1, $1, 'Fixture Artist',
                     TIMESTAMPTZ '2019-01-01 00:00:00Z',
                     TIMESTAMPTZ '2022-01-01 00:00:00Z')
             RETURNING id",
        )
        .bind(provider_track_id)
        .fetch_one(database.pool())
        .await?;
        identities.push(identity_id);
    }
    for (identity, played_at, completed, skipped) in [
        (0_usize, "2020-01-01T00:00:00Z", Some(true), Some(false)),
        (0, "2020-06-01T00:00:00Z", Some(true), Some(false)),
        (0, "2021-02-01T00:00:00Z", Some(true), Some(false)),
        (1, "2021-03-01T00:00:00Z", Some(false), Some(true)),
        (2, "2022-01-01T00:00:00Z", Some(true), Some(false)),
        (3, "2019-01-01T00:00:00Z", None, None),
        (3, "2019-02-01T00:00:00Z", Some(false), Some(false)),
    ] {
        sqlx::query(
            "INSERT INTO normalized_listening_events
             (id, provider_account_id, historical_identity_id, source_import_id,
              source_kind, played_at, ms_played, completed, skipped)
             VALUES ($1, $2, $3, $4, 'archive', $5::timestamptz, 180000, $6, $7)",
        )
        .bind(Uuid::new_v4())
        .bind(provider_account_id)
        .bind(identities[identity])
        .bind(import_id)
        .bind(played_at)
        .bind(completed)
        .bind(skipped)
        .execute(database.pool())
        .await?;
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires CHORDRIFT_TEST_DATABASE_URL for a disposable PostgreSQL database"]
async fn audits_current_intake_without_mutation() -> chordrift::Result<()> {
    let config = DatabaseConfig::from_env_var("CHORDRIFT_TEST_DATABASE_URL")?
        .with_name("chordrift-intake-audit-test")?
        .with_provider(PostgresProvider::Neon)?
        .with_min_connections(0)
        .with_max_connections(2);
    let database = db::connect(config).await?;
    db::migrate(&database).await?;

    let suffix = Uuid::new_v4().simple().to_string();
    let account_label = format!("intake-{suffix}");
    let account_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_accounts
         (provider, provider_account_id, account_label)
         VALUES ('spotify', $1, $2) RETURNING id",
    )
    .bind(format!("provider-{suffix}"))
    .bind(&account_label)
    .fetch_one(database.pool())
    .await?;
    let chordrift_account_id: Uuid =
        sqlx::query_scalar("SELECT chordrift_account_id FROM provider_accounts WHERE id = $1")
            .bind(account_id)
            .fetch_one(database.pool())
            .await?;
    let snapshot_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_library_snapshots
         (provider, source, provider_account_id)
         VALUES ('spotify', 'intake-audit-test', $1) RETURNING id",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;

    let mut tracks = Vec::new();
    for (index, title) in [
        "Already Covered",
        "Previously Excluded",
        "Known From History",
        "Genuinely New",
    ]
    .into_iter()
    .enumerate()
    {
        let track_id: Uuid = sqlx::query_scalar(
            "INSERT INTO tracks (title, normalized_title)
             VALUES ($1, lower($1)) RETURNING id",
        )
        .bind(title)
        .fetch_one(database.pool())
        .await?;
        let provider_track_id: Uuid = sqlx::query_scalar(
            "INSERT INTO provider_tracks (track_id, provider, provider_track_id)
             VALUES ($1, 'spotify', $2) RETURNING id",
        )
        .bind(track_id)
        .bind(format!("intake-track-{index}-{suffix}"))
        .fetch_one(database.pool())
        .await?;
        tracks.push((track_id, provider_track_id));
    }

    let saved_track_revision_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_saved_track_revisions
         (provider_account_id, content_sha256, item_count)
         VALUES ($1, $2, 4) RETURNING id",
    )
    .bind(account_id)
    .bind("a".repeat(64))
    .fetch_one(database.pool())
    .await?;
    for (position, (_, provider_track_id)) in tracks.iter().enumerate() {
        sqlx::query(
            "INSERT INTO provider_saved_track_revision_tracks
             (revision_id, provider_track_id, position)
             VALUES ($1, $2, $3)",
        )
        .bind(saved_track_revision_id)
        .bind(provider_track_id)
        .bind(i32::try_from(position).expect("fixture position fits i32"))
        .execute(database.pool())
        .await?;
    }
    let saved_album_revision_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_saved_album_revisions
         (provider_account_id, content_sha256, album_count, track_count)
         VALUES ($1, $2, 0, 0) RETURNING id",
    )
    .bind(account_id)
    .bind("b".repeat(64))
    .fetch_one(database.pool())
    .await?;
    sqlx::query(
        "INSERT INTO provider_current_inventories
         (provider_account_id, provider, source_snapshot_id,
          saved_track_revision_id, saved_album_revision_id,
          state_sha256, captured_at)
         VALUES ($1, 'spotify', $2, $3, $4, $5, now())",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .bind(saved_track_revision_id)
    .bind(saved_album_revision_id)
    .bind("c".repeat(64))
    .execute(database.pool())
    .await?;

    let playlist_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlists (name, kind) VALUES ('Fixture Canonical', 'historical')
         RETURNING id",
    )
    .fetch_one(database.pool())
    .await?;
    let provider_playlist_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_playlists (playlist_id, provider, provider_playlist_id)
         VALUES ($1, 'spotify', $2) RETURNING id",
    )
    .bind(playlist_id)
    .bind(format!("intake-playlist-{suffix}"))
    .fetch_one(database.pool())
    .await?;
    let playlist_revision_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_playlist_revisions
         (provider_playlist_id, content_sha256, item_count)
         VALUES ($1, $2, 1) RETURNING id",
    )
    .bind(provider_playlist_id)
    .bind("d".repeat(64))
    .fetch_one(database.pool())
    .await?;
    sqlx::query(
        "INSERT INTO provider_playlist_revision_tracks
         (revision_id, provider_track_id, position) VALUES ($1, $2, 0)",
    )
    .bind(playlist_revision_id)
    .bind(tracks[0].1)
    .execute(database.pool())
    .await?;
    sqlx::query(
        "INSERT INTO provider_current_playlists
         (provider_account_id, provider_playlist_id, revision_id, name,
          collaborative, reported_item_count)
         VALUES ($1, $2, $3, 'Fixture Canonical', FALSE, 1)",
    )
    .bind(account_id)
    .bind(provider_playlist_id)
    .bind(playlist_revision_id)
    .execute(database.pool())
    .await?;
    sqlx::query(
        "INSERT INTO provider_account_playlists
         (provider_account_id, provider_playlist_id, role, drift_policy,
          present_in_latest_snapshot, semantic_weight, signal_class)
         VALUES ($1, $2, 'managed', 'neon_wins', TRUE, 0.0, 'canonical')",
    )
    .bind(account_id)
    .bind(provider_playlist_id)
    .execute(database.pool())
    .await?;

    sqlx::query(
        "INSERT INTO excluded_tracks
         (provider_account_id, track_id, source_provider, excluded_at,
          exclusion_reason)
         VALUES ($1, $2, 'user_explicit', now(), 'fixture exclusion')",
    )
    .bind(account_id)
    .bind(tracks[1].0)
    .execute(database.pool())
    .await?;
    sqlx::query(
        "INSERT INTO account_listening_track_statistics
         (provider_account_id, provider_track_id, track_id, event_count,
          play_count, total_ms_played, average_ms_played, skip_count,
          completed_count, first_played_at, last_played_at)
         VALUES ($1, $2, $3, 7, 3, 21000, 3000, 1, 2, now(), now())",
    )
    .bind(account_id)
    .bind(format!("history-{suffix}"))
    .bind(tracks[2].0)
    .execute(database.pool())
    .await?;

    let before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM provider_saved_track_revision_tracks
         WHERE revision_id = $1",
    )
    .bind(saved_track_revision_id)
    .fetch_one(database.pool())
    .await?;
    let audit = intake::audit(&database, &account_label).await?;
    let after: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM provider_saved_track_revision_tracks
         WHERE revision_id = $1",
    )
    .bind(saved_track_revision_id)
    .fetch_one(database.pool())
    .await?;
    assert_eq!(before, after, "intake audit must be read-only");
    assert_eq!(audit.snapshot_id, snapshot_id);
    assert_eq!(audit.items.len(), 4);
    assert_eq!(audit.items[0].state, IntakeState::DirectManagedAddition);
    assert_eq!(audit.items[1].state, IntakeState::GenuinelyNew);
    assert_eq!(audit.items[2].state, IntakeState::KnownFromHistory);
    assert_eq!(audit.items[3].state, IntakeState::PreviouslyExcluded);

    let already_covered_spotify_id = format!("intake-track-0-{suffix}");
    intake::set_saved_track_disposition(
        &database,
        &account_label,
        &already_covered_spotify_id,
        intake::SavedTrackDisposition::Preserve,
        "fixture keep decision",
    )
    .await?;
    let decided_audit = intake::audit(&database, &account_label).await?;
    let decided = decided_audit
        .items
        .iter()
        .find(|item| item.spotify_id == already_covered_spotify_id)
        .expect("decided saved track remains auditable");
    assert_eq!(decided.saved_track_disposition.as_deref(), Some("preserve"));
    let active_decisions: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM playlist_surfaces surface
         JOIN playlist_track_directives directive
           ON directive.surface_id = surface.id
          AND directive.chordrift_account_id = surface.chordrift_account_id
         WHERE surface.chordrift_account_id = $1
           AND surface.stable_key = 'provider-saved-tracks:' || $2::text
           AND directive.track_id = $3 AND directive.superseded_at IS NULL",
    )
    .bind(chordrift_account_id)
    .bind(account_id)
    .bind(tracks[0].0)
    .fetch_one(database.pool())
    .await?;
    assert_eq!(active_decisions, 1);

    // Record-only convergence must preserve an already accepted provider order
    // even when active assignment revisions are replayed into a new generation.
    let ordered_revision_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_playlist_revisions
         (provider_playlist_id, content_sha256, item_count)
         VALUES ($1, $2, 3) RETURNING id",
    )
    .bind(provider_playlist_id)
    .bind("e".repeat(64))
    .fetch_one(database.pool())
    .await?;
    for (position, track_index) in [0_usize, 2, 3].into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO provider_playlist_revision_tracks
             (revision_id, provider_track_id, position) VALUES ($1, $2, $3)",
        )
        .bind(ordered_revision_id)
        .bind(tracks[track_index].1)
        .bind(i32::try_from(position).expect("fixture position fits i32"))
        .execute(database.pool())
        .await?;
    }
    sqlx::query(
        "UPDATE provider_current_playlists
         SET revision_id = $2, reported_item_count = 3
         WHERE provider_account_id = $1 AND provider_playlist_id = $3",
    )
    .bind(account_id)
    .bind(ordered_revision_id)
    .bind(provider_playlist_id)
    .execute(database.pool())
    .await?;
    let embedding_generation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO embedding_generations
         (provider_account_id, source_snapshot_id, model, model_version,
          dimensions, seed, input_hash, track_count)
         VALUES ($1, $2, 'fixture', '1', 16, 1, $3, 0) RETURNING id",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .bind("f".repeat(64))
    .fetch_one(database.pool())
    .await?;
    let cluster_generation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO cluster_generations
         (embedding_model, embedding_version, algorithm, algorithm_version,
          provider_account_id, embedding_generation_id, input_hash,
          track_count, cluster_count, unassigned_count)
         VALUES ('fixture', '1', 'fixture', '1', $1, $2, $3, 3, 1, 0)
         RETURNING id",
    )
    .bind(account_id)
    .bind(embedding_generation_id)
    .bind("1".repeat(64))
    .fetch_one(database.pool())
    .await?;
    let concept_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlist_concepts
         (provider_account_id, stable_key, origin, manual_name,
          manual_description, manual_tags)
         VALUES ($1, 'fixture-canonical', 'manual', 'Fixture Canonical',
                 'Provider-first fixture', '[]') RETURNING id",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    sqlx::query("UPDATE provider_playlists SET concept_id = $2 WHERE id = $1")
        .bind(provider_playlist_id)
        .bind(concept_id)
        .execute(database.pool())
        .await?;
    let generation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlist_generations
         (model, model_version, status, approved_at, provider_account_id,
          cluster_generation_id, input_hash, coverage_complete,
          required_track_count, represented_track_count, approved_by)
         VALUES ('fixture', '1', 'approved', now(), $1, $2, $3,
                 TRUE, 3, 3, 'fixture') RETURNING id",
    )
    .bind(account_id)
    .bind(cluster_generation_id)
    .bind("2".repeat(64))
    .fetch_one(database.pool())
    .await?;
    let proposed_playlist_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlists
         (generation_id, concept_id, name, description, kind, machine_label)
         VALUES ($1, $2, 'Fixture Canonical', 'Provider-first fixture',
                 'manual', 'fixture-canonical') RETURNING id",
    )
    .bind(generation_id)
    .bind(concept_id)
    .fetch_one(database.pool())
    .await?;
    sqlx::query(
        "INSERT INTO playlist_name_revisions
         (playlist_id, name, description, generator_provider,
          generator_model, generator_model_version, artifact_sha256)
         VALUES ($1, 'Fixture Canonical', 'Provider-first fixture',
                 'fixture', 'fixture', '1', $2)",
    )
    .bind(proposed_playlist_id)
    .bind("3".repeat(64))
    .execute(database.pool())
    .await?;
    for (position, track_index) in [0_usize, 2, 3].into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO playlist_tracks
             (playlist_id, track_id, position, source)
             VALUES ($1, $2, $3, 'manual')",
        )
        .bind(proposed_playlist_id)
        .bind(tracks[track_index].0)
        .bind(i32::try_from(position).expect("fixture position fits i32"))
        .execute(database.pool())
        .await?;
    }
    sqlx::query(
        "INSERT INTO track_playlist_assignment_revisions
         (provider_account_id, track_id, destination_concept_id, decision,
          source_generation_id, reason)
         VALUES ($1, $2, $3, 'assign', $4,
                 'Inferred from direct provider move')",
    )
    .bind(account_id)
    .bind(tracks[2].0)
    .bind(concept_id)
    .bind(generation_id)
    .execute(database.pool())
    .await?;
    sqlx::query(
        "INSERT INTO track_playlist_assignment_revisions
         (provider_account_id, track_id, destination_concept_id, decision,
          source_generation_id, reason)
         VALUES ($1, $2, $3, 'assign', $4,
                 'Stale assignment that must not override an active exclusion')",
    )
    .bind(account_id)
    .bind(tracks[1].0)
    .bind(concept_id)
    .bind(generation_id)
    .execute(database.pool())
    .await?;

    let extended = proposals::extend_approved(&database, &account_label, 1.0).await?;
    let extended_order: Vec<Uuid> = sqlx::query_scalar(
        "SELECT membership.track_id
         FROM playlists playlist
         JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
         WHERE playlist.generation_id = $1 AND playlist.concept_id = $2
         ORDER BY membership.position",
    )
    .bind(extended.generation_id)
    .bind(concept_id)
    .fetch_all(database.pool())
    .await?;
    assert_eq!(
        extended_order,
        vec![tracks[0].0, tracks[2].0, tracks[3].0],
        "replaying an assignment already in its destination must preserve provider order"
    );
    assert!(
        !extended_order.contains(&tracks[1].0),
        "an active exclusion must win over a stale assignment revision"
    );
    let editable_audit = intake::audit(&database, &account_label).await?;
    let copied_current_track = editable_audit
        .items
        .iter()
        .find(|item| item.spotify_id == already_covered_spotify_id)
        .expect("current fixture track remains auditable");
    assert_eq!(
        copied_current_track.state,
        IntakeState::AlreadyCovered,
        "an editable copy of accepted membership is not new provider intake"
    );
    proposals::approve(&database, &account_label, extended.generation_id).await?;
    let accepted = apply::accept_current_provider_state(&database, &account_label).await?;
    assert_eq!(accepted.snapshot_id, snapshot_id);
    assert_eq!(accepted.playlist_count, 1);

    let clear_spotify_id = format!("intake-track-2-{suffix}");
    intake::set_saved_track_disposition(
        &database,
        &account_label,
        &clear_spotify_id,
        intake::SavedTrackDisposition::ClearAfterVerifiedAssignment,
        "fixture clear decision",
    )
    .await?;
    let saved_cleanup_plan = sync_plan::create(&database, &account_label, None).await?;
    let (_, _, saved_cleanup_operations) =
        sync_plan::show(&database, &account_label, Some(saved_cleanup_plan.plan_id)).await?;
    assert!(saved_cleanup_operations.iter().any(|operation| {
        operation.operation_type == "remove_saved_track"
            && operation.spotify_track_id.as_deref() == Some(clear_spotify_id.as_str())
    }));
    assert!(!saved_cleanup_operations.iter().any(|operation| {
        operation.operation_type == "remove_saved_track"
            && operation.spotify_track_id.as_deref() == Some(already_covered_spotify_id.as_str())
    }));
    let undecided_spotify_id = format!("intake-track-3-{suffix}");
    assert!(!saved_cleanup_operations.iter().any(|operation| {
        operation.operation_type == "remove_saved_track"
            && operation.spotify_track_id.as_deref() == Some(undecided_spotify_id.as_str())
    }));

    // A later complete observation removes the formerly direct-intake track.
    // The accepted baseline must turn that delta into an exclusion, never an add.
    let removed_snapshot_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_library_snapshots
         (provider, source, provider_account_id)
         VALUES ('spotify', 'provider-removal-test', $1) RETURNING id",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let removed_revision_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_playlist_revisions
         (provider_playlist_id, content_sha256, item_count)
         VALUES ($1, $2, 2) RETURNING id",
    )
    .bind(provider_playlist_id)
    .bind("4".repeat(64))
    .fetch_one(database.pool())
    .await?;
    for (position, track_index) in [0_usize, 3].into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO provider_playlist_revision_tracks
             (revision_id, provider_track_id, position) VALUES ($1, $2, $3)",
        )
        .bind(removed_revision_id)
        .bind(tracks[track_index].1)
        .bind(i32::try_from(position).expect("fixture position fits i32"))
        .execute(database.pool())
        .await?;
    }
    sqlx::query(
        "UPDATE provider_current_inventories
         SET source_snapshot_id = $2, captured_at = now()
         WHERE provider_account_id = $1",
    )
    .bind(account_id)
    .bind(removed_snapshot_id)
    .execute(database.pool())
    .await?;
    sqlx::query(
        "UPDATE provider_current_playlists
         SET revision_id = $2, reported_item_count = 2
         WHERE provider_account_id = $1 AND provider_playlist_id = $3",
    )
    .bind(account_id)
    .bind(removed_revision_id)
    .bind(provider_playlist_id)
    .execute(database.pool())
    .await?;
    let plan = sync_plan::create(&database, &account_label, None).await?;
    let (_, _, operations) = sync_plan::show(&database, &account_label, Some(plan.plan_id)).await?;
    let removed_spotify_id = format!("intake-track-2-{suffix}");
    assert!(operations.iter().any(|operation| {
        operation.operation_type == "exclude_track"
            && operation.spotify_track_id.as_deref() == Some(removed_spotify_id.as_str())
    }));
    assert!(!operations.iter().any(|operation| {
        matches!(
            operation.operation_type.as_str(),
            "add_track" | "restore_track"
        ) && operation.spotify_track_id.as_deref() == Some(removed_spotify_id.as_str())
    }));

    // Emptying the exclusion archive is a separate Neon-only lifecycle step.
    // It is allowed only after the current provider observation no longer
    // contains the excluded tracks and it supersedes stale placement intent.
    let reduced_saved_revision_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_saved_track_revisions
         (provider_account_id, content_sha256, item_count)
         VALUES ($1, $2, 1) RETURNING id",
    )
    .bind(account_id)
    .bind("5".repeat(64))
    .fetch_one(database.pool())
    .await?;
    for (position, track_index) in [3_usize].into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO provider_saved_track_revision_tracks
             (revision_id, provider_track_id, position) VALUES ($1, $2, $3)",
        )
        .bind(reduced_saved_revision_id)
        .bind(tracks[track_index].1)
        .bind(i32::try_from(position).expect("fixture position fits i32"))
        .execute(database.pool())
        .await?;
    }
    sqlx::query(
        "UPDATE provider_current_inventories SET saved_track_revision_id = $2
         WHERE provider_account_id = $1",
    )
    .bind(account_id)
    .bind(reduced_saved_revision_id)
    .execute(database.pool())
    .await?;
    let artwork_generation_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM playlist_generations
         WHERE provider_account_id = $1 AND status = 'approved'
         ORDER BY approved_at DESC, created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let artwork_playlist_id: Uuid =
        sqlx::query_scalar("SELECT id FROM playlists WHERE generation_id = $1 AND concept_id = $2")
            .bind(artwork_generation_id)
            .bind(concept_id)
            .fetch_one(database.pool())
            .await?;
    let artwork_batch_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlist_artwork_batches
         (provider_account_id, proposal_generation_id, input_hash, state,
          visual_system, generator_provider, generator_model, generator_version,
          manifest_path, contact_sheet_path, artifact_count, approved_at)
         VALUES ($1, $2, $3, 'approved', 'fixture', 'fixture', 'fixture', '1',
                 'fixture/manifest.json', 'fixture/contact.png', 1, now()) RETURNING id",
    )
    .bind(account_id)
    .bind(artwork_generation_id)
    .bind("6".repeat(64))
    .fetch_one(database.pool())
    .await?;
    sqlx::query(
        "INSERT INTO playlist_artwork_artifacts
         (batch_id, playlist_id, stable_key, playlist_name, artifact_path,
          media_type, pixel_width, pixel_height, byte_size, content_sha256,
          prompt, semantic_tags, target_kind)
         VALUES ($1, $2, 'fixture-canonical', 'Fixture Canonical',
                 'fixture/cover.png', 'image/png', 1000, 1000, 100,
                 $3, 'fixture prompt', '[]', 'canonical')",
    )
    .bind(artwork_batch_id)
    .bind(artwork_playlist_id)
    .bind("7".repeat(64))
    .execute(database.pool())
    .await?;

    let removal_view = MaintenanceWorkflow::new(
        MaintenanceSessionId::new(),
        MaintenanceProjection {
            provider_snapshot_id: ResourceId::from_uuid(removed_snapshot_id),
            observed_changes: vec![MaintenanceChangeView {
                change_id: MaintenanceChangeId::new(),
                kind: MaintenanceChangeKind::Removal,
                track: Some(MaintenanceTrackView {
                    track_id: ResourceId::from_uuid(tracks[2].0),
                    title: "Known From History".to_owned(),
                    artists: Vec::new(),
                }),
                previous_surface: Some(MaintenanceSurfaceView {
                    surface_id: ResourceId::new(),
                    name: "Fixture Canonical".to_owned(),
                }),
                current_surface: None,
                summary: "Accepted provider removal".to_owned(),
                resolution: Some(MaintenanceResolution::Exclude),
            }],
            provider_effects: Vec::new(),
            review_id: None,
        },
    )
    .expect("fixture removal is valid")
    .view();
    let subject = AuthenticatedSubject {
        subject_id: ResourceId::new(),
        account_id: ResourceId::from_uuid(chordrift_account_id),
    };
    CanonicalMaintenanceProjector::new(&database)
        .project(subject, ResourceId::from_uuid(account_id), &removal_view)
        .await
        .expect("record-only removal projects into canonical intent");
    let projected_generation_count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM playlist_generations
         WHERE provider_account_id = $1",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    CanonicalMaintenanceProjector::new(&database)
        .project(subject, ResourceId::from_uuid(account_id), &removal_view)
        .await
        .expect("retrying projected removal is a no-op");
    let retried_generation_count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM playlist_generations
         WHERE provider_account_id = $1",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    assert_eq!(projected_generation_count, retried_generation_count);
    let inherited_artwork: (i64, i64) = sqlx::query_as(
        "WITH latest AS (
           SELECT id FROM playlist_generations
           WHERE provider_account_id = $1 AND status = 'approved'
           ORDER BY created_at DESC, id DESC LIMIT 1
         )
         SELECT count(DISTINCT batch.id)::bigint,
                count(artifact.id)::bigint
         FROM latest
         JOIN playlist_generations generation ON generation.id = latest.id
         JOIN playlist_artwork_batches batch
           ON batch.proposal_generation_id = generation.id AND batch.state = 'approved'
         JOIN playlist_artwork_artifacts artifact ON artifact.batch_id = batch.id
         WHERE generation.provider_account_id = $1
           AND generation.status = 'approved'
           AND artifact.content_sha256 = $2",
    )
    .bind(account_id)
    .bind("7".repeat(64))
    .fetch_one(database.pool())
    .await?;
    assert_eq!(inherited_artwork, (1, 1));
    let accepted_removal = apply::accept_current_provider_state(&database, &account_label).await?;
    assert_eq!(accepted_removal.snapshot_id, removed_snapshot_id);
    let old_keep_still_active: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM playlist_surfaces surface
             JOIN playlist_track_directives directive
               ON directive.surface_id = surface.id
              AND directive.chordrift_account_id = surface.chordrift_account_id
             WHERE surface.chordrift_account_id = $1
               AND surface.stable_key = 'provider-saved-tracks:' || $2::text
               AND directive.track_id = $3 AND directive.directive = 'include'
               AND directive.superseded_at IS NULL)",
    )
    .bind(chordrift_account_id)
    .bind(account_id)
    .bind(tracks[0].0)
    .fetch_one(database.pool())
    .await?;
    assert!(
        !old_keep_still_active,
        "a direct provider-side Unlike must supersede the older keep decision"
    );
    assert_eq!(
        track_ops::active_exclusions(&database, &account_label)
            .await?
            .len(),
        2
    );
    let emptied = track_ops::empty_exclusions(&database, &account_label, &account_label).await?;
    assert_eq!(emptied.cleared, 2);
    assert!(
        track_ops::active_exclusions(&database, &account_label)
            .await?
            .is_empty()
    );
    let post_empty_plan = sync_plan::create(&database, &account_label, None).await?;
    let (_, _, post_empty_operations) =
        sync_plan::show(&database, &account_label, Some(post_empty_plan.plan_id)).await?;
    assert!(!post_empty_operations.iter().any(|operation| {
        matches!(
            operation.operation_type.as_str(),
            "add_track" | "restore_track"
        ) && operation.spotify_track_id.as_deref() == Some(removed_spotify_id.as_str())
    }));
    let assignment_still_active: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM track_playlist_assignment_revisions
             WHERE provider_account_id = $1 AND track_id = $2
               AND superseded_at IS NULL)",
    )
    .bind(account_id)
    .bind(tracks[2].0)
    .fetch_one(database.pool())
    .await?;
    assert!(!assignment_still_active);

    sqlx::query("DELETE FROM provider_current_inventories WHERE provider_account_id = $1")
        .bind(account_id)
        .execute(database.pool())
        .await?;
    sqlx::query("DELETE FROM managed_playlist_verifications WHERE provider_account_id = $1")
        .bind(account_id)
        .execute(database.pool())
        .await?;
    sqlx::query("DELETE FROM sync_runs WHERE provider_account_id = $1")
        .bind(account_id)
        .execute(database.pool())
        .await?;
    sqlx::query("DELETE FROM track_playlist_assignment_revisions WHERE provider_account_id = $1")
        .bind(account_id)
        .execute(database.pool())
        .await?;
    sqlx::query("DELETE FROM playlist_artwork_batches WHERE provider_account_id = $1")
        .bind(account_id)
        .execute(database.pool())
        .await?;
    sqlx::query("DELETE FROM playlist_generations WHERE provider_account_id = $1")
        .bind(account_id)
        .execute(database.pool())
        .await?;
    sqlx::query("DELETE FROM cluster_generations WHERE provider_account_id = $1")
        .bind(account_id)
        .execute(database.pool())
        .await?;
    sqlx::query("DELETE FROM embedding_generations WHERE provider_account_id = $1")
        .bind(account_id)
        .execute(database.pool())
        .await?;
    sqlx::query("DELETE FROM provider_library_snapshots WHERE id = ANY($1)")
        .bind(vec![snapshot_id, removed_snapshot_id])
        .execute(database.pool())
        .await?;
    sqlx::query(
        "UPDATE playlists SET concept_id = NULL
         WHERE concept_id = $1 AND generation_id IS NULL",
    )
    .bind(concept_id)
    .execute(database.pool())
    .await?;
    sqlx::query("UPDATE provider_playlists SET concept_id = NULL WHERE concept_id = $1")
        .bind(concept_id)
        .execute(database.pool())
        .await?;
    sqlx::query("DELETE FROM provider_accounts WHERE id = $1")
        .bind(account_id)
        .execute(database.pool())
        .await?;
    sqlx::query("DELETE FROM chordrift_accounts WHERE id = $1")
        .bind(chordrift_account_id)
        .execute(database.pool())
        .await?;
    database.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires CHORDRIFT_TEST_DATABASE_URL for a disposable PostgreSQL database"]
async fn migrates_and_reports_the_canonical_schema() -> chordrift::Result<()> {
    let config = DatabaseConfig::from_env_var("CHORDRIFT_TEST_DATABASE_URL")?
        .with_name("chordrift-integration-test")?
        .with_provider(PostgresProvider::Neon)?
        .with_min_connections(0)
        .with_max_connections(2);
    let database = db::connect(config).await?;

    let expected_migrations = db::MIGRATOR.iter().count();
    let report = db::migrate(&database).await?;
    assert_eq!(report.available, expected_migrations);

    let status = db::status(&database).await?;
    assert_eq!(status.available_migrations, expected_migrations);
    assert_eq!(status.applied_migrations, expected_migrations);
    assert_eq!(status.pending_migrations, 0);
    assert_eq!(status.failed_migrations, 0);

    let replay = db::migrate(&database).await?;
    assert_eq!(replay.available, expected_migrations);
    let replay_status = db::status(&database).await?;
    assert_eq!(replay_status.applied_migrations, expected_migrations);
    assert_eq!(replay_status.pending_migrations, 0);

    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'",
    )
    .fetch_all(database.pool())
    .await?;
    for expected in [
        "albums",
        "chordrift_account_memberships",
        "chordrift_accounts",
        "collection_relationships",
        "collection_rule_revisions",
        "library_collections",
        "onboarding_sessions",
        "playlist_recipe_dependencies",
        "playlist_recipe_revisions",
        "playlist_recipes",
        "playlist_spin_publications",
        "playlist_spin_tracks",
        "playlist_spins",
        "playlist_surface_provider_links",
        "playlist_surfaces",
        "playlist_track_directives",
        "product_external_identities",
        "product_sessions",
        "product_subjects",
        "provider_capability_observations",
        "track_collection_membership_revisions",
        "account_analysis_state",
        "account_listening_track_statistics",
        "account_track_statistics",
        "artists",
        "account_track_embeddings",
        "account_track_signals",
        "cluster_generations",
        "cluster_tracks",
        "clusters",
        "embedding_generations",
        "database_v2_migration_runs",
        "database_v2_cleanup_runs",
        "enrichment_runs",
        "excluded_tracks",
        "external_playlist_bookmark_snapshots",
        "external_playlist_bookmark_tracks",
        "external_playlist_bookmarks",
        "external_playlist_cleanup_batches",
        "external_playlist_cleanup_items",
        "playlist_artwork_batches",
        "playlist_artwork_artifacts",
        "sync_readiness_assessments",
        "sync_readiness_checks",
        "external_playlist_bookmark_refreshes",
        "external_playlist_bookmark_refresh_tracks",
        "sync_apply_runs",
        "sync_apply_operations",
        "sync_apply_playlist_targets",
        "sync_retirement_approvals",
        "listening_events",
        "listening_evidence_imports",
        "listening_evidence_source_files",
        "historical_provider_track_identities",
        "normalized_listening_events",
        "managed_playlist_verifications",
        "managed_playlist_verified_tracks",
        "model_inference_imports",
        "playlist_concepts",
        "playlist_generations",
        "playlist_name_revisions",
        "playlist_tracks",
        "playlists",
        "provider_albums",
        "provider_accounts",
        "provider_account_playlists",
        "provider_artists",
        "provider_import_runs",
        "provider_current_inventories",
        "provider_current_playlists",
        "provider_inventory_checkpoints",
        "provider_inventory_checkpoint_playlists",
        "provider_inventory_checkpoint_saved_surfaces",
        "provider_library_snapshots",
        "provider_playlist_revisions",
        "provider_playlist_revision_tracks",
        "provider_playlist_snapshots",
        "provider_playlist_tracks",
        "provider_playlists",
        "provider_saved_tracks",
        "provider_saved_track_revisions",
        "provider_saved_track_revision_tracks",
        "provider_saved_album_revisions",
        "provider_saved_album_revision_albums",
        "provider_saved_album_revision_tracks",
        "provider_tracks",
        "routing_surfaces",
        "reevaluation_events",
        "signal_generations",
        "spotify_archive_imports",
        "spotify_recent_play_syncs",
        "sync_operations",
        "sync_runs",
        "track_artists",
        "track_artist_area_resolutions",
        "track_embeddings",
        "track_enrichment_lookups",
        "track_enrichment_matches",
        "track_semantic_facts",
        "track_matches",
        "track_model_facts",
        "track_model_inferences",
        "track_playlist_assignment_revisions",
        "track_statistics",
        "tracks",
    ] {
        assert!(tables.iter().any(|table| table == expected), "{expected}");
    }

    let current_view: Option<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.views
         WHERE table_schema = 'public' AND table_name = 'current_spotify_playlists'",
    )
    .fetch_optional(database.pool())
    .await?;
    assert_eq!(current_view.as_deref(), Some("current_spotify_playlists"));

    let runtime_views: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.views
         WHERE table_schema = 'public' AND table_name = ANY($1)",
    )
    .bind([
        "provider_inventory_observations",
        "provider_observed_playlists",
        "provider_observed_playlist_tracks",
        "provider_observed_saved_tracks",
        "provider_observed_saved_albums",
        "provider_observed_saved_album_tracks",
        "listening_evidence_events",
    ])
    .fetch_all(database.pool())
    .await?;
    assert_eq!(runtime_views.len(), 7);

    let account_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO provider_accounts
         (id, provider, provider_account_id, account_label)
         VALUES ($1, 'spotify', 'fixture-user', 'fixture')",
    )
    .bind(account_id)
    .execute(database.pool())
    .await?;
    let chordrift_account_id: Uuid =
        sqlx::query_scalar("SELECT chordrift_account_id FROM provider_accounts WHERE id = $1")
            .bind(account_id)
            .fetch_one(database.pool())
            .await?;
    let product_account_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM chordrift_accounts WHERE id = $1")
            .bind(chordrift_account_id)
            .fetch_one(database.pool())
            .await?;
    assert_eq!(product_account_count, 1);

    let replayed_account_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_accounts
         (provider, provider_account_id, account_label, display_name)
         VALUES ('spotify', 'fixture-user', 'fixture', 'Fixture Updated')
         ON CONFLICT (provider, account_label) DO UPDATE SET
           provider_account_id = EXCLUDED.provider_account_id,
           display_name = EXCLUDED.display_name,
           updated_at = now()
         RETURNING id",
    )
    .fetch_one(database.pool())
    .await?;
    assert_eq!(replayed_account_id, account_id);
    let product_account_count_after_replay: i64 =
        sqlx::query_scalar("SELECT count(*) FROM chordrift_accounts WHERE id = $1")
            .bind(chordrift_account_id)
            .fetch_one(database.pool())
            .await?;
    assert_eq!(product_account_count_after_replay, 1);

    let other_chordrift_account_id = Uuid::new_v4();
    sqlx::query("INSERT INTO chordrift_accounts (id, display_name) VALUES ($1, 'Other')")
        .bind(other_chordrift_account_id)
        .execute(database.pool())
        .await?;
    sqlx::query(
        "INSERT INTO provider_accounts
         (provider, provider_account_id, account_label, chordrift_account_id)
         VALUES ('spotify', 'explicit-owner-user', 'explicit-owner', $1)",
    )
    .bind(other_chordrift_account_id)
    .execute(database.pool())
    .await?;
    let explicit_owner_after_legacy_replay: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_accounts
         (provider, provider_account_id, account_label)
         VALUES ('spotify', 'explicit-owner-user', 'explicit-owner')
         ON CONFLICT (provider, account_label) DO UPDATE SET
           provider_account_id = EXCLUDED.provider_account_id,
           updated_at = now()
         RETURNING chordrift_account_id",
    )
    .fetch_one(database.pool())
    .await?;
    assert_eq!(
        explicit_owner_after_legacy_replay,
        other_chordrift_account_id
    );
    let owner_count_after_explicit_replay: i64 =
        sqlx::query_scalar("SELECT count(*) FROM chordrift_accounts WHERE id = $1 OR id = $2")
            .bind(chordrift_account_id)
            .bind(other_chordrift_account_id)
            .fetch_one(database.pool())
            .await?;
    assert_eq!(owner_count_after_explicit_replay, 2);

    let first_collection_id: Uuid = sqlx::query_scalar(
        "INSERT INTO library_collections
         (chordrift_account_id, stable_key, name)
         VALUES ($1, 'first', 'First') RETURNING id",
    )
    .bind(chordrift_account_id)
    .fetch_one(database.pool())
    .await?;
    let other_collection_id: Uuid = sqlx::query_scalar(
        "INSERT INTO library_collections
         (chordrift_account_id, stable_key, name)
         VALUES ($1, 'other', 'Other') RETURNING id",
    )
    .bind(other_chordrift_account_id)
    .fetch_one(database.pool())
    .await?;
    let cross_account_relationship = sqlx::query(
        "INSERT INTO collection_relationships
         (chordrift_account_id, parent_collection_id, child_collection_id)
         VALUES ($1, $2, $3)",
    )
    .bind(chordrift_account_id)
    .bind(first_collection_id)
    .bind(other_collection_id)
    .execute(database.pool())
    .await;
    assert!(cross_account_relationship.is_err());

    let capability_observation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_capability_observations
         (chordrift_account_id, provider_account_id,
          provider_capabilities, evidence_capabilities)
         VALUES ($1, $2, '{\"library_inventory_read\":\"available\"}', '{}')
         RETURNING id",
    )
    .bind(chordrift_account_id)
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let cross_account_capability = sqlx::query(
        "INSERT INTO provider_capability_observations
         (chordrift_account_id, provider_account_id)
         VALUES ($1, $2)",
    )
    .bind(other_chordrift_account_id)
    .bind(account_id)
    .execute(database.pool())
    .await;
    assert!(cross_account_capability.is_err());

    // Keep the onboarding/audit inventory on its own provider connection. The
    // retained Spotify persistence proof runs later against the legacy
    // `fixture` label in the same CI database and must not inherit these
    // content-addressed saved-surface revisions.
    let audit_provider_account_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO provider_accounts
         (id, provider, provider_account_id, account_label, chordrift_account_id)
         VALUES ($1, 'spotify', 'v02007-fixture-user', 'v02007-fixture', $2)",
    )
    .bind(audit_provider_account_id)
    .bind(chordrift_account_id)
    .execute(database.pool())
    .await?;
    let checkpoint_id = seed_inventory_checkpoint(&database, audit_provider_account_id).await?;
    seed_extended_history(&database, audit_provider_account_id).await?;
    let provider_connection = ProviderConnectionIdentity {
        connection_id: ProviderConnectionId::from_uuid(audit_provider_account_id),
        account_id: ChordriftAccountId::from_uuid(chordrift_account_id),
        provider_account_id: ProviderAccountId::new(
            ProviderNamespace::new("spotify").expect("namespace is valid"),
            "v02007-fixture-user",
        )
        .expect("provider account is valid"),
    };
    let onboarding_context = AccountContext::new(
        provider_connection.account_id,
        provider_connection.clone(),
        ProviderCapabilities::new(
            provider_connection.connection_id,
            BTreeMap::from([
                (
                    ProviderCapability::LibraryInventoryRead,
                    CapabilityStatus::Available,
                ),
                (
                    ProviderCapability::PlaylistRead,
                    CapabilityStatus::Available,
                ),
                (
                    ProviderCapability::SavedTracksRead,
                    CapabilityStatus::Available,
                ),
                (
                    ProviderCapability::SavedAlbumsRead,
                    CapabilityStatus::Degraded,
                ),
            ]),
        ),
        EvidenceCapabilities::new(BTreeMap::from([
            (
                EvidenceCapability::CurrentInventory,
                CapabilityStatus::Available,
            ),
            (
                EvidenceCapability::ExtendedPlaybackHistory,
                CapabilityStatus::Available,
            ),
        ])),
    )
    .expect("onboarding account context is valid");
    let fake_reader = FakeOnboardingReader {
        connection: provider_connection.clone(),
        checkpoint_id,
        reads: Cell::new(0),
    };
    let onboarding = OnboardingSessionBoundary::new(&database);
    let inventory_only_request = onboarding_request(chordrift_account_id, false);
    let inventory_only = ApplicationFacade::new()
        .invoke(onboarding.invocation(&onboarding_context, &inventory_only_request, &fake_reader))
        .await?
        .expect("inventory-only input capture succeeds");
    assert!(inventory_only.ignored_existing_intent);
    assert!(!inventory_only.include_extended_history);
    assert_eq!(
        inventory_only.input_manifest["ignore_existing_intent"],
        true
    );
    assert_eq!(inventory_only.input_manifest["evidence"], json!([]));
    assert_eq!(
        inventory_only.output_provenance["chordrift_intent_read"],
        false
    );
    assert_eq!(
        inventory_only.output_provenance["provider_write_requested"],
        false
    );

    sqlx::query(
        "INSERT INTO library_collections
         (chordrift_account_id, stable_key, name)
         VALUES ($1, 'added-after-input-capture', 'Added After Input Capture')",
    )
    .bind(chordrift_account_id)
    .execute(database.pool())
    .await?;
    let intent_count_before_replay: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM library_collections WHERE chordrift_account_id = $1",
    )
    .bind(chordrift_account_id)
    .fetch_one(database.pool())
    .await?;
    assert_eq!(intent_count_before_replay, 2);
    let replay = ApplicationFacade::new()
        .invoke(onboarding.invocation(&onboarding_context, &inventory_only_request, &fake_reader))
        .await?
        .expect("identical input capture is idempotent");
    assert_eq!(replay.id, inventory_only.id);
    assert_eq!(replay.input_fingerprint, inventory_only.input_fingerprint);
    assert_eq!(fake_reader.reads.get(), 1);

    let mut conflicting_replay = inventory_only_request.clone();
    conflicting_replay.command = Command::CreateOnboardingSession {
        account_id: ResourceId::from_uuid(chordrift_account_id),
        include_extended_history: true,
    };
    let conflict = ApplicationFacade::new()
        .invoke(onboarding.invocation(&onboarding_context, &conflicting_replay, &fake_reader))
        .await?
        .expect_err("one idempotency key cannot select different onboarding inputs");
    assert_eq!(
        conflict.client_error().code,
        chordrift::contract::ErrorCode::StateConflict
    );
    assert_eq!(fake_reader.reads.get(), 1);

    let unavailable_context = AccountContext::new(
        provider_connection.account_id,
        provider_connection.clone(),
        ProviderCapabilities::new(
            provider_connection.connection_id,
            BTreeMap::from([(
                ProviderCapability::LibraryInventoryRead,
                CapabilityStatus::Unavailable,
            )]),
        ),
        EvidenceCapabilities::new(BTreeMap::from([(
            EvidenceCapability::CurrentInventory,
            CapabilityStatus::Available,
        )])),
    )
    .expect("unavailable capability fixture is internally valid");
    let reads_before_capability_failure = fake_reader.reads.get();
    let capability_error = ApplicationFacade::new()
        .invoke(onboarding.invocation(
            &unavailable_context,
            &onboarding_request(chordrift_account_id, false),
            &fake_reader,
        ))
        .await?
        .expect_err("unavailable inventory capability fails visibly");
    assert_eq!(
        capability_error.client_error().code,
        chordrift::contract::ErrorCode::CapabilityUnavailable
    );
    assert_eq!(fake_reader.reads.get(), reads_before_capability_failure);

    let enriched_request = onboarding_request(chordrift_account_id, true);
    let enriched = ApplicationFacade::new()
        .invoke(onboarding.invocation(&onboarding_context, &enriched_request, &fake_reader))
        .await?
        .expect("selected extended evidence is captured");
    assert_ne!(enriched.id, inventory_only.id);
    assert!(enriched.include_extended_history);
    assert_eq!(
        enriched.input_manifest["evidence"][0]["capability"],
        "extended_playback_history"
    );
    assert_eq!(fake_reader.reads.get(), 2);

    let audit_boundary = InventoryOnlyAuditBoundary::new(&database);
    let audit_request = onboarding_audit_request(inventory_only.id.as_uuid());
    let session_before_audit: (String, serde_json::Value) =
        sqlx::query_as("SELECT status, output_provenance FROM onboarding_sessions WHERE id = $1")
            .bind(inventory_only.id.as_uuid())
            .fetch_one(database.pool())
            .await?;
    let intent_before_audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM library_collections WHERE chordrift_account_id = $1",
    )
    .bind(chordrift_account_id)
    .fetch_one(database.pool())
    .await?;
    let provider_reads_before_audit = fake_reader.reads.get();
    let audit = ApplicationFacade::new()
        .invoke(audit_boundary.invocation(&onboarding_context, &audit_request))
        .await?
        .expect("inventory-only audit succeeds");
    assert_eq!(
        audit.value.evidence_basis,
        AuditEvidenceBasis::CurrentInventoryOnly
    );
    assert!(!audit.value.capabilities.extended_history_used);
    assert_eq!(
        audit.value.capabilities.saved_albums_read,
        CapabilityStatus::Degraded
    );
    assert_eq!(audit.value.library.playlists, 2);
    assert_eq!(audit.value.library.reported_playlist_entries, 6);
    assert_eq!(audit.value.library.readable_playlist_entries, 5);
    assert_eq!(audit.value.library.saved_tracks, 2);
    assert_eq!(audit.value.library.saved_albums, 0);
    assert_eq!(audit.value.library.unique_tracks, 4);
    assert_eq!(audit.value.overlap.tracks_in_multiple_playlists, 1);
    assert_eq!(audit.value.overlap.maximum_playlist_occurrences, 2);
    assert_eq!(audit.value.overlap.saved_and_playlisted_tracks, 1);
    assert_eq!(audit.value.overlap.saved_outside_playlists, 1);
    assert_eq!(audit.value.overlap.playlist_only_tracks, 2);
    assert_eq!(audit.value.overlap.duplicate_playlist_entries, 1);
    assert_eq!(audit.value.uncertainty.unreadable_item_references, 1);
    assert_eq!(audit.value.uncertainty.capability_gaps.len(), 1);
    assert!(
        audit
            .value
            .uncertainty
            .limitations
            .contains(&AuditLimitation::UserIntentNotInferred)
    );
    assert!(
        audit
            .value
            .uncertainty
            .limitations
            .contains(&AuditLimitation::ExtendedHistoryNotUsed)
    );
    assert_eq!(audit.value.starter_organization.collections.len(), 5);
    assert_eq!(
        audit.value.starter_organization.collections[0].basis,
        StarterCollectionBasis::AllObservedInventory
    );
    assert_eq!(
        audit.value.starter_organization.collections[1].name,
        "Alpha"
    );
    assert_eq!(audit.value.starter_organization.collections[2].name, "Beta");
    assert_eq!(
        audit.value.starter_organization.collections[4].confidence,
        StarterProposalConfidence::ReviewRequired
    );
    assert!(audit.value.starter_organization.preserve_existing_playlists);
    assert!(!audit.value.starter_organization.approved);

    let replayed_audit = ApplicationFacade::new()
        .invoke(audit_boundary.invocation(&onboarding_context, &audit_request))
        .await?
        .expect("inventory-only audit replay succeeds");
    assert_eq!(replayed_audit.value, audit.value);
    assert_eq!(fake_reader.reads.get(), provider_reads_before_audit);
    let session_after_audit: (String, serde_json::Value) =
        sqlx::query_as("SELECT status, output_provenance FROM onboarding_sessions WHERE id = $1")
            .bind(inventory_only.id.as_uuid())
            .fetch_one(database.pool())
            .await?;
    assert_eq!(session_after_audit, session_before_audit);
    let intent_after_audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM library_collections WHERE chordrift_account_id = $1",
    )
    .bind(chordrift_account_id)
    .fetch_one(database.pool())
    .await?;
    assert_eq!(intent_after_audit, intent_before_audit);

    let enriched_audit_error = ApplicationFacade::new()
        .invoke(audit_boundary.invocation(
            &onboarding_context,
            &onboarding_audit_request(enriched.id.as_uuid()),
        ))
        .await?
        .expect_err("inventory-only path refuses extended-history inputs");
    assert_eq!(
        enriched_audit_error.client_error().code,
        chordrift::contract::ErrorCode::StateConflict
    );

    let enriched_boundary = EnrichedAuditBoundary::new(&database);
    let inventory_on_enriched_error = ApplicationFacade::new()
        .invoke(enriched_boundary.invocation(&onboarding_context, &audit_request))
        .await?
        .expect_err("enriched path requires explicitly selected history");
    assert_eq!(
        inventory_on_enriched_error.client_error().code,
        chordrift::contract::ErrorCode::StateConflict
    );
    let enriched_audit_request = onboarding_audit_request(enriched.id.as_uuid());
    let enriched_audit = ApplicationFacade::new()
        .invoke(enriched_boundary.invocation(&onboarding_context, &enriched_audit_request))
        .await?
        .expect("enriched audit succeeds");
    assert_eq!(
        enriched_audit.value.evidence_basis,
        EnrichedAuditEvidenceBasis::CurrentInventoryAndExtendedHistory
    );
    assert_eq!(
        enriched_audit.value.inventory_baseline.library,
        audit.value.library
    );
    assert_eq!(
        enriched_audit.value.inventory_baseline.playlists,
        audit.value.playlists
    );
    assert_eq!(
        enriched_audit.value.inventory_baseline.overlap,
        audit.value.overlap
    );
    assert_eq!(
        enriched_audit.value.inventory_baseline.uncertainty,
        audit.value.uncertainty
    );
    assert_eq!(
        enriched_audit.value.inventory_baseline.starter_organization,
        audit.value.starter_organization
    );
    assert_ne!(
        enriched_audit.value.inventory_baseline.audit_fingerprint, audit.value.audit_fingerprint,
        "session-owned complete audit identities intentionally differ"
    );
    assert_eq!(
        inventory_findings_fingerprint(&enriched_audit.value.inventory_baseline)
            .expect("comparable enriched inventory findings fingerprint"),
        inventory_findings_fingerprint(&audit.value)
            .expect("comparable inventory-only findings fingerprint"),
        "enrichment must preserve the comparable inventory findings"
    );
    assert_eq!(enriched_audit.value.history.declared_records, 7);
    assert_eq!(enriched_audit.value.history.readable_records, 7);
    assert_eq!(enriched_audit.value.history.usable_records, 7);
    assert_eq!(enriched_audit.value.history.superseded_records, 0);
    assert_eq!(enriched_audit.value.history.distinct_historical_tracks, 4);
    assert_eq!(enriched_audit.value.history.current_tracks_with_history, 3);
    assert_eq!(enriched_audit.value.history.history_only_tracks, 1);
    assert_eq!(enriched_audit.value.history.repeatedly_played_tracks, 2);
    assert_eq!(enriched_audit.value.history.repeated_track_records, 5);
    assert_eq!(enriched_audit.value.history.long_term_observed_tracks, 1);
    assert_eq!(enriched_audit.value.history.long_term_observed_records, 3);
    assert_eq!(enriched_audit.value.history.history_only_records, 2);
    assert_eq!(enriched_audit.value.history.maximum_track_plays, 3);
    assert_eq!(enriched_audit.value.history.completed_records, 4);
    assert_eq!(enriched_audit.value.history.completed_tracks, 2);
    assert_eq!(enriched_audit.value.history.skipped_records, 1);
    assert_eq!(enriched_audit.value.history.skipped_tracks, 1);
    assert_eq!(enriched_audit.value.strengthened_conclusions.len(), 6);
    assert_eq!(
        enriched_audit.value.strengthened_conclusions[0].conclusion,
        StrengthenedConclusionKind::ListeningObserved
    );
    assert_eq!(
        enriched_audit.value.strengthened_conclusions[0].supporting_records,
        7
    );
    assert_eq!(
        enriched_audit.value.strengthened_conclusions[1].conclusion,
        StrengthenedConclusionKind::RepeatedListeningObserved
    );
    assert_eq!(
        enriched_audit.value.strengthened_conclusions[1].supporting_tracks,
        2
    );
    assert!(
        enriched_audit
            .value
            .remaining_limitations
            .contains(&EnrichedAuditLimitation::PreferenceNotInferred)
    );
    let replayed_enriched_audit = ApplicationFacade::new()
        .invoke(enriched_boundary.invocation(&onboarding_context, &enriched_audit_request))
        .await?
        .expect("enriched audit replay succeeds");
    assert_eq!(replayed_enriched_audit.value, enriched_audit.value);
    assert_eq!(fake_reader.reads.get(), provider_reads_before_audit);
    sqlx::query(
        "UPDATE listening_evidence_imports SET event_count = 8
          WHERE provider_account_id = $1 AND archive_sha256 = $2",
    )
    .bind(audit_provider_account_id)
    .bind("e".repeat(64))
    .execute(database.pool())
    .await?;
    let changed_evidence_error = ApplicationFacade::new()
        .invoke(enriched_boundary.invocation(&onboarding_context, &enriched_audit_request))
        .await?
        .expect_err("captured and current evidence counts must match");
    assert_eq!(
        changed_evidence_error.client_error().code,
        chordrift::contract::ErrorCode::StateConflict
    );
    sqlx::query(
        "UPDATE listening_evidence_imports SET event_count = 7
          WHERE provider_account_id = $1 AND archive_sha256 = $2",
    )
    .bind(audit_provider_account_id)
    .bind("e".repeat(64))
    .execute(database.pool())
    .await?;
    let session_after_enriched_audit: (String, serde_json::Value) =
        sqlx::query_as("SELECT status, output_provenance FROM onboarding_sessions WHERE id = $1")
            .bind(enriched.id.as_uuid())
            .fetch_one(database.pool())
            .await?;
    assert_eq!(session_after_enriched_audit.0, "created");
    assert_eq!(session_after_enriched_audit.1, enriched.output_provenance);
    let intent_after_enriched_audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM library_collections WHERE chordrift_account_id = $1",
    )
    .bind(chordrift_account_id)
    .fetch_one(database.pool())
    .await?;
    assert_eq!(intent_after_enriched_audit, intent_before_audit);

    let captured_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM onboarding_sessions
          WHERE chordrift_account_id = $1 AND provider_account_id = $2
            AND input_fingerprint IS NOT NULL",
    )
    .bind(chordrift_account_id)
    .bind(audit_provider_account_id)
    .fetch_one(database.pool())
    .await?;
    assert_eq!(captured_rows, 2);
    let publication_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM playlist_spin_publications")
            .fetch_one(database.pool())
            .await?;
    assert_eq!(publication_rows, 0);

    let wrong_owner_connection = ProviderConnectionIdentity {
        connection_id: ProviderConnectionId::from_uuid(audit_provider_account_id),
        account_id: ChordriftAccountId::from_uuid(other_chordrift_account_id),
        provider_account_id: provider_connection.provider_account_id.clone(),
    };
    let wrong_owner_context = AccountContext::new(
        wrong_owner_connection.account_id,
        wrong_owner_connection.clone(),
        ProviderCapabilities::new(
            wrong_owner_connection.connection_id,
            BTreeMap::from([(
                ProviderCapability::LibraryInventoryRead,
                CapabilityStatus::Available,
            )]),
        ),
        EvidenceCapabilities::new(BTreeMap::from([(
            EvidenceCapability::CurrentInventory,
            CapabilityStatus::Available,
        )])),
    )
    .expect("internally consistent cross-account fixture");
    let reads_before_cross_account = fake_reader.reads.get();
    let cross_account_error = ApplicationFacade::new()
        .invoke(onboarding.invocation(
            &wrong_owner_context,
            &onboarding_request(other_chordrift_account_id, false),
            &fake_reader,
        ))
        .await?
        .expect_err("database ownership rejects another account's connection");
    assert_eq!(
        cross_account_error.client_error().code,
        chordrift::contract::ErrorCode::PermissionDenied
    );
    assert_eq!(fake_reader.reads.get(), reads_before_cross_account);
    let cross_account_audit_error = ApplicationFacade::new()
        .invoke(audit_boundary.invocation(&wrong_owner_context, &audit_request))
        .await?
        .expect_err("another account cannot read the onboarding audit");
    assert_eq!(
        cross_account_audit_error.client_error().code,
        chordrift::contract::ErrorCode::PermissionDenied
    );
    let cross_account_enriched_error = ApplicationFacade::new()
        .invoke(enriched_boundary.invocation(&wrong_owner_context, &enriched_audit_request))
        .await?
        .expect_err("another account cannot read the enriched audit");
    assert_eq!(
        cross_account_enriched_error.client_error().code,
        chordrift::contract::ErrorCode::PermissionDenied
    );

    let onboarding_session_id: Uuid = sqlx::query_scalar(
        "INSERT INTO onboarding_sessions
         (chordrift_account_id, provider_account_id, capability_observation_id)
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(chordrift_account_id)
    .bind(account_id)
    .bind(capability_observation_id)
    .fetch_one(database.pool())
    .await?;
    let recipe_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlist_recipes
         (chordrift_account_id, stable_key, name)
         VALUES ($1, 'discovery', 'Discovery') RETURNING id",
    )
    .bind(chordrift_account_id)
    .fetch_one(database.pool())
    .await?;
    let recipe_revision_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlist_recipe_revisions
         (chordrift_account_id, recipe_id, revision, recipe_document)
         VALUES ($1, $2, 1, '{}') RETURNING id",
    )
    .bind(chordrift_account_id)
    .bind(recipe_id)
    .fetch_one(database.pool())
    .await?;
    sqlx::query(
        "INSERT INTO playlist_recipe_dependencies
         (chordrift_account_id, recipe_revision_id, lane, dependency_kind,
          collection_id, allocation_weight)
         VALUES ($1, $2, 'discovery', 'collection', $3, 1)",
    )
    .bind(chordrift_account_id)
    .bind(recipe_revision_id)
    .bind(first_collection_id)
    .execute(database.pool())
    .await?;

    let product_account = ChordriftAccountId::from_uuid(chordrift_account_id);
    let collection_source = RecipeSource::Collection(AccountOwnedId::new(
        product_account,
        CollectionId::from_uuid(first_collection_id),
    ));
    let spin_recipe = RecipeV1::new(
        RecipeRevisionIdentity {
            recipe_id: AccountOwnedId::new(product_account, RecipeId::from_uuid(recipe_id)),
            revision_id: RecipeRevisionId::from_uuid(recipe_revision_id),
        },
        vec![
            SourceAllocation {
                lane: SourceLane::Discovery,
                source: collection_source.clone(),
                weight: AllocationWeight::new(1),
            },
            SourceAllocation {
                lane: SourceLane::Familiar,
                source: collection_source.clone(),
                weight: AllocationWeight::new(1),
            },
        ],
        FamiliarityCadence::Every(NonZeroU16::new(2).expect("nonzero")),
        OrderingNarrative::SectionedJourney,
        vec![
            RecipeSection::WarmUp,
            RecipeSection::Focus,
            RecipeSection::Landing,
        ],
        vec![
            GuardrailKind::HardBoundaries,
            GuardrailKind::ArtistRepetition,
            GuardrailKind::ArtistSpacing,
        ],
    )
    .expect("Spin fixture recipe is valid");
    sqlx::query("UPDATE playlist_recipe_revisions SET recipe_document = $1 WHERE id = $2")
        .bind(serde_json::to_value(&spin_recipe).expect("recipe serializes"))
        .bind(recipe_revision_id)
        .execute(database.pool())
        .await?;

    let collections_request = QueryRequest {
        contract_version: CONTRACT_VERSION,
        request_id: Default::default(),
        query: Query::Collections {
            account_id: ResourceId::from_uuid(chordrift_account_id),
        },
    };
    let collections = ApplicationFacade::new()
        .invoke(
            CollectionReviewBoundary::new(&database)
                .invocation(product_account, &collections_request),
        )
        .await??;
    assert_eq!(collections.value.account_id, product_account);
    assert!(
        collections
            .value
            .collections
            .iter()
            .any(|collection| collection.collection_id.as_uuid() == first_collection_id)
    );
    assert!(
        ApplicationFacade::new()
            .invoke(CollectionReviewBoundary::new(&database).invocation(
                ChordriftAccountId::from_uuid(other_chordrift_account_id),
                &collections_request,
            ))
            .await?
            .is_err(),
        "collection review cannot cross account ownership"
    );

    let recipe_request = QueryRequest {
        contract_version: CONTRACT_VERSION,
        request_id: Default::default(),
        query: Query::Recipe {
            recipe_revision_id: ResourceId::from_uuid(recipe_revision_id),
        },
    };
    let reviewed_recipe = ApplicationFacade::new()
        .invoke(RecipeReviewBoundary::new(&database).invocation(product_account, &recipe_request))
        .await??;
    assert_eq!(
        reviewed_recipe.value.recipe_revision_id.as_uuid(),
        recipe_revision_id
    );
    assert_eq!(reviewed_recipe.value.recipe, spin_recipe);
    assert!(
        ApplicationFacade::new()
            .invoke(RecipeReviewBoundary::new(&database).invocation(
                ChordriftAccountId::from_uuid(other_chordrift_account_id),
                &recipe_request,
            ))
            .await?
            .is_err(),
        "recipe review cannot cross account ownership"
    );

    sqlx::query(
        "INSERT INTO playlist_recipe_dependencies
         (chordrift_account_id, recipe_revision_id, lane, dependency_kind,
          collection_id, allocation_weight)
         VALUES ($1, $2, 'familiar', 'collection', $3, 1)",
    )
    .bind(chordrift_account_id)
    .bind(recipe_revision_id)
    .bind(first_collection_id)
    .execute(database.pool())
    .await?;

    let canonical_tracks: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM tracks ORDER BY title, id LIMIT 4")
            .fetch_all(database.pool())
            .await?;
    assert_eq!(canonical_tracks.len(), 4);
    let spin_candidates = canonical_tracks
        .iter()
        .enumerate()
        .map(|(index, track_id)| {
            RecipeCandidate::new(
                AccountOwnedId::new(product_account, CanonicalTrackId::from_uuid(*track_id)),
                vec![CanonicalArtistId::from_uuid(Uuid::from_u128(
                    500 + u128::try_from(index % 3).expect("fixture index fits"),
                ))],
                if index < 2 {
                    SourceLane::Discovery
                } else {
                    SourceLane::Familiar
                },
                collection_source.clone(),
                100 - u64::try_from(index).expect("fixture index fits"),
                CandidateEligibility {
                    in_current_inventory: true,
                    playable: true,
                    explicitly_excluded: false,
                },
                vec![AccountOwnedId::new(
                    product_account,
                    CollectionId::from_uuid(first_collection_id),
                )],
            )
            .expect("Spin fixture candidate is valid")
        })
        .collect();
    let recipe_request = RecipeExecutionRequest::new(
        spin_recipe,
        NonZeroU16::new(4).expect("nonzero"),
        spin_candidates,
        EvidenceCapabilities::default(),
        vec![AccountOwnedId::new(
            product_account,
            CollectionId::from_uuid(first_collection_id),
        )],
        SelectionBudgets {
            max_occurrences_per_track: NonZeroU16::new(1).expect("nonzero"),
            max_tracks_per_artist: NonZeroU16::new(2).expect("nonzero"),
        },
    )
    .expect("Spin fixture request is valid");
    let spin_input = SpinPreviewInput {
        draft: RecipeExecutor::new()
            .execute(&recipe_request)
            .expect("Spin fixture recipe executes"),
        capability_snapshot: EvidenceCapabilities::default(),
        seed: u64::MAX,
    };
    let preview_request = CommandRequest {
        contract_version: CONTRACT_VERSION,
        request_id: Default::default(),
        idempotency_key: IdempotencyKey::new(),
        command: Command::PreviewSpin {
            recipe_revision_id: ResourceId::from_uuid(recipe_revision_id),
        },
    };
    let preview_boundary = SpinPreviewBoundary::new(&database);
    let preview = ApplicationFacade::new()
        .invoke(preview_boundary.create_invocation(product_account, &preview_request, &spin_input))
        .await?
        .expect("provider-free Spin preview persists");
    assert!(preview.playback_order_assigned);
    assert!(preview.verify_fingerprint().expect("preview verifies"));
    assert_eq!(preview.tracks.len(), 4);
    assert_eq!(preview.tracks[1].position, 2);
    assert_eq!(
        preview.tracks[1].selection_reason.lane,
        SourceLane::Familiar
    );
    assert_eq!(preview.tracks[3].position, 4);
    assert_eq!(
        preview.tracks[3].selection_reason.lane,
        SourceLane::Familiar
    );

    let replayed_preview = ApplicationFacade::new()
        .invoke(preview_boundary.create_invocation(product_account, &preview_request, &spin_input))
        .await?
        .expect("identical preview replay succeeds");
    assert_eq!(replayed_preview, preview);
    let preview_query = QueryRequest {
        contract_version: CONTRACT_VERSION,
        request_id: Default::default(),
        query: Query::SpinPreview {
            spin_id: ResourceId::from_uuid(preview.identity.spin_id().into_resource_id().as_uuid()),
        },
    };
    let displayed_preview = ApplicationFacade::new()
        .invoke(preview_boundary.read_invocation(product_account, &preview_query))
        .await?
        .expect("persisted preview displays");
    assert_eq!(displayed_preview.value, preview);
    let persisted_seed: String =
        sqlx::query_scalar("SELECT seed::text FROM playlist_spins WHERE id = $1")
            .bind(preview.identity.spin_id().into_resource_id().as_uuid())
            .fetch_one(database.pool())
            .await?;
    assert_eq!(persisted_seed, u64::MAX.to_string());
    let persisted_track_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM playlist_spin_tracks WHERE spin_id = $1
           AND jsonb_typeof(selection_reason) = 'object'
           AND jsonb_typeof(ordering_reason) = 'object'",
    )
    .bind(preview.identity.spin_id().into_resource_id().as_uuid())
    .fetch_one(database.pool())
    .await?;
    assert_eq!(persisted_track_rows, 4);
    let cross_account_preview = ApplicationFacade::new()
        .invoke(preview_boundary.create_invocation(
            ChordriftAccountId::from_uuid(other_chordrift_account_id),
            &preview_request,
            &spin_input,
        ))
        .await?
        .expect_err("another account cannot persist this Spin");
    assert_eq!(
        cross_account_preview.client_error().code,
        chordrift::contract::ErrorCode::PermissionDenied
    );
    let cross_account_read = ApplicationFacade::new()
        .invoke(preview_boundary.read_invocation(
            ChordriftAccountId::from_uuid(other_chordrift_account_id),
            &preview_query,
        ))
        .await?
        .expect_err("another account cannot display this Spin");
    assert_eq!(
        cross_account_read.client_error().code,
        chordrift::contract::ErrorCode::ResourceNotFound
    );
    let preview_publications: i64 =
        sqlx::query_scalar("SELECT count(*) FROM playlist_spin_publications")
            .fetch_one(database.pool())
            .await?;
    assert_eq!(preview_publications, 0);

    sqlx::query(
        "UPDATE playlist_recipe_revisions
            SET state = 'approved', approved_at = now()
          WHERE id = $1",
    )
    .bind(recipe_revision_id)
    .execute(database.pool())
    .await?;
    let publication_surface_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlist_surfaces
             (chordrift_account_id, stable_key, name, authority, purpose,
              refresh_policy, recipe_id)
         VALUES ($1, 'v02012-spin', 'V020-12 Spin', 'collaborative',
                 'renewable_experience', 'manual_spin', $2) RETURNING id",
    )
    .bind(chordrift_account_id)
    .bind(recipe_id)
    .fetch_one(database.pool())
    .await?;
    let publication_target_id: Uuid = sqlx::query_scalar(
        "SELECT provider_playlist_id
           FROM provider_inventory_checkpoint_playlists
          WHERE checkpoint_id = $1 ORDER BY name, provider_playlist_id LIMIT 1",
    )
    .bind(checkpoint_id)
    .fetch_one(database.pool())
    .await?;
    sqlx::query(
        "INSERT INTO playlist_surface_provider_links
             (chordrift_account_id, surface_id, provider_account_id,
              provider_namespace, provider_playlist_id, state)
         VALUES ($1, $2, $3, 'spotify', $4, 'active')",
    )
    .bind(chordrift_account_id)
    .bind(publication_surface_id)
    .bind(audit_provider_account_id)
    .bind(publication_target_id)
    .execute(database.pool())
    .await?;
    let excluded_spin_track = preview
        .tracks
        .last()
        .expect("Spin has an exclusion fixture")
        .track_id
        .into_resource_id()
        .as_uuid();
    let excluded_provider_track: String = sqlx::query_scalar(
        "SELECT provider_track_id FROM provider_tracks
          WHERE provider = 'spotify' AND track_id = $1",
    )
    .bind(excluded_spin_track)
    .fetch_one(database.pool())
    .await?;
    sqlx::query(
        "INSERT INTO playlist_track_directives
             (chordrift_account_id, surface_id, track_id, directive, reason)
         VALUES ($1, $2, $3, 'exclude', 'user removed this track from this surface')",
    )
    .bind(chordrift_account_id)
    .bind(publication_surface_id)
    .bind(excluded_spin_track)
    .execute(database.pool())
    .await?;
    let approve_publication = CommandRequest {
        contract_version: CONTRACT_VERSION,
        request_id: Default::default(),
        idempotency_key: IdempotencyKey::new(),
        command: Command::ApprovePublication {
            spin_id: ResourceId::from_uuid(preview.identity.spin_id().into_resource_id().as_uuid()),
        },
    };
    let publication_request = SpinPublicationRequest {
        surface_id: AccountOwnedId::new(
            product_account,
            chordrift::domain::SurfaceId::from_uuid(publication_surface_id),
        ),
        provider_connection_id: ProviderConnectionId::from_uuid(audit_provider_account_id),
    };
    let publication_boundary = SpinPublicationBoundary::new(&database);
    let publication = ApplicationFacade::new()
        .invoke(publication_boundary.invocation(
            product_account,
            &approve_publication,
            publication_request,
        ))
        .await?
        .expect("approved Spin becomes an immutable publication plan");
    assert_eq!(publication.excluded_tracks, 1);
    assert!(
        publication
            .operations
            .iter()
            .all(|operation| match operation {
                chordrift::spin_publication::SpinPublicationOperation::CreatePlaylist {
                    ..
                } => true,
                chordrift::spin_publication::SpinPublicationOperation::AddTrack {
                    provider_track_id,
                    ..
                } => provider_track_id != &excluded_provider_track,
            })
    );
    let persisted_origin: String =
        sqlx::query_scalar("SELECT preconditions ->> 'plan_origin' FROM sync_runs WHERE id = $1")
            .bind(publication.plan_id)
            .fetch_one(database.pool())
            .await?;
    assert_eq!(persisted_origin, "spin_publication");
    let (shown_publication, snapshot_current, _) =
        sync_plan::show(&database, "v02007-fixture", Some(publication.plan_id)).await?;
    assert_eq!(shown_publication.origin, PlanOrigin::SpinPublication);
    assert_eq!(shown_publication.proposal_generation_id, None);
    assert!(snapshot_current);
    let publication_replay = ApplicationFacade::new()
        .invoke(publication_boundary.invocation(
            product_account,
            &approve_publication,
            publication_request,
        ))
        .await?
        .expect("identical publication planning reuses the immutable plan");
    assert_eq!(publication_replay.plan_id, publication.plan_id);
    assert!(publication_replay.reused);
    let cross_account_publication = ApplicationFacade::new()
        .invoke(publication_boundary.invocation(
            ChordriftAccountId::from_uuid(other_chordrift_account_id),
            &approve_publication,
            publication_request,
        ))
        .await?
        .expect_err("another account cannot publish this Spin or surface");
    assert_eq!(
        cross_account_publication.client_error().code,
        chordrift::contract::ErrorCode::PermissionDenied
    );

    let spin_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlist_spins
         (chordrift_account_id, recipe_revision_id, onboarding_session_id,
          input_fingerprint, seed)
         VALUES ($1, $2, $3, repeat('a', 64), 7) RETURNING id",
    )
    .bind(chordrift_account_id)
    .bind(recipe_revision_id)
    .bind(onboarding_session_id)
    .fetch_one(database.pool())
    .await?;
    let surface_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlist_surfaces
         (chordrift_account_id, stable_key, name, authority, purpose, refresh_policy)
         VALUES ($1, 'discovery-spin', 'Discovery Spin', 'chordrift',
                 'renewable_experience', 'manual_spin') RETURNING id",
    )
    .bind(chordrift_account_id)
    .fetch_one(database.pool())
    .await?;
    sqlx::query(
        "INSERT INTO playlist_surface_provider_links
         (chordrift_account_id, surface_id, provider_account_id,
          provider_namespace, state)
         VALUES ($1, $2, $3, 'spotify', 'planned')",
    )
    .bind(chordrift_account_id)
    .bind(surface_id)
    .bind(account_id)
    .execute(database.pool())
    .await?;

    let sync_run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO sync_runs
         (provider, mode, status, provider_account_id)
         VALUES ('spotify', 'dry_run', 'planned', $1) RETURNING id",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    sqlx::query(
        "INSERT INTO playlist_spin_publications
         (chordrift_account_id, spin_id, surface_id, provider_account_id, sync_run_id)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(chordrift_account_id)
    .bind(spin_id)
    .bind(surface_id)
    .bind(account_id)
    .bind(sync_run_id)
    .execute(database.pool())
    .await?;

    let other_recipe_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlist_recipes
         (chordrift_account_id, stable_key, name)
         VALUES ($1, 'other-recipe', 'Other Recipe') RETURNING id",
    )
    .bind(other_chordrift_account_id)
    .fetch_one(database.pool())
    .await?;
    let other_recipe_revision_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlist_recipe_revisions
         (chordrift_account_id, recipe_id, revision, recipe_document)
         VALUES ($1, $2, 1, '{}') RETURNING id",
    )
    .bind(other_chordrift_account_id)
    .bind(other_recipe_id)
    .fetch_one(database.pool())
    .await?;
    let cross_account_spin = sqlx::query(
        "INSERT INTO playlist_spins
         (chordrift_account_id, recipe_revision_id, input_fingerprint, seed)
         VALUES ($1, $2, repeat('b', 64), 8)",
    )
    .bind(chordrift_account_id)
    .bind(other_recipe_revision_id)
    .execute(database.pool())
    .await;
    assert!(cross_account_spin.is_err());

    sqlx::query(
        "INSERT INTO listening_events
         (provider_account_id, provider, provider_track_id, source_event_id,
          played_at, ms_played, source_occurrence, source_kind, raw_metadata)
         VALUES ($1, 'spotify', 'track-1', 'archive-a',
                 '2026-08-20T04:33:23Z', 12345, 0, 'recent_api',
                 $2)",
    )
    .bind(account_id)
    .bind(json!({
        "track_name": "Fixture Track",
        "artist_name": "Fixture Artist",
        "context_type": "playlist"
    }))
    .execute(database.pool())
    .await?;
    let duplicate = sqlx::query(
        "INSERT INTO listening_events
         (provider_account_id, provider, provider_track_id, source_event_id,
          played_at, ms_played, source_occurrence, source_kind)
         VALUES ($1, 'spotify', 'track-1', 'archive-b',
                 '2026-08-20T04:33:23Z', 12345, 0, 'recent_api')
         ON CONFLICT (provider_account_id, provider, provider_track_id,
                      played_at, ms_played, source_occurrence)
         WHERE provider_account_id IS NOT NULL
           AND provider_track_id IS NOT NULL
           AND ms_played IS NOT NULL
         DO NOTHING",
    )
    .bind(account_id)
    .execute(database.pool())
    .await?;
    assert_eq!(duplicate.rows_affected(), 0);

    let normalized: (i64, i64, Option<String>) = sqlx::query_as(
        "SELECT count(*), COALESCE(sum(event.ms_played), 0)::bigint,
                max(identity.track_name)
           FROM normalized_listening_events event
           JOIN historical_provider_track_identities identity
             ON identity.id = event.historical_identity_id
          WHERE event.provider_account_id = $1",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    assert_eq!(normalized, (1, 12_345, Some("Fixture Track".to_owned())));

    let rows_before: i64 = sqlx::query_scalar("SELECT count(*) FROM listening_events")
        .fetch_one(database.pool())
        .await?;
    let compaction = db_reports::compaction_plan(&database, "fixture").await?;
    assert_eq!(compaction.snapshots_total, 0);
    assert_eq!(compaction.listening_events, 1);
    let rows_after: i64 = sqlx::query_scalar("SELECT count(*) FROM listening_events")
        .fetch_one(database.pool())
        .await?;
    assert_eq!(
        rows_after, rows_before,
        "planning must not mutate the database"
    );

    sqlx::query(
        "UPDATE listening_events SET superseded_at = '2026-08-21T00:00:00Z'
          WHERE provider_account_id = $1",
    )
    .bind(account_id)
    .execute(database.pool())
    .await?;
    let normalized_active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM normalized_listening_events
          WHERE provider_account_id = $1 AND superseded_at IS NULL",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    assert_eq!(normalized_active, 0);

    let storage = db_reports::storage_report(&database).await?;
    assert!(storage.database_bytes > 0);
    assert!(
        storage
            .tables
            .iter()
            .any(|table| table.table == "public.listening_events")
    );

    // Simulate the cleanup relation renames and prove migration 0045's
    // candidate function contains no late-bound database-v1 table names. Keep
    // the simulation transactional so the next integration surface receives
    // the same clean migrated schema rather than this deliberately partial
    // cleanup state.
    let mut cleanup_simulation = database.pool().begin().await?;
    for statement in [
        "DROP VIEW provider_inventory_import_playlist_tracks",
        "DROP VIEW provider_inventory_import_playlists",
        "DROP VIEW provider_inventory_import_saved_album_tracks",
        "DROP VIEW provider_inventory_import_saved_albums",
        "DROP VIEW provider_inventory_import_saved_tracks",
        "DROP VIEW provider_inventory_observations",
        "ALTER TABLE provider_library_snapshots RENAME TO provider_inventory_observations",
        "ALTER TABLE provider_playlist_snapshots RENAME TO provider_inventory_import_playlists",
        "ALTER TABLE provider_playlist_tracks RENAME TO provider_inventory_import_playlist_tracks",
        "ALTER TABLE provider_saved_tracks RENAME TO provider_inventory_import_saved_tracks",
        "ALTER TABLE provider_saved_albums RENAME TO provider_inventory_import_saved_albums",
        "ALTER TABLE provider_saved_album_tracks RENAME TO provider_inventory_import_saved_album_tracks",
    ] {
        sqlx::query(statement)
            .execute(&mut *cleanup_simulation)
            .await?;
    }
    let candidate_after_cleanup: bool =
        sqlx::query_scalar("SELECT account_track_is_library_candidate($1, gen_random_uuid())")
            .bind(account_id)
            .fetch_one(&mut *cleanup_simulation)
            .await?;
    assert!(!candidate_after_cleanup);
    cleanup_simulation.rollback().await?;

    database.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires CHORDRIFT_TEST_DATABASE_URL for an isolated PostgreSQL 18 rehearsal"]
async fn rehearses_v020_05_upgrade_from_migration_45() -> chordrift::Result<()> {
    let config = DatabaseConfig::from_env_var("CHORDRIFT_TEST_DATABASE_URL")?
        .with_name("chordrift-v020-05-upgrade-rehearsal")?
        .with_provider(PostgresProvider::Neon)?
        .with_min_connections(0)
        .with_max_connections(1);
    let database = db::connect(config).await?;
    let version: String = sqlx::query_scalar("SHOW server_version_num")
        .fetch_one(database.pool())
        .await?;
    let version: u32 = version.parse().expect("PostgreSQL version is numeric");
    assert!(
        (180_000..190_000).contains(&version),
        "V020-05 must be rehearsed on PostgreSQL 18"
    );

    let mut rehearsal = database.pool().begin().await?;
    sqlx::query("CREATE SCHEMA v020_05_upgrade_rehearsal")
        .execute(&mut *rehearsal)
        .await?;
    sqlx::query("SET LOCAL search_path TO v020_05_upgrade_rehearsal")
        .execute(&mut *rehearsal)
        .await?;

    for migration in db::MIGRATOR
        .iter()
        .filter(|migration| migration.version < 46)
    {
        sqlx::raw_sql(migration.sql.clone())
            .execute(&mut *rehearsal)
            .await?;
    }

    let legacy_provider_account_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO provider_accounts
         (id, provider, provider_account_id, account_label)
         VALUES ($1, 'spotify', 'upgrade-user', 'upgrade-fixture')",
    )
    .bind(legacy_provider_account_id)
    .execute(&mut *rehearsal)
    .await?;
    sqlx::query(
        "INSERT INTO sync_runs
         (provider, mode, status, provider_account_id)
         VALUES ('spotify', 'dry_run', 'planned', $1)",
    )
    .bind(legacy_provider_account_id)
    .execute(&mut *rehearsal)
    .await?;
    sqlx::query(
        "INSERT INTO playlist_concepts
         (provider_account_id, stable_key, origin, manual_name, manual_description)
         VALUES ($1, 'existing-concept', 'manual', 'Existing', 'Preserved')",
    )
    .bind(legacy_provider_account_id)
    .execute(&mut *rehearsal)
    .await?;
    let tables_before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.tables
         WHERE table_schema = 'v020_05_upgrade_rehearsal'",
    )
    .fetch_one(&mut *rehearsal)
    .await?;

    let product_migration = db::MIGRATOR
        .iter()
        .find(|migration| migration.version == 46)
        .expect("V020-05 migration is embedded");
    sqlx::raw_sql(product_migration.sql.clone())
        .execute(&mut *rehearsal)
        .await?;

    let tables_after: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.tables
         WHERE table_schema = 'v020_05_upgrade_rehearsal'",
    )
    .fetch_one(&mut *rehearsal)
    .await?;
    assert_eq!(tables_after, tables_before + 16);
    let preserved_concepts: i64 = sqlx::query_scalar("SELECT count(*) FROM playlist_concepts")
        .fetch_one(&mut *rehearsal)
        .await?;
    assert_eq!(preserved_concepts, 1);
    let preserved_sync_runs: i64 = sqlx::query_scalar("SELECT count(*) FROM sync_runs")
        .fetch_one(&mut *rehearsal)
        .await?;
    assert_eq!(preserved_sync_runs, 1);
    let owner: Option<Uuid> =
        sqlx::query_scalar("SELECT chordrift_account_id FROM provider_accounts WHERE id = $1")
            .bind(legacy_provider_account_id)
            .fetch_one(&mut *rehearsal)
            .await?;
    assert!(owner.is_some());
    let unowned_provider_accounts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM provider_accounts WHERE chordrift_account_id IS NULL",
    )
    .fetch_one(&mut *rehearsal)
    .await?;
    assert_eq!(unowned_provider_accounts, 0);

    rehearsal.rollback().await?;
    database.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires CHORDRIFT_TEST_DATABASE_URL for a disposable PostgreSQL database"]
async fn persists_product_ownership_sessions_and_immediate_revocation() -> chordrift::Result<()> {
    let config = DatabaseConfig::from_env_var("CHORDRIFT_TEST_DATABASE_URL")?
        .with_name("chordrift-product-identity-test")?
        .with_provider(PostgresProvider::Neon)?
        .with_min_connections(0)
        .with_max_connections(2);
    let database = db::connect(config).await?;
    db::migrate(&database).await?;

    let account_id: Uuid = sqlx::query_scalar(
        "INSERT INTO chordrift_accounts (display_name) VALUES ('Identity Fixture') RETURNING id",
    )
    .fetch_one(database.pool())
    .await?;
    let other_account_id: Uuid = sqlx::query_scalar(
        "INSERT INTO chordrift_accounts (display_name) VALUES ('Other Tenant') RETURNING id",
    )
    .fetch_one(database.pool())
    .await?;
    let store = PostgresProductIdentityStore::new(database.pool().clone());
    let identity =
        VerifiedExternalIdentity::new("https://identity.test", "fixture-person").unwrap();
    let provisioned = store
        .provision_account_owner(&identity, ResourceId::from_uuid(account_id))
        .await
        .unwrap();
    let replayed_provisioning = store
        .provision_account_owner(&identity, ResourceId::from_uuid(account_id))
        .await
        .unwrap();
    assert_eq!(replayed_provisioning, provisioned);
    let subject_id = provisioned.subject_id.as_uuid();
    let takeover = store
        .provision_account_owner(
            &VerifiedExternalIdentity::new("https://identity.test", "intruder").unwrap(),
            ResourceId::from_uuid(account_id),
        )
        .await;
    assert!(matches!(
        takeover,
        Err(chordrift::contract::ClientError {
            code: chordrift::contract::ErrorCode::PermissionDenied,
            ..
        })
    ));
    let now = Utc::now();
    let first_digest = [17_u8; 32];
    let first = NewProductSession {
        session_id: ResourceId::new(),
        account_id: ResourceId::from_uuid(account_id),
        token_sha256: first_digest,
        created_at: now,
        expires_at: now + TimeDelta::hours(1),
    };
    let subject = store.create_session(&identity, &first).await.unwrap();
    assert_eq!(subject.subject_id, ResourceId::from_uuid(subject_id));
    assert_eq!(subject.account_id, ResourceId::from_uuid(account_id));
    assert_eq!(
        store.authenticate_session(first_digest, now).await.unwrap(),
        subject
    );
    let stored_digest: Vec<u8> =
        sqlx::query_scalar("SELECT token_sha256 FROM product_sessions WHERE id = $1")
            .bind(first.session_id.as_uuid())
            .fetch_one(database.pool())
            .await?;
    assert_eq!(stored_digest, first_digest);

    let cross_tenant = NewProductSession {
        session_id: ResourceId::new(),
        account_id: ResourceId::from_uuid(other_account_id),
        token_sha256: [18_u8; 32],
        created_at: now,
        expires_at: now + TimeDelta::hours(1),
    };
    let cross_tenant = store.create_session(&identity, &cross_tenant).await;
    assert!(matches!(
        cross_tenant,
        Err(chordrift::contract::ClientError {
            code: chordrift::contract::ErrorCode::PermissionDenied,
            ..
        })
    ));

    store.revoke_session(first_digest, now).await.unwrap();
    assert_eq!(
        store
            .authenticate_session(first_digest, now)
            .await
            .expect_err("revoked session fails immediately")
            .code,
        chordrift::contract::ErrorCode::AuthenticationRequired
    );

    let second_digest = [19_u8; 32];
    let second = NewProductSession {
        session_id: ResourceId::new(),
        account_id: ResourceId::from_uuid(account_id),
        token_sha256: second_digest,
        created_at: now,
        expires_at: now + TimeDelta::hours(1),
    };
    store.create_session(&identity, &second).await.unwrap();
    sqlx::query(
        "UPDATE chordrift_account_memberships SET status = 'revoked'
         WHERE chordrift_account_id = $1 AND product_subject_id = $2",
    )
    .bind(account_id)
    .bind(subject_id)
    .execute(database.pool())
    .await?;
    assert_eq!(
        store
            .authenticate_session(second_digest, now)
            .await
            .expect_err("membership revocation invalidates existing sessions")
            .code,
        chordrift::contract::ErrorCode::AuthenticationRequired
    );

    sqlx::query("DELETE FROM chordrift_accounts WHERE id = ANY($1)")
        .bind(vec![account_id, other_account_id])
        .execute(database.pool())
        .await?;
    sqlx::query("DELETE FROM product_subjects WHERE id = $1")
        .bind(subject_id)
        .execute(database.pool())
        .await?;
    database.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires CHORDRIFT_TEST_DATABASE_URL for a disposable PostgreSQL database"]
async fn encrypts_rotates_and_revokes_provider_credentials_with_tenant_isolation()
-> chordrift::Result<()> {
    let config = DatabaseConfig::from_env_var("CHORDRIFT_TEST_DATABASE_URL")?
        .with_name("chordrift-provider-vault-test")?
        .with_provider(PostgresProvider::Neon)?
        .with_min_connections(0)
        .with_max_connections(2);
    let database = db::connect(config).await?;
    db::migrate(&database).await?;

    let account_id: Uuid = sqlx::query_scalar(
        "INSERT INTO chordrift_accounts (display_name) VALUES ('Vault Fixture') RETURNING id",
    )
    .fetch_one(database.pool())
    .await?;
    let other_account_id: Uuid = sqlx::query_scalar(
        "INSERT INTO chordrift_accounts (display_name) VALUES ('Vault Other Tenant') RETURNING id",
    )
    .fetch_one(database.pool())
    .await?;
    let identity_store = PostgresProductIdentityStore::new(database.pool().clone());
    let owner = identity_store
        .provision_account_owner(
            &VerifiedExternalIdentity::new("https://identity.test", "vault-owner").unwrap(),
            ResourceId::from_uuid(account_id),
        )
        .await
        .unwrap();
    let other_owner = identity_store
        .provision_account_owner(
            &VerifiedExternalIdentity::new("https://identity.test", "vault-other-owner").unwrap(),
            ResourceId::from_uuid(other_account_id),
        )
        .await
        .unwrap();
    let member_subject_id: Uuid =
        sqlx::query_scalar("INSERT INTO product_subjects DEFAULT VALUES RETURNING id")
            .fetch_one(database.pool())
            .await?;
    sqlx::query(
        "INSERT INTO chordrift_account_memberships
         (chordrift_account_id, product_subject_id, role)
         VALUES ($1, $2, 'member')",
    )
    .bind(account_id)
    .bind(member_subject_id)
    .execute(database.pool())
    .await?;
    let member = chordrift::service::AuthenticatedSubject {
        subject_id: ResourceId::from_uuid(member_subject_id),
        account_id: ResourceId::from_uuid(account_id),
    };
    let provider_account_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_accounts
         (provider, provider_account_id, account_label, chordrift_account_id)
         VALUES ('spotify', $1, $2, $3) RETURNING id",
    )
    .bind(format!("vault-provider-{account_id}"))
    .bind(format!("vault-{account_id}"))
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let credential_identity = ProviderCredentialIdentity::new(
        ResourceId::from_uuid(account_id),
        ResourceId::from_uuid(provider_account_id),
        "spotify",
    )
    .unwrap();
    let key_material = vec![41_u8; 32];
    let keyring = ProviderVaultKeyring::new(
        "vault-key-2026-08",
        [("vault-key-2026-08".to_owned(), key_material.clone())],
    )
    .unwrap();
    let store = PostgresProviderCredentialStore::new(database.pool().clone());
    store.verify_schema().await.unwrap();
    let vault = ProviderCredentialVault::new(store, keyring);
    let now = Utc::now();
    let first_secret = "database-must-only-see-ciphertext";
    let first = vault
        .rotate(
            owner,
            credential_identity.clone(),
            &ProviderRefreshCredential::new(first_secret, ["playlist-read-private".to_owned()])
                .unwrap(),
            now,
        )
        .await
        .unwrap();
    assert_eq!(first.generation, 1);
    assert!(!first.rotated);
    let (ciphertext, stored_key_id): (Vec<u8>, String) =
        sqlx::query_as("SELECT ciphertext, key_id FROM provider_credential_vault WHERE id = $1")
            .bind(first.revision_id.as_uuid())
            .fetch_one(database.pool())
            .await?;
    assert_eq!(stored_key_id, "vault-key-2026-08");
    assert!(
        !ciphertext
            .windows(first_secret.len())
            .any(|window| window == first_secret.as_bytes())
    );
    assert!(
        !ciphertext
            .windows(key_material.len())
            .any(|window| window == key_material.as_slice())
    );
    assert_eq!(
        vault
            .lease(member, &credential_identity)
            .await
            .unwrap()
            .refresh_token(),
        first_secret
    );
    assert_eq!(
        vault
            .rotate(
                member,
                credential_identity.clone(),
                &ProviderRefreshCredential::new("member-write", Vec::<String>::new()).unwrap(),
                now + TimeDelta::seconds(1),
            )
            .await
            .expect_err("member cannot rotate")
            .code,
        chordrift::contract::ErrorCode::PermissionDenied
    );
    let spoofed_other_tenant = chordrift::service::AuthenticatedSubject {
        subject_id: other_owner.subject_id,
        account_id: ResourceId::from_uuid(account_id),
    };
    assert_eq!(
        vault
            .lease(spoofed_other_tenant, &credential_identity)
            .await
            .err()
            .expect("other tenant cannot lease")
            .code,
        chordrift::contract::ErrorCode::PermissionDenied
    );

    let second = vault
        .rotate(
            owner,
            credential_identity.clone(),
            &ProviderRefreshCredential::new(
                "rotated-provider-secret",
                ["playlist-modify-private".to_owned()],
            )
            .unwrap(),
            now + TimeDelta::seconds(2),
        )
        .await
        .unwrap();
    assert_eq!(second.generation, 2);
    assert!(second.rotated);
    let active_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM provider_credential_vault
         WHERE provider_account_id = $1 AND revoked_at IS NULL",
    )
    .bind(provider_account_id)
    .fetch_one(database.pool())
    .await?;
    assert_eq!(active_count, 1);
    let revoked_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM provider_credential_vault
         WHERE provider_account_id = $1 AND revoked_at IS NOT NULL",
    )
    .bind(provider_account_id)
    .fetch_one(database.pool())
    .await?;
    assert_eq!(revoked_count, 1);

    sqlx::query(
        "UPDATE provider_credential_vault
         SET ciphertext = set_byte(ciphertext, 0, get_byte(ciphertext, 0) # 128)
         WHERE id = $1",
    )
    .bind(second.revision_id.as_uuid())
    .execute(database.pool())
    .await?;
    assert_eq!(
        vault
            .lease(owner, &credential_identity)
            .await
            .err()
            .expect("tampered ciphertext fails closed")
            .code,
        chordrift::contract::ErrorCode::DependencyUnavailable
    );
    let third = vault
        .rotate(
            owner,
            credential_identity.clone(),
            &ProviderRefreshCredential::new("final-provider-secret", Vec::<String>::new()).unwrap(),
            now + TimeDelta::seconds(3),
        )
        .await
        .unwrap();
    assert_eq!(third.generation, 3);
    let revoked = vault
        .revoke(
            owner,
            &credential_identity,
            "provider disconnected",
            now + TimeDelta::seconds(4),
        )
        .await
        .unwrap();
    assert_eq!(revoked.generation, 3);
    assert_eq!(
        vault
            .lease(owner, &credential_identity)
            .await
            .err()
            .expect("revoked credential unavailable")
            .code,
        chordrift::contract::ErrorCode::PermissionDenied
    );

    database.close().await;
    Ok(())
}

#[derive(Clone)]
struct DurableFixedClock(DateTime<Utc>);

impl ServiceClock for DurableFixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

fn durable_request(idempotency_key: IdempotencyKey) -> CommandRequest {
    CommandRequest {
        contract_version: CONTRACT_VERSION,
        request_id: RequestId::new(),
        idempotency_key,
        command: Command::StartMaintenance {
            session_id: Default::default(),
            provider_connection_id: ResourceId::new(),
        },
    }
}

#[tokio::test]
#[ignore = "requires CHORDRIFT_TEST_DATABASE_URL for a disposable PostgreSQL database"]
async fn persists_restart_safe_operation_replay_recovery_retry_and_cancellation()
-> chordrift::Result<()> {
    let config = DatabaseConfig::from_env_var("CHORDRIFT_TEST_DATABASE_URL")?
        .with_name("chordrift-durable-operations-test")?
        .with_provider(PostgresProvider::Neon)?
        .with_min_connections(0)
        .with_max_connections(3);
    let database = db::connect(config).await?;
    db::migrate(&database).await?;
    let account_id: Uuid = sqlx::query_scalar(
        "INSERT INTO chordrift_accounts (display_name)
         VALUES ('Durable Operation Fixture') RETURNING id",
    )
    .fetch_one(database.pool())
    .await?;
    let other_account_id: Uuid = sqlx::query_scalar(
        "INSERT INTO chordrift_accounts (display_name)
         VALUES ('Durable Other Tenant') RETURNING id",
    )
    .fetch_one(database.pool())
    .await?;
    let identity_store = PostgresProductIdentityStore::new(database.pool().clone());
    let owner = identity_store
        .provision_account_owner(
            &VerifiedExternalIdentity::new("https://identity.test", "durable-owner").unwrap(),
            ResourceId::from_uuid(account_id),
        )
        .await
        .unwrap();
    let other_owner = identity_store
        .provision_account_owner(
            &VerifiedExternalIdentity::new("https://identity.test", "durable-other").unwrap(),
            ResourceId::from_uuid(other_account_id),
        )
        .await
        .unwrap();
    let store = Arc::new(PostgresDurableOperationStore::new(database.pool().clone()));
    store.verify_schema().await.unwrap();
    let started_at: DateTime<Utc> = "2026-08-30T20:00:00Z".parse().unwrap();
    let first_queue = DurableOperationQueue::with_clock(
        Arc::clone(&store),
        Arc::new(DurableFixedClock(started_at)),
    );
    let idempotency_key = IdempotencyKey::new();
    let request = durable_request(idempotency_key);
    let policy = OperationRetryPolicy::new(3, Duration::ZERO).unwrap();
    let accepted = first_queue
        .accept(owner, request.clone(), policy)
        .await
        .unwrap();
    assert!(!accepted.replayed);
    assert_eq!(
        first_queue
            .operation(owner, accepted.receipt.operation_id)
            .await
            .unwrap()
            .state,
        OperationState::Queued
    );

    // A new queue instance simulates another service process. The exact
    // idempotent receipt and queued operation survive the restart.
    let restarted_queue = DurableOperationQueue::with_clock(
        Arc::new(PostgresDurableOperationStore::new(database.pool().clone())),
        Arc::new(DurableFixedClock(started_at)),
    );
    let replay = restarted_queue
        .accept(owner, request.clone(), policy)
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.receipt, accepted.receipt);
    let mut collision = request.clone();
    collision.command = Command::RefreshMaintenance {
        session_id: Default::default(),
        expected_revision: 1,
    };
    assert_eq!(
        restarted_queue
            .accept(owner, collision, policy)
            .await
            .expect_err("idempotency collision fails closed")
            .code,
        ErrorCode::StateConflict
    );
    assert_eq!(
        restarted_queue
            .operation(other_owner, accepted.receipt.operation_id)
            .await
            .expect_err("other tenant cannot read operation")
            .code,
        ErrorCode::ResourceNotFound
    );

    let competing_queue = DurableOperationQueue::with_clock(
        Arc::new(PostgresDurableOperationStore::new(database.pool().clone())),
        Arc::new(DurableFixedClock(started_at)),
    );
    let (claim_a, claim_b) = tokio::join!(
        restarted_queue.claim_next("worker-a", Duration::from_secs(1)),
        competing_queue.claim_next("worker-b", Duration::from_secs(1))
    );
    let claims = [claim_a.unwrap(), claim_b.unwrap()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(claims.len(), 1, "only one worker obtains the command");
    let first_lease = claims.into_iter().next().unwrap();
    assert_eq!(first_lease.attempt, 1);
    restarted_queue
        .renew_lease(&first_lease, Duration::from_secs(1))
        .await
        .unwrap();
    restarted_queue
        .record_progress(
            &first_lease,
            Progress::new("observe_provider", 2, Some(5), ProgressUnit::Playlists).unwrap(),
        )
        .await
        .unwrap();

    // After the lease expires, a third process recovers and reclaims it. The
    // stale worker capability can no longer append progress or complete work.
    let recovery_time = started_at + TimeDelta::seconds(2);
    let recovery_queue = DurableOperationQueue::with_clock(
        Arc::new(PostgresDurableOperationStore::new(database.pool().clone())),
        Arc::new(DurableFixedClock(recovery_time)),
    );
    let second_lease = recovery_queue
        .claim_next("worker-b", Duration::from_secs(30))
        .await
        .unwrap()
        .expect("expired lease is recovered");
    assert_eq!(second_lease.operation_id, first_lease.operation_id);
    assert_eq!(second_lease.attempt, 2);
    assert_eq!(
        recovery_queue
            .complete(&first_lease, None)
            .await
            .expect_err("stale lease cannot complete")
            .code,
        ErrorCode::StateConflict
    );
    let retry_state = recovery_queue
        .fail(
            &second_lease,
            ClientError::new(ErrorCode::DependencyUnavailable, true),
        )
        .await
        .unwrap();
    assert!(matches!(retry_state, OperationState::Recoverable { .. }));
    let third_lease = recovery_queue
        .claim_next("worker-c", Duration::from_secs(30))
        .await
        .unwrap()
        .expect("recoverable work is retried");
    assert_eq!(third_lease.attempt, 3);
    assert_eq!(
        recovery_queue
            .request_cancellation(
                owner,
                CancellationRequest {
                    operation_id: accepted.receipt.operation_id,
                    cancellation_id: accepted.receipt.cancellation_id,
                },
            )
            .await
            .unwrap(),
        CancellationOutcome::Requested
    );
    assert!(
        recovery_queue
            .cancellation_requested(&third_lease)
            .await
            .unwrap()
    );
    recovery_queue
        .acknowledge_cancellation(&third_lease)
        .await
        .unwrap();
    assert_eq!(
        recovery_queue
            .operation(owner, accepted.receipt.operation_id)
            .await
            .unwrap()
            .state,
        OperationState::Cancelled
    );
    let events = recovery_queue
        .events(owner, accepted.receipt.operation_id, None)
        .await
        .unwrap();
    assert_eq!(
        events
            .events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        (1_u64..=events.events.len() as u64).collect::<Vec<_>>()
    );
    assert!(events.events.iter().any(|event| matches!(
        event.state,
        OperationState::Running {
            progress: Some(ref progress)
        } if progress.phase == "observe_provider" && progress.completed == 2
    )));

    let terminal_request = durable_request(IdempotencyKey::new());
    let terminal = recovery_queue
        .accept(
            owner,
            terminal_request,
            OperationRetryPolicy::new(1, Duration::ZERO).unwrap(),
        )
        .await
        .unwrap();
    let terminal_lease = recovery_queue
        .claim_next("worker-terminal", Duration::from_secs(30))
        .await
        .unwrap()
        .expect("terminal fixture claimed");
    let terminal_state = recovery_queue
        .fail(
            &terminal_lease,
            ClientError::new(ErrorCode::DependencyUnavailable, true),
        )
        .await
        .unwrap();
    assert!(matches!(terminal_state, OperationState::Failed { .. }));
    assert_eq!(
        recovery_queue
            .request_cancellation(
                owner,
                CancellationRequest {
                    operation_id: terminal.receipt.operation_id,
                    cancellation_id: terminal.receipt.cancellation_id,
                },
            )
            .await
            .unwrap(),
        CancellationOutcome::TooLate
    );
    assert_eq!(
        recovery_queue
            .history(owner)
            .await
            .unwrap()
            .operations
            .len(),
        2
    );

    database.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires CHORDRIFT_TEST_DATABASE_URL for a disposable PostgreSQL database"]
async fn persists_tenant_isolated_maintenance_sessions_and_exact_revision_events()
-> chordrift::Result<()> {
    let config = DatabaseConfig::from_env_var("CHORDRIFT_TEST_DATABASE_URL")?
        .with_name("chordrift-maintenance-session-test")?
        .with_provider(PostgresProvider::Neon)?
        .with_min_connections(0)
        .with_max_connections(3);
    let database = db::connect(config).await?;
    db::migrate(&database).await?;
    let account_id: Uuid = sqlx::query_scalar(
        "INSERT INTO chordrift_accounts (display_name)
         VALUES ('Maintenance Session Fixture') RETURNING id",
    )
    .fetch_one(database.pool())
    .await?;
    let other_account_id: Uuid = sqlx::query_scalar(
        "INSERT INTO chordrift_accounts (display_name)
         VALUES ('Maintenance Session Other Tenant') RETURNING id",
    )
    .fetch_one(database.pool())
    .await?;
    let identity_store = PostgresProductIdentityStore::new(database.pool().clone());
    let owner = identity_store
        .provision_account_owner(
            &VerifiedExternalIdentity::new("https://identity.test", "maintenance-owner").unwrap(),
            ResourceId::from_uuid(account_id),
        )
        .await
        .unwrap();
    let other = identity_store
        .provision_account_owner(
            &VerifiedExternalIdentity::new("https://identity.test", "maintenance-other-owner")
                .unwrap(),
            ResourceId::from_uuid(other_account_id),
        )
        .await
        .unwrap();
    let provider_account_id: Uuid = sqlx::query_scalar(
        "INSERT INTO provider_accounts
         (provider, provider_account_id, account_label, chordrift_account_id)
         VALUES ('spotify', $1, 'maintenance-fixture', $2) RETURNING id",
    )
    .bind(format!("maintenance-provider-{account_id}"))
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let provider_connection_id = ResourceId::from_uuid(provider_account_id);
    let session_id = MaintenanceSessionId::new();
    let first_snapshot = ResourceId::new();
    let mut workflow = MaintenanceWorkflow::new(
        session_id,
        MaintenanceProjection {
            provider_snapshot_id: first_snapshot,
            observed_changes: Vec::new(),
            provider_effects: Vec::new(),
            review_id: None,
        },
    )
    .unwrap();
    let first = workflow.view();
    let at: DateTime<Utc> = "2026-08-31T19:00:00Z".parse().unwrap();
    let store = PostgresMaintenanceSessionStore::new(database.pool().clone());
    store.verify_schema().await.unwrap();
    store
        .create(owner, provider_connection_id, &first, None, at)
        .await
        .unwrap();

    let restarted = PostgresMaintenanceSessionStore::new(database.pool().clone());
    let loaded = restarted.load(owner, session_id).await.unwrap();
    assert_eq!(loaded.subject, owner);
    assert_eq!(loaded.provider_connection_id, provider_connection_id);
    assert_eq!(loaded.view, first);
    assert_eq!(
        restarted
            .load(
                AuthenticatedSubject {
                    subject_id: other.subject_id,
                    account_id: other.account_id,
                },
                session_id,
            )
            .await
            .expect_err("other tenant cannot discover the session")
            .code,
        ErrorCode::ResourceNotFound
    );

    let second = workflow
        .rebase(
            1,
            MaintenanceProjection {
                provider_snapshot_id: ResourceId::new(),
                observed_changes: Vec::new(),
                provider_effects: Vec::new(),
                review_id: None,
            },
        )
        .unwrap();
    restarted
        .replace(
            owner,
            1,
            &second,
            MaintenanceTransition::Refreshed,
            None,
            at + TimeDelta::seconds(1),
        )
        .await
        .unwrap();
    assert_eq!(
        restarted.load(owner, session_id).await.unwrap().view,
        second
    );
    assert_eq!(
        restarted
            .replace(
                owner,
                1,
                &second,
                MaintenanceTransition::Refreshed,
                None,
                at + TimeDelta::seconds(2),
            )
            .await
            .expect_err("stale writer cannot overwrite the accepted revision")
            .code,
        ErrorCode::StateConflict
    );
    let event_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM maintenance_session_events
          WHERE maintenance_session_id = $1",
    )
    .bind(session_id.as_uuid())
    .fetch_one(database.pool())
    .await?;
    assert_eq!(event_count, 2);

    let authority = DurableMaintenanceAuthority::new(restarted.clone());
    let review_id = MaintenanceReviewId::new();
    let reviewed = authority
        .refresh(
            owner,
            session_id,
            2,
            MaintenanceProjection {
                provider_snapshot_id: second.provider_snapshot_id,
                observed_changes: Vec::new(),
                provider_effects: vec![MaintenanceProviderEffectView {
                    effect_id: ResourceId::new(),
                    kind: MaintenanceProviderEffectKind::UpdateSavedState,
                    track: Some(MaintenanceTrackView {
                        track_id: ResourceId::new(),
                        title: "Fixture saved track".to_owned(),
                        artists: Vec::new(),
                    }),
                    surface: Some(MaintenanceSurfaceView {
                        surface_id: ResourceId::new(),
                        name: "Liked Songs".to_owned(),
                    }),
                    summary: "Remove Fixture saved track from Liked Songs".to_owned(),
                }],
                review_id: Some(review_id),
            },
            None,
            at + TimeDelta::seconds(3),
        )
        .await
        .unwrap();
    assert_eq!(
        reviewed.state,
        MaintenanceSessionState::ReadyForAuthorization
    );
    let authorized = authority
        .authorize(
            owner,
            session_id,
            reviewed.revision,
            review_id,
            None,
            at + TimeDelta::seconds(4),
        )
        .await
        .unwrap();
    let applying = authority
        .mark_execution_state(
            owner,
            session_id,
            authorized.revision,
            MaintenanceSessionState::Applying,
            None,
            at + TimeDelta::seconds(5),
        )
        .await
        .unwrap();
    let verifying = authority
        .mark_execution_state(
            owner,
            session_id,
            applying.revision,
            MaintenanceSessionState::Verifying,
            None,
            at + TimeDelta::seconds(6),
        )
        .await
        .unwrap();
    let completed = authority
        .complete_verification(
            owner,
            session_id,
            verifying.revision,
            MaintenanceProjection {
                provider_snapshot_id: ResourceId::new(),
                observed_changes: Vec::new(),
                provider_effects: Vec::new(),
                review_id: None,
            },
            None,
            at + TimeDelta::seconds(7),
        )
        .await
        .unwrap();
    assert_eq!(completed.state, MaintenanceSessionState::InSync);
    let transitions: Vec<String> = sqlx::query_scalar(
        "SELECT transition_name FROM maintenance_session_events
          WHERE maintenance_session_id = $1 ORDER BY revision",
    )
    .bind(session_id.as_uuid())
    .fetch_all(database.pool())
    .await?;
    assert_eq!(
        transitions,
        vec![
            "started",
            "refreshed",
            "refreshed",
            "authorized",
            "applying",
            "verifying",
            "verified",
        ]
    );

    database.close().await;
    Ok(())
}

#[test]
fn uses_an_application_specific_database_secret_name() {
    assert_eq!(config::DATABASE_URL_VARIABLE, "CHORDRIFT_DATABASE_URL");
}
