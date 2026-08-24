//! Durable external playlist bookmarks kept outside the active music library.

use chrono::{DateTime, Utc};
use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result};

/// Selects one bookmark by stable provider ID or its current display name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BookmarkSelector {
    /// Select by exact Spotify playlist ID.
    ProviderId(String),
    /// Select by case-insensitive name; the match must be unambiguous.
    Name(String),
}

/// Current durable metadata for one externally owned playlist.
#[derive(Clone, Debug, PartialEq)]
pub struct BookmarkRecord {
    /// Stable Spotify playlist ID.
    pub provider_playlist_id: String,
    /// Latest known display name.
    pub name: String,
    /// Spotify owner ID.
    pub owner_provider_id: String,
    /// Owner display name, when Spotify supplied one.
    pub owner_display_name: Option<String>,
    /// How the account encountered the external playlist.
    pub relationship: String,
    /// Whether Spotify still reports it in the user's playlist inventory.
    pub present: bool,
    /// Availability of the latest observed contents.
    pub content_status: String,
    /// Item count reported by Spotify.
    pub item_count: i32,
    /// Public provider link, when supplied.
    pub provider_url: Option<String>,
    /// When Spotify last exposed this bookmark to Chordrift.
    pub last_seen_at: DateTime<Utc>,
    /// When its Spotify snapshot signature last changed.
    pub last_changed_at: DateTime<Utc>,
}

/// One ordered track retained in a bookmark snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookmarkTrackRecord {
    /// Zero-based provider position.
    pub position: i32,
    /// Canonical title.
    pub title: String,
    /// Ordered display artists.
    pub artists: String,
    /// Album title, when known.
    pub album: Option<String>,
    /// Stable Spotify track ID.
    pub provider_track_id: String,
}

/// Last readable contents retained for one bookmark.
#[derive(Clone, Debug, PartialEq)]
pub struct BookmarkTracks {
    /// Durable bookmark metadata.
    pub bookmark: BookmarkRecord,
    /// Immutable provider pull that captured these contents.
    pub snapshot_id: Uuid,
    /// When the retained observation was captured.
    pub captured_at: DateTime<Utc>,
    /// Ordered retained entries.
    pub tracks: Vec<BookmarkTrackRecord>,
}

/// Lists all bookmarks, including those no longer present in the provider library.
pub async fn list(database: &Database, account_label: &str) -> Result<Vec<BookmarkRecord>> {
    let rows = sqlx::query(
        "SELECT bookmark.provider_playlist_id, bookmark.name,
                bookmark.owner_provider_id, bookmark.owner_display_name,
                bookmark.relationship, bookmark.present_in_provider_library,
                bookmark.content_status, bookmark.item_count, bookmark.provider_url,
                bookmark.last_seen_at, bookmark.last_changed_at
         FROM external_playlist_bookmarks bookmark
         JOIN provider_accounts account ON account.id = bookmark.provider_account_id
         WHERE account.provider = 'spotify' AND account.account_label = $1
         ORDER BY bookmark.present_in_provider_library DESC,
                  lower(bookmark.name), bookmark.provider_playlist_id",
    )
    .bind(account_label)
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| bookmark_from_row(&row))
        .collect()
}

