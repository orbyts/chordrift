use std::{collections::HashSet, time::Duration};

use reqwest::{Client, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::time::sleep;
use url::Url;

use crate::{ChordriftError, Result};

use super::models::{
    CurrentUser, Page, PlaylistInventory, PlaylistItem, ReusePlan, SavedTrack, SavedTrackReuse,
    SavedTracksInventory, SpotifyInventory, SpotifyPlaylist,
};

const API_ROOT: &str = "https://api.spotify.com/v1/";
const PAGE_LIMIT: &str = "50";
const MAX_RATE_LIMIT_RETRIES: usize = 5;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Authenticated, read-only Spotify Web API client.
#[derive(Clone, Debug)]
pub struct SpotifyClient {
    http: Client,
    access_token: String,
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
        })
    }

    /// Returns the authenticated account's stable identity.
    pub async fn current_user(&self) -> Result<CurrentUser> {
        self.get_json(api_url("me")?).await
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

        for playlist in all_playlists {
            let owned = playlist.owner.id == profile.id;
            if !owned && !playlist.collaborative {
                followed_playlists_skipped += 1;
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
                });
                playlists_reused += 1;
                continue;
            }

            playlist_item_fetches += 1;
            eprintln!("spotify fetch: changed playlist items {playlist_item_fetches}");
            let mut items_url = playlist_items_url(&playlist.id)?;
            items_url.query_pairs_mut().append_pair("limit", PAGE_LIMIT);
            match self.all_pages::<PlaylistItem>(items_url, None).await {
                Ok(items) => playlists.push(PlaylistInventory {
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
        eprintln!("spotify fetch: playlists reused from Neon {playlists_reused}");

        eprintln!("spotify fetch: saved tracks");
        let mut saved_url = api_url("me/tracks")?;
        saved_url.query_pairs_mut().append_pair("limit", PAGE_LIMIT);
        let saved_tracks = self
            .saved_tracks(saved_url, reuse.saved_tracks.as_ref())
            .await?;

        Ok(SpotifyInventory {
            profile,
            playlists,
            saved_tracks,
            playlists_seen,
            followed_playlists_skipped,
            inaccessible_collaborative_playlists,
        })
    }

    async fn saved_tracks(
        &self,
        url: Url,
        reuse: Option<&SavedTrackReuse>,
    ) -> Result<SavedTracksInventory> {
        let first: Page<SavedTrack> = self.get_json(url).await?;
        eprintln!(
            "spotify fetch: saved tracks {}/{}",
            first.items.len(),
            first.total
        );
        if reuse.is_none() {
            eprintln!("spotify reuse: no saved-track baseline available");
        }
        if let Some(previous) = reuse
            && saved_page_matches(&first, previous)
        {
            eprintln!("spotify fetch: saved tracks unchanged; reusing Neon snapshot");
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
            eprintln!("spotify fetch: saved tracks {}/{total}", values.len());
            next = page.next;
        }
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
                eprintln!("spotify fetch: {label} {}/{total}", values.len());
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
        Ok(values)
    }

    async fn get_json<T: DeserializeOwned>(&self, url: Url) -> Result<T> {
        validate_api_url(&url)?;
        let mut attempt = 0;
        loop {
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
                let delay = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(1)
                    .min(60);
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

fn api_url(path: &str) -> Result<Url> {
    Url::parse(API_ROOT)
        .expect("static Spotify API root is valid")
        .join(path)
        .map_err(|_| ChordriftError::Configuration("invalid Spotify API path".to_owned()))
}

fn playlist_items_url(id: &str) -> Result<Url> {
    if id.is_empty()
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(ChordriftError::Configuration(
            "Spotify returned an invalid playlist ID".to_owned(),
        ));
    }
    api_url(&format!("playlists/{id}/items"))
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

    use super::{playlist_items_url, saved_page_matches, validate_api_url};
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
