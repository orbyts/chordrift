//! Read-only saved-album inventory and account-scoped cleanup policy.

use chrono::{DateTime, Utc};
use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result};

/// One currently saved album and its preservation coverage.
#[derive(Clone, Debug)]
pub struct SavedAlbumSummary {
    /// Stable Spotify album ID.
    pub spotify_id: String,
    /// Album title.
    pub title: String,
    /// First credited artist, when present.
    pub artist: Option<String>,
    /// When the album was saved.
    pub saved_at: Option<DateTime<Utc>>,
    /// Inventoried album tracks.
    pub tracks: i64,
    /// Tracks already present in saved songs or a current playlist.
    pub preserved: i64,
    /// Tracks explicitly excluded by the user.
    pub excluded: i64,
    /// Tracks requiring a keep-or-discard review before the album is unsaved.
    pub pending: i64,
}

/// Aggregate safety report for the current saved-album snapshot.
#[derive(Clone, Debug)]
pub struct AlbumAudit {
    /// Immutable source snapshot.
    pub snapshot_id: Uuid,
    /// Current account policy.
    pub policy: String,
    /// Saved albums.
    pub albums: i64,
    /// Distinct album tracks.
    pub unique_tracks: i64,
    /// Distinct tracks already preserved elsewhere.
    pub preserved: i64,
    /// Distinct tracks explicitly excluded.
    pub excluded: i64,
    /// Distinct unresolved tracks.
    pub pending: i64,
    /// Albums with no unresolved tracks.
    pub review_complete_albums: i64,
}

/// One ordered track within a currently saved album.
#[derive(Clone, Debug)]
pub struct SavedAlbumTrack {
    /// Zero-based provider order.
    pub position: i32,
    /// Track title.
    pub title: String,
    /// Credited artists.
    pub artists: String,
    /// Stable Spotify track ID.
    pub spotify_id: String,
    /// Current review disposition.
    pub disposition: String,
}

async fn account_and_snapshot(database: &Database, account: &str) -> Result<(Uuid, Uuid)> {
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
    .bind(account)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| ChordriftError::Configuration("Spotify account is not imported".to_owned()))
}

/// Lists every album in the latest immutable Spotify snapshot.
pub async fn list(database: &Database, account: &str) -> Result<Vec<SavedAlbumSummary>> {
    let (account_id, snapshot_id) = account_and_snapshot(database, account).await?;
    let rows = sqlx::query(
        "SELECT provider.provider_album_id, album.title,
                saved.metadata #>> '{artists,0,name}' AS artist, saved.saved_at,
                count(membership.*)::bigint AS tracks,
                count(membership.*) FILTER (WHERE NOT EXISTS (
                    SELECT 1 FROM excluded_tracks exclusion
                    JOIN provider_tracks track ON track.track_id = exclusion.track_id
                    WHERE exclusion.provider_account_id = $1
                      AND exclusion.restored_at IS NULL
                      AND track.id = membership.provider_track_id
                ) AND (
                    EXISTS (SELECT 1 FROM provider_saved_tracks st
                            WHERE st.snapshot_id = $2
                              AND st.provider_track_id = membership.provider_track_id)
                    OR EXISTS (
                        SELECT 1 FROM provider_playlist_tracks pt
                        JOIN provider_account_playlists ap
                          ON ap.provider_playlist_id = pt.provider_playlist_id
                         AND ap.provider_account_id = $1
                         AND ap.present_in_latest_snapshot
                        WHERE pt.snapshot_id = $2
                          AND pt.provider_track_id = membership.provider_track_id
                    )))::bigint AS preserved,
                count(membership.*) FILTER (WHERE EXISTS (
                    SELECT 1 FROM excluded_tracks exclusion
                    JOIN provider_tracks track ON track.track_id = exclusion.track_id
                    WHERE exclusion.provider_account_id = $1
                      AND exclusion.restored_at IS NULL
                      AND track.id = membership.provider_track_id
                ))::bigint AS excluded
         FROM provider_saved_albums saved
         JOIN provider_albums provider ON provider.id = saved.provider_album_id
         JOIN albums album ON album.id = provider.album_id
         LEFT JOIN provider_saved_album_tracks membership
           ON membership.snapshot_id = saved.snapshot_id
          AND membership.provider_album_id = saved.provider_album_id
         WHERE saved.snapshot_id = $2
         GROUP BY provider.provider_album_id, album.title, artist, saved.saved_at, saved.position
         ORDER BY saved.position",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            let tracks = row.try_get("tracks")?;
            let preserved = row.try_get("preserved")?;
            let excluded = row.try_get("excluded")?;
            Ok(SavedAlbumSummary {
                spotify_id: row.try_get("provider_album_id")?,
                title: row.try_get("title")?,
                artist: row.try_get("artist")?,
                saved_at: row.try_get("saved_at")?,
                tracks,
                preserved,
                excluded,
                pending: tracks - preserved - excluded,
            })
        })
        .collect()
}

