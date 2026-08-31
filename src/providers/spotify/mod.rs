//! Spotify OAuth, read-only Web API inventory, and snapshot persistence.

mod auth;
mod client;
mod import;
mod models;

pub use auth::{AuthReport, AuthStatus, SpotifyOAuthConfig, authenticate, logout, status};
pub(crate) use auth::{has_required_apply_scopes, local_refresh_credential};
pub(crate) use client::{RetryPolicy, retry_policy};
pub use import::{ImportReport, import};
use models::SpotifyPlaylist;

use crate::{
    ChordriftError, Result,
    bookmarks::{BookmarkFetchOutcome, FetchedBookmark, FetchedBookmarkItem},
};

pub(crate) struct MutationSession {
    session: auth::SpotifySession,
}

pub(crate) async fn mutation_session(account_label: &str) -> Result<MutationSession> {
    let session = auth::session(account_label).await?;
    if !has_required_apply_scopes(&session.scopes) {
        return Err(ChordriftError::Configuration(format!(
            "Spotify authorization lacks v0.1.0 write scopes; run `chordrift spotify auth --account {account_label}` and approve the requested access"
        )));
    }
    Ok(MutationSession { session })
}

impl MutationSession {
    pub(crate) fn account_id(&self) -> &str {
        &self.session.profile.account_id
    }

    pub(crate) fn user_id(&self) -> &str {
        &self.session.profile.id
    }

    pub(crate) async fn playlists(&self) -> Result<Vec<SpotifyPlaylist>> {
        self.session.client.current_playlists().await
    }

    pub(crate) async fn playlist_items(&self, id: &str) -> Result<Vec<String>> {
        let (_, items) = self.session.client.external_playlist(id).await?;
        Ok(items
            .into_iter()
            .filter_map(|item| item.track().and_then(|track| track.id.clone()))
            .collect())
    }

    pub(crate) async fn create_playlist(
        &self,
        name: &str,
        description: &str,
        public: bool,
    ) -> Result<SpotifyPlaylist> {
        self.session
            .client
            .create_playlist(name, description, public)
            .await
    }

    pub(crate) async fn update_playlist(
        &self,
        id: &str,
        name: &str,
        description: Option<&str>,
    ) -> Result<()> {
        self.session
            .client
            .update_playlist(id, name, description)
            .await
    }

    pub(crate) async fn add_items(
        &self,
        id: &str,
        track_ids: &[String],
        position: Option<usize>,
    ) -> Result<String> {
        let uris = track_ids
            .iter()
            .map(|track_id| format!("spotify:track:{track_id}"))
            .collect::<Vec<_>>();
        self.session
            .client
            .add_playlist_items(id, &uris, position)
            .await
    }

    pub(crate) async fn replace_items(&self, id: &str, track_ids: &[String]) -> Result<String> {
        let uris = track_ids
            .iter()
            .map(|track_id| format!("spotify:track:{track_id}"))
            .collect::<Vec<_>>();
        self.session.client.replace_playlist_items(id, &uris).await
    }

    pub(crate) async fn remove_items(
        &self,
        id: &str,
        track_ids: &[String],
        snapshot_id: Option<&str>,
    ) -> Result<String> {
        let uris = track_ids
            .iter()
            .map(|track_id| format!("spotify:track:{track_id}"))
            .collect::<Vec<_>>();
        self.session
            .client
            .remove_playlist_items(id, &uris, snapshot_id)
            .await
    }

    pub(crate) async fn remove_library_playlists(&self, ids: &[String]) -> Result<()> {
        self.session.client.remove_library_playlists(ids).await
    }

    pub(crate) async fn remove_library_tracks(&self, ids: &[String]) -> Result<()> {
        self.session.client.remove_library_tracks(ids).await
    }

    pub(crate) async fn remove_library_albums(&self, ids: &[String]) -> Result<()> {
        self.session.client.remove_library_albums(ids).await
    }

    pub(crate) async fn upload_cover(&self, id: &str, jpeg_base64: &str) -> Result<()> {
        self.session
            .client
            .upload_playlist_cover(id, jpeg_base64)
            .await
    }
}

/// Fetches exactly one known bookmark on explicit request.
pub async fn fetch_bookmark(
    account_label: &str,
    playlist_id: &str,
) -> Result<BookmarkFetchOutcome> {
    let session = auth::session(account_label).await?;
    let (playlist, items) = match session.client.external_playlist(playlist_id).await {
        Ok(value) => value,
        Err(ChordriftError::SpotifyApi { status: 403, .. }) => {
            return Ok(BookmarkFetchOutcome::Inaccessible { http_status: 403 });
        }
        Err(ChordriftError::SpotifyApi { status: 404, .. }) => {
            return Ok(BookmarkFetchOutcome::NotFound { http_status: 404 });
        }
        Err(error) => return Err(error),
    };
    let mut fetched = Vec::new();
    let mut unavailable_items = 0;
    let mut unsupported_items = 0;
    for (position, item) in items.into_iter().enumerate() {
        let Some(track) = item.track() else {
            unavailable_items += 1;
            continue;
        };
        let Some(provider_track_id) = track.id.as_deref().filter(|id| !id.trim().is_empty()) else {
            unavailable_items += 1;
            continue;
        };
        if track.kind != "track" || track.is_local {
            unsupported_items += 1;
            continue;
        }
        fetched.push(FetchedBookmarkItem {
            position: i32::try_from(position).map_err(|_| {
                ChordriftError::Configuration(
                    "Spotify bookmark position exceeds platform limits".to_owned(),
                )
            })?,
            provider_track_id: provider_track_id.to_owned(),
            title: track.name.clone(),
            artists: track
                .artists
                .iter()
                .map(|artist| artist.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            album: track.album.as_ref().map(|album| album.name.clone()),
            added_at: item.added_at,
            provider_url: track.external_urls.spotify().map(str::to_owned),
        });
    }
    let item_count = playlist.total_items();
    Ok(BookmarkFetchOutcome::Complete(FetchedBookmark {
        name: playlist.name,
        owner_provider_id: playlist.owner.id,
        owner_display_name: playlist.owner.display_name,
        provider_url: playlist.external_urls.spotify().map(str::to_owned),
        provider_snapshot_id: playlist.snapshot_id.clone(),
        public: playlist.public,
        collaborative: playlist.collaborative,
        item_count,
        unavailable_items,
        unsupported_items,
        items: fetched,
    }))
}
