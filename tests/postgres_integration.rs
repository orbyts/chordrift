use std::{cell::Cell, collections::BTreeMap, future, num::NonZeroU16};

use chordrift::{
    application::ApplicationFacade,
    config,
    contract::{
        CONTRACT_VERSION, Command, CommandRequest, IdempotencyKey, Query, QueryRequest, ResourceId,
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
    recipe_execution::{
        CandidateEligibility, RecipeCandidate, RecipeExecutionRequest, RecipeExecutor,
        SelectionBudgets,
    },
    spin_preview::{SpinPreviewBoundary, SpinPreviewInput},
};
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
         (provider_account_id, provider, checkpoint_kind, label, state_sha256, captured_at)
         VALUES ($1, 'spotify', 'named_baseline', 'V020-07 fixture', $2, now())
         RETURNING id",
    )
    .bind(provider_account_id)
    .bind("d".repeat(64))
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
    let product_account_count: i64 = sqlx::query_scalar("SELECT count(*) FROM chordrift_accounts")
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
        sqlx::query_scalar("SELECT count(*) FROM chordrift_accounts")
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
        sqlx::query_scalar("SELECT count(*) FROM chordrift_accounts")
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

#[test]
fn uses_an_application_specific_database_secret_name() {
    assert_eq!(config::DATABASE_URL_VARIABLE, "CHORDRIFT_DATABASE_URL");
}