/// Audits whether album tracks have an explicit durable disposition.
pub async fn audit(database: &Database, account: &str) -> Result<AlbumAudit> {
    let (account_id, snapshot_id) = account_and_snapshot(database, account).await?;
    let albums = list(database, account).await?;
    let policy = policy(database, account).await?;
    let row = sqlx::query(
        "WITH inventory AS (
             SELECT DISTINCT membership.provider_track_id
             FROM provider_saved_album_tracks membership
             WHERE membership.snapshot_id = $2
         ), disposition AS (
             SELECT inventory.provider_track_id,
                    EXISTS (
                      SELECT 1 FROM excluded_tracks exclusion
                      JOIN provider_tracks track ON track.track_id = exclusion.track_id
                      WHERE exclusion.provider_account_id = $1
                        AND exclusion.restored_at IS NULL
                        AND track.id = inventory.provider_track_id
                    ) AS excluded,
                    EXISTS (SELECT 1 FROM provider_saved_tracks st
                            WHERE st.snapshot_id = $2
                              AND st.provider_track_id = inventory.provider_track_id)
                    OR EXISTS (
                      SELECT 1 FROM provider_playlist_tracks pt
                      JOIN provider_account_playlists ap
                        ON ap.provider_playlist_id = pt.provider_playlist_id
                       AND ap.provider_account_id = $1
                       AND ap.present_in_latest_snapshot
                      WHERE pt.snapshot_id = $2
                        AND pt.provider_track_id = inventory.provider_track_id
                    ) AS preserved
             FROM inventory
         )
         SELECT count(*)::bigint AS unique_tracks,
                count(*) FILTER (WHERE preserved AND NOT excluded)::bigint AS preserved,
                count(*) FILTER (WHERE excluded)::bigint AS excluded,
                count(*) FILTER (WHERE NOT preserved AND NOT excluded)::bigint AS pending
         FROM disposition",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .fetch_one(database.pool())
    .await?;
    Ok(AlbumAudit {
        snapshot_id,
        policy,
        albums: albums.len() as i64,
        unique_tracks: row.try_get("unique_tracks")?,
        preserved: row.try_get("preserved")?,
        excluded: row.try_get("excluded")?,
        pending: row.try_get("pending")?,
        review_complete_albums: albums.iter().filter(|album| album.pending == 0).count() as i64,
    })
}

