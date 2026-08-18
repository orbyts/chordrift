use chordrift::{config, db};
use storexa::{DatabaseConfig, PostgresProvider};

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
    assert_eq!(report.available, 1);

    let status = db::status(&database).await?;
    assert_eq!(status.available_migrations, 1);
    assert_eq!(status.applied_migrations, 1);
    assert_eq!(status.pending_migrations, 0);
    assert_eq!(status.failed_migrations, 0);

    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'",
    )
    .fetch_all(database.pool())
    .await?;
    for expected in [
        "albums",
        "artists",
        "cluster_generations",
        "cluster_tracks",
        "clusters",
        "listening_events",
        "playlist_generations",
        "playlist_tracks",
        "playlists",
        "provider_albums",
        "provider_artists",
        "provider_library_snapshots",
        "provider_playlist_tracks",
        "provider_playlists",
        "provider_tracks",
        "sync_operations",
        "sync_runs",
        "track_artists",
        "track_embeddings",
        "track_matches",
        "track_statistics",
        "tracks",
    ] {
        assert!(tables.iter().any(|table| table == expected), "{expected}");
    }

    database.close().await;
    Ok(())
}

#[test]
fn uses_an_application_specific_database_secret_name() {
    assert_eq!(config::DATABASE_URL_VARIABLE, "CHORDRIFT_DATABASE_URL");
}
