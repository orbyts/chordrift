use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One page returned by Spotify's offset-based APIs.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<String>,
    pub total: usize,
}

/// Current Spotify account identity.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct CurrentUser {
    /// Stable account-linking identity introduced by Spotify in 2026.
    pub account_id: String,
    /// Spotify user ID used by playlist owner objects.
    pub id: String,
    /// User-facing account name, when available.
    pub display_name: Option<String>,
    /// Public Spotify URI.
    pub uri: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct UserReference {
    pub id: String,
    pub display_name: Option<String>,
    pub uri: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct ExternalUrls {
    #[serde(flatten)]
    pub values: BTreeMap<String, String>,
}

impl ExternalUrls {
    pub fn spotify(&self) -> Option<&str> {
        self.values.get("spotify").map(String::as_str)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ItemSummary {
    pub total: usize,
}

/// Playlist metadata returned from the current user's inventory.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct SpotifyPlaylist {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub owner: UserReference,
    #[serde(default)]
    pub public: Option<bool>,
    #[serde(default)]
    pub collaborative: bool,
    pub snapshot_id: Option<String>,
    pub uri: String,
    #[serde(default)]
    pub external_urls: ExternalUrls,
    #[serde(default)]
    pub items: Option<ItemSummary>,
    #[serde(default)]
    pub tracks: Option<ItemSummary>,
}

impl SpotifyPlaylist {
    pub fn total_items(&self) -> usize {
        self.items
            .as_ref()
            .or(self.tracks.as_ref())
            .map_or(0, |summary| summary.total)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct SpotifyArtist {
    pub id: Option<String>,
    pub name: String,
    pub uri: Option<String>,
    #[serde(default)]
    pub external_urls: ExternalUrls,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct SpotifyAlbum {
    pub id: Option<String>,
    pub name: String,
    pub uri: Option<String>,
    pub album_type: Option<String>,
    pub release_date: Option<String>,
    #[serde(default)]
    pub artists: Vec<SpotifyArtist>,
    #[serde(default)]
    pub external_urls: ExternalUrls,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct ExternalIds {
    #[serde(default)]
    pub isrc: Option<String>,
}

/// Track metadata embedded in playlist and saved-track responses.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct SpotifyTrack {
    pub id: Option<String>,
    pub name: String,
    pub duration_ms: Option<i32>,
    pub explicit: Option<bool>,
    pub uri: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub is_local: bool,
    #[serde(default)]
    pub artists: Vec<SpotifyArtist>,
    pub album: Option<SpotifyAlbum>,
    #[serde(default)]
    pub external_ids: ExternalIds,
    #[serde(default)]
    pub external_urls: ExternalUrls,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PlaylistItem {
    pub added_at: Option<DateTime<Utc>>,
    pub added_by: Option<UserReference>,
    #[serde(default)]
    pub is_local: bool,
    /// February 2026 response field.
    pub item: Option<SpotifyTrack>,
    /// Compatibility field for older/extended-quota responses.
    pub track: Option<SpotifyTrack>,
}

impl PlaylistItem {
    pub fn track(&self) -> Option<&SpotifyTrack> {
        self.item.as_ref().or(self.track.as_ref())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct SavedTrack {
    pub added_at: Option<DateTime<Utc>>,
    pub track: Option<SpotifyTrack>,
}

/// One playlist and its complete ordered item inventory.
#[derive(Clone, Debug)]
pub(crate) struct PlaylistInventory {
    /// Playlist metadata.
    pub playlist: SpotifyPlaylist,
    /// Ordered items returned by Spotify.
    pub items: Vec<PlaylistItem>,
    /// Prior database snapshot used instead of another Spotify item request.
    pub reused_from_snapshot: Option<Uuid>,
}

/// How the current account is related to an externally owned playlist.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ExternalPlaylistRelationship {
    /// The account follows or saved a playlist owned elsewhere.
    Followed,
    /// The account collaborates on a playlist owned elsewhere.
    Collaborative,
}

impl ExternalPlaylistRelationship {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Followed => "followed_external",
            Self::Collaborative => "collaborative_external",
        }
    }
}

/// Whether track membership was available for one bookmark observation.
#[derive(Clone, Copy, Debug)]
pub(crate) enum BookmarkContentStatus {
    /// All readable track items were fetched or copied from Neon.
    Complete,
    /// Spotify exposed metadata but the importer deliberately made no item request.
    MetadataOnly,
    /// Spotify refused access to the playlist items.
    Inaccessible,
}

impl BookmarkContentStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::MetadataOnly => "metadata_only",
            Self::Inaccessible => "inaccessible",
        }
    }
}

/// One externally owned playlist retained as an internal bookmark.
#[derive(Clone, Debug)]
pub(crate) struct ExternalPlaylistInventory {
    /// Playlist metadata returned by Spotify.
    pub playlist: SpotifyPlaylist,
    /// The account's relationship to the source playlist.
    pub relationship: ExternalPlaylistRelationship,
    /// Ordered items, when Spotify permits access.
    pub items: Vec<PlaylistItem>,
    /// Availability of track membership for this observation.
    pub content_status: BookmarkContentStatus,
    /// Prior bookmark snapshot used instead of another item request.
    pub reused_from_snapshot: Option<Uuid>,
}

/// Complete or copy-forward saved-track inventory.
#[derive(Clone, Debug)]
pub(crate) struct SavedTracksInventory {
    pub items: Vec<SavedTrack>,
    pub total: usize,
    pub reused_from_snapshot: Option<Uuid>,
}

#[derive(Clone, Debug)]
pub(crate) struct PlaylistReuse {
    pub provider_snapshot_id: String,
    pub source_snapshot_id: Uuid,
}

#[derive(Clone, Debug)]
pub(crate) struct SavedTrackReuse {
    pub source_snapshot_id: Uuid,
    pub total: usize,
    pub leading_items: Vec<(usize, String, Option<DateTime<Utc>>)>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ReusePlan {
    pub playlists: HashMap<String, PlaylistReuse>,
    pub bookmark_playlists: HashMap<String, PlaylistReuse>,
    pub saved_tracks: Option<SavedTrackReuse>,
}

/// Complete read-only Spotify inventory fetched before persistence begins.
#[derive(Clone, Debug)]
pub(crate) struct SpotifyInventory {
    /// Authenticated Spotify account.
    pub profile: CurrentUser,
    /// Account-owned and private Spotify-personalized signal playlists.
    pub playlists: Vec<PlaylistInventory>,
    /// Externally owned playlists retained outside the active library.
    pub external_playlists: Vec<ExternalPlaylistInventory>,
    /// Ordered saved-track library.
    pub saved_tracks: SavedTracksInventory,
    /// All owned, followed, and collaborative playlists Spotify reported.
    pub playlists_seen: usize,
    /// Followed playlists intentionally excluded under 2026 Development Mode.
    pub followed_playlists_skipped: usize,
    /// Collaborative playlists for which Spotify denied item access.
    pub inaccessible_collaborative_playlists: usize,
}

#[cfg(test)]
mod tests {
    use super::{Page, PlaylistItem, SavedTrack, SpotifyPlaylist};

    #[test]
    fn decodes_february_2026_playlist_shapes() {
        let playlists: Page<SpotifyPlaylist> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/spotify/playlists.json"
        ))
        .expect("playlist fixture");
        assert_eq!(playlists.items[0].total_items(), 2);

        let items: Page<PlaylistItem> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/spotify/playlist_items.json"
        ))
        .expect("playlist-items fixture");
        assert_eq!(
            items.items[0].track().unwrap().id.as_deref(),
            Some("track123")
        );
        assert!(items.items[1].track().is_none());
    }

    #[test]
    fn decodes_unavailable_saved_tracks() {
        let saved: Page<SavedTrack> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/spotify/saved_tracks.json"
        ))
        .expect("saved-track fixture");
        assert!(saved.items[0].track.is_some());
        assert!(saved.items[1].track.is_none());
    }

    #[test]
    fn accepts_the_legacy_playlist_track_summary() {
        let mut value: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/spotify/playlists.json"
        ))
        .expect("playlist fixture");
        let playlist = &mut value["items"][0];
        playlist["tracks"] = playlist["items"].take();
        let playlists: Page<SpotifyPlaylist> =
            serde_json::from_value(value).expect("legacy playlist shape");
        assert_eq!(playlists.items[0].total_items(), 2);
    }
}