/// Returns the newest complete contents retained for one bookmark.
pub async fn tracks(
    database: &Database,
    account_label: &str,
    selector: &BookmarkSelector,
) -> Result<BookmarkTracks> {
    let candidates = match selector {
        BookmarkSelector::ProviderId(id) => {
            sqlx::query(
                "SELECT bookmark.id, bookmark.provider_playlist_id, bookmark.name,
                        bookmark.owner_provider_id, bookmark.owner_display_name,
                        bookmark.relationship, bookmark.present_in_provider_library,
                        bookmark.content_status, bookmark.item_count, bookmark.provider_url,
                        bookmark.last_seen_at, bookmark.last_changed_at
                 FROM external_playlist_bookmarks bookmark
                 JOIN provider_accounts account ON account.id = bookmark.provider_account_id
                 WHERE account.provider = 'spotify' AND account.account_label = $1
                   AND bookmark.provider_playlist_id = $2",
            )
            .bind(account_label)
            .bind(id)
            .fetch_all(database.pool())
            .await?
        }
        BookmarkSelector::Name(name) => {
            sqlx::query(
                "SELECT bookmark.id, bookmark.provider_playlist_id, bookmark.name,
                        bookmark.owner_provider_id, bookmark.owner_display_name,
                        bookmark.relationship, bookmark.present_in_provider_library,
                        bookmark.content_status, bookmark.item_count, bookmark.provider_url,
                        bookmark.last_seen_at, bookmark.last_changed_at
                 FROM external_playlist_bookmarks bookmark
                 JOIN provider_accounts account ON account.id = bookmark.provider_account_id
                 WHERE account.provider = 'spotify' AND account.account_label = $1
                   AND lower(bookmark.name) = lower($2)",
            )
            .bind(account_label)
            .bind(name)
            .fetch_all(database.pool())
            .await?
        }
    };
    if candidates.is_empty() {
        return Err(ChordriftError::Configuration(
            "external playlist bookmark was not found for this account".to_owned(),
        ));
    }
    if candidates.len() != 1 {
        return Err(ChordriftError::Configuration(
            "bookmark name is ambiguous; select it with --spotify-id".to_owned(),
        ));
    }
    let row = &candidates[0];
    let bookmark_id: Uuid = row.try_get("id")?;
    let bookmark = bookmark_from_row(row)?;
    let snapshot = sqlx::query(
        "SELECT snapshot_id, captured_at
         FROM external_playlist_bookmark_snapshots
         WHERE bookmark_id = $1 AND content_status = 'complete'
         ORDER BY captured_at DESC, snapshot_id DESC LIMIT 1",
    )
    .bind(bookmark_id)
    .fetch_optional(database.pool())
    .await?;
    let Some(snapshot) = snapshot else {
        return Err(ChordriftError::Configuration(format!(
            "Spotify has not exposed readable contents for bookmark '{}' (status: {})",
            bookmark.name, bookmark.content_status
        )));
    };
    let snapshot_id: Uuid = snapshot.try_get("snapshot_id")?;
    let captured_at: DateTime<Utc> = snapshot.try_get("captured_at")?;
    let rows = sqlx::query(
        "SELECT entry.position, track.title, album.title AS album,
                provider_track.provider_track_id,
                COALESCE(artists.names, '') AS artists
         FROM external_playlist_bookmark_tracks entry
         JOIN provider_tracks provider_track ON provider_track.id = entry.provider_track_id
         JOIN tracks track ON track.id = provider_track.track_id
         LEFT JOIN albums album ON album.id = track.album_id
         LEFT JOIN LATERAL (
           SELECT string_agg(artist.name, ', ' ORDER BY link.position) AS names
           FROM track_artists link
           JOIN artists artist ON artist.id = link.artist_id
           WHERE link.track_id = track.id
         ) artists ON TRUE
         WHERE entry.snapshot_id = $1 AND entry.bookmark_id = $2
         ORDER BY entry.position",
    )
    .bind(snapshot_id)
    .bind(bookmark_id)
    .fetch_all(database.pool())
    .await?;
    let tracks = rows
        .into_iter()
        .map(|row| {
            Ok(BookmarkTrackRecord {
                position: row.try_get("position")?,
                title: row.try_get("title")?,
                artists: row.try_get("artists")?,
                album: row.try_get("album")?,
                provider_track_id: row.try_get("provider_track_id")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(BookmarkTracks {
        bookmark,
        snapshot_id,
        captured_at,
        tracks,
    })
}

fn bookmark_from_row(row: &sqlx::postgres::PgRow) -> Result<BookmarkRecord> {
    Ok(BookmarkRecord {
        provider_playlist_id: row.try_get("provider_playlist_id")?,
        name: row.try_get("name")?,
        owner_provider_id: row.try_get("owner_provider_id")?,
        owner_display_name: row.try_get("owner_display_name")?,
        relationship: row.try_get("relationship")?,
        present: row.try_get("present_in_provider_library")?,
        content_status: row.try_get("content_status")?,
        item_count: row.try_get("item_count")?,
        provider_url: row.try_get("provider_url")?,
        last_seen_at: row.try_get("last_seen_at")?,
        last_changed_at: row.try_get("last_changed_at")?,
    })
}
