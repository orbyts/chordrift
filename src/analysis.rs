//! Account-scoped analysis of the latest canonical provider snapshot.

use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result};

/// Aggregate state calculated from one account's latest snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalysisSummary {
    /// Snapshot used for every statistic in this report.
    pub snapshot_id: Uuid,
    /// Accessible playlists represented in the snapshot.
    pub playlists: i64,
    /// Ordered playlist entries, including duplicates.
    pub playlist_entries: i64,
    /// Distinct canonical tracks appearing in playlists.
    pub unique_playlist_tracks: i64,
    /// Saved-track entries represented in the snapshot.
    pub saved_tracks: i64,
    /// Canonical tracks appearing in more than one playlist.
    pub overlapping_tracks: i64,
    /// Extra within-playlist entries beyond one per canonical track.
    pub duplicate_entries: i64,
}

/// One canonical track's current playlist overlap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverlapRow {
    /// Canonical track identity.
    pub track_id: Uuid,
    /// Current canonical title.
    pub title: String,
    /// Ordered display artist string.
    pub artists: String,
    /// Number of distinct playlists containing the track.
    pub playlist_count: i32,
    /// Total entries across those playlists.
    pub total_entries: i32,
    /// Whether the track is also in the saved library.
    pub saved: bool,
}

/// One duplicate canonical membership within a playlist.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DuplicateRow {
    /// Provider playlist name at snapshot time.
    pub playlist_name: String,
    /// Canonical track title.
    pub track_title: String,
    /// Number of entries for that canonical track in the playlist.
    pub entries: i64,
}

/// Recalculates account-scoped statistics from the latest immutable snapshot.
pub async fn refresh(database: &Database, account_label: &str) -> Result<AnalysisSummary> {
    let (account_id, snapshot_id) = latest_snapshot(database, account_label).await?;
    let mut transaction = database.pool().begin().await?;
    sqlx::query("DELETE FROM account_track_statistics WHERE provider_account_id = $1")
        .bind(account_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "WITH playlist_stats AS (
             SELECT provider_track.track_id,
                    count(DISTINCT membership.provider_playlist_id)::integer AS playlist_count,
                    count(*)::integer AS total_entries
             FROM provider_playlist_tracks membership
             JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
             WHERE membership.snapshot_id = $2
             GROUP BY provider_track.track_id
         ), saved AS (
             SELECT DISTINCT provider_track.track_id
             FROM provider_saved_tracks membership
             JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
             WHERE membership.snapshot_id = $2
         )
         INSERT INTO account_track_statistics
             (provider_account_id, track_id, playlist_occurrence_count,
              total_playlist_entries, saved_in_library,
              calculated_from_snapshot_id)
         SELECT $1, COALESCE(playlist.track_id, saved.track_id),
                COALESCE(playlist.playlist_count, 0),
                COALESCE(playlist.total_entries, 0),
                saved.track_id IS NOT NULL, $2
         FROM playlist_stats playlist
         FULL OUTER JOIN saved ON saved.track_id = playlist.track_id",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO account_analysis_state
             (provider_account_id, calculated_from_snapshot_id, calculated_at)
         VALUES ($1, $2, now())
         ON CONFLICT (provider_account_id) DO UPDATE SET
           calculated_from_snapshot_id = EXCLUDED.calculated_from_snapshot_id,
           calculated_at = now()",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    summary(database, account_label).await
}

