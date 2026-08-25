use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result};

use super::{
    auth,
    models::{
        ExternalPlaylistInventory, PlaylistReuse, ReusePlan, SavedTrackReuse, SpotifyAlbum,
        SpotifyArtist, SpotifyInventory, SpotifyTrack,
    },
};

const PROVIDER: &str = "spotify";

/// Summary of one immutable Spotify inventory snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportReport {
    /// Local label used for the authorized Spotify account.
    pub account_label: String,
    /// Stable Spotify account identity.
    pub account_id: String,
    /// Spotify display name, when present.
    pub display_name: Option<String>,
    /// Database identity of the immutable library snapshot.
    pub snapshot_id: Uuid,
    /// Owned, followed, and collaborative playlists reported by Spotify.
    pub playlists_seen: usize,
    /// Account-owned and private Spotify-personalized playlists persisted.
    pub playlists_imported: usize,
    /// Playlists copied from the previous Neon snapshot without an item request.
    pub playlists_reused: usize,
    /// Ordered playlist entries persisted, including duplicates.
    pub playlist_entries: usize,
    /// Saved-track entries persisted.
    pub saved_tracks: usize,
    /// Whether saved tracks were copied from Neon after a one-page probe.
    pub saved_tracks_reused: bool,
    /// Playlist or saved-library items unavailable from Spotify.
    pub unavailable_items: usize,
    /// Local, non-track, or identifier-less items not persisted.
    pub unsupported_items: usize,
    /// Followed playlists excluded by Spotify's Development Mode constraints.
    pub followed_playlists_skipped: usize,
    /// Collaborative playlists Spotify did not permit Chordrift to read.
    pub inaccessible_collaborative_playlists: usize,
    /// Externally owned playlists retained as durable Neon bookmarks.
    pub external_bookmarks: usize,
    /// Bookmark observations whose track contents were copied from Neon.
    pub external_bookmarks_reused: usize,
    /// Ordered track entries retained in external bookmark snapshots.
    pub external_bookmark_entries: usize,
}

/// Fetches a complete read-only Spotify inventory and persists it atomically.
pub async fn import(account_label: &str, database: &Database) -> Result<ImportReport> {
    let reuse = load_reuse_plan(account_label, database).await?;
    let session = auth::session(account_label).await?;
    let inventory = session.client.inventory(session.profile, &reuse).await?;
    persist(account_label, inventory, database).await
}

async fn load_reuse_plan(account_label: &str, database: &Database) -> Result<ReusePlan> {
    let latest = sqlx::query(
        "SELECT snapshots.id, snapshots.metadata
         FROM provider_library_snapshots snapshots
         JOIN provider_accounts accounts ON accounts.id = snapshots.provider_account_id
         WHERE accounts.provider = $1 AND accounts.account_label = $2
         ORDER BY snapshots.captured_at DESC, snapshots.id DESC LIMIT 1",
    )
    .bind(PROVIDER)
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?;
    let Some(latest) = latest else {
        return Ok(ReusePlan::default());
    };
    let source_snapshot_id: Uuid = latest.try_get("id")?;
    let metadata: Value = latest.try_get("metadata")?;
    let playlist_rows = sqlx::query(
        "SELECT playlists.provider_playlist_id, snapshots.provider_snapshot_id
         FROM provider_playlist_snapshots snapshots
         JOIN provider_playlists playlists ON playlists.id = snapshots.provider_playlist_id
         WHERE snapshots.snapshot_id = $1 AND snapshots.provider_snapshot_id IS NOT NULL",
    )
    .bind(source_snapshot_id)
    .fetch_all(database.pool())
    .await?;
    let playlists = playlist_rows
        .into_iter()
        .map(|row| {
            let provider_playlist_id: String = row.try_get("provider_playlist_id")?;
            let provider_snapshot_id: String = row.try_get("provider_snapshot_id")?;
            Ok((
                provider_playlist_id,
                PlaylistReuse {
                    provider_snapshot_id,
                    source_snapshot_id,
                },
            ))
        })
        .collect::<Result<HashMap<_, _>>>()?;

    let bookmark_rows = sqlx::query(
        "SELECT bookmarks.provider_playlist_id, snapshots.provider_snapshot_id
         FROM external_playlist_bookmark_snapshots snapshots
         JOIN external_playlist_bookmarks bookmarks ON bookmarks.id = snapshots.bookmark_id
         WHERE snapshots.snapshot_id = $1
           AND snapshots.content_status = 'complete'
           AND snapshots.provider_snapshot_id IS NOT NULL",
    )
    .bind(source_snapshot_id)
    .fetch_all(database.pool())
    .await?;
    let bookmark_playlists = bookmark_rows
        .into_iter()
        .map(|row| {
            let provider_playlist_id: String = row.try_get("provider_playlist_id")?;
            let provider_snapshot_id: String = row.try_get("provider_snapshot_id")?;
            Ok((
                provider_playlist_id,
                PlaylistReuse {
                    provider_snapshot_id,
                    source_snapshot_id,
                },
            ))
        })
        .collect::<Result<HashMap<_, _>>>()?;

    let saved_tracks = if let Some(total) = metadata
        .get("saved_items_seen")
        .and_then(Value::as_u64)
        .and_then(|total| usize::try_from(total).ok())
    {
        let rows = sqlx::query(
            "SELECT saved.position, tracks.provider_track_id, saved.saved_at
             FROM provider_saved_tracks saved
             JOIN provider_tracks tracks ON tracks.id = saved.provider_track_id
             WHERE saved.snapshot_id = $1 AND saved.position < 50
             ORDER BY saved.position",
        )
        .bind(source_snapshot_id)
        .fetch_all(database.pool())
        .await?;
        Some(SavedTrackReuse {
            source_snapshot_id,
            total,
            leading_items: rows
                .into_iter()
                .map(|row| {
                    let position: i32 = row.try_get("position")?;
                    Ok((
                        usize::try_from(position).map_err(|_| {
                            ChordriftError::Configuration(
                                "stored saved-track position was negative".to_owned(),
                            )
                        })?,
                        row.try_get("provider_track_id")?,
                        row.try_get("saved_at")?,
                    ))
                })
                .collect::<Result<Vec<_>>>()?,
        })
    } else {
        None
    };
    Ok(ReusePlan {
        playlists,
        bookmark_playlists,
        saved_tracks,
    })
}

