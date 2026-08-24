//! Account-scoped provider playlist roles and drift policy.

use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result};

/// How Chordrift treats a provider playlist in the orchestration workflow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaylistRole {
    /// Provider-owned playlist mirrored into Neon without remote management.
    Observed,
    /// Provider-native discovery surface intended for later consumption.
    Inbox,
    /// Canonical playlist whose approved desired state will be owned by Neon.
    Managed,
}

impl PlaylistRole {
    /// Stable database representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Observed => "observed",
            Self::Inbox => "inbox",
            Self::Managed => "managed",
        }
    }
}

/// Which side wins when a provider playlist differs from approved Neon state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriftPolicy {
    /// Import provider edits into Neon.
    ProviderWins,
    /// Restore approved Neon state to the provider during a future apply operation.
    NeonWins,
    /// Require an explicit decision before either side is changed.
    Manual,
}

impl DriftPolicy {
    /// Stable database representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProviderWins => "provider_wins",
            Self::NeonWins => "neon_wins",
            Self::Manual => "manual",
        }
    }
}

/// One account-scoped playlist configuration.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistRecord {
    /// Spotify playlist ID.
    pub provider_playlist_id: String,
    /// Most recently imported name.
    pub name: String,
    /// Orchestration role.
    pub role: String,
    /// Configured drift policy.
    pub drift_policy: String,
    /// Relative contribution to personal embeddings; zero excludes the playlist.
    pub embedding_weight: f64,
    /// Whether it exists in the latest imported snapshot.
    pub present: bool,
    /// Item count reported by the latest snapshot, when present.
    pub total_items: Option<i32>,
}

/// One ordered track entry from a playlist's latest imported snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaylistTrackRecord {
    /// Zero-based provider position retained in Neon.
    pub position: i32,
    /// Canonical track title.
    pub title: String,
    /// Ordered display artist string.
    pub artists: String,
    /// Canonical album title, when Spotify supplied one.
    pub album: Option<String>,
    /// Stable Spotify track ID.
    pub provider_track_id: String,
}

/// Current ordered contents of one account-scoped playlist.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistTracks {
    /// Playlist resolved from the user selector.
    pub playlist: PlaylistRecord,
    /// Immutable library snapshot supplying the entries.
    pub snapshot_id: Uuid,
    /// Ordered entries. Canonical duplicates remain separate rows.
    pub tracks: Vec<PlaylistTrackRecord>,
}

/// Selects one playlist without relying exclusively on a mutable display name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlaylistSelector {
    /// Select by exact Spotify playlist ID.
    ProviderId(String),
    /// Select by case-insensitive current name; the match must be unambiguous.
    Name(String),
}

/// Lists every playlist observed for one local account label.
pub async fn list(database: &Database, account_label: &str) -> Result<Vec<PlaylistRecord>> {
    let account_id = account_id(database, account_label).await?;
    let rows = sqlx::query(
        "SELECT provider.provider_playlist_id,
                COALESCE(provider.metadata->>'name', canonical.name) AS name,
                account_playlist.role, account_playlist.drift_policy,
                account_playlist.embedding_weight,
                account_playlist.present_in_latest_snapshot,
                latest_playlist.total_items
         FROM provider_account_playlists account_playlist
         JOIN provider_playlists provider ON provider.id = account_playlist.provider_playlist_id
         JOIN playlists canonical ON canonical.id = provider.playlist_id
         LEFT JOIN LATERAL (
             SELECT snapshots.total_items
             FROM provider_playlist_snapshots snapshots
             JOIN provider_library_snapshots library ON library.id = snapshots.snapshot_id
             WHERE snapshots.provider_playlist_id = provider.id
               AND library.provider_account_id = account_playlist.provider_account_id
             ORDER BY library.captured_at DESC, library.id DESC
             LIMIT 1
         ) latest_playlist ON TRUE
         WHERE account_playlist.provider_account_id = $1
         ORDER BY lower(COALESCE(provider.metadata->>'name', canonical.name)),
                  provider.provider_playlist_id",
    )
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(PlaylistRecord {
                provider_playlist_id: row.try_get("provider_playlist_id")?,
                name: row.try_get("name")?,
                role: row.try_get("role")?,
                drift_policy: row.try_get("drift_policy")?,
                embedding_weight: row.try_get("embedding_weight")?,
                present: row.try_get("present_in_latest_snapshot")?,
                total_items: row.try_get("total_items")?,
            })
        })
        .collect()
}

