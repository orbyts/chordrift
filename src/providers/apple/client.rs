use std::time::Duration;

use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use url::Url;

use crate::{ChordriftError, Result};

const API_ROOT: &str = "https://api.music.apple.com/v1/";

/// Read-only Apple Music API client backed by a short-lived developer token.
#[derive(Clone, Debug)]
pub struct AppleMusicClient {
    http: Client,
    developer_token: String,
    music_user_token: Option<String>,
}

/// Apple Music storefront identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct Storefront {
    /// ISO-like Apple storefront ID, such as `us`.
    pub id: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ResourceResponse<T> {
    data: Vec<T>,
}

/// Useful, non-editorial Apple catalog song metadata.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct CatalogSong {
    /// Stable Apple Music catalog song ID.
    pub id: String,
    /// Song attributes.
    pub attributes: CatalogSongAttributes,
}

/// Attributes used to score cross-provider catalog matches.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSongAttributes {
    /// Track title.
    pub name: String,
    /// Display artist string.
    pub artist_name: String,
    /// Album title.
    pub album_name: String,
    /// Recording duration.
    pub duration_in_millis: u64,
    /// Recording ISRC, when Apple supplies it.
    pub isrc: Option<String>,
    /// Public Apple Music URL.
    pub url: Option<String>,
    /// Spatial-audio availability exposed by the catalog.
    #[serde(default)]
    pub has_immersive_audio: bool,
}

impl AppleMusicClient {
    /// Creates a client; a user token is only required for `/me` endpoints.
    pub fn new(developer_token: String, music_user_token: Option<String>) -> Result<Self> {
        let http = Client::builder()
            .user_agent(concat!("chordrift/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http,
            developer_token,
            music_user_token,
        })
    }

    /// Validates developer access without accessing a subscriber account.
    pub async fn test(&self) -> Result<()> {
        let _: Value = self.get(api_url("test")?).await?;
        Ok(())
    }

    /// Returns the authorized subscriber's storefront.
    pub async fn storefront(&self) -> Result<Storefront> {
        if self.music_user_token.is_none() {
            return Err(ChordriftError::Configuration(
                "Apple Music user authorization is required".to_owned(),
            ));
        }
        let response: ResourceResponse<Storefront> = self.get(api_url("me/storefront")?).await?;
        response.data.into_iter().next().ok_or_else(|| {
            ChordriftError::Configuration(
                "Apple Music returned no storefront for the authorized account".to_owned(),
            )
        })
    }

    /// Fetches up to 25 ISRC groups in one Apple catalog request.
    pub async fn songs_by_isrc(
        &self,
        storefront: &str,
        isrcs: &[String],
    ) -> Result<Vec<CatalogSong>> {
        if isrcs.is_empty() || isrcs.len() > 25 {
            return Err(ChordriftError::Configuration(
                "Apple Music ISRC requests require between 1 and 25 values".to_owned(),
            ));
        }
        validate_storefront(storefront)?;
        let mut url = api_url(&format!("catalog/{storefront}/songs"))?;
        url.query_pairs_mut()
            .append_pair("filter[isrc]", &isrcs.join(","));
        let response: ResourceResponse<CatalogSong> = self.get(url).await?;
        Ok(response.data)
    }

    /// Searches the storefront catalog for candidate songs.
    pub async fn search_songs(
        &self,
        storefront: &str,
        term: &str,
        limit: u8,
    ) -> Result<Vec<CatalogSong>> {
        validate_storefront(storefront)?;
        if term.trim().is_empty() || !(1..=25).contains(&limit) {
            return Err(ChordriftError::Configuration(
                "Apple Music search requires a term and a limit from 1 through 25".to_owned(),
            ));
        }
        let mut url = api_url(&format!("catalog/{storefront}/search"))?;
        url.query_pairs_mut()
            .append_pair("term", term)
            .append_pair("types", "songs")
            .append_pair("limit", &limit.to_string());
        let response: SearchResponse = self.get(url).await?;
        Ok(response
            .results
            .songs
            .map(|songs| songs.data)
            .unwrap_or_default())
    }

    async fn get<T: DeserializeOwned>(&self, url: Url) -> Result<T> {
        validate_api_url(&url)?;
        let mut request = self.http.get(url).bearer_auth(&self.developer_token);
        if let Some(token) = &self.music_user_token {
            request = request.header("Music-User-Token", token);
        }
        let response = request.send().await?;
        let status = response.status();
        let body = response.bytes().await?;
        if status.is_success() {
            return serde_json::from_slice(&body).map_err(Into::into);
        }
        Err(api_error(status, &body))
    }
}

#[derive(Clone, Debug, Deserialize)]
struct SearchResponse {
    results: SearchResults,
}

#[derive(Clone, Debug, Deserialize)]
struct SearchResults {
    songs: Option<SongSearchResult>,
}

#[derive(Clone, Debug, Deserialize)]
struct SongSearchResult {
    data: Vec<CatalogSong>,
}

fn api_url(path: &str) -> Result<Url> {
    Url::parse(API_ROOT)
        .expect("static Apple Music API root is valid")
        .join(path)
        .map_err(|_| ChordriftError::Configuration("invalid Apple Music API path".to_owned()))
}

fn validate_api_url(url: &Url) -> Result<()> {
    if url.scheme() != "https" || url.host_str() != Some("api.music.apple.com") {
        return Err(ChordriftError::Configuration(
            "refusing a non-Apple Music API URL".to_owned(),
        ));
    }
    Ok(())
}

fn validate_storefront(storefront: &str) -> Result<()> {
    if storefront.len() != 2 || !storefront.chars().all(|value| value.is_ascii_alphabetic()) {
        return Err(ChordriftError::Configuration(
            "Apple Music storefront must be a two-letter identifier such as `us`".to_owned(),
        ));
    }
    Ok(())
}

fn api_error(status: StatusCode, body: &[u8]) -> ChordriftError {
    let value = serde_json::from_slice::<Value>(body).ok();
    let message = value
        .as_ref()
        .and_then(|value| value.get("errors"))
        .and_then(Value::as_array)
        .and_then(|errors| errors.first())
        .and_then(|error| {
            error
                .get("detail")
                .or_else(|| error.get("title"))
                .and_then(Value::as_str)
        })
        .unwrap_or("Apple Music did not provide an error description")
        .to_owned();
    ChordriftError::AppleMusicApi {
        status: status.as_u16(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::{CatalogSong, validate_storefront};

    #[test]
    fn decodes_matching_metadata() {
        let song: CatalogSong = serde_json::from_str(
            r#"{"id":"1613600188","attributes":{"name":"Erase Me","artistName":"Lizzy McAlpine","albumName":"five seconds flat","durationInMillis":237000,"isrc":"USRC12103144","url":"https://music.apple.com/us/song/1613600188","hasImmersiveAudio":true}}"#,
        )
        .expect("valid fixture");
        assert_eq!(song.id, "1613600188");
        assert!(song.attributes.has_immersive_audio);
    }

    #[test]
    fn validates_storefront_ids() {
        assert!(validate_storefront("us").is_ok());
        assert!(validate_storefront("USA").is_err());
        assert!(validate_storefront("u/").is_err());
    }
}
