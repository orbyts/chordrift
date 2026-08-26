use std::collections::{HashMap, HashSet};

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Value, json};
use sqlx::{Postgres, QueryBuilder, Row, Transaction};
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result, terminal::TerminalProgress};

use super::{
    auth,
    models::{
        ExternalPlaylistInventory, PlaylistReuse, ReusePlan, SavedAlbumReuse, SavedTrackReuse,
        SpotifyAlbum, SpotifyArtist, SpotifyInventory, SpotifyTrack,
    },
};

const PROVIDER: &str = "spotify";
const MEMBERSHIP_INSERT_BATCH_SIZE: usize = 1_000;

#[derive(Clone, Copy, Debug)]
struct ResolvedTrack {
    provider_track_id: Uuid,
    canonical_track_id: Uuid,
}

#[derive(Debug)]
struct KnownTrack {
    resolved: ResolvedTrack,
    metadata: Value,
}

#[derive(Debug, Default)]
struct TrackCache {
    /// Tracks actually encountered in this import.
    resolved: HashMap<String, ResolvedTrack>,
    /// Existing provider rows loaded in one database request. Entries leave this
    /// map on first use so metadata is compared at most once per import.
    known: HashMap<String, KnownTrack>,
}

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
    /// Saved album entries persisted as a distinct, read-only library surface.
    pub saved_albums: usize,
    /// Ordered tracks inventoried inside saved albums.
    pub saved_album_tracks: usize,
    /// Whether saved albums and their tracks were copied from the prior snapshot.
    pub saved_albums_reused: bool,
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
    /// Incremental player-history observations returned by Spotify.
    pub recent_plays_seen: usize,
    /// New provisional player-history observations retained in Neon.
    pub recent_plays_inserted: usize,
    /// Newest player-history observation returned by Spotify.
    pub recent_plays_through: Option<DateTime<Utc>>,
}

/// Fetches a complete read-only Spotify inventory and persists it atomically.
pub async fn import(account_label: &str, database: &Database) -> Result<ImportReport> {
    let reuse = load_reuse_plan(account_label, database).await?;
    let session = auth::session(account_label).await?;
    let inventory = session.client.inventory(session.profile, &reuse).await?;
    persist(account_label, inventory, database).await
}