/// Returns aggregate statistics for the latest analyzed snapshot.
pub async fn summary(database: &Database, account_label: &str) -> Result<AnalysisSummary> {
    let (account_id, snapshot_id) = latest_snapshot(database, account_label).await?;
    let calculated_snapshot: Option<Uuid> = sqlx::query_scalar(
        "SELECT calculated_from_snapshot_id
         FROM account_analysis_state WHERE provider_account_id = $1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?;
    if calculated_snapshot != Some(snapshot_id) {
        return Err(ChordriftError::Configuration(
            "analysis is stale; run `chordrift analyze refresh` or `chordrift sync pull`"
                .to_owned(),
        ));
    }
    let row = sqlx::query(
        "SELECT
           (SELECT count(*) FROM provider_playlist_snapshots WHERE snapshot_id = $1) AS playlists,
           (SELECT count(*) FROM provider_playlist_tracks WHERE snapshot_id = $1) AS playlist_entries,
           (SELECT count(DISTINCT provider_track.track_id)
              FROM provider_playlist_tracks membership
              JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
             WHERE membership.snapshot_id = $1) AS unique_playlist_tracks,
           (SELECT count(*) FROM provider_saved_tracks WHERE snapshot_id = $1) AS saved_tracks,
           (SELECT count(*) FROM account_track_statistics
             WHERE provider_account_id = $2 AND playlist_occurrence_count > 1) AS overlapping_tracks,
           (SELECT COALESCE(sum(duplicates.entries - 1), 0)::bigint
              FROM (
                SELECT count(*) AS entries
                FROM provider_playlist_tracks membership
                JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
                WHERE membership.snapshot_id = $1
                GROUP BY membership.provider_playlist_id, provider_track.track_id
                HAVING count(*) > 1
              ) duplicates) AS duplicate_entries",
    )
    .bind(snapshot_id)
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    Ok(AnalysisSummary {
        snapshot_id,
        playlists: row.try_get("playlists")?,
        playlist_entries: row.try_get("playlist_entries")?,
        unique_playlist_tracks: row.try_get("unique_playlist_tracks")?,
        saved_tracks: row.try_get("saved_tracks")?,
        overlapping_tracks: row.try_get("overlapping_tracks")?,
        duplicate_entries: row.try_get("duplicate_entries")?,
    })
}

/// Lists the most widely shared canonical tracks across current playlists.
pub async fn overlap(
    database: &Database,
    account_label: &str,
    limit: u32,
) -> Result<Vec<OverlapRow>> {
    let (account_id, snapshot_id) = latest_snapshot(database, account_label).await?;
    ensure_current_analysis(database, account_id, snapshot_id).await?;
    let rows = sqlx::query(
        "SELECT statistics.track_id, track.title,
                COALESCE(string_agg(artist.name, ', ' ORDER BY track_artist.position), '') AS artists,
                statistics.playlist_occurrence_count,
                statistics.total_playlist_entries,
                statistics.saved_in_library
         FROM account_track_statistics statistics
         JOIN tracks track ON track.id = statistics.track_id
         LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
         LEFT JOIN artists artist ON artist.id = track_artist.artist_id
         WHERE statistics.provider_account_id = $1
           AND statistics.playlist_occurrence_count > 1
         GROUP BY statistics.track_id, track.title,
                  statistics.playlist_occurrence_count,
                  statistics.total_playlist_entries,
                  statistics.saved_in_library
         ORDER BY statistics.playlist_occurrence_count DESC,
                  statistics.total_playlist_entries DESC, lower(track.title)
         LIMIT $2",
    )
    .bind(account_id)
    .bind(i64::from(limit))
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(OverlapRow {
                track_id: row.try_get("track_id")?,
                title: row.try_get("title")?,
                artists: row.try_get("artists")?,
                playlist_count: row.try_get("playlist_occurrence_count")?,
                total_entries: row.try_get("total_playlist_entries")?,
                saved: row.try_get("saved_in_library")?,
            })
        })
        .collect()
}

/// Lists duplicate canonical memberships within individual current playlists.
pub async fn duplicates(
    database: &Database,
    account_label: &str,
    limit: u32,
) -> Result<Vec<DuplicateRow>> {
    let (account_id, snapshot_id) = latest_snapshot(database, account_label).await?;
    ensure_current_analysis(database, account_id, snapshot_id).await?;
    let rows = sqlx::query(
        "SELECT snapshot.name AS playlist_name, track.title AS track_title,
                count(*) AS entries
         FROM provider_playlist_tracks membership
         JOIN provider_playlist_snapshots snapshot
           ON snapshot.snapshot_id = membership.snapshot_id
          AND snapshot.provider_playlist_id = membership.provider_playlist_id
         JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
         JOIN tracks track ON track.id = provider_track.track_id
         WHERE membership.snapshot_id = $1
         GROUP BY membership.provider_playlist_id, snapshot.name,
                  provider_track.track_id, track.title
         HAVING count(*) > 1
         ORDER BY count(*) DESC, lower(snapshot.name), lower(track.title)
         LIMIT $2",
    )
    .bind(snapshot_id)
    .bind(i64::from(limit))
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(DuplicateRow {
                playlist_name: row.try_get("playlist_name")?,
                track_title: row.try_get("track_title")?,
                entries: row.try_get("entries")?,
            })
        })
        .collect()
}

async fn latest_snapshot(database: &Database, account_label: &str) -> Result<(Uuid, Uuid)> {
    sqlx::query_as(
        "SELECT account.id, snapshot.id
         FROM provider_accounts account
         JOIN LATERAL (
             SELECT id FROM provider_library_snapshots
             WHERE provider_account_id = account.id
             ORDER BY captured_at DESC, id DESC LIMIT 1
         ) snapshot ON TRUE
         WHERE account.provider = 'spotify' AND account.account_label = $1",
    )
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(format!(
            "Spotify account {account_label:?} has no imported snapshot"
        ))
    })
}

async fn ensure_current_analysis(
    database: &Database,
    account_id: Uuid,
    snapshot_id: Uuid,
) -> Result<()> {
    let current: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM account_analysis_state
             WHERE provider_account_id = $1 AND calculated_from_snapshot_id = $2
         )",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .fetch_one(database.pool())
    .await?;
    if current {
        Ok(())
    } else {
        Err(ChordriftError::Configuration(
            "analysis is stale; run `chordrift analyze refresh` or `chordrift sync pull`"
                .to_owned(),
        ))
    }
}
