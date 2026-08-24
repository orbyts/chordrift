//! Durable external playlist bookmarks kept outside the active music library.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
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

/// One track returned by an explicit provider bookmark refresh.
#[derive(Clone, Debug, PartialEq)]
pub struct FetchedBookmarkItem {
    /// Zero-based provider position.
    pub position: i32,
    /// Stable Spotify track ID.
    pub provider_track_id: String,
    /// Track title.
    pub title: String,
    /// Ordered display artists.
    pub artists: String,
    /// Album title, when supplied.
    pub album: Option<String>,
    /// Time the source playlist added the item, when supplied.
    pub added_at: Option<DateTime<Utc>>,
    /// Public Spotify track link, when supplied.
    pub provider_url: Option<String>,
}

/// Readable metadata and membership returned for one bookmark.
#[derive(Clone, Debug, PartialEq)]
pub struct FetchedBookmark {
    /// Latest provider name.
    pub name: String,
    /// Stable owner ID.
    pub owner_provider_id: String,
    /// Owner display name, when supplied.
    pub owner_display_name: Option<String>,
    /// Public playlist link, when supplied.
    pub provider_url: Option<String>,
    /// Provider snapshot signature, when supplied.
    pub provider_snapshot_id: Option<String>,
    /// Provider public flag.
    pub public: Option<bool>,
    /// Whether the account collaborates on the playlist.
    pub collaborative: bool,
    /// Provider-reported item count.
    pub item_count: usize,
    /// Unavailable items skipped from the retained list.
    pub unavailable_items: usize,
    /// Unsupported local or non-track items skipped from the retained list.
    pub unsupported_items: usize,
    /// Readable supported tracks.
    pub items: Vec<FetchedBookmarkItem>,
}

/// Outcome of an explicit, targeted provider request.
#[derive(Clone, Debug, PartialEq)]
pub enum BookmarkFetchOutcome {
    /// Metadata and every accessible supported track were returned.
    Complete(FetchedBookmark),
    /// Spotify denied the playlist or its items to this application.
    Inaccessible {
        /// Provider HTTP status.
        http_status: u16,
    },
    /// Spotify no longer exposed the playlist ID.
    NotFound {
        /// Provider HTTP status.
        http_status: u16,
    },
}

/// Result of recording one explicit bookmark refresh attempt.
#[derive(Clone, Debug, PartialEq)]
pub struct RefreshReport {
    /// Immutable refresh identity.
    pub refresh_id: Uuid,
    /// Stable Spotify playlist ID.
    pub provider_playlist_id: String,
    /// Bookmark display name after the refresh.
    pub name: String,
    /// Complete, inaccessible, or not_found.
    pub status: String,
    /// Provider-reported item count.
    pub item_count: usize,
    /// Readable supported tracks retained.
    pub captured_items: usize,
    /// Unavailable items skipped.
    pub unavailable_items: usize,
    /// Unsupported items skipped.
    pub unsupported_items: usize,
    /// Refresh time.
    pub refreshed_at: DateTime<Utc>,
}

/// Immutable review batch for provider-library cleanup.
#[derive(Clone, Debug, PartialEq)]
pub struct CleanupBatch {
    /// Stable batch identifier used for explicit approval.
    pub batch_id: Uuid,
    /// Provider snapshot at which the candidates were observed.
    pub source_snapshot_id: Uuid,
    /// Pending, approved, or superseded state.
    pub state: String,
    /// Number of external relationships in the batch.
    pub candidate_count: i32,
    /// Stable hash of the complete candidate set.
    pub input_hash: String,
    /// Whether an identical batch already existed.
    pub reused: bool,
    /// When approval was recorded, if approved.
    pub approved_at: Option<DateTime<Utc>>,
    /// When the immutable batch was created.
    pub created_at: DateTime<Utc>,
}

