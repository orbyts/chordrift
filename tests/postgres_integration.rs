use chordrift::{
    config, db, db_reports,
    intake::{self, IntakeState},
};
use serde_json::json;
use storexa::{DatabaseConfig, PostgresProvider};
use uuid::Uuid;

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
    assert_eq!(audit.items[0].state, IntakeState::AlreadyCovered);
    assert_eq!(audit.items[1].state, IntakeState::GenuinelyNew);
    assert_eq!(audit.items[2].state, IntakeState::KnownFromHistory);
    assert_eq!(audit.items[3].state, IntakeState::PreviouslyExcluded);

    sqlx::query("DELETE FROM provider_current_inventories WHERE provider_account_id = $1")
        .bind(account_id)
        .execute(database.pool())
        .await?;
    sqlx::query("DELETE FROM provider_library_snapshots WHERE id = $1")
        .bind(snapshot_id)
        .execute(database.pool())
        .await?;
    sqlx::query("DELETE FROM provider_accounts WHERE id = $1")
        .bind(account_id)
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

    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'",
    )
    .fetch_all(database.pool())
    .await?;
    for expected in [
        "albums",
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

#[test]
fn uses_an_application_specific_database_secret_name() {
    assert_eq!(config::DATABASE_URL_VARIABLE, "CHORDRIFT_DATABASE_URL");
}
