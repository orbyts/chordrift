use chordrift::{config, db};
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

    let report = db::migrate(&database).await?;
    assert_eq!(report.available, 20);

    let status = db::status(&database).await?;
    assert_eq!(status.available_migrations, 20);
    assert_eq!(status.applied_migrations, 20);
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
        "enrichment_runs",
        "excluded_tracks",
        "listening_events",
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
        "provider_library_snapshots",
        "provider_playlist_snapshots",
        "provider_playlist_tracks",
        "provider_playlists",
        "provider_saved_tracks",
        "provider_tracks",
        "signal_generations",
        "spotify_archive_imports",
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
          played_at, ms_played, source_occurrence)
         VALUES ($1, 'spotify', 'track-1', 'archive-a',
                 '2026-08-20T04:33:23Z', 12345, 0)",
    )
    .bind(account_id)
    .execute(database.pool())
    .await?;
    let duplicate = sqlx::query(
        "INSERT INTO listening_events
         (provider_account_id, provider, provider_track_id, source_event_id,
          played_at, ms_played, source_occurrence)
         VALUES ($1, 'spotify', 'track-1', 'archive-b',
                 '2026-08-20T04:33:23Z', 12345, 0)
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

    database.close().await;
    Ok(())
}

#[test]
fn uses_an_application_specific_database_secret_name() {
    assert_eq!(config::DATABASE_URL_VARIABLE, "CHORDRIFT_DATABASE_URL");
}