/// Lists ordered tracks for one exact saved album ID or unambiguous title.
pub async fn tracks(
    database: &Database,
    account: &str,
    name: Option<&str>,
    spotify_id: Option<&str>,
) -> Result<Vec<SavedAlbumTrack>> {
    let (account_id, snapshot_id) = account_and_snapshot(database, account).await?;
    let rows = sqlx::query(
        "SELECT membership.position, track.title, provider_track.provider_track_id,
                COALESCE(string_agg(artist.name, ', ' ORDER BY credit.position), '-') AS artists,
                CASE
                  WHEN EXISTS (SELECT 1 FROM excluded_tracks exclusion
                               WHERE exclusion.provider_account_id = $1
                                 AND exclusion.track_id = track.id
                                 AND exclusion.restored_at IS NULL) THEN 'excluded'
                  WHEN EXISTS (SELECT 1 FROM provider_saved_tracks st
                               WHERE st.snapshot_id = $2
                                 AND st.provider_track_id = provider_track.id)
                    OR EXISTS (
                       SELECT 1 FROM provider_playlist_tracks pt
                       JOIN provider_account_playlists ap
                         ON ap.provider_playlist_id = pt.provider_playlist_id
                        AND ap.provider_account_id = $1
                        AND ap.present_in_latest_snapshot
                       WHERE pt.snapshot_id = $2
                         AND pt.provider_track_id = provider_track.id
                    ) THEN 'preserved'
                  ELSE 'review'
                END AS disposition
         FROM provider_saved_albums saved
         JOIN provider_albums provider_album ON provider_album.id = saved.provider_album_id
         JOIN albums album ON album.id = provider_album.album_id
         JOIN provider_saved_album_tracks membership
           ON membership.snapshot_id = saved.snapshot_id
          AND membership.provider_album_id = saved.provider_album_id
         JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
         JOIN tracks track ON track.id = provider_track.track_id
         LEFT JOIN track_artists credit ON credit.track_id = track.id
         LEFT JOIN artists artist ON artist.id = credit.artist_id
         WHERE saved.snapshot_id = $2
           AND (($3::text IS NOT NULL AND provider_album.provider_album_id = $3)
             OR ($3::text IS NULL AND lower(album.title) = lower($4)))
         GROUP BY membership.position, track.id, track.title,
                  provider_track.id, provider_track.provider_track_id
         ORDER BY membership.position",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .bind(spotify_id)
    .bind(name)
    .fetch_all(database.pool())
    .await?;
    if rows.is_empty() {
        return Err(ChordriftError::Configuration(
            "no matching currently saved album exists".to_owned(),
        ));
    }
    rows.into_iter()
        .map(|row| {
            Ok(SavedAlbumTrack {
                position: row.try_get("position")?,
                title: row.try_get("title")?,
                artists: row.try_get("artists")?,
                spotify_id: row.try_get("provider_track_id")?,
                disposition: row.try_get("disposition")?,
            })
        })
        .collect()
}

/// Returns the account policy, defaulting safely to preserve.
pub async fn policy(database: &Database, account: &str) -> Result<String> {
    let (account_id, _) = account_and_snapshot(database, account).await?;
    Ok(sqlx::query_scalar(
        "SELECT saved_album_policy FROM provider_account_library_policies
         WHERE provider_account_id = $1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .unwrap_or_else(|| "preserve".to_owned()))
}

/// Explicitly sets one account's saved-album policy without changing Spotify.
pub async fn set_policy(database: &Database, account: &str, value: &str) -> Result<()> {
    let (account_id, _) = account_and_snapshot(database, account).await?;
    sqlx::query(
        "INSERT INTO provider_account_library_policies
             (provider_account_id, saved_album_policy)
         VALUES ($1, $2)
         ON CONFLICT (provider_account_id) DO UPDATE SET
           saved_album_policy = EXCLUDED.saved_album_policy, updated_at = now()",
    )
    .bind(account_id)
    .bind(value)
    .execute(database.pool())
    .await?;
    Ok(())
}

/// Explicitly configures whether verified saved tracks remain in Liked Songs.
pub async fn set_saved_track_policy(database: &Database, account: &str, value: &str) -> Result<()> {
    let (account_id, _) = account_and_snapshot(database, account).await?;
    sqlx::query(
        "INSERT INTO provider_account_library_policies
             (provider_account_id, saved_track_clear_policy)
         VALUES ($1, $2)
         ON CONFLICT (provider_account_id) DO UPDATE SET
           saved_track_clear_policy = EXCLUDED.saved_track_clear_policy, updated_at = now()",
    )
    .bind(account_id)
    .bind(value)
    .execute(database.pool())
    .await?;
    Ok(())
}