async fn load_reuse_plan(account_label: &str, database: &Database) -> Result<ReusePlan> {
    let recent_after = sqlx::query_scalar(
        "SELECT max(event.played_at)
         FROM normalized_listening_events event
         JOIN provider_accounts account ON account.id = event.provider_account_id
         JOIN historical_provider_track_identities identity
           ON identity.id = event.historical_identity_id
         WHERE account.provider = $1 AND account.account_label = $2
           AND identity.provider = $1 AND event.superseded_at IS NULL",
    )
    .bind(PROVIDER)
    .bind(account_label)
    .fetch_one(database.pool())
    .await?;
    let latest = sqlx::query(
        "SELECT snapshots.id, snapshots.metadata
         FROM provider_inventory_observations snapshots
         JOIN provider_accounts accounts ON accounts.id = snapshots.provider_account_id
         WHERE accounts.provider = $1 AND accounts.account_label = $2
         ORDER BY snapshots.captured_at DESC, snapshots.id DESC LIMIT 1",
    )
    .bind(PROVIDER)
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?;
    let Some(latest) = latest else {
        return Ok(ReusePlan {
            recent_after,
            ..ReusePlan::default()
        });
    };
    let source_snapshot_id: Uuid = latest.try_get("id")?;
    let metadata: Value = latest.try_get("metadata")?;
    let playlist_rows = sqlx::query(
        "SELECT playlists.provider_playlist_id, snapshots.provider_snapshot_id
         FROM provider_observed_playlists snapshots
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
             FROM provider_observed_saved_tracks saved
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
    let saved_albums = if let Some(total) = metadata
        .get("saved_albums_seen")
        .and_then(Value::as_u64)
        .and_then(|total| usize::try_from(total).ok())
    {
        let rows = sqlx::query(
            "SELECT saved.position, album.provider_album_id, saved.saved_at
             FROM provider_observed_saved_albums saved
             JOIN provider_albums album ON album.id = saved.provider_album_id
             WHERE saved.snapshot_id = $1 AND saved.position < 50
             ORDER BY saved.position",
        )
        .bind(source_snapshot_id)
        .fetch_all(database.pool())
        .await?;
        Some(SavedAlbumReuse {
            source_snapshot_id,
            total,
            leading_items: rows
                .into_iter()
                .map(|row| {
                    let position: i32 = row.try_get("position")?;
                    Ok((
                        usize::try_from(position).map_err(|_| {
                            ChordriftError::Configuration(
                                "stored saved-album position was negative".to_owned(),
                            )
                        })?,
                        row.try_get("provider_album_id")?,
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
        saved_albums,
        recent_after,
    })
}

async fn persist(
    account_label: &str,
    inventory: SpotifyInventory,
    database: &Database,
) -> Result<ImportReport> {
    let mut transaction = database.pool().begin().await?;
    let mut tracks = load_track_cache(&mut transaction).await?;
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
        "INSERT INTO provider_inventory_observations
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
        "saved_albums_seen": inventory.saved_albums.total,
    }))
    .execute(&mut *transaction)
    .await?;

    let mut playlist_entries = 0;
    let mut playlists_reused = 0;
    let mut saved_tracks = 0;
    let mut saved_albums = 0;
    let mut saved_album_tracks = 0;
    let mut unavailable_items = 0;
    let mut unsupported_items = 0;
    let mut external_bookmark_entries = 0;
    let mut external_bookmarks_reused = 0;
    let recent_plays_seen = inventory.recently_played.len();
    let recent_plays_through = inventory
        .recently_played
        .iter()
        .map(|item| item.played_at)
        .max()
        .or(inventory.recent_requested_after);

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
             SET role = 'inbox', drift_policy = 'provider_wins',
                 signal_class = 'routing', semantic_weight = 0.0,
                 behavioral_signal = NULL,
                 clear_policy = 'after_verified_assignment', updated_at = now()
             FROM provider_playlists provider
             JOIN playlists playlist ON playlist.id = provider.playlist_id
             WHERE account_playlist.provider_account_id = $1
               AND account_playlist.provider_playlist_id = $2
               AND provider.id = account_playlist.provider_playlist_id
               AND playlist.kind = 'routing'",
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
                 behavioral_signal = (
                     SELECT NULLIF(planned.payload->'detail'->>'behavioral_signal', '')
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
                     ORDER BY run.started_at DESC LIMIT 1
                 ),
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
            "INSERT INTO provider_inventory_import_playlists
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
                "INSERT INTO provider_inventory_import_playlist_tracks
                 (snapshot_id, provider_playlist_id, provider_track_id,
                  position, added_at, metadata, captured_at)
                 SELECT $1, provider_playlist_id, provider_track_id,
                        position, added_at, metadata, now()
                 FROM provider_observed_playlist_tracks
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

        let mut memberships = Vec::with_capacity(playlist_inventory.items.len());
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
            memberships.push((
                provider_track_id,
                to_i32(position, "playlist position")?,
                item.added_at,
                json!({
                "added_by": item.added_by,
                "is_local": item.is_local,
                }),
            ));
        }
        for chunk in memberships.chunks(MEMBERSHIP_INSERT_BATCH_SIZE) {
            let mut insert = QueryBuilder::<Postgres>::new(
                "INSERT INTO provider_inventory_import_playlist_tracks \
                 (snapshot_id, provider_playlist_id, provider_track_id, \
                  position, added_at, metadata) ",
            );
            insert.push_values(
                chunk,
                |mut row, (provider_track_id, position, added_at, metadata)| {
                    row.push_bind(snapshot_id)
                        .push_bind(provider_playlist_id)
                        .push_bind(*provider_track_id)
                        .push_bind(*position)
                        .push_bind(*added_at)
                        .push_bind(metadata);
                },
            );
            insert.build().execute(&mut *transaction).await?;
            playlist_entries += chunk.len();
        }
    }

    // Provider additions to a route are corrective intent. Capture them into
    // durable desired membership before any later verified reassignment clears
    // the provider inbox. Existing rows are retained if the user removes an
    // item before that reassignment is complete.
    sqlx::query(
        "WITH candidates AS (
             SELECT provider.playlist_id,
                    provider_track.track_id,
                    membership.position,
                    provider_track.provider_track_id,
                    row_number() OVER (
                        PARTITION BY provider.playlist_id, provider_track.track_id
                        ORDER BY membership.position
                    ) AS duplicate_rank
             FROM routing_surfaces route
             JOIN provider_playlists provider
               ON provider.playlist_id = route.playlist_id
              AND provider.provider = 'spotify'
             JOIN provider_inventory_import_playlist_tracks membership
               ON membership.provider_playlist_id = provider.id
              AND membership.snapshot_id = $1
             JOIN provider_tracks provider_track
               ON provider_track.id = membership.provider_track_id
             WHERE route.provider_account_id = $2 AND route.active
               AND NOT EXISTS (
                   SELECT 1 FROM playlist_tracks desired
                   WHERE desired.playlist_id = provider.playlist_id
                     AND desired.track_id = provider_track.track_id
               )
         ), route_tracks AS (
             SELECT playlist_id, track_id, position, provider_track_id,
                    row_number() OVER (
                        PARTITION BY playlist_id
                        ORDER BY position
                    ) - 1 AS offset
             FROM candidates WHERE duplicate_rank = 1
         ), next_positions AS (
             SELECT route.playlist_id,
                    COALESCE(max(existing.position) + 1, 0) AS next_position
             FROM (SELECT DISTINCT playlist_id FROM route_tracks) route
             LEFT JOIN playlist_tracks existing ON existing.playlist_id = route.playlist_id
             GROUP BY route.playlist_id
         )
         INSERT INTO playlist_tracks
             (playlist_id, track_id, position, source, provenance)
         SELECT route.playlist_id, route.track_id,
                next.next_position + route.offset::integer,
                'manual',
                jsonb_build_object(
                    'captured_via', 'spotify_route_pull',
                    'source_snapshot_id', $1::text,
                    'source_position', route.position,
                    'spotify_track_id', route.provider_track_id
                )
         FROM route_tracks route
         JOIN next_positions next ON next.playlist_id = route.playlist_id",
    )
    .bind(snapshot_id)
    .bind(account_id)
    .execute(&mut *transaction)
    .await?;

    // Re-evaluate is a provider-owned holding queue. Preserve every observed
    // entry/exit as an immutable event, then mirror only its current Spotify
    // membership into the operational queue. This prevents Chordrift from
    // re-adding a track after the user deliberately removes it from the queue.
    sqlx::query(
        "WITH queue AS (
             SELECT route.playlist_id, provider.id AS provider_playlist_id
             FROM routing_surfaces route
             JOIN provider_playlists provider
               ON provider.playlist_id = route.playlist_id
              AND provider.provider = 'spotify'
             JOIN provider_inventory_import_playlists observed
               ON observed.provider_playlist_id = provider.id
              AND observed.snapshot_id = $1
             WHERE route.provider_account_id = $2
               AND route.active AND route.purpose = 'reevaluate'
         ), previous AS (
             SELECT id
             FROM provider_inventory_observations
             WHERE provider_account_id = $2 AND id <> $1
             ORDER BY captured_at DESC, id DESC LIMIT 1
         ), current_tracks AS (
             SELECT queue.playlist_id, provider_track.track_id
             FROM queue
             JOIN provider_inventory_import_playlist_tracks membership
               ON membership.snapshot_id = $1
              AND membership.provider_playlist_id = queue.provider_playlist_id
             JOIN provider_tracks provider_track
               ON provider_track.id = membership.provider_track_id
         ), previous_tracks AS (
             SELECT queue.playlist_id, provider_track.track_id
             FROM queue
             CROSS JOIN previous
             JOIN provider_inventory_import_playlist_tracks membership
               ON membership.snapshot_id = previous.id
              AND membership.provider_playlist_id = queue.provider_playlist_id
             JOIN provider_tracks provider_track
               ON provider_track.id = membership.provider_track_id
         ), changes AS (
             SELECT current.playlist_id, current.track_id, 'entered'::text AS event_type,
                    (
                        SELECT previous_provider.concept_id
                        FROM previous prior_snapshot
                        JOIN provider_playlists previous_provider
                          ON previous_provider.provider = 'spotify'
                         AND previous_provider.concept_id IS NOT NULL
                        JOIN provider_inventory_import_playlist_tracks prior_membership
                          ON prior_membership.snapshot_id = prior_snapshot.id
                         AND prior_membership.provider_playlist_id = previous_provider.id
                        JOIN provider_tracks prior_track
                          ON prior_track.id = prior_membership.provider_track_id
                         AND prior_track.track_id = current.track_id
                        WHERE NOT EXISTS (
                            SELECT 1
                            FROM provider_inventory_import_playlist_tracks current_membership
                            JOIN provider_tracks current_track
                              ON current_track.id = current_membership.provider_track_id
                            WHERE current_membership.snapshot_id = $1
                              AND current_membership.provider_playlist_id = previous_provider.id
                              AND current_track.track_id = current.track_id)
                        ORDER BY previous_provider.id LIMIT 1
                    ) AS previous_concept_id
             FROM current_tracks current
             WHERE NOT EXISTS (
                 SELECT 1 FROM previous_tracks prior
                 WHERE prior.playlist_id = current.playlist_id
                   AND prior.track_id = current.track_id
             )
             UNION ALL
             SELECT prior.playlist_id, prior.track_id, 'left'::text AS event_type,
                    NULL::uuid AS previous_concept_id
             FROM previous_tracks prior
             WHERE NOT EXISTS (
                 SELECT 1 FROM current_tracks current
                 WHERE current.playlist_id = prior.playlist_id
                   AND current.track_id = prior.track_id
             )
         )
         INSERT INTO reevaluation_events
             (provider_account_id, track_id, playlist_id,
              provider_snapshot_id, event_type,
              metadata)
         SELECT $2, change.track_id, change.playlist_id, $1,
                change.event_type,
                jsonb_build_object(
                    'captured_via', 'spotify_sync_pull',
                    'previous_concept_id', change.previous_concept_id
                )
         FROM changes change
         ON CONFLICT DO NOTHING",
    )
    .bind(snapshot_id)
    .bind(account_id)
    .execute(&mut *transaction)
    .await?;

    sqlx::query(
        "DELETE FROM playlist_tracks desired
         USING routing_surfaces route, provider_playlists provider,
               provider_inventory_import_playlists observed
         WHERE route.provider_account_id = $2
           AND route.active AND route.purpose = 'reevaluate'
           AND desired.playlist_id = route.playlist_id
           AND provider.playlist_id = route.playlist_id
           AND provider.provider = 'spotify'
           AND observed.provider_playlist_id = provider.id
           AND observed.snapshot_id = $1",
    )
    .bind(snapshot_id)
    .bind(account_id)
    .execute(&mut *transaction)
    .await?;

    // The preceding delete makes this an exact provider-owned replacement.
    // `playlist_tracks` deliberately has no unconditional membership key
    // because manual playlists may contain the same track more than once, so
    // this insert must not name the generated-membership partial index as an
    // ON CONFLICT target.
    sqlx::query(
        "WITH queue_tracks AS (
             SELECT route.playlist_id, provider_track.track_id,
                    membership.position,
                    provider_track.provider_track_id
             FROM routing_surfaces route
             JOIN provider_playlists provider
               ON provider.playlist_id = route.playlist_id
              AND provider.provider = 'spotify'
             JOIN provider_inventory_import_playlist_tracks membership
               ON membership.provider_playlist_id = provider.id
              AND membership.snapshot_id = $1
             JOIN provider_tracks provider_track
               ON provider_track.id = membership.provider_track_id
             WHERE route.provider_account_id = $2
               AND route.active AND route.purpose = 'reevaluate'
         )
         INSERT INTO playlist_tracks
             (playlist_id, track_id, position, source, provenance)
         SELECT playlist_id, track_id, position, 'manual',
                jsonb_build_object(
                    'captured_via', 'spotify_reevaluate_pull',
                    'source_snapshot_id', $1::text,
                    'spotify_track_id', provider_track_id
                )
         FROM queue_tracks",
    )
    .bind(snapshot_id)
    .bind(account_id)
    .execute(&mut *transaction)
    .await?;

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
            "INSERT INTO provider_inventory_import_saved_tracks
             (snapshot_id, provider_track_id, position, saved_at, metadata)
             SELECT $1, provider_track_id, position, saved_at, metadata
             FROM provider_observed_saved_tracks WHERE snapshot_id = $2",
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
    let mut saved_memberships = Vec::with_capacity(inventory.saved_tracks.items.len());
    let mut saved_progress = TerminalProgress::new(
        "Neon · resolve saved tracks",
        inventory.saved_tracks.items.len(),
    );
    for (position, saved) in inventory.saved_tracks.items.iter().enumerate() {
        saved_progress.set_position(position + 1);
        let Some(track) = saved.track.as_ref() else {
            unavailable_items += 1;
            continue;
        };
        let Some(provider_track_id) =
            persist_track(track, &mut tracks, &mut transaction, &mut unsupported_items).await?
        else {
            continue;
        };
        saved_memberships.push((
            provider_track_id,
            to_i32(position, "saved-track position")?,
            saved.added_at,
        ));
    }
    saved_progress.finish();
    for chunk in saved_memberships.chunks(MEMBERSHIP_INSERT_BATCH_SIZE) {
        let mut insert = QueryBuilder::<Postgres>::new(
            "INSERT INTO provider_inventory_import_saved_tracks \
             (snapshot_id, provider_track_id, position, saved_at, metadata) ",
        );
        insert.push_values(chunk, |mut row, (provider_track_id, position, saved_at)| {
            row.push_bind(snapshot_id)
                .push_bind(*provider_track_id)
                .push_bind(*position)
                .push_bind(*saved_at)
                .push("'{}'::jsonb");
        });
        insert.build().execute(&mut *transaction).await?;
        saved_tracks += chunk.len();
        eprintln!(
            "spotify persist: saved-track memberships {saved_tracks}/{}",
            saved_memberships.len()
        );
    }

    let saved_albums_reused = inventory.saved_albums.reused_from_snapshot.is_some();
    if let Some(source_snapshot_id) = inventory.saved_albums.reused_from_snapshot {
        saved_albums = usize::try_from(
            sqlx::query(
                "INSERT INTO provider_inventory_import_saved_albums
                 (snapshot_id, provider_album_id, position, saved_at, metadata)
                 SELECT $1, provider_album_id, position, saved_at, metadata
                 FROM provider_observed_saved_albums WHERE snapshot_id = $2",
            )
            .bind(snapshot_id)
            .bind(source_snapshot_id)
            .execute(&mut *transaction)
            .await?
            .rows_affected(),
        )
        .map_err(|_| {
            ChordriftError::Configuration(
                "copied saved-album count exceeds platform limits".to_owned(),
            )
        })?;
        saved_album_tracks = usize::try_from(
            sqlx::query(
                "INSERT INTO provider_inventory_import_saved_album_tracks
                 (snapshot_id, provider_album_id, provider_track_id, position, metadata)
                 SELECT $1, provider_album_id, provider_track_id, position, metadata
                 FROM provider_observed_saved_album_tracks WHERE snapshot_id = $2",
            )
            .bind(snapshot_id)
            .bind(source_snapshot_id)
            .execute(&mut *transaction)
            .await?
            .rows_affected(),
        )
        .map_err(|_| {
            ChordriftError::Configuration(
                "copied saved-album track count exceeds platform limits".to_owned(),
            )
        })?;
    } else {
        let album_track_total: usize = inventory
            .saved_albums
            .items
            .iter()
            .map(|album| album.tracks.len())
            .sum();
        let mut album_progress =
            TerminalProgress::new("Neon · inventory album tracks", album_track_total);
        let mut album_tracks_seen = 0_usize;
        let mut album_memberships = Vec::with_capacity(album_track_total);
        for (album_position, saved) in inventory.saved_albums.items.iter().enumerate() {
            let Some(_) = persist_album(&saved.album, &mut transaction).await? else {
                unsupported_items += 1;
                continue;
            };
            let provider_album_id: Uuid = sqlx::query_scalar(
                "SELECT id FROM provider_albums
                 WHERE provider = $1 AND provider_album_id = $2",
            )
            .bind(PROVIDER)
            .bind(saved.album.id.as_deref().unwrap_or_default())
            .fetch_one(&mut *transaction)
            .await?;
            sqlx::query(
                "INSERT INTO provider_inventory_import_saved_albums
                 (snapshot_id, provider_album_id, position, saved_at, metadata)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(snapshot_id)
            .bind(provider_album_id)
            .bind(to_i32(album_position, "saved-album position")?)
            .bind(saved.saved_at)
            .bind(serde_json::to_value(&saved.album)?)
            .execute(&mut *transaction)
            .await?;
            saved_albums += 1;
            for (track_position, track) in saved.tracks.iter().enumerate() {
                album_tracks_seen += 1;
                album_progress.set_position(album_tracks_seen);
                let Some(provider_track_id) = persist_album_track(
                    track,
                    &saved.album,
                    &mut tracks,
                    &mut transaction,
                    &mut unsupported_items,
                )
                .await?
                else {
                    continue;
                };
                album_memberships.push((
                    provider_album_id,
                    provider_track_id,
                    to_i32(track_position, "saved-album track position")?,
                ));
            }
        }
        album_progress.finish();
        for chunk in album_memberships.chunks(MEMBERSHIP_INSERT_BATCH_SIZE) {
            let mut insert = QueryBuilder::<Postgres>::new(
                "INSERT INTO provider_inventory_import_saved_album_tracks \
                 (snapshot_id, provider_album_id, provider_track_id, position, metadata) ",
            );
            insert.push_values(
                chunk,
                |mut row, (provider_album_id, provider_track_id, position)| {
                    row.push_bind(snapshot_id)
                        .push_bind(*provider_album_id)
                        .push_bind(*provider_track_id)
                        .push_bind(*position)
                        .push("'{}'::jsonb");
                },
            );
            insert.build().execute(&mut *transaction).await?;
            saved_album_tracks += chunk.len();
        }
    }

    let mut recent_plays_inserted = 0_usize;
    for item in &inventory.recently_played {
        let Some(provider_track_row_id) = persist_track(
            &item.track,
            &mut tracks,
            &mut transaction,
            &mut unsupported_items,
        )
        .await?
        else {
            continue;
        };
        let canonical_track_id: Uuid =
            sqlx::query_scalar("SELECT track_id FROM provider_tracks WHERE id = $1")
                .bind(provider_track_row_id)
                .fetch_one(&mut *transaction)
                .await?;
        let Some(provider_track_id) = item.track.id.as_deref() else {
            continue;
        };
        let source_event_id = format!(
            "spotify-recent-v1:{provider_track_id}:{}",
            item.played_at.to_rfc3339()
        );
        let historical_identity_id: Uuid = sqlx::query_scalar(
            "INSERT INTO historical_provider_track_identities
             (provider, provider_track_id, canonical_track_id, track_name,
              artist_name, album_name, first_observed_at, last_observed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
             ON CONFLICT (provider, provider_track_id) DO UPDATE SET
               canonical_track_id = COALESCE(EXCLUDED.canonical_track_id,
                   historical_provider_track_identities.canonical_track_id),
               track_name = COALESCE(EXCLUDED.track_name,
                   historical_provider_track_identities.track_name),
               artist_name = COALESCE(EXCLUDED.artist_name,
                   historical_provider_track_identities.artist_name),
               album_name = COALESCE(EXCLUDED.album_name,
                   historical_provider_track_identities.album_name),
               first_observed_at = LEAST(
                   historical_provider_track_identities.first_observed_at,
                   EXCLUDED.first_observed_at),
               last_observed_at = GREATEST(
                   historical_provider_track_identities.last_observed_at,
                   EXCLUDED.last_observed_at)
             RETURNING id",
        )
        .bind(PROVIDER)
        .bind(provider_track_id)
        .bind(canonical_track_id)
        .bind(&item.track.name)
        .bind(
            item.track
                .artists
                .first()
                .map(|artist| artist.name.as_str()),
        )
        .bind(item.track.album.as_ref().map(|album| album.name.as_str()))
        .bind(item.played_at)
        .fetch_one(&mut *transaction)
        .await?;
        let inserted = sqlx::query(
            "INSERT INTO normalized_listening_events
             (id, provider_account_id, historical_identity_id, source_kind,
              source_event_id, played_at, context_uri, context_type,
              provider_extensions)
             VALUES ($1, $2, $3, 'recent_api', $4, $5, $6, $7, $8)
             ON CONFLICT DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(account_id)
        .bind(historical_identity_id)
        .bind(source_event_id)
        .bind(item.played_at)
        .bind(item.context.as_ref().map(|context| context.uri.as_str()))
        .bind(item.context.as_ref().map(|context| context.kind.as_str()))
        .bind(json!({
            "observation": "recently_played",
            "duration_unknown": true,
        }))
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        recent_plays_inserted =
            recent_plays_inserted.saturating_add(usize::try_from(inserted).map_err(|_| {
                ChordriftError::Configuration(
                    "recent play insert count exceeded platform limits".to_owned(),
                )
            })?);
    }
    sqlx::query(
        "INSERT INTO spotify_recent_play_syncs
         (provider_account_id, requested_after, newest_played_at,
          observations_seen, observations_inserted)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(account_id)
    .bind(inventory.recent_requested_after)
    .bind(recent_plays_through)
    .bind(to_i32(recent_plays_seen, "recent play observation count")?)
    .bind(to_i32(recent_plays_inserted, "inserted recent play count")?)
    .execute(&mut *transaction)
    .await?;

    // One set-based observation update replaces one write per already-known
    // track while preserving the meaning of provider_tracks.last_seen_at.
    let observed_track_ids = tracks
        .resolved
        .values()
        .map(|track| track.provider_track_id)
        .collect::<Vec<_>>();
    if !observed_track_ids.is_empty() {
        sqlx::query(
            "UPDATE provider_tracks SET last_seen_at = now()
             WHERE id = ANY($1)",
        )
        .bind(&observed_track_ids)
        .execute(&mut *transaction)
        .await?;
    }

    // Database v2 keeps one replaceable current inventory while reusing
    // content-addressed playlist and saved-surface bodies. The import rows are
    // transaction-local staging in practice and are removed after the durable
    // revisions and current pointers have been materialized.
    sqlx::query("SELECT materialize_provider_current_state_v2($1, $2)")
        .bind(account_id)
        .bind(snapshot_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "WITH playlist_tracks AS (
             DELETE FROM provider_inventory_import_playlist_tracks
              WHERE snapshot_id = $1 RETURNING 1
         ), playlists AS (
             DELETE FROM provider_inventory_import_playlists
              WHERE snapshot_id = $1 RETURNING 1
         ), saved_album_tracks AS (
             DELETE FROM provider_inventory_import_saved_album_tracks
              WHERE snapshot_id = $1 RETURNING 1
         ), saved_albums AS (
             DELETE FROM provider_inventory_import_saved_albums
              WHERE snapshot_id = $1 RETURNING 1
         )
         DELETE FROM provider_inventory_import_saved_tracks
          WHERE snapshot_id = $1",
    )
    .bind(snapshot_id)
    .execute(&mut *transaction)
    .await?;

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
        "unique_tracks": tracks.resolved.len(),
        "external_bookmarks": inventory.external_playlists.len(),
        "external_bookmark_entries": external_bookmark_entries,
        "recent_plays_seen": recent_plays_seen,
            "recent_plays_inserted": recent_plays_inserted,
            "saved_albums": saved_albums,
            "saved_album_tracks": saved_album_tracks,
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
        saved_albums,
        saved_album_tracks,
        saved_albums_reused,
        unavailable_items,
        unsupported_items,
        followed_playlists_skipped: inventory.followed_playlists_skipped,
        inaccessible_collaborative_playlists: inventory.inaccessible_collaborative_playlists,
        external_bookmarks: inventory.external_playlists.len(),
        external_bookmarks_reused,
        external_bookmark_entries,
        recent_plays_seen,
        recent_plays_inserted,
        recent_plays_through,
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
    cache: &mut TrackCache,
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
    if let Some(track) = cache.resolved.get(provider_id) {
        return Ok(Some(track.provider_track_id));
    }

    let metadata = serde_json::to_value(track)?;
    let known = cache.known.remove(provider_id);
    if let Some(known) = known.as_ref()
        && known.metadata == metadata
    {
        cache.resolved.insert(provider_id.clone(), known.resolved);
        return Ok(Some(known.resolved.provider_track_id));
    }

    let album_id = match track.album.as_ref() {
        Some(album) => persist_album(album, transaction).await?,
        None => None,
    };
    let resolved = if let Some(known) = known {
        let provider_track_id = known.resolved.provider_track_id;
        let canonical_track_id = known.resolved.canonical_track_id;
        sqlx::query(
            "UPDATE tracks SET album_id = COALESCE($2, album_id), title = $3, normalized_title = $4,
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
        .bind(&metadata)
        .execute(&mut **transaction)
        .await?;
        known.resolved
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
        .bind(&metadata)
        .fetch_one(&mut **transaction)
        .await?;
        ResolvedTrack {
            provider_track_id,
            canonical_track_id,
        }
    };

    sqlx::query("DELETE FROM track_artists WHERE track_id = $1")
        .bind(resolved.canonical_track_id)
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
            .bind(resolved.canonical_track_id)
            .bind(artist_id)
            .bind(to_i32(position, "artist position")?)
            .execute(&mut **transaction)
            .await?;
        }
    }
    cache.resolved.insert(provider_id.clone(), resolved);
    Ok(Some(resolved.provider_track_id))
}

async fn load_track_cache(transaction: &mut Transaction<'_, Postgres>) -> Result<TrackCache> {
    let rows = sqlx::query(
        "SELECT id, track_id, provider_track_id, metadata
         FROM provider_tracks WHERE provider = $1",
    )
    .bind(PROVIDER)
    .fetch_all(&mut **transaction)
    .await?;
    let mut known = HashMap::with_capacity(rows.len());
    for row in rows {
        let provider_id: String = row.try_get("provider_track_id")?;
        known.insert(
            provider_id,
            KnownTrack {
                resolved: ResolvedTrack {
                    provider_track_id: row.try_get("id")?,
                    canonical_track_id: row.try_get("track_id")?,
                },
                metadata: row.try_get("metadata")?,
            },
        );
    }
    Ok(TrackCache {
        resolved: HashMap::new(),
        known,
    })
}

async fn persist_album_track(
    track: &SpotifyTrack,
    album: &SpotifyAlbum,
    cache: &mut TrackCache,
    transaction: &mut Transaction<'_, Postgres>,
    unsupported_items: &mut usize,
) -> Result<Option<Uuid>> {
    let Some(provider_id) = track.id.as_ref() else {
        *unsupported_items += 1;
        return Ok(None);
    };
    if let Some(resolved) = cache.resolved.get(provider_id) {
        return Ok(Some(resolved.provider_track_id));
    }
    // Saved-album responses contain simplified tracks. Reuse existing full
    // metadata instead of downgrading it and issuing artist/album writes.
    if let Some(known) = cache.known.remove(provider_id) {
        cache.resolved.insert(provider_id.clone(), known.resolved);
        return Ok(Some(known.resolved.provider_track_id));
    }
    let mut full_context = track.clone();
    let mut album = album.clone();
    album.tracks = None;
    full_context.album = Some(album);
    persist_track(&full_context, cache, transaction, unsupported_items).await
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
        db, db_reports, playlists,
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
            saved_albums: super::super::models::SavedAlbumsInventory {
                total: 0,
                items: Vec::new(),
                reused_from_snapshot: None,
            },
            recently_played: Vec::new(),
            recent_requested_after: None,
            playlists_seen: 2,
            followed_playlists_skipped: 1,
            inaccessible_collaborative_playlists: 0,
        };

        let repeated_inventory = inventory.clone();
        let report = persist("fixture", inventory, &database).await?;
        assert_eq!(report.playlists_seen, 2);
        assert_eq!(report.playlists_imported, 1);
        assert_eq!(report.playlist_entries, 1);
        assert_eq!(report.saved_tracks, 1);
        assert_eq!(report.unavailable_items, 2);

        let playlist_rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM provider_inventory_import_playlist_tracks WHERE snapshot_id = $1",
        )
        .bind(report.snapshot_id)
        .fetch_one(database.pool())
        .await?;
        let saved_rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM provider_inventory_import_saved_tracks WHERE snapshot_id = $1",
        )
        .bind(report.snapshot_id)
        .fetch_one(database.pool())
        .await?;
        assert_eq!(playlist_rows, 0, "playlist import staging must be empty");
        assert_eq!(saved_rows, 0, "saved-track import staging must be empty");

        // A changed-snapshot import may contain the same provider metadata.
        // Repeating it exercises the known-track fast path and batched saved
        // membership insert without creating duplicate canonical rows.
        let repeated = persist("fixture", repeated_inventory, &database).await?;
        assert_eq!(repeated.saved_tracks, 1);
        let provider_tracks: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM provider_tracks
             WHERE provider = 'spotify' AND provider_track_id = 'track123'",
        )
        .fetch_one(database.pool())
        .await?;
        assert_eq!(provider_tracks, 1);
        let fixture_account_id: Uuid =
            sqlx::query_scalar("SELECT id FROM provider_accounts WHERE account_label = 'fixture'")
                .fetch_one(database.pool())
                .await?;
        let current_inventories: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM provider_current_inventories
             WHERE provider_account_id = $1",
        )
        .bind(fixture_account_id)
        .fetch_one(database.pool())
        .await?;
        let current_playlists: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM provider_current_playlists
             WHERE provider_account_id = $1",
        )
        .bind(fixture_account_id)
        .fetch_one(database.pool())
        .await?;
        let playlist_revisions: i64 = sqlx::query_scalar(
            "SELECT count(DISTINCT revision_id) FROM provider_current_playlists
             WHERE provider_account_id = $1",
        )
        .bind(fixture_account_id)
        .fetch_one(database.pool())
        .await?;
        let revision_tracks: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM provider_current_playlists current
             JOIN provider_playlist_revision_tracks track
               ON track.revision_id = current.revision_id
             WHERE current.provider_account_id = $1",
        )
        .bind(fixture_account_id)
        .fetch_one(database.pool())
        .await?;
        let saved_revisions: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM provider_saved_track_revisions
             WHERE provider_account_id = $1",
        )
        .bind(fixture_account_id)
        .fetch_one(database.pool())
        .await?;
        assert_eq!(current_inventories, 1);
        assert_eq!(current_playlists, 1);
        assert_eq!(playlist_revisions, 1, "unchanged bodies must be reused");
        assert_eq!(revision_tracks, 1);
        assert_eq!(saved_revisions, 1, "unchanged saved state must be reused");
        let v2_status = db_reports::database_v2_status(&database, "fixture").await?;
        assert!(v2_status.current_playlist_headers_match);
        assert!(v2_status.current_playlist_order_matches);
        assert!(v2_status.current_saved_tracks_match);
        assert!(v2_status.current_saved_albums_match);
        assert!(v2_status.ready_for_cutover);

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
