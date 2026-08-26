use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use reqwest::{Client, Method, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::time::sleep;
use url::Url;

use crate::{ChordriftError, Result, terminal::TerminalProgress};

use super::models::{
    BookmarkContentStatus, CurrentUser, CursorPage, ExternalPlaylistInventory,
    ExternalPlaylistRelationship, Page, PlaylistInventory, PlaylistItem, ReusePlan, SavedAlbum,
    SavedAlbumInventory, SavedAlbumReuse, SavedAlbumsInventory, SavedTrack, SavedTrackReuse,
    SavedTracksInventory, SpotifyInventory, SpotifyPlaylist, SpotifyTrack,
};

const API_ROOT: &str = "https://api.spotify.com/v1/";
const PAGE_LIMIT: &str = "50";
const MAX_RATE_LIMIT_RETRIES: usize = 5;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Bounded 429 retry behavior shared with apply-readiness validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RetryPolicy {
    pub(crate) max_retries: usize,
    pub(crate) max_delay_seconds: u64,
}

pub(crate) fn retry_policy() -> RetryPolicy {
    RetryPolicy {
        max_retries: MAX_RATE_LIMIT_RETRIES,
        max_delay_seconds: 60,
    }
}

fn retry_delay_seconds(value: Option<&str>) -> u64 {
    value
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(1)
        .min(retry_policy().max_delay_seconds)
}

/// Authenticated, read-only Spotify Web API client.
#[derive(Clone, Debug)]
pub struct SpotifyClient {
    http: Client,
    access_token: String,
    request_count: Arc<AtomicUsize>,
}

impl SpotifyClient {
    /// Creates a client for one short-lived OAuth access token.
    pub fn new(access_token: impl Into<String>) -> Result<Self> {
        let http = Client::builder()
            .user_agent(concat!("chordrift/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(REQUEST_TIMEOUT)
            .build()?;
        Ok(Self {
            http,
            access_token: access_token.into(),
            request_count: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub(crate) fn request_count(&self) -> usize {
        self.request_count.load(Ordering::Relaxed)
    }

    /// Returns the authenticated account's stable identity.
    pub async fn current_user(&self) -> Result<CurrentUser> {
        self.get_json(api_url("me")?).await
    }

    pub(crate) async fn current_playlists(&self) -> Result<Vec<SpotifyPlaylist>> {
        let mut url = api_url("me/playlists")?;
        url.query_pairs_mut().append_pair("limit", PAGE_LIMIT);
        self.all_pages(url, None).await
    }

    pub(crate) async fn create_playlist(
        &self,
        name: &str,
        description: &str,
        public: bool,
    ) -> Result<SpotifyPlaylist> {
        self.request_json(
            Method::POST,
            api_url("me/playlists")?,
            Some(serde_json::json!({
                "name": name,
                "description": description,
                "public": public,
            })),
        )
        .await
    }

    pub(crate) async fn update_playlist(
        &self,
        playlist_id: &str,
        name: &str,
        description: Option<&str>,
    ) -> Result<()> {
        self.request_empty(
            Method::PUT,
            playlist_url(playlist_id)?,
            Some(serde_json::json!({"name": name, "description": description})),
        )
        .await
    }

    pub(crate) async fn add_playlist_items(
        &self,
        playlist_id: &str,
        uris: &[String],
        position: Option<usize>,
    ) -> Result<String> {
        if uris.is_empty() || uris.len() > 100 {
            return Err(ChordriftError::Configuration(
                "Spotify item additions must contain between 1 and 100 URIs".to_owned(),
            ));
        }
        let response: SnapshotResponse = self
            .request_json(
                Method::POST,
                playlist_items_url(playlist_id)?,
                Some(serde_json::json!({"uris": uris, "position": position})),
            )
            .await?;
        Ok(response.snapshot_id)
    }

    pub(crate) async fn replace_playlist_items(
        &self,
        playlist_id: &str,
        uris: &[String],
    ) -> Result<String> {
        if uris.len() > 100 {
            return Err(ChordriftError::Configuration(
                "Spotify item replacement accepts at most 100 URIs".to_owned(),
            ));
        }
        let response: SnapshotResponse = self
            .request_json(
                Method::PUT,
                playlist_items_url(playlist_id)?,
                Some(serde_json::json!({"uris": uris})),
            )
            .await?;
        Ok(response.snapshot_id)
    }

    pub(crate) async fn remove_playlist_items(
        &self,
        playlist_id: &str,
        uris: &[String],
        snapshot_id: Option<&str>,
    ) -> Result<String> {
        if uris.is_empty() || uris.len() > 100 {
            return Err(ChordriftError::Configuration(
                "Spotify item removals must contain between 1 and 100 URIs".to_owned(),
            ));
        }
        let items: Vec<Value> = uris
            .iter()
            .map(|uri| serde_json::json!({"uri": uri}))
            .collect();
        let response: SnapshotResponse = self
            .request_json(
                Method::DELETE,
                playlist_items_url(playlist_id)?,
                Some(serde_json::json!({"items": items, "snapshot_id": snapshot_id})),
            )
            .await?;
        Ok(response.snapshot_id)
    }

    pub(crate) async fn remove_library_playlists(&self, playlist_ids: &[String]) -> Result<()> {
        if playlist_ids.is_empty() || playlist_ids.len() > 40 {
            return Err(ChordriftError::Configuration(
                "Spotify library removals must contain between 1 and 40 playlist IDs".to_owned(),
            ));
        }
        let mut url = api_url("me/library")?;
        let uris = playlist_ids
            .iter()
            .map(|id| format!("spotify:playlist:{id}"))
            .collect::<Vec<_>>()
            .join(",");
        url.query_pairs_mut().append_pair("uris", &uris);
        self.request_empty(Method::DELETE, url, None).await
    }

    pub(crate) async fn remove_library_tracks(&self, track_ids: &[String]) -> Result<()> {
        if track_ids.is_empty() || track_ids.len() > 40 {
            return Err(ChordriftError::Configuration(
                "Spotify saved-track removals must contain between 1 and 40 track IDs".to_owned(),
            ));
        }
        let mut url = api_url("me/library")?;
        let uris = track_ids
            .iter()
            .map(|id| format!("spotify:track:{id}"))
            .collect::<Vec<_>>()
            .join(",");
        url.query_pairs_mut().append_pair("uris", &uris);
        self.request_empty(Method::DELETE, url, None).await
    }

    pub(crate) async fn remove_library_albums(&self, album_ids: &[String]) -> Result<()> {
        if album_ids.is_empty() || album_ids.len() > 40 {
            return Err(ChordriftError::Configuration(
                "Spotify saved-album removals must contain between 1 and 40 album IDs".to_owned(),
            ));
        }
        let mut url = api_url("me/library")?;
        let uris = album_ids
            .iter()
            .map(|id| format!("spotify:album:{id}"))
            .collect::<Vec<_>>()
            .join(",");
        url.query_pairs_mut().append_pair("uris", &uris);
        self.request_empty(Method::DELETE, url, None).await
    }

    pub(crate) async fn upload_playlist_cover(
        &self,
        playlist_id: &str,
        jpeg_base64: &str,
    ) -> Result<()> {
        let url = api_url(&format!("playlists/{playlist_id}/images"))?;
        validate_api_url(&url)?;
        let mut attempt = 0;
        loop {
            self.request_count.fetch_add(1, Ordering::Relaxed);
            let response = self
                .http
                .put(url.clone())
                .bearer_auth(&self.access_token)
                .header(reqwest::header::CONTENT_TYPE, "image/jpeg")
                .body(jpeg_base64.to_owned())
                .send()
                .await?;
            if should_retry(&response, &mut attempt).await {
                continue;
            }
            let status = response.status();
            let body = response.bytes().await?;
            return if status.is_success() {
                Ok(())
            } else {
                Err(api_error(status, &body))
            };
        }
    }

    pub(crate) async fn external_playlist(
        &self,
        playlist_id: &str,
    ) -> Result<(SpotifyPlaylist, Vec<PlaylistItem>)> {
        let playlist: SpotifyPlaylist = self.get_json(playlist_url(playlist_id)?).await?;
        let mut items_url = playlist_items_url(playlist_id)?;
        items_url.query_pairs_mut().append_pair("limit", PAGE_LIMIT);
        let items = self.all_pages(items_url, None).await?;
        Ok((playlist, items))
    }

    /// Fetches the complete read-only library into memory before persistence.
    pub(crate) async fn inventory(
        &self,
        profile: CurrentUser,
        reuse: &ReusePlan,
    ) -> Result<SpotifyInventory> {
        let mut playlists_url = api_url("me/playlists")?;
        playlists_url
            .query_pairs_mut()
            .append_pair("limit", PAGE_LIMIT);
        let all_playlists: Vec<SpotifyPlaylist> =
            self.all_pages(playlists_url, Some("playlists")).await?;
        let playlists_seen = all_playlists.len();
        let mut followed_playlists_skipped = 0;
        let mut inaccessible_collaborative_playlists = 0;
        let mut playlists_reused = 0;
        let mut playlist_item_fetches = 0;
        let mut playlists = Vec::new();
        let mut external_playlists = Vec::new();

        for playlist in all_playlists {
            let owned = playlist.owner.id == profile.id;
            // Spotify owns personalized, account-specific surfaces such as mixes.
            // Their non-public membership is behavioral evidence, not a followed
            // public relationship to somebody else's library.
            let provider_curated =
                is_private_spotify_personalized(&playlist.owner.id, playlist.public);
            let active_library = owned || provider_curated;
            if !active_library && !playlist.collaborative {
                followed_playlists_skipped += 1;
                external_playlists.push(ExternalPlaylistInventory {
                    playlist,
                    relationship: ExternalPlaylistRelationship::Followed,
                    items: Vec::new(),
                    content_status: BookmarkContentStatus::MetadataOnly,
                    reused_from_snapshot: None,
                });
                continue;
            }

            if !active_library {
                if let (Some(current_snapshot), Some(previous)) = (
                    playlist.snapshot_id.as_deref(),
                    reuse.bookmark_playlists.get(&playlist.id),
                ) && current_snapshot == previous.provider_snapshot_id
                {
                    external_playlists.push(ExternalPlaylistInventory {
                        playlist,
                        relationship: ExternalPlaylistRelationship::Collaborative,
                        items: Vec::new(),
                        content_status: BookmarkContentStatus::Complete,
                        reused_from_snapshot: Some(previous.source_snapshot_id),
                    });
                    continue;
                }

                playlist_item_fetches += 1;
                eprintln!("Spotify · changed external playlists {playlist_item_fetches}");
                let mut items_url = playlist_items_url(&playlist.id)?;
                items_url.query_pairs_mut().append_pair("limit", PAGE_LIMIT);
                let (items, content_status) =
                    match self.all_pages::<PlaylistItem>(items_url, None).await {
                        Ok(items) => (items, BookmarkContentStatus::Complete),
                        Err(ChordriftError::SpotifyApi { status: 403, .. }) => {
                            inaccessible_collaborative_playlists += 1;
                            (Vec::new(), BookmarkContentStatus::Inaccessible)
                        }
                        Err(error) => return Err(error),
                    };
                external_playlists.push(ExternalPlaylistInventory {
                    playlist,
                    relationship: ExternalPlaylistRelationship::Collaborative,
                    items,
                    content_status,
                    reused_from_snapshot: None,
                });
                continue;
            }

            if let (Some(current_snapshot), Some(previous)) = (
                playlist.snapshot_id.as_deref(),
                reuse.playlists.get(&playlist.id),
            ) && current_snapshot == previous.provider_snapshot_id
            {
                playlists.push(PlaylistInventory {
                    playlist,
                    items: Vec::new(),
                    reused_from_snapshot: Some(previous.source_snapshot_id),
                    known_provider_playlist_id: previous.provider_playlist_id,
                });
                playlists_reused += 1;
                continue;
            }

            playlist_item_fetches += 1;
            eprintln!("Spotify · changed playlists {playlist_item_fetches}");
            let mut items_url = playlist_items_url(&playlist.id)?;
            items_url.query_pairs_mut().append_pair("limit", PAGE_LIMIT);
            match self.all_pages::<PlaylistItem>(items_url, None).await {
                Ok(items) => playlists.push(PlaylistInventory {
                    known_provider_playlist_id: reuse
                        .playlists
                        .get(&playlist.id)
                        .and_then(|previous| previous.provider_playlist_id),
                    playlist,
                    items,
                    reused_from_snapshot: None,
                }),
                Err(ChordriftError::SpotifyApi { status: 403, .. }) if !owned => {
                    inaccessible_collaborative_playlists += 1;
                }
                Err(error) => return Err(error),
            }
        }
        eprintln!("Spotify · playlists reused from Neon {playlists_reused}");

        eprintln!("Spotify · checking saved tracks, saved albums, and recent plays");
        let mut saved_url = api_url("me/tracks")?;
        saved_url.query_pairs_mut().append_pair("limit", PAGE_LIMIT);
        let mut albums_url = api_url("me/albums")?;
        albums_url
            .query_pairs_mut()
            .append_pair("limit", PAGE_LIMIT);
        let mut recent_url = api_url("me/player/recently-played")?;
        recent_url
            .query_pairs_mut()
            .append_pair("limit", PAGE_LIMIT);
        if let Some(after) = reuse.recent_after {
            recent_url
                .query_pairs_mut()
                .append_pair("after", &after.timestamp_millis().to_string());
        }
        let (saved_tracks, saved_albums, recently_played) = tokio::try_join!(
            self.saved_tracks(saved_url, reuse.saved_tracks.as_ref()),
            self.saved_albums(albums_url, reuse.saved_albums.as_ref()),
            self.all_cursor_pages(recent_url),
        )?;
        eprintln!(
            "Spotify · recent plays {} new observations",
            recently_played.len()
        );

        let active_playlists_unchanged =
            playlists_reused == playlists.len() && playlists.len() == reuse.playlists.len();
        Ok(SpotifyInventory {
            profile,
            playlists,
            external_playlists,
            saved_tracks,
            saved_albums,
            recently_played,
            recent_requested_after: reuse.recent_after,
            playlists_seen,
            active_playlists_unchanged,
            followed_playlists_skipped,
            inaccessible_collaborative_playlists,
        })
    }

    async fn saved_albums(
        &self,
        url: Url,
        reuse: Option<&SavedAlbumReuse>,
    ) -> Result<SavedAlbumsInventory> {
        let first: Page<SavedAlbum> = self.get_json(url).await?;
        if let Some(previous) = reuse
            && saved_album_page_matches(&first, previous)
        {
            eprintln!("Spotify · saved albums unchanged; reusing Neon state");
            return Ok(SavedAlbumsInventory {
                items: Vec::new(),
                total: first.total,
                reused_from_snapshot: Some(previous.source_snapshot_id),
            });
        }
        let total = first.total;
        let mut saved = first.items;
        let mut next = first.next;
        let mut visited = HashSet::new();
        while let Some(next_url) = next {
            let url = Url::parse(&next_url).map_err(|_| {
                ChordriftError::Configuration(
                    "Spotify saved-album pagination returned an invalid URL".to_owned(),
                )
            })?;
            validate_api_url(&url)?;
            if !visited.insert(url.as_str().to_owned()) {
                return Err(ChordriftError::Configuration(
                    "Spotify saved-album pagination returned a repeated page".to_owned(),
                ));
            }
            let page: Page<SavedAlbum> = self.get_json(url).await?;
            saved.extend(page.items);
            next = page.next;
        }
        let mut items = Vec::with_capacity(saved.len());
        for mut saved_album in saved {
            let embedded = saved_album.album.tracks.take();
            let (mut tracks, mut next) = embedded
                .map(|page| (page.items, page.next))
                .unwrap_or_default();
            while let Some(next_url) = next {
                let url = Url::parse(&next_url).map_err(|_| {
                    ChordriftError::Configuration(
                        "Spotify album-track pagination returned an invalid URL".to_owned(),
                    )
                })?;
                validate_api_url(&url)?;
                let page: Page<SpotifyTrack> = self.get_json(url).await?;
                tracks.extend(page.items);
                next = page.next;
            }
            items.push(SavedAlbumInventory {
                saved_at: saved_album.added_at,
                album: saved_album.album,
                tracks,
            });
        }
        Ok(SavedAlbumsInventory {
            items,
            total,
            reused_from_snapshot: None,
        })
    }

    async fn saved_tracks(
        &self,
        url: Url,
        reuse: Option<&SavedTrackReuse>,
    ) -> Result<SavedTracksInventory> {
        let first: Page<SavedTrack> = self.get_json(url).await?;
        let mut progress = TerminalProgress::new("Spotify · saved tracks", first.total);
        progress.set_position(first.items.len());
        if reuse.is_none() {
            progress.note("spotify reuse: no saved-track baseline available");
        }
        if let Some(previous) = reuse
            && saved_page_matches(&first, previous)
        {
            progress.note("Spotify · saved tracks unchanged; reusing Neon state");
            progress.finish();
            return Ok(SavedTracksInventory {
                items: Vec::new(),
                total: first.total,
                reused_from_snapshot: Some(previous.source_snapshot_id),
            });
        }

        let total = first.total;
        let mut values = first.items;
        let mut next = first.next;
        let mut visited = HashSet::new();
        while let Some(next_url) = next {
            let url = Url::parse(&next_url).map_err(|_| {
                ChordriftError::Configuration(
                    "Spotify pagination returned an invalid URL".to_owned(),
                )
            })?;
            validate_api_url(&url)?;
            if !visited.insert(url.as_str().to_owned()) {
                return Err(ChordriftError::Configuration(
                    "Spotify pagination returned a repeated page".to_owned(),
                ));
            }
            let page: Page<SavedTrack> = self.get_json(url).await?;
            values.extend(page.items);
            progress.set_position(values.len());
            next = page.next;
        }
        progress.finish();
        Ok(SavedTracksInventory {
            items: values,
            total,
            reused_from_snapshot: None,
        })
    }

    async fn all_pages<T: DeserializeOwned>(
        &self,
        mut url: Url,
        progress_label: Option<&str>,
    ) -> Result<Vec<T>> {
        let mut values = Vec::new();
        let mut visited = HashSet::new();
        let mut progress = None;

        loop {
            validate_api_url(&url)?;
            if !visited.insert(url.as_str().to_owned()) {
                return Err(ChordriftError::Configuration(
                    "Spotify pagination returned a repeated page".to_owned(),
                ));
            }
            let page: Page<T> = self.get_json(url).await?;
            let total = page.total;
            values.reserve(page.total.saturating_sub(values.len()));
            values.extend(page.items);
            if let Some(label) = progress_label {
                let progress = progress.get_or_insert_with(|| {
                    TerminalProgress::new(format!("Spotify · {label}"), total)
                });
                progress.set_position(values.len());
            }
            let Some(next) = page.next else {
                break;
            };
            url = Url::parse(&next).map_err(|_| {
                ChordriftError::Configuration(
                    "Spotify pagination returned an invalid URL".to_owned(),
                )
            })?;
        }
        if let Some(progress) = progress {
            progress.finish();
        }
        Ok(values)
    }

    async fn all_cursor_pages<T: DeserializeOwned>(&self, mut url: Url) -> Result<Vec<T>> {
        let mut values = Vec::new();
        let mut visited = HashSet::new();
        loop {
            validate_api_url(&url)?;
            if !visited.insert(url.as_str().to_owned()) {
                return Err(ChordriftError::Configuration(
                    "Spotify cursor pagination returned a repeated page".to_owned(),
                ));
            }
            let page: CursorPage<T> = self.get_json(url).await?;
            values.extend(page.items);
            let Some(next) = page.next else {
                break;
            };
            url = Url::parse(&next).map_err(|_| {
                ChordriftError::Configuration(
                    "Spotify cursor pagination returned an invalid URL".to_owned(),
                )
            })?;
        }
        Ok(values)
    }

    async fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T> {
        validate_api_url(&url)?;
        let mut attempt = 0;
        loop {
            self.request_count.fetch_add(1, Ordering::Relaxed);
            let response = self
                .http
                .get(url.clone())
                .bearer_auth(&self.access_token)
                .send()
                .await?;

            if response.status() == StatusCode::TOO_MANY_REQUESTS
                && attempt < MAX_RATE_LIMIT_RETRIES
            {
                attempt += 1;
                let delay = retry_delay_seconds(
                    response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|value| value.to_str().ok()),
                );
                sleep(Duration::from_secs(delay)).await;
                continue;
            }

            let status = response.status();
            let body = response.bytes().await?;
            if status.is_success() {
                return serde_json::from_slice(&body).map_err(Into::into);
            }
            return Err(api_error(status, &body));
        }
    }

    async fn request_json<T: DeserializeOwned>(
        &self,
        method: Method,
        url: Url,
        body: Option<Value>,
    ) -> Result<T> {
        let bytes = self.request(method, url, body).await?;
        serde_json::from_slice(&bytes).map_err(Into::into)
    }

    async fn request_empty(&self, method: Method, url: Url, body: Option<Value>) -> Result<()> {
        self.request(method, url, body).await.map(|_| ())
    }

    async fn request(&self, method: Method, url: Url, body: Option<Value>) -> Result<Vec<u8>> {
        validate_api_url(&url)?;
        let mut attempt = 0;
        loop {
            let mut request = self
                .http
                .request(method.clone(), url.clone())
                .bearer_auth(&self.access_token);
            if let Some(body) = &body {
                request = request.json(body);
            }
            self.request_count.fetch_add(1, Ordering::Relaxed);
            let response = request.send().await?;
            if should_retry(&response, &mut attempt).await {
                continue;
            }
            let status = response.status();
            let bytes = response.bytes().await?;
            return if status.is_success() {
                Ok(bytes.to_vec())
            } else {
                Err(api_error(status, &bytes))
            };
        }
    }
}

#[derive(Debug, Deserialize)]
struct SnapshotResponse {
    snapshot_id: String,
}

async fn should_retry(response: &reqwest::Response, attempt: &mut usize) -> bool {
    if response.status() != StatusCode::TOO_MANY_REQUESTS || *attempt >= MAX_RATE_LIMIT_RETRIES {
        return false;
    }
    *attempt += 1;
    let delay = retry_delay_seconds(
        response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok()),
    );
    sleep(Duration::from_secs(delay)).await;
    true
}

fn is_private_spotify_personalized(owner_id: &str, public: Option<bool>) -> bool {
    owner_id == "spotify" && public == Some(false)
}

fn saved_page_matches(page: &Page<SavedTrack>, previous: &SavedTrackReuse) -> bool {
    if page.total != previous.total {
        eprintln!(
            "spotify reuse: saved-track total changed (current={}, previous={})",
            page.total, previous.total
        );
        return false;
    }
    let current: Vec<_> = page
        .items
        .iter()
        .enumerate()
        .filter_map(|(position, item)| {
            let track = item.track.as_ref()?;
            let id = track.id.as_ref()?;
            (!id.trim().is_empty()
                && track.kind == "track"
                && !track.is_local
                && track.name.split_whitespace().next().is_some())
            .then_some((position, id.as_str(), item.added_at))
        })
        .collect();
    if current.len() != previous.leading_items.len() {
        eprintln!(
            "spotify reuse: leading valid-item count changed (current={}, previous={})",
            current.len(),
            previous.leading_items.len()
        );
        return false;
    }
    current.iter().zip(&previous.leading_items).all(
            |(
                (position, current_id, current_added_at),
                (previous_position, previous_id, previous_added_at),
            )| {
                let matches = position == previous_position
                    && *current_id == previous_id
                    && *current_added_at == *previous_added_at;
                if !matches {
                    eprintln!(
                        "spotify reuse: leading signature changed (current_position={}, previous_position={}, id_matches={}, timestamp_matches={})",
                        position,
                        previous_position,
                        *current_id == previous_id,
                        *current_added_at == *previous_added_at
                    );
                }
                matches
            },
        )
}

fn saved_album_page_matches(page: &Page<SavedAlbum>, previous: &SavedAlbumReuse) -> bool {
    if page.total != previous.total {
        eprintln!(
            "spotify reuse: saved-album total changed (current={}, previous={})",
            page.total, previous.total
        );
        return false;
    }
    let current: Vec<_> = page
        .items
        .iter()
        .enumerate()
        .filter_map(|(position, item)| {
            let id = item.album.id.as_deref()?;
            (!id.trim().is_empty()).then_some((position, id, item.added_at))
        })
        .collect();
    current.len() == previous.leading_items.len()
        && current.iter().zip(&previous.leading_items).all(
            |((position, id, saved_at), (old_position, old_id, old_saved_at))| {
                position == old_position && *id == old_id && *saved_at == *old_saved_at
            },
        )
}

fn api_url(path: &str) -> Result<Url> {
    Url::parse(API_ROOT)
        .expect("static Spotify API root is valid")
        .join(path)
        .map_err(|_| ChordriftError::Configuration("invalid Spotify API path".to_owned()))
}

fn playlist_items_url(id: &str) -> Result<Url> {
    playlist_url(id)?;
    api_url(&format!("playlists/{id}/items"))
}

fn playlist_url(id: &str) -> Result<Url> {
    if id.is_empty()
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(ChordriftError::Configuration(
            "Spotify returned an invalid playlist ID".to_owned(),
        ));
    }
    api_url(&format!("playlists/{id}"))
}

fn validate_api_url(url: &Url) -> Result<()> {
    if url.scheme() != "https"
        || url.host_str() != Some("api.spotify.com")
        || url.port().is_some()
        || !url.path().starts_with("/v1/")
    {
        return Err(ChordriftError::Configuration(
            "refusing to send Spotify credentials to an unexpected URL".to_owned(),
        ));
    }
    Ok(())
}

fn api_error(status: StatusCode, body: &[u8]) -> ChordriftError {
    let message = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .or_else(|| value.get("error_description"))
                .or_else(|| value.get("error"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "Spotify did not provide an error description".to_owned());
    ChordriftError::SpotifyApi {
        status: status.as_u16(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use url::Url;
    use uuid::Uuid;

    use super::{
        is_private_spotify_personalized, playlist_items_url, retry_delay_seconds, retry_policy,
        saved_page_matches, validate_api_url,
    };
    use crate::providers::spotify::models::{Page, SavedTrack, SavedTrackReuse};

    #[test]
    fn accepts_only_spotify_api_pagination_urls() {
        assert!(
            validate_api_url(
                &Url::parse("https://api.spotify.com/v1/me/playlists?limit=50").unwrap()
            )
            .is_ok()
        );
        assert!(
            validate_api_url(&Url::parse("https://attacker.example/v1/me/playlists").unwrap())
                .is_err()
        );
        assert!(playlist_items_url("37i9dQZF1DXcBWIGoYBM5M").is_ok());
        assert!(playlist_items_url("../me").is_err());
    }

    #[test]
    fn distinguishes_private_spotify_signals_from_public_editorial_playlists() {
        assert!(is_private_spotify_personalized("spotify", Some(false)));
        assert!(!is_private_spotify_personalized("spotify", Some(true)));
        assert!(!is_private_spotify_personalized("spotify", None));
        assert!(!is_private_spotify_personalized("friend", Some(false)));
    }

    #[test]
    fn bounds_rate_limit_retries_and_retry_after() {
        let policy = retry_policy();
        assert_eq!(policy.max_retries, 5);
        assert_eq!(retry_delay_seconds(None), 1);
        assert_eq!(retry_delay_seconds(Some("invalid")), 1);
        assert_eq!(retry_delay_seconds(Some("17")), 17);
        assert_eq!(retry_delay_seconds(Some("999")), policy.max_delay_seconds);
    }

    #[test]
    fn reuses_saved_tracks_only_when_the_leading_signature_matches() {
        let page: Page<SavedTrack> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/spotify/saved_tracks.json"
        ))
        .expect("saved-track fixture");
        let first = &page.items[0];
        let reuse = SavedTrackReuse {
            source_snapshot_id: Uuid::nil(),
            total: page.total,
            leading_items: vec![
                (
                    0,
                    first.track.as_ref().unwrap().id.clone().unwrap(),
                    first.added_at,
                ),
                (1, "unavailable".to_owned(), None::<DateTime<chrono::Utc>>),
            ],
        };
        assert!(!saved_page_matches(&page, &reuse));

        let one_item = Page {
            items: vec![first.clone()],
            next: None,
            total: 1,
        };
        let matching = SavedTrackReuse {
            total: 1,
            leading_items: vec![(
                0,
                first.track.as_ref().unwrap().id.clone().unwrap(),
                first.added_at,
            )],
            ..reuse
        };
        assert!(saved_page_matches(&one_item, &matching));
    }
}
