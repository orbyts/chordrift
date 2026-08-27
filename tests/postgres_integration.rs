use chordrift::{config, db, db_reports};
use serde_json::json;
use storexa::{DatabaseConfig, PostgresProvider};
use uuid::Uuid;

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