/// Sets one playlist's semantic contribution; zero excludes it from embeddings.
pub async fn set_embedding_weight(
    database: &Database,
    account_label: &str,
    selector: &PlaylistSelector,
    weight: f64,
) -> Result<PlaylistRecord> {
    if !weight.is_finite() || !(0.0..=10.0).contains(&weight) {
        return Err(ChordriftError::Configuration(
            "playlist embedding weight must be between 0 and 10".to_owned(),
        ));
    }
    let account_id = account_id(database, account_label).await?;
    let selected = resolve_selector(database, account_label, selector).await?;
    sqlx::query(
        "UPDATE provider_account_playlists account_playlist
         SET embedding_weight = $3, updated_at = now()
         FROM provider_playlists provider
         WHERE account_playlist.provider_account_id = $1
           AND account_playlist.provider_playlist_id = provider.id
           AND provider.provider_playlist_id = $2",
    )
    .bind(account_id)
    .bind(&selected.provider_playlist_id)
    .bind(weight)
    .execute(database.pool())
    .await?;
    Ok(PlaylistRecord {
        embedding_weight: weight,
        ..selected
    })
}

/// Lists the ordered tracks in one playlist's latest imported snapshot.
pub async fn tracks(
    database: &Database,
    account_label: &str,
    selector: &PlaylistSelector,
) -> Result<PlaylistTracks> {
    let account_id = account_id(database, account_label).await?;
    let playlist = resolve_selector(database, account_label, selector).await?;
    if !playlist.present {
        return Err(ChordriftError::Configuration(
            "playlist is not present in this account's latest imported snapshot".to_owned(),
        ));
    }
    let snapshot_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM provider_library_snapshots
         WHERE provider_account_id = $1
         ORDER BY captured_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let rows = sqlx::query(
        "SELECT membership.position, track.title,
                COALESCE(string_agg(artist.name, ', ' ORDER BY track_artist.position), '') AS artists,
                album.title AS album, provider_track.provider_track_id
         FROM provider_playlist_tracks membership
         JOIN provider_playlists provider_playlist
           ON provider_playlist.id = membership.provider_playlist_id
         JOIN provider_tracks provider_track
           ON provider_track.id = membership.provider_track_id
         JOIN tracks track ON track.id = provider_track.track_id
         LEFT JOIN albums album ON album.id = track.album_id
         LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
         LEFT JOIN artists artist ON artist.id = track_artist.artist_id
         WHERE membership.snapshot_id = $1
           AND provider_playlist.provider = 'spotify'
           AND provider_playlist.provider_playlist_id = $2
         GROUP BY membership.position, track.title, album.title,
                  provider_track.provider_track_id
         ORDER BY membership.position",
    )
    .bind(snapshot_id)
    .bind(&playlist.provider_playlist_id)
    .fetch_all(database.pool())
    .await?;
    let tracks = rows
        .into_iter()
        .map(|row| {
            Ok(PlaylistTrackRecord {
                position: row.try_get("position")?,
                title: row.try_get("title")?,
                artists: row.try_get("artists")?,
                album: row.try_get("album")?,
                provider_track_id: row.try_get("provider_track_id")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(PlaylistTracks {
        playlist,
        snapshot_id,
        tracks,
    })
}

/// Updates one playlist's role and drift policy.
pub async fn configure(
    database: &Database,
    account_label: &str,
    selector: &PlaylistSelector,
    role: PlaylistRole,
    drift_policy: DriftPolicy,
) -> Result<PlaylistRecord> {
    let account_id = account_id(database, account_label).await?;
    let selected = resolve_selector(database, account_label, selector).await?;
    sqlx::query(
        "UPDATE provider_account_playlists account_playlist
         SET role = $3, drift_policy = $4, updated_at = now()
         FROM provider_playlists provider
         WHERE account_playlist.provider_account_id = $1
           AND account_playlist.provider_playlist_id = provider.id
           AND provider.provider_playlist_id = $2",
    )
    .bind(account_id)
    .bind(&selected.provider_playlist_id)
    .bind(role.as_str())
    .bind(drift_policy.as_str())
    .execute(database.pool())
    .await?;
    Ok(PlaylistRecord {
        role: role.as_str().to_owned(),
        drift_policy: drift_policy.as_str().to_owned(),
        ..selected
    })
}

async fn resolve_selector(
    database: &Database,
    account_label: &str,
    selector: &PlaylistSelector,
) -> Result<PlaylistRecord> {
    let rows = list(database, account_label).await?;
    let matches: Vec<_> = rows
        .into_iter()
        .filter(|playlist| match selector {
            PlaylistSelector::ProviderId(id) => playlist.provider_playlist_id == *id,
            PlaylistSelector::Name(name) => playlist.name.eq_ignore_ascii_case(name),
        })
        .collect();
    let [selected] = matches.as_slice() else {
        return Err(ChordriftError::Configuration(if matches.is_empty() {
            "playlist selector did not match this account's imported playlists".to_owned()
        } else {
            "playlist name is ambiguous; select it by Spotify playlist ID".to_owned()
        }));
    };
    Ok(selected.clone())
}

async fn account_id(database: &Database, account_label: &str) -> Result<Uuid> {
    sqlx::query_scalar(
        "SELECT id FROM provider_accounts
         WHERE provider = 'spotify' AND account_label = $1",
    )
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(format!(
            "Spotify account {account_label:?} has not been imported"
        ))
    })
}