/// One exact external relationship captured in a cleanup batch.
#[derive(Clone, Debug, PartialEq)]
pub struct CleanupItem {
    /// Stable Spotify playlist ID.
    pub provider_playlist_id: String,
    /// Playlist name at review time.
    pub name: String,
    /// Source owner ID.
    pub owner_provider_id: String,
    /// Preservation availability at review time.
    pub content_status: String,
    /// Item count reported by Spotify.
    pub item_count: i32,
    /// Spotify snapshot signature expected before cleanup.
    pub provider_snapshot_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct CleanupCandidate {
    bookmark_id: Uuid,
    provider_playlist_id: String,
    provider_snapshot_id: Option<String>,
    name: String,
    owner_provider_id: String,
    content_status: String,
    item_count: i32,
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

/// Resolves exactly one durable bookmark without contacting Spotify.
pub async fn resolve(
    database: &Database,
    account_label: &str,
    selector: &BookmarkSelector,
) -> Result<BookmarkRecord> {
    let rows = list(database, account_label).await?;
    let matches: Vec<_> = rows
        .into_iter()
        .filter(|bookmark| match selector {
            BookmarkSelector::ProviderId(id) => bookmark.provider_playlist_id == *id,
            BookmarkSelector::Name(name) => bookmark.name.eq_ignore_ascii_case(name),
        })
        .collect();
    match matches.len() {
        0 => Err(ChordriftError::Configuration(
            "external playlist bookmark was not found for this account".to_owned(),
        )),
        1 => Ok(matches.into_iter().next().expect("one bookmark")),
        _ => Err(ChordriftError::Configuration(
            "bookmark name is ambiguous; select it with --spotify-id".to_owned(),
        )),
    }
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
    let source = sqlx::query(
        "SELECT source, source_id, captured_at FROM (
             SELECT 'pull'::text AS source, snapshot_id AS source_id, captured_at
             FROM external_playlist_bookmark_snapshots
             WHERE bookmark_id = $1 AND content_status = 'complete'
             UNION ALL
             SELECT 'refresh'::text, id, refreshed_at
             FROM external_playlist_bookmark_refreshes
             WHERE bookmark_id = $1 AND status = 'complete'
         ) readable ORDER BY captured_at DESC, source_id DESC LIMIT 1",
    )
    .bind(bookmark_id)
    .fetch_optional(database.pool())
    .await?;
    let Some(source) = source else {
        return Err(ChordriftError::Configuration(format!(
            "Spotify has not exposed readable contents for bookmark '{}' (status: {})",
            bookmark.name, bookmark.content_status
        )));
    };
    let source_id: Uuid = source.try_get("source_id")?;
    let captured_at: DateTime<Utc> = source.try_get("captured_at")?;
    let source_kind: String = source.try_get("source")?;
    let tracks = if source_kind == "refresh" {
        sqlx::query(
            "SELECT position, title, artists, album, provider_track_id
             FROM external_playlist_bookmark_refresh_tracks
             WHERE refresh_id = $1 ORDER BY position",
        )
        .bind(source_id)
        .fetch_all(database.pool())
        .await?
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
        .collect::<Result<Vec<_>>>()?
    } else {
        sqlx::query(
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
        .bind(source_id)
        .bind(bookmark_id)
        .fetch_all(database.pool())
        .await?
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
        .collect::<Result<Vec<_>>>()?
    };
    Ok(BookmarkTracks {
        bookmark,
        snapshot_id: source_id,
        captured_at,
        tracks,
    })
}

/// Persists one explicit targeted refresh attempt without changing active library state.
pub async fn record_refresh(
    database: &Database,
    account_label: &str,
    provider_playlist_id: &str,
    outcome: BookmarkFetchOutcome,
) -> Result<RefreshReport> {
    let row = sqlx::query(
        "SELECT bookmark.id, bookmark.name
         FROM external_playlist_bookmarks bookmark
         JOIN provider_accounts account ON account.id = bookmark.provider_account_id
         WHERE account.provider = 'spotify' AND account.account_label = $1
           AND bookmark.provider_playlist_id = $2",
    )
    .bind(account_label)
    .bind(provider_playlist_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(
            "external playlist bookmark was not found for this account".to_owned(),
        )
    })?;
    let bookmark_id: Uuid = row.try_get("id")?;
    let previous_name: String = row.try_get("name")?;
    let mut transaction = database.pool().begin().await?;
    let (status, item_count, captured, unavailable, unsupported, http_status, metadata) =
        match &outcome {
            BookmarkFetchOutcome::Complete(fetched) => (
                "complete",
                fetched.item_count,
                fetched.items.len(),
                fetched.unavailable_items,
                fetched.unsupported_items,
                None,
                json!({"name": fetched.name, "owner_provider_id": fetched.owner_provider_id,
                    "owner_display_name": fetched.owner_display_name, "provider_url": fetched.provider_url,
                    "public": fetched.public, "collaborative": fetched.collaborative}),
            ),
            BookmarkFetchOutcome::Inaccessible { http_status } => {
                ("inaccessible", 0, 0, 0, 0, Some(*http_status), json!({}))
            }
            BookmarkFetchOutcome::NotFound { http_status } => {
                ("not_found", 0, 0, 0, 0, Some(*http_status), json!({}))
            }
        };
    let provider_snapshot_id = match &outcome {
        BookmarkFetchOutcome::Complete(fetched) => fetched.provider_snapshot_id.as_deref(),
        _ => None,
    };
    let refresh = sqlx::query(
        "INSERT INTO external_playlist_bookmark_refreshes
         (bookmark_id, status, provider_snapshot_id, item_count,
          captured_item_count, unavailable_item_count, unsupported_item_count,
          http_status, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id, refreshed_at",
    )
    .bind(bookmark_id)
    .bind(status)
    .bind(provider_snapshot_id)
    .bind(to_i32(item_count, "bookmark item count")?)
    .bind(to_i32(captured, "captured bookmark item count")?)
    .bind(to_i32(unavailable, "unavailable bookmark item count")?)
    .bind(to_i32(unsupported, "unsupported bookmark item count")?)
    .bind(http_status.map(i32::from))
    .bind(metadata)
    .fetch_one(&mut *transaction)
    .await?;
    let refresh_id: Uuid = refresh.try_get("id")?;
    let refreshed_at: DateTime<Utc> = refresh.try_get("refreshed_at")?;
    let name = if let BookmarkFetchOutcome::Complete(fetched) = &outcome {
        sqlx::query(
            "UPDATE external_playlist_bookmarks SET
               name = $2, owner_provider_id = $3, owner_display_name = $4,
               provider_url = $5, provider_snapshot_id = $6, public = $7,
               collaborative = $8, item_count = $9, content_status = 'complete',
               last_changed_at = CASE
                 WHEN provider_snapshot_id IS DISTINCT FROM $6 THEN now()
                 ELSE last_changed_at END,
               last_checked_at = now(), updated_at = now()
             WHERE id = $1",
        )
        .bind(bookmark_id)
        .bind(&fetched.name)
        .bind(&fetched.owner_provider_id)
        .bind(&fetched.owner_display_name)
        .bind(&fetched.provider_url)
        .bind(&fetched.provider_snapshot_id)
        .bind(fetched.public)
        .bind(fetched.collaborative)
        .bind(to_i32(fetched.item_count, "bookmark item count")?)
        .execute(&mut *transaction)
        .await?;
        for item in &fetched.items {
            sqlx::query(
                "INSERT INTO external_playlist_bookmark_refresh_tracks
                 (refresh_id, position, provider_track_id, title, artists,
                  album, added_at, provider_url)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(refresh_id)
            .bind(item.position)
            .bind(&item.provider_track_id)
            .bind(&item.title)
            .bind(&item.artists)
            .bind(&item.album)
            .bind(item.added_at)
            .bind(&item.provider_url)
            .execute(&mut *transaction)
            .await?;
        }
        fetched.name.clone()
    } else {
        sqlx::query(
            "UPDATE external_playlist_bookmarks
             SET last_checked_at = now(), updated_at = now() WHERE id = $1",
        )
        .bind(bookmark_id)
        .execute(&mut *transaction)
        .await?;
        previous_name
    };
    transaction.commit().await?;
    Ok(RefreshReport {
        refresh_id,
        provider_playlist_id: provider_playlist_id.to_owned(),
        name,
        status: status.to_owned(),
        item_count,
        captured_items: captured,
        unavailable_items: unavailable,
        unsupported_items: unsupported,
        refreshed_at,
    })
}

/// Creates or reuses an immutable batch containing every present bookmark.
pub async fn cleanup_plan(database: &Database, account_label: &str) -> Result<CleanupBatch> {
    let (account_id, source_snapshot_id) = account_and_snapshot(database, account_label).await?;
    let candidates = cleanup_candidates(database, account_id).await?;
    if candidates.is_empty() {
        return Err(ChordriftError::Configuration(
            "no present external playlist bookmarks require cleanup".to_owned(),
        ));
    }
    let input_hash = hex_sha256(&serde_json::to_vec(&json!({
        "provider": "spotify",
        "candidates": candidates,
    }))?);
    if let Some(row) = sqlx::query(
        "SELECT id, source_snapshot_id, state, candidate_count, input_hash,
                approved_at, created_at
         FROM external_playlist_cleanup_batches
         WHERE provider_account_id = $1 AND input_hash = $2",
    )
    .bind(account_id)
    .bind(&input_hash)
    .fetch_optional(database.pool())
    .await?
    {
        return cleanup_batch_from_row(&row, true);
    }

    let mut transaction = database.pool().begin().await?;
    let row = sqlx::query(
        "INSERT INTO external_playlist_cleanup_batches
         (provider_account_id, source_snapshot_id, input_hash, candidate_count)
         VALUES ($1, $2, $3, $4)
         RETURNING id, source_snapshot_id, state, candidate_count, input_hash,
                   approved_at, created_at",
    )
    .bind(account_id)
    .bind(source_snapshot_id)
    .bind(&input_hash)
    .bind(i32::try_from(candidates.len()).map_err(|_| {
        ChordriftError::Configuration("bookmark cleanup count exceeds limits".to_owned())
    })?)
    .fetch_one(&mut *transaction)
    .await?;
    let batch = cleanup_batch_from_row(&row, false)?;
    for candidate in candidates {
        sqlx::query(
            "INSERT INTO external_playlist_cleanup_items
             (batch_id, bookmark_id, provider_playlist_id, provider_snapshot_id,
              name, owner_provider_id, content_status, item_count)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(batch.batch_id)
        .bind(candidate.bookmark_id)
        .bind(candidate.provider_playlist_id)
        .bind(candidate.provider_snapshot_id)
        .bind(candidate.name)
        .bind(candidate.owner_provider_id)
        .bind(candidate.content_status)
        .bind(candidate.item_count)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(batch)
}

/// Shows the latest or selected cleanup batch and its exact candidates.
pub async fn cleanup_show(
    database: &Database,
    account_label: &str,
    batch_id: Option<Uuid>,
) -> Result<(CleanupBatch, Vec<CleanupItem>)> {
    let row = sqlx::query(
        "SELECT batch.id, batch.source_snapshot_id, batch.state,
                batch.candidate_count, batch.input_hash, batch.approved_at,
                batch.created_at
         FROM external_playlist_cleanup_batches batch
         JOIN provider_accounts account ON account.id = batch.provider_account_id
         WHERE account.provider = 'spotify' AND account.account_label = $1
           AND ($2::uuid IS NULL OR batch.id = $2)
         ORDER BY batch.created_at DESC, batch.id DESC LIMIT 1",
    )
    .bind(account_label)
    .bind(batch_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| ChordriftError::Configuration("no bookmark cleanup batch exists".to_owned()))?;
    let batch = cleanup_batch_from_row(&row, true)?;
    let rows = sqlx::query(
        "SELECT provider_playlist_id, provider_snapshot_id, name,
                owner_provider_id, content_status, item_count
         FROM external_playlist_cleanup_items
         WHERE batch_id = $1 ORDER BY lower(name), provider_playlist_id",
    )
    .bind(batch.batch_id)
    .fetch_all(database.pool())
    .await?;
    let items = rows
        .into_iter()
        .map(|row| {
            Ok(CleanupItem {
                provider_playlist_id: row.try_get("provider_playlist_id")?,
                provider_snapshot_id: row.try_get("provider_snapshot_id")?,
                name: row.try_get("name")?,
                owner_provider_id: row.try_get("owner_provider_id")?,
                content_status: row.try_get("content_status")?,
                item_count: row.try_get("item_count")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((batch, items))
}

/// Explicitly approves one exact cleanup batch; this performs no provider write.
pub async fn cleanup_approve(
    database: &Database,
    account_label: &str,
    confirm: Uuid,
) -> Result<CleanupBatch> {
    let row = sqlx::query(
        "SELECT batch.id, batch.source_snapshot_id, batch.state,
                batch.candidate_count, batch.input_hash, batch.approved_at,
                batch.created_at
         FROM external_playlist_cleanup_batches batch
         JOIN provider_accounts account ON account.id = batch.provider_account_id
         WHERE account.provider = 'spotify' AND account.account_label = $1
           AND batch.id = $2",
    )
    .bind(account_label)
    .bind(confirm)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| ChordriftError::Configuration("cleanup batch was not found".to_owned()))?;
    let batch = cleanup_batch_from_row(&row, true)?;
    if batch.state == "approved" {
        return Ok(batch);
    }
    if batch.state != "pending" {
        return Err(ChordriftError::Configuration(
            "cleanup batch is not awaiting approval".to_owned(),
        ));
    }
    let row = sqlx::query(
        "UPDATE external_playlist_cleanup_batches
         SET state = 'approved', approved_at = now()
         WHERE id = $1 AND state = 'pending'
         RETURNING id, source_snapshot_id, state, candidate_count, input_hash,
                   approved_at, created_at",
    )
    .bind(confirm)
    .fetch_one(database.pool())
    .await?;
    cleanup_batch_from_row(&row, false)
}

async fn account_and_snapshot(database: &Database, account_label: &str) -> Result<(Uuid, Uuid)> {
    let row = sqlx::query(
        "SELECT account.id,
                (SELECT snapshot.id FROM provider_library_snapshots snapshot
                 WHERE snapshot.provider_account_id = account.id
                 ORDER BY snapshot.captured_at DESC, snapshot.id DESC LIMIT 1) AS snapshot_id
         FROM provider_accounts account
         WHERE account.provider = 'spotify' AND account.account_label = $1",
    )
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| ChordriftError::Configuration("Spotify account was not imported".to_owned()))?;
    let snapshot_id = row.try_get("snapshot_id")?;
    Ok((row.try_get("id")?, snapshot_id))
}

async fn cleanup_candidates(
    database: &Database,
    account_id: Uuid,
) -> Result<Vec<CleanupCandidate>> {
    let rows = sqlx::query(
        "SELECT id, provider_playlist_id, provider_snapshot_id, name,
                owner_provider_id, content_status, item_count
         FROM external_playlist_bookmarks
         WHERE provider_account_id = $1 AND provider = 'spotify'
           AND present_in_provider_library
         ORDER BY provider_playlist_id",
    )
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(CleanupCandidate {
                bookmark_id: row.try_get("id")?,
                provider_playlist_id: row.try_get("provider_playlist_id")?,
                provider_snapshot_id: row.try_get("provider_snapshot_id")?,
                name: row.try_get("name")?,
                owner_provider_id: row.try_get("owner_provider_id")?,
                content_status: row.try_get("content_status")?,
                item_count: row.try_get("item_count")?,
            })
        })
        .collect()
}

fn cleanup_batch_from_row(row: &sqlx::postgres::PgRow, reused: bool) -> Result<CleanupBatch> {
    Ok(CleanupBatch {
        batch_id: row.try_get("id")?,
        source_snapshot_id: row.try_get("source_snapshot_id")?,
        state: row.try_get("state")?,
        candidate_count: row.try_get("candidate_count")?,
        input_hash: row.try_get("input_hash")?,
        reused,
        approved_at: row.try_get("approved_at")?,
        created_at: row.try_get("created_at")?,
    })
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn to_i32(value: usize, label: &str) -> Result<i32> {
    i32::try_from(value).map_err(|_| {
        ChordriftError::Configuration(format!("Spotify {label} exceeds PostgreSQL limits"))
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
