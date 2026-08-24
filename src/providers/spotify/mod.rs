//! Spotify OAuth, read-only Web API inventory, and snapshot persistence.

mod auth;
mod client;
mod import;
mod models;

pub use auth::{AuthReport, AuthStatus, SpotifyOAuthConfig, authenticate, logout, status};
pub(crate) use client::{RetryPolicy, retry_policy};
pub use import::{ImportReport, import};

use crate::{
    ChordriftError, Result,
    bookmarks::{BookmarkFetchOutcome, FetchedBookmark, FetchedBookmarkItem},
};

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