async fn persist(
    account_label: &str,
    inventory: SpotifyInventory,
    database: &Database,
) -> Result<ImportReport> {
    let mut transaction = database.pool().begin().await?;
    let account_id = upsert_account(account_label, &inventory, &mut transaction).await?;
    sqlx::query(
        "UPDATE provider_account_playlists
         SET present_in_latest_snapshot = FALSE, updated_at = now()
         WHERE provider_account_id = $1",
    )
    .bind(account_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "UPDATE external_playlist_bookmarks
         SET present_in_provider_library = FALSE, last_checked_at = now(), updated_at = now()
         WHERE provider_account_id = $1 AND provider = $2",
    )
    .bind(account_id)
    .bind(PROVIDER)
    .execute(&mut *transaction)
    .await?;
    let snapshot_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO provider_library_snapshots
         (id, provider, source, provider_account_id, metadata)
         VALUES ($1, $2, 'spotify_web_api', $3, $4)",
    )
    .bind(snapshot_id)
    .bind(PROVIDER)
    .bind(account_id)
    .bind(json!({
        "followed_playlists_skipped": inventory.followed_playlists_skipped,
        "inaccessible_collaborative_playlists": inventory.inaccessible_collaborative_playlists,
        "external_bookmarks_seen": inventory.external_playlists.len(),
        "saved_items_seen": inventory.saved_tracks.total,
    }))
    .execute(&mut *transaction)
    .await?;

    let mut tracks = HashMap::new();
    let mut playlist_entries = 0;
    let mut playlists_reused = 0;
    let mut saved_tracks = 0;
    let mut unavailable_items = 0;
    let mut unsupported_items = 0;
    let mut external_bookmark_entries = 0;
    let mut external_bookmarks_reused = 0;

    for playlist_inventory in &inventory.playlists {
        let playlist = &playlist_inventory.playlist;
        let playlist_name = nonempty_or(&playlist.name, "Untitled Spotify playlist");
        let provider_playlist_id = upsert_playlist(playlist, &mut transaction).await?;
        sqlx::query(
            "INSERT INTO provider_account_playlists
             (provider_account_id, provider_playlist_id, present_in_latest_snapshot)
             VALUES ($1, $2, TRUE)
             ON CONFLICT (provider_account_id, provider_playlist_id) DO UPDATE SET
               present_in_latest_snapshot = TRUE,
               last_seen_at = now(), updated_at = now()",
        )
        .bind(account_id)
        .bind(provider_playlist_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE provider_account_playlists account_playlist
             SET role = 'managed', drift_policy = 'neon_wins',
                 signal_class = 'canonical', semantic_weight = 0.0,
                 clear_policy = 'never', updated_at = now()
             FROM provider_playlists provider
             WHERE account_playlist.provider_account_id = $1
               AND account_playlist.provider_playlist_id = $2
               AND provider.id = account_playlist.provider_playlist_id
               AND provider.concept_id IS NOT NULL",
        )
        .bind(account_id)
        .bind(provider_playlist_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE provider_account_playlists account_playlist
             SET role = 'inbox', drift_policy = 'provider_wins',
                 signal_class = 'intake', semantic_weight = 0.0,
                 clear_policy = 'after_verified_assignment', updated_at = now()
             WHERE account_playlist.provider_account_id = $1
               AND account_playlist.provider_playlist_id = $2
               AND EXISTS (
                   SELECT 1
                   FROM provider_playlists provider
                   JOIN sync_apply_playlist_targets target
                     ON target.spotify_playlist_id = provider.provider_playlist_id
                   JOIN sync_apply_runs run ON run.id = target.apply_run_id
                   JOIN sync_apply_operations execution ON execution.apply_run_id = run.id
                   JOIN sync_operations planned
                     ON planned.id = execution.planned_operation_id
                   WHERE provider.id = account_playlist.provider_playlist_id
                     AND run.status = 'succeeded'
                     AND planned.operation_type = 'create_playlist'
                     AND planned.payload->>'playlist_name' = target.playlist_name
                     AND planned.payload->'detail'->>'surface' = 'intake'
               )",
        )
        .bind(account_id)
        .bind(provider_playlist_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO provider_playlist_snapshots
             (snapshot_id, provider_playlist_id, name, description,
              provider_snapshot_id, public, collaborative, total_items, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(snapshot_id)
        .bind(provider_playlist_id)
        .bind(playlist_name)
        .bind(&playlist.description)
        .bind(&playlist.snapshot_id)
        .bind(playlist.public)
        .bind(playlist.collaborative)
        .bind(to_i32(playlist.total_items(), "playlist item count")?)
        .bind(serde_json::to_value(playlist)?)
        .execute(&mut *transaction)
        .await?;

        if let Some(source_snapshot_id) = playlist_inventory.reused_from_snapshot {
            let copied = sqlx::query(
                "INSERT INTO provider_playlist_tracks
                 (snapshot_id, provider_playlist_id, provider_track_id,
                  position, added_at, metadata, captured_at)
                 SELECT $1, provider_playlist_id, provider_track_id,
                        position, added_at, metadata, now()
                 FROM provider_playlist_tracks
                 WHERE snapshot_id = $2 AND provider_playlist_id = $3",
            )
            .bind(snapshot_id)
            .bind(source_snapshot_id)
            .bind(provider_playlist_id)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
            playlist_entries += usize::try_from(copied).map_err(|_| {
                ChordriftError::Configuration(
                    "copied playlist entry count exceeds platform limits".to_owned(),
                )
            })?;
            playlists_reused += 1;
            continue;
        }

        for (position, item) in playlist_inventory.items.iter().enumerate() {
            let Some(track) = item.track() else {
                unavailable_items += 1;
                continue;
            };
            let Some(provider_track_id) =
                persist_track(track, &mut tracks, &mut transaction, &mut unsupported_items).await?
            else {
                continue;
            };
            sqlx::query(
                "INSERT INTO provider_playlist_tracks
                 (snapshot_id, provider_playlist_id, provider_track_id,
                  position, added_at, metadata)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(snapshot_id)
            .bind(provider_playlist_id)
            .bind(provider_track_id)
            .bind(to_i32(position, "playlist position")?)
            .bind(item.added_at)
            .bind(json!({
                "added_by": item.added_by,
                "is_local": item.is_local,
            }))
            .execute(&mut *transaction)
            .await?;
            playlist_entries += 1;
        }
    }

    for external in &inventory.external_playlists {
        let bookmark_id = upsert_external_bookmark(account_id, external, &mut transaction).await?;
        let playlist = &external.playlist;
        sqlx::query(
            "INSERT INTO external_playlist_bookmark_snapshots
             (snapshot_id, bookmark_id, relationship, name, owner_provider_id,
              owner_display_name, provider_url, provider_snapshot_id, content_status,
              item_count, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(snapshot_id)
        .bind(bookmark_id)
        .bind(external.relationship.as_str())
        .bind(nonempty_or(&playlist.name, "Untitled external playlist"))
        .bind(&playlist.owner.id)
        .bind(&playlist.owner.display_name)
        .bind(playlist.external_urls.spotify())
        .bind(&playlist.snapshot_id)
        .bind(external.content_status.as_str())
        .bind(to_i32(playlist.total_items(), "bookmark item count")?)
        .bind(serde_json::to_value(playlist)?)
        .execute(&mut *transaction)
        .await?;

        if let Some(source_snapshot_id) = external.reused_from_snapshot {
            let copied = sqlx::query(
                "INSERT INTO external_playlist_bookmark_tracks
                 (snapshot_id, bookmark_id, provider_track_id, position,
                  added_at, metadata, captured_at)
                 SELECT $1, bookmark_id, provider_track_id, position,
                        added_at, metadata, now()
                 FROM external_playlist_bookmark_tracks
                 WHERE snapshot_id = $2 AND bookmark_id = $3",
            )
            .bind(snapshot_id)
            .bind(source_snapshot_id)
            .bind(bookmark_id)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
            external_bookmark_entries += usize::try_from(copied).map_err(|_| {
                ChordriftError::Configuration(
                    "copied bookmark entry count exceeds platform limits".to_owned(),
                )
            })?;
            external_bookmarks_reused += 1;
            continue;
        }

        for (position, item) in external.items.iter().enumerate() {
            let Some(track) = item.track() else {
                unavailable_items += 1;
                continue;
            };
            let Some(provider_track_id) =
                persist_track(track, &mut tracks, &mut transaction, &mut unsupported_items).await?
            else {
                continue;
            };
            sqlx::query(
                "INSERT INTO external_playlist_bookmark_tracks
                 (snapshot_id, bookmark_id, provider_track_id, position, added_at, metadata)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(snapshot_id)
            .bind(bookmark_id)
            .bind(provider_track_id)
            .bind(to_i32(position, "bookmark playlist position")?)
            .bind(item.added_at)
            .bind(json!({
                "added_by": item.added_by,
                "is_local": item.is_local,
            }))
            .execute(&mut *transaction)
            .await?;
            external_bookmark_entries += 1;
        }
    }

    let saved_tracks_reused = inventory.saved_tracks.reused_from_snapshot.is_some();
    if let Some(source_snapshot_id) = inventory.saved_tracks.reused_from_snapshot {
        let copied = sqlx::query(
            "INSERT INTO provider_saved_tracks
             (snapshot_id, provider_track_id, position, saved_at, metadata)
             SELECT $1, provider_track_id, position, saved_at, metadata
             FROM provider_saved_tracks WHERE snapshot_id = $2",
        )
        .bind(snapshot_id)
        .bind(source_snapshot_id)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        saved_tracks = usize::try_from(copied).map_err(|_| {
            ChordriftError::Configuration(
                "copied saved-track count exceeds platform limits".to_owned(),
            )
        })?;
    }
    for (position, saved) in inventory.saved_tracks.items.iter().enumerate() {
        let Some(track) = saved.track.as_ref() else {
            unavailable_items += 1;
            continue;
        };
        let Some(provider_track_id) =
            persist_track(track, &mut tracks, &mut transaction, &mut unsupported_items).await?
        else {
            continue;
        };
        sqlx::query(
            "INSERT INTO provider_saved_tracks
             (snapshot_id, provider_track_id, position, saved_at, metadata)
             VALUES ($1, $2, $3, $4, '{}'::jsonb)",
        )
        .bind(snapshot_id)
        .bind(provider_track_id)
        .bind(to_i32(position, "saved-track position")?)
        .bind(saved.added_at)
        .execute(&mut *transaction)
        .await?;
        saved_tracks += 1;
    }

    let playlists_imported = inventory.playlists.len();
    sqlx::query(
        "INSERT INTO provider_import_runs
         (provider_account_id, snapshot_id, status, playlists_seen,
          playlists_imported, playlist_entries, saved_tracks,
          unavailable_items, unsupported_items, metadata, finished_at)
         VALUES ($1, $2, 'succeeded', $3, $4, $5, $6, $7, $8, $9, now())",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .bind(to_i32(inventory.playlists_seen, "playlist count")?)
    .bind(to_i32(playlists_imported, "imported playlist count")?)
    .bind(to_i32(playlist_entries, "playlist entry count")?)
    .bind(to_i32(saved_tracks, "saved-track count")?)
    .bind(to_i32(unavailable_items, "unavailable item count")?)
    .bind(to_i32(unsupported_items, "unsupported item count")?)
    .bind(json!({
        "followed_playlists_skipped": inventory.followed_playlists_skipped,
        "inaccessible_collaborative_playlists": inventory.inaccessible_collaborative_playlists,
        "unique_tracks": tracks.len(),
        "external_bookmarks": inventory.external_playlists.len(),
        "external_bookmark_entries": external_bookmark_entries,
    }))
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "UPDATE provider_accounts
         SET last_imported_at = now(), updated_at = now()
         WHERE id = $1",
    )
    .bind(account_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok(ImportReport {
        account_label: account_label.to_owned(),
        account_id: inventory.profile.account_id,
        display_name: inventory.profile.display_name,
        snapshot_id,
        playlists_seen: inventory.playlists_seen,
        playlists_imported,
        playlists_reused,
        playlist_entries,
        saved_tracks,
        saved_tracks_reused,
        unavailable_items,
        unsupported_items,
        followed_playlists_skipped: inventory.followed_playlists_skipped,
        inaccessible_collaborative_playlists: inventory.inaccessible_collaborative_playlists,
        external_bookmarks: inventory.external_playlists.len(),
        external_bookmarks_reused,
        external_bookmark_entries,
    })
}

async fn upsert_external_bookmark(
    account_id: Uuid,
    external: &ExternalPlaylistInventory,
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<Uuid> {
    let playlist = &external.playlist;
    sqlx::query_scalar(
        "INSERT INTO external_playlist_bookmarks
         (provider_account_id, provider, provider_playlist_id, relationship,
          name, owner_provider_id, owner_display_name, provider_uri, provider_url,
          provider_snapshot_id, public, collaborative, item_count, content_status,
          present_in_provider_library, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                 TRUE, $15)
         ON CONFLICT (provider_account_id, provider, provider_playlist_id) DO UPDATE SET
           relationship = EXCLUDED.relationship,
           name = EXCLUDED.name,
           owner_provider_id = EXCLUDED.owner_provider_id,
           owner_display_name = EXCLUDED.owner_display_name,
           provider_uri = EXCLUDED.provider_uri,
           provider_url = EXCLUDED.provider_url,
           last_changed_at = CASE
             WHEN external_playlist_bookmarks.provider_snapshot_id
                  IS DISTINCT FROM EXCLUDED.provider_snapshot_id THEN now()
             ELSE external_playlist_bookmarks.last_changed_at
           END,
           provider_snapshot_id = EXCLUDED.provider_snapshot_id,
           public = EXCLUDED.public,
           collaborative = EXCLUDED.collaborative,
           item_count = EXCLUDED.item_count,
           content_status = EXCLUDED.content_status,
           present_in_provider_library = TRUE,
           metadata = EXCLUDED.metadata,
           last_seen_at = now(), last_checked_at = now(), updated_at = now()
         RETURNING id",
    )
    .bind(account_id)
    .bind(PROVIDER)
    .bind(&playlist.id)
    .bind(external.relationship.as_str())
    .bind(nonempty_or(&playlist.name, "Untitled external playlist"))
    .bind(&playlist.owner.id)
    .bind(&playlist.owner.display_name)
    .bind(&playlist.uri)
    .bind(playlist.external_urls.spotify())
    .bind(&playlist.snapshot_id)
    .bind(playlist.public)
    .bind(playlist.collaborative)
    .bind(to_i32(playlist.total_items(), "bookmark item count")?)
    .bind(external.content_status.as_str())
    .bind(serde_json::to_value(playlist)?)
    .fetch_one(&mut **transaction)
    .await
    .map_err(Into::into)
}

async fn upsert_account(
    account_label: &str,
    inventory: &SpotifyInventory,
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<Uuid> {
    sqlx::query_scalar(
        "INSERT INTO provider_accounts
         (provider, provider_account_id, account_label, display_name,
          metadata, last_authenticated_at)
         VALUES ($1, $2, $3, $4, $5, now())
         ON CONFLICT (provider, account_label) DO UPDATE SET
           provider_account_id = EXCLUDED.provider_account_id,
           display_name = EXCLUDED.display_name,
           metadata = EXCLUDED.metadata,
           last_authenticated_at = now(), updated_at = now()
         RETURNING id",
    )
    .bind(PROVIDER)
    .bind(&inventory.profile.account_id)
    .bind(account_label)
    .bind(&inventory.profile.display_name)
    .bind(serde_json::to_value(&inventory.profile)?)
    .fetch_one(&mut **transaction)
    .await
    .map_err(Into::into)
}

async fn upsert_playlist(
    playlist: &super::models::SpotifyPlaylist,
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<Uuid> {
    let playlist_name = nonempty_or(&playlist.name, "Untitled Spotify playlist");
    if let Some(id) = sqlx::query_scalar::<_, Uuid>(
        "UPDATE provider_playlists SET
           provider_uri = $3, provider_url = $4, snapshot_id = $5,
           metadata = $6, imported_at = now(), last_seen_at = now(), updated_at = now()
         WHERE provider = $1 AND provider_playlist_id = $2
         RETURNING id",
    )
    .bind(PROVIDER)
    .bind(&playlist.id)
    .bind(&playlist.uri)
    .bind(playlist.external_urls.spotify())
    .bind(&playlist.snapshot_id)
    .bind(serde_json::to_value(playlist)?)
    .fetch_optional(&mut **transaction)
    .await?
    {
        return Ok(id);
    }

    let applied = sqlx::query(
        "SELECT target.playlist_id, target.concept_id
         FROM sync_apply_playlist_targets target
         JOIN sync_apply_runs run ON run.id = target.apply_run_id
         WHERE target.spotify_playlist_id = $1
         ORDER BY run.started_at DESC, run.id DESC LIMIT 1",
    )
    .bind(&playlist.id)
    .fetch_optional(&mut **transaction)
    .await?;
    let concept_id: Option<Uuid> = applied
        .as_ref()
        .and_then(|row| row.try_get("concept_id").ok())
        .flatten();
    let playlist_id: Uuid = match applied
        .as_ref()
        .and_then(|row| row.try_get("playlist_id").ok())
        .flatten()
    {
        Some(id) => id,
        None => {
            sqlx::query_scalar(
                "INSERT INTO playlists (name, description, kind)
                 VALUES ($1, $2, 'historical') RETURNING id",
            )
            .bind(playlist_name)
            .bind(&playlist.description)
            .fetch_one(&mut **transaction)
            .await?
        }
    };
    sqlx::query_scalar(
        "INSERT INTO provider_playlists
         (playlist_id, concept_id, provider, provider_playlist_id, provider_uri,
          provider_url, snapshot_id, metadata, imported_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now()) RETURNING id",
    )
    .bind(playlist_id)
    .bind(concept_id)
    .bind(PROVIDER)
    .bind(&playlist.id)
    .bind(&playlist.uri)
    .bind(playlist.external_urls.spotify())
    .bind(&playlist.snapshot_id)
    .bind(serde_json::to_value(playlist)?)
    .fetch_one(&mut **transaction)
    .await
    .map_err(Into::into)
}

async fn persist_track(
    track: &SpotifyTrack,
    cache: &mut HashMap<String, Uuid>,
    transaction: &mut Transaction<'_, Postgres>,
    unsupported_items: &mut usize,
) -> Result<Option<Uuid>> {
    let Some(provider_id) = track.id.as_ref() else {
        *unsupported_items += 1;
        return Ok(None);
    };
    let normalized_title = normalize(&track.name);
    if provider_id.trim().is_empty()
        || normalized_title.is_empty()
        || track.kind != "track"
        || track.is_local
    {
        *unsupported_items += 1;
        return Ok(None);
    }
    if let Some(id) = cache.get(provider_id) {
        return Ok(Some(*id));
    }

    let album_id = match track.album.as_ref() {
        Some(album) => persist_album(album, transaction).await?,
        None => None,
    };
    let existing = sqlx::query(
        "SELECT id, track_id FROM provider_tracks
         WHERE provider = $1 AND provider_track_id = $2",
    )
    .bind(PROVIDER)
    .bind(provider_id)
    .fetch_optional(&mut **transaction)
    .await?;
    let (provider_track_id, canonical_track_id) = if let Some(row) = existing {
        let provider_track_id: Uuid = row.try_get("id")?;
        let canonical_track_id: Uuid = row.try_get("track_id")?;
        sqlx::query(
            "UPDATE tracks SET album_id = $2, title = $3, normalized_title = $4,
              duration_ms = $5, isrc = $6, explicit = $7, updated_at = now()
             WHERE id = $1",
        )
        .bind(canonical_track_id)
        .bind(album_id)
        .bind(&track.name)
        .bind(&normalized_title)
        .bind(track.duration_ms)
        .bind(clean_isrc(track.external_ids.isrc.as_deref()))
        .bind(track.explicit)
        .execute(&mut **transaction)
        .await?;
        sqlx::query(
            "UPDATE provider_tracks SET provider_uri = $2, provider_url = $3,
              metadata = $4, last_seen_at = now(), updated_at = now() WHERE id = $1",
        )
        .bind(provider_track_id)
        .bind(&track.uri)
        .bind(track.external_urls.spotify())
        .bind(serde_json::to_value(track)?)
        .execute(&mut **transaction)
        .await?;
        (provider_track_id, canonical_track_id)
    } else {
        let canonical_track_id: Uuid = sqlx::query_scalar(
            "INSERT INTO tracks
             (album_id, title, normalized_title, duration_ms, isrc, explicit)
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(album_id)
        .bind(&track.name)
        .bind(&normalized_title)
        .bind(track.duration_ms)
        .bind(clean_isrc(track.external_ids.isrc.as_deref()))
        .bind(track.explicit)
        .fetch_one(&mut **transaction)
        .await?;
        let provider_track_id: Uuid = sqlx::query_scalar(
            "INSERT INTO provider_tracks
             (track_id, provider, provider_track_id, provider_uri, provider_url, metadata)
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(canonical_track_id)
        .bind(PROVIDER)
        .bind(provider_id)
        .bind(&track.uri)
        .bind(track.external_urls.spotify())
        .bind(serde_json::to_value(track)?)
        .fetch_one(&mut **transaction)
        .await?;
        (provider_track_id, canonical_track_id)
    };

    sqlx::query("DELETE FROM track_artists WHERE track_id = $1")
        .bind(canonical_track_id)
        .execute(&mut **transaction)
        .await?;
    let mut seen_artists = HashSet::new();
    for (position, artist) in track.artists.iter().enumerate() {
        if let Some(artist_id) = persist_artist(artist, transaction).await? {
            if !seen_artists.insert(artist_id) {
                continue;
            }
            sqlx::query(
                "INSERT INTO track_artists (track_id, artist_id, position)
                 VALUES ($1, $2, $3)",
            )
            .bind(canonical_track_id)
            .bind(artist_id)
            .bind(to_i32(position, "artist position")?)
            .execute(&mut **transaction)
            .await?;
        }
    }
    cache.insert(provider_id.clone(), provider_track_id);
    Ok(Some(provider_track_id))
}

async fn persist_artist(
    artist: &SpotifyArtist,
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<Option<Uuid>> {
    let Some(provider_id) = artist.id.as_ref() else {
        return Ok(None);
    };
    let normalized_name = normalize(&artist.name);
    if provider_id.trim().is_empty() || normalized_name.is_empty() {
        return Ok(None);
    }
    if let Some(artist_id) = sqlx::query_scalar::<_, Uuid>(
        "UPDATE provider_artists SET provider_uri = $3, provider_url = $4,
          metadata = $5, last_seen_at = now(), updated_at = now()
         WHERE provider = $1 AND provider_artist_id = $2 RETURNING artist_id",
    )
    .bind(PROVIDER)
    .bind(provider_id)
    .bind(&artist.uri)
    .bind(artist.external_urls.spotify())
    .bind(serde_json::to_value(artist)?)
    .fetch_optional(&mut **transaction)
    .await?
    {
        sqlx::query(
            "UPDATE artists SET name = $2, normalized_name = $3, updated_at = now()
             WHERE id = $1",
        )
        .bind(artist_id)
        .bind(&artist.name)
        .bind(&normalized_name)
        .execute(&mut **transaction)
        .await?;
        return Ok(Some(artist_id));
    }
    let artist_id: Uuid = sqlx::query_scalar(
        "INSERT INTO artists (name, normalized_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(&artist.name)
    .bind(&normalized_name)
    .fetch_one(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO provider_artists
         (artist_id, provider, provider_artist_id, provider_uri, provider_url, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(artist_id)
    .bind(PROVIDER)
    .bind(provider_id)
    .bind(&artist.uri)
    .bind(artist.external_urls.spotify())
    .bind(serde_json::to_value(artist)?)
    .execute(&mut **transaction)
    .await?;
    Ok(Some(artist_id))
}

async fn persist_album(
    album: &SpotifyAlbum,
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<Option<Uuid>> {
    let Some(provider_id) = album.id.as_ref() else {
        return Ok(None);
    };
    let normalized_title = normalize(&album.name);
    if provider_id.trim().is_empty() || normalized_title.is_empty() {
        return Ok(None);
    }
    let release_date = parse_release_date(album.release_date.as_deref());
    if let Some(album_id) = sqlx::query_scalar::<_, Uuid>(
        "UPDATE provider_albums SET provider_uri = $3, provider_url = $4,
          metadata = $5, last_seen_at = now(), updated_at = now()
         WHERE provider = $1 AND provider_album_id = $2 RETURNING album_id",
    )
    .bind(PROVIDER)
    .bind(provider_id)
    .bind(&album.uri)
    .bind(album.external_urls.spotify())
    .bind(serde_json::to_value(album)?)
    .fetch_optional(&mut **transaction)
    .await?
    {
        sqlx::query(
            "UPDATE albums SET title = $2, normalized_title = $3, release_date = $4,
              album_type = $5, updated_at = now() WHERE id = $1",
        )
        .bind(album_id)
        .bind(&album.name)
        .bind(&normalized_title)
        .bind(release_date)
        .bind(&album.album_type)
        .execute(&mut **transaction)
        .await?;
        return Ok(Some(album_id));
    }
    let album_id: Uuid = sqlx::query_scalar(
        "INSERT INTO albums (title, normalized_title, release_date, album_type)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(&album.name)
    .bind(&normalized_title)
    .bind(release_date)
    .bind(&album.album_type)
    .fetch_one(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO provider_albums
         (album_id, provider, provider_album_id, provider_uri, provider_url, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(album_id)
    .bind(PROVIDER)
    .bind(provider_id)
    .bind(&album.uri)
    .bind(album.external_urls.spotify())
    .bind(serde_json::to_value(album)?)
    .execute(&mut **transaction)
    .await?;
    Ok(Some(album_id))
}

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn clean_isrc(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_uppercase)
}

fn nonempty_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value
    }
}

fn parse_release_date(value: Option<&str>) -> Option<NaiveDate> {
    let value = value?;
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .or_else(|| NaiveDate::parse_from_str(&format!("{value}-01"), "%Y-%m-%d").ok())
        .or_else(|| NaiveDate::parse_from_str(&format!("{value}-01-01"), "%Y-%m-%d").ok())
}

fn to_i32(value: usize, label: &str) -> Result<i32> {
    i32::try_from(value).map_err(|_| {
        ChordriftError::Configuration(format!("Spotify {label} exceeds PostgreSQL limits"))
    })
}

#[cfg(test)]
mod tests {
    use storexa::{DatabaseConfig, PostgresProvider};
    use uuid::Uuid;

    use super::{
        SpotifyInventory, clean_isrc, nonempty_or, normalize, parse_release_date, persist,
    };
    use crate::{
        analysis,
        bookmarks::{
            self, BookmarkFetchOutcome, BookmarkSelector, FetchedBookmark, FetchedBookmarkItem,
        },
        db, playlists,
        providers::spotify::models::{
            CurrentUser, Page, PlaylistInventory, PlaylistItem, SavedTrack, SpotifyPlaylist,
        },
    };

    #[test]
    fn provisional_normalization_is_deliberately_conservative() {
        assert_eq!(normalize("  A   Track Name "), "a track name");
        assert!(normalize(" \t ").is_empty());
        assert_eq!(
            clean_isrc(Some(" usaaa2600001 ")).as_deref(),
            Some("USAAA2600001")
        );
        assert_eq!(clean_isrc(Some(" ")), None);
        assert_eq!(nonempty_or(" ", "Untitled"), "Untitled");
    }

    #[test]
    fn parses_spotify_release_date_precision() {
        assert_eq!(
            parse_release_date(Some("2026-08-18")).unwrap().to_string(),
            "2026-08-18"
        );
        assert_eq!(
            parse_release_date(Some("2026-08")).unwrap().to_string(),
            "2026-08-01"
        );
        assert_eq!(
            parse_release_date(Some("2026")).unwrap().to_string(),
            "2026-01-01"
        );
        assert!(parse_release_date(Some("unknown")).is_none());
    }

    #[tokio::test]
    #[ignore = "requires CHORDRIFT_TEST_DATABASE_URL for disposable PostgreSQL"]
    async fn spotify_persistence_round_trip() -> crate::Result<()> {
        let config = DatabaseConfig::from_env_var("CHORDRIFT_TEST_DATABASE_URL")?
            .with_name("chordrift-spotify-import-test")?
            .with_provider(PostgresProvider::Neon)?
            .with_min_connections(0)
            .with_max_connections(2);
        let database = db::connect(config).await?;
        db::migrate(&database).await?;

        let playlists: Page<SpotifyPlaylist> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/spotify/playlists.json"
        ))?;
        let items: Page<PlaylistItem> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/spotify/playlist_items.json"
        ))?;
        let saved_tracks: Page<SavedTrack> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/spotify/saved_tracks.json"
        ))?;
        let inventory = SpotifyInventory {
            profile: CurrentUser {
                account_id: "account-fixture".to_owned(),
                id: "spotify-user".to_owned(),
                display_name: Some("Suhail".to_owned()),
                uri: "spotify:user:spotify-user".to_owned(),
            },
            playlists: vec![PlaylistInventory {
                playlist: playlists.items.into_iter().next().unwrap(),
                items: items.items,
                reused_from_snapshot: None,
            }],
            external_playlists: Vec::new(),
            saved_tracks: super::super::models::SavedTracksInventory {
                total: saved_tracks.total,
                items: saved_tracks.items,
                reused_from_snapshot: None,
            },
            playlists_seen: 2,
            followed_playlists_skipped: 1,
            inaccessible_collaborative_playlists: 0,
        };

        let report = persist("fixture", inventory, &database).await?;
        assert_eq!(report.playlists_seen, 2);
        assert_eq!(report.playlists_imported, 1);
        assert_eq!(report.playlist_entries, 1);
        assert_eq!(report.saved_tracks, 1);
        assert_eq!(report.unavailable_items, 2);

        let playlist_rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM provider_playlist_tracks WHERE snapshot_id = $1",
        )
        .bind(report.snapshot_id)
        .fetch_one(database.pool())
        .await?;
        let saved_rows: i64 =
            sqlx::query_scalar("SELECT count(*) FROM provider_saved_tracks WHERE snapshot_id = $1")
                .bind(report.snapshot_id)
                .fetch_one(database.pool())
                .await?;
        assert_eq!(playlist_rows, 1);
        assert_eq!(saved_rows, 1);

        let summary = analysis::refresh(&database, "fixture").await?;
        assert_eq!(summary.playlists, 1);
        assert_eq!(summary.playlist_entries, 1);
        assert_eq!(summary.unique_playlist_tracks, 1);
        assert_eq!(summary.saved_tracks, 1);

        let account_id: Uuid =
            sqlx::query_scalar("SELECT id FROM provider_accounts WHERE account_label = 'fixture'")
                .fetch_one(database.pool())
                .await?;
        sqlx::query(
            "INSERT INTO external_playlist_bookmarks
             (provider_account_id, provider, provider_playlist_id, relationship,
              name, owner_provider_id, item_count, content_status,
              present_in_provider_library)
             VALUES ($1, 'spotify', 'shared123', 'followed_external',
                     'Shared Fixture', 'friend', 1, 'metadata_only', TRUE)",
        )
        .bind(account_id)
        .execute(database.pool())
        .await?;
        let refresh = bookmarks::record_refresh(
            &database,
            "fixture",
            "shared123",
            BookmarkFetchOutcome::Complete(FetchedBookmark {
                name: "Shared Fixture Updated".to_owned(),
                owner_provider_id: "friend".to_owned(),
                owner_display_name: Some("Friend".to_owned()),
                provider_url: Some("https://open.spotify.com/playlist/shared123".to_owned()),
                provider_snapshot_id: Some("shared-snapshot-2".to_owned()),
                public: Some(true),
                collaborative: true,
                item_count: 1,
                unavailable_items: 0,
                unsupported_items: 0,
                items: vec![FetchedBookmarkItem {
                    position: 0,
                    provider_track_id: "track123".to_owned(),
                    title: "Refreshed Track".to_owned(),
                    artists: "Fixture Artist".to_owned(),
                    album: Some("Fixture Album".to_owned()),
                    added_at: None,
                    provider_url: None,
                }],
            }),
        )
        .await?;
        assert_eq!(refresh.status, "complete");
        assert_eq!(refresh.captured_items, 1);
        let retained = bookmarks::tracks(
            &database,
            "fixture",
            &BookmarkSelector::ProviderId("shared123".to_owned()),
        )
        .await?;
        assert_eq!(retained.bookmark.name, "Shared Fixture Updated");
        assert_eq!(retained.tracks[0].title, "Refreshed Track");

        let configured = playlists::configure(
            &database,
            "fixture",
            &playlists::PlaylistSelector::Name("Morning Drift".to_owned()),
            playlists::PlaylistRole::Inbox,
            playlists::DriftPolicy::ProviderWins,
        )
        .await?;
        assert_eq!(configured.role, "inbox");

        database.close().await;
        Ok(())
    }
}
