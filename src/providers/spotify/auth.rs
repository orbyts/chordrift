use std::{collections::HashMap, env, net::IpAddr, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    time::timeout,
};
use url::Url;

use crate::{
    ChordriftError, Result,
    credentials::{CredentialStore, SecretId, SystemCredentialStore},
};

use super::{client::SpotifyClient, models::CurrentUser};

const AUTHORIZE_ENDPOINT: &str = "https://accounts.spotify.com/authorize";
const TOKEN_ENDPOINT: &str = "https://accounts.spotify.com/api/token";
const CLIENT_ID_VARIABLE: &str = "CHORDRIFT_SPOTIFY_CLIENT_ID";
const REDIRECT_URI_VARIABLE: &str = "CHORDRIFT_SPOTIFY_REDIRECT_URI";
const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:8888/callback";
const SCOPES: &str = "playlist-read-private playlist-read-collaborative user-library-read user-read-recently-played user-top-read playlist-modify-private playlist-modify-public user-library-modify ugc-image-upload";
pub(crate) const REQUIRED_SCOPES: [&str; 9] = [
    "playlist-read-private",
    "playlist-read-collaborative",
    "user-library-read",
    "user-read-recently-played",
    "user-top-read",
    "playlist-modify-private",
    "playlist-modify-public",
    "user-library-modify",
    "ugc-image-upload",
];
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Secret-free Spotify OAuth application configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpotifyOAuthConfig {
    /// Public Spotify application Client ID.
    pub client_id: String,
    /// Exact loopback callback registered in Spotify's dashboard.
    pub redirect_uri: Url,
}

impl SpotifyOAuthConfig {
    /// Loads the Client ID and optional redirect URI from the environment.
    pub fn from_environment() -> Result<Self> {
        let client_id = env::var(CLIENT_ID_VARIABLE).map_err(|_| {
            ChordriftError::Configuration(format!(
                "set {CLIENT_ID_VARIABLE} to the Client ID from Spotify's developer dashboard"
            ))
        })?;
        let redirect_uri =
            env::var(REDIRECT_URI_VARIABLE).unwrap_or_else(|_| DEFAULT_REDIRECT_URI.to_owned());
        Self::new(client_id, &redirect_uri)
    }

    fn new(client_id: String, redirect_uri: &str) -> Result<Self> {
        if client_id.trim().is_empty()
            || !client_id
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
        {
            return Err(ChordriftError::Configuration(format!(
                "{CLIENT_ID_VARIABLE} is not a valid Spotify Client ID"
            )));
        }
        let redirect_uri = Url::parse(redirect_uri).map_err(|_| {
            ChordriftError::Configuration(format!("{REDIRECT_URI_VARIABLE} is not a valid URL"))
        })?;
        validate_redirect_uri(&redirect_uri)?;
        Ok(Self {
            client_id,
            redirect_uri,
        })
    }

    fn authorization_request(&self) -> PkceRequest {
        let verifier = random_urlsafe(64);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let state = random_urlsafe(32);
        let mut authorization_url =
            Url::parse(AUTHORIZE_ENDPOINT).expect("static Spotify OAuth URL is valid");
        authorization_url
            .query_pairs_mut()
            .append_pair("client_id", &self.client_id)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", self.redirect_uri.as_str())
            .append_pair("scope", SCOPES)
            .append_pair("code_challenge_method", "S256")
            .append_pair("code_challenge", &challenge)
            .append_pair("state", &state);
        PkceRequest {
            verifier,
            state,
            authorization_url,
        }
    }
}

#[derive(Clone, Debug)]
struct PkceRequest {
    verifier: String,
    state: String,
    authorization_url: Url,
}

#[derive(Clone, Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    scope: Option<String>,
    expires_in: u64,
    refresh_token: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct StoredCredential {
    account_id: String,
    spotify_user_id: String,
    refresh_token: String,
    scopes: Vec<String>,
}

/// Result of a completed Spotify authorization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthReport {
    /// Local account label used to find the credential later.
    pub account_label: String,
    /// Stable Spotify account identity.
    pub account_id: String,
    /// Spotify display name, when present.
    pub display_name: Option<String>,
    /// Granted read-only OAuth scopes.
    pub scopes: Vec<String>,
    /// Access token lifetime reported by Spotify.
    pub expires_in_seconds: u64,
}

/// Result of verifying a stored Spotify authorization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthStatus {
    /// Local account label.
    pub account_label: String,
    /// Stable Spotify account identity.
    pub account_id: String,
    /// Spotify display name, when present.
    pub display_name: Option<String>,
    /// OAuth scopes retained with the credential.
    pub scopes: Vec<String>,
}

/// Authenticated short-lived API session.
pub(crate) struct SpotifySession {
    pub client: SpotifyClient,
    pub profile: CurrentUser,
    pub scopes: Vec<String>,
}

/// Runs browser-based Authorization Code with PKCE and stores the refresh token.
pub async fn authenticate(account_label: &str) -> Result<AuthReport> {
    let credential_id = credential_id(account_label)?;
    let config = SpotifyOAuthConfig::from_environment()?;
    let http = oauth_http_client()?;
    let token = authorize(&http, &config).await?;
    validate_token(&token)?;
    let refresh_token = token.refresh_token.as_deref().ok_or_else(|| {
        ChordriftError::Configuration(
            "Spotify did not issue a refresh token; revoke Chordrift access and authorize again"
                .to_owned(),
        )
    })?;
    let client = SpotifyClient::new(&token.access_token)?;
    let profile = client.current_user().await?;
    let mut scopes = token_scopes(&token);
    if scopes.is_empty() {
        scopes = requested_scopes();
    }
    let credential = StoredCredential {
        account_id: profile.account_id.clone(),
        spotify_user_id: profile.id.clone(),
        refresh_token: refresh_token.to_owned(),
        scopes: scopes.clone(),
    };
    SystemCredentialStore.save(&credential_id, &serde_json::to_vec(&credential)?)?;

    Ok(AuthReport {
        account_label: account_label.to_owned(),
        account_id: profile.account_id,
        display_name: profile.display_name,
        scopes,
        expires_in_seconds: token.expires_in,
    })
}

/// Verifies a stored refresh token against Spotify.
pub async fn status(account_label: &str) -> Result<AuthStatus> {
    let session = session(account_label).await?;
    Ok(AuthStatus {
        account_label: account_label.to_owned(),
        account_id: session.profile.account_id,
        display_name: session.profile.display_name,
        scopes: session.scopes,
    })
}

/// Removes one local Spotify credential without revoking remote account access.
pub fn logout(account_label: &str) -> Result<bool> {
    SystemCredentialStore.delete(&credential_id(account_label)?)
}

pub(crate) async fn session(account_label: &str) -> Result<SpotifySession> {
    let mut credential = load_credential(account_label)?;
    let original = credential.clone();
    let credential_id = credential_id(account_label)?;
    let config = SpotifyOAuthConfig::from_environment()?;
    let http = oauth_http_client()?;
    let token = refresh_access_token(&http, &config, &credential.refresh_token).await?;
    validate_token(&token)?;
    if let Some(rotated) = &token.refresh_token {
        credential.refresh_token.clone_from(rotated);
    }
    let scopes = token_scopes(&token);
    if !scopes.is_empty() {
        credential.scopes = scopes;
    }
    let client = SpotifyClient::new(token.access_token)?;
    let profile = client.current_user().await?;
    if profile.account_id != credential.account_id {
        return Err(ChordriftError::Configuration(
            "stored Spotify credential resolved to a different account".to_owned(),
        ));
    }
    credential.spotify_user_id = profile.id.clone();
    if credential != original {
        SystemCredentialStore.save(&credential_id, &serde_json::to_vec(&credential)?)?;
    }
    Ok(SpotifySession {
        client,
        profile,
        scopes: credential.scopes,
    })
}

pub(crate) fn has_required_apply_scopes(scopes: &[String]) -> bool {
    REQUIRED_SCOPES
        .iter()
        .all(|required| scopes.iter().any(|scope| scope == required))
}

fn load_credential(account_label: &str) -> Result<StoredCredential> {
    let bytes = SystemCredentialStore
        .load(&credential_id(account_label)?)?
        .ok_or_else(|| {
            ChordriftError::Configuration(format!(
                "no Spotify credential is stored for account {account_label:?}; run `chordrift spotify auth --account {account_label}`"
            ))
        })?;
    serde_json::from_slice(&bytes).map_err(|_| {
        ChordriftError::Configuration(
            "stored Spotify credential is invalid; authenticate again".to_owned(),
        )
    })
}

fn credential_id(account_label: &str) -> Result<SecretId> {
    SecretId::new("spotify", account_label, "oauth")
}

fn oauth_http_client() -> Result<Client> {
    Client::builder()
        .user_agent(concat!("chordrift/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(Into::into)
}

async fn authorize(http: &Client, config: &SpotifyOAuthConfig) -> Result<TokenResponse> {
    let request = config.authorization_request();
    let receiver = CallbackReceiver::bind(&config.redirect_uri).await?;
    println!(
        "Open this Spotify authorization URL if the browser does not open automatically:\n{}",
        request.authorization_url
    );
    let _ = webbrowser::open(request.authorization_url.as_str());
    eprintln!("Waiting for Spotify authorization...");
    let callback = receiver.wait().await?;
    let code = authorization_code(&callback, &request.state)?;
    token_request(
        http,
        &[
            ("client_id", config.client_id.as_str()),
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", config.redirect_uri.as_str()),
            ("code_verifier", request.verifier.as_str()),
        ],
    )
    .await
}

async fn refresh_access_token(
    http: &Client,
    config: &SpotifyOAuthConfig,
    refresh_token: &str,
) -> Result<TokenResponse> {
    token_request(
        http,
        &[
            ("client_id", config.client_id.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ],
    )
    .await
}

async fn token_request(http: &Client, form: &[(&str, &str)]) -> Result<TokenResponse> {
    let response = http.post(TOKEN_ENDPOINT).form(form).send().await?;
    let status = response.status();
    let body = response.bytes().await?;
    if status.is_success() {
        return serde_json::from_slice(&body).map_err(Into::into);
    }
    Err(token_error(status, &body))
}

fn token_error(status: StatusCode, body: &[u8]) -> ChordriftError {
    let value = serde_json::from_slice::<serde_json::Value>(body).ok();
    let message = value
        .as_ref()
        .and_then(|value| {
            value
                .get("error_description")
                .or_else(|| value.get("error"))
                .and_then(serde_json::Value::as_str)
        })
        .unwrap_or("Spotify did not provide an OAuth error description")
        .to_owned();
    ChordriftError::SpotifyApi {
        status: status.as_u16(),
        message,
    }
}

fn validate_token(token: &TokenResponse) -> Result<()> {
    if !token.token_type.eq_ignore_ascii_case("bearer") || token.access_token.is_empty() {
        return Err(ChordriftError::Configuration(
            "Spotify returned an invalid OAuth token".to_owned(),
        ));
    }
    Ok(())
}

fn token_scopes(token: &TokenResponse) -> Vec<String> {
    token
        .scope
        .as_deref()
        .unwrap_or_default()
        .split_ascii_whitespace()
        .map(str::to_owned)
        .collect()
}

fn requested_scopes() -> Vec<String> {
    SCOPES.split_ascii_whitespace().map(str::to_owned).collect()
}

fn random_urlsafe(bytes: usize) -> String {
    let mut value = vec![0_u8; bytes];
    rand::rng().fill_bytes(&mut value);
    URL_SAFE_NO_PAD.encode(value)
}

fn validate_redirect_uri(uri: &Url) -> Result<()> {
    let is_loopback = uri
        .host_str()
        .and_then(|host| host.parse::<IpAddr>().ok())
        .is_some_and(|address| address.is_loopback());
    if uri.scheme() != "http"
        || !is_loopback
        || uri.port().is_none()
        || uri.query().is_some()
        || uri.fragment().is_some()
        || uri.path() == "/"
    {
        return Err(ChordriftError::Configuration(format!(
            "{REDIRECT_URI_VARIABLE} must be an HTTP loopback URI with an explicit port and callback path"
        )));
    }
    Ok(())
}

fn authorization_code(callback: &Url, expected_state: &str) -> Result<String> {
    let values: HashMap<_, _> = callback.query_pairs().collect();
    if let Some(error) = values.get("error") {
        return Err(ChordriftError::Configuration(format!(
            "Spotify authorization failed: {error}"
        )));
    }
    let state = values.get("state").ok_or_else(|| {
        ChordriftError::Configuration("Spotify callback did not contain OAuth state".to_owned())
    })?;
    if state.as_ref() != expected_state {
        return Err(ChordriftError::Configuration(
            "Spotify callback OAuth state did not match the request".to_owned(),
        ));
    }
    values.get("code").map(ToString::to_string).ok_or_else(|| {
        ChordriftError::Configuration(
            "Spotify callback did not contain an authorization code".to_owned(),
        )
    })
}

struct CallbackReceiver {
    listener: TcpListener,
    redirect_uri: Url,
}

impl CallbackReceiver {
    async fn bind(redirect_uri: &Url) -> Result<Self> {
        let host = redirect_uri.host_str().expect("validated callback host");
        let port = redirect_uri.port().expect("validated callback port");
        let listener = TcpListener::bind((host, port)).await.map_err(|error| {
            ChordriftError::Configuration(format!(
                "could not listen on Spotify callback {host}:{port}: {error}"
            ))
        })?;
        Ok(Self {
            listener,
            redirect_uri: redirect_uri.clone(),
        })
    }

    async fn wait(self) -> Result<Url> {
        timeout(CALLBACK_TIMEOUT, self.accept_callback())
            .await
            .map_err(|_| {
                ChordriftError::Configuration(
                    "Spotify authorization timed out after five minutes".to_owned(),
                )
            })?
    }

    async fn accept_callback(self) -> Result<Url> {
        loop {
            let (mut stream, _) = self.listener.accept().await?;
            let mut request = vec![0_u8; 8192];
            let read = stream.read(&mut request).await?;
            let request = String::from_utf8_lossy(&request[..read]);
            let target = request
                .lines()
                .next()
                .and_then(|line| line.split_ascii_whitespace().nth(1));
            let Some(target) = target else {
                continue;
            };
            let callback = self.redirect_uri.join(target).map_err(|_| {
                ChordriftError::Configuration(
                    "Spotify returned an invalid callback request".to_owned(),
                )
            })?;
            if callback.path() != self.redirect_uri.path() {
                write_callback_response(&mut stream, 404, "Not found").await?;
                continue;
            }
            write_callback_response(
                &mut stream,
                200,
                "Spotify authorization received. You may close this window.",
            )
            .await?;
            return Ok(callback);
        }
    }
}

async fn write_callback_response(
    stream: &mut tokio::net::TcpStream,
    status: u16,
    message: &str,
) -> Result<()> {
    let reason = if status == 200 { "OK" } else { "Not Found" };
    let body =
        format!("<!doctype html><meta charset=utf-8><title>Chordrift</title><p>{message}</p>");
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::{REQUIRED_SCOPES, SpotifyOAuthConfig, authorization_code};

    #[test]
    fn builds_v010_pkce_request() {
        let config = SpotifyOAuthConfig::new("abc123".to_owned(), "http://127.0.0.1:8888/callback")
            .expect("valid config");
        let request = config.authorization_request();
        let query = request.authorization_url.query().expect("query");
        assert!(query.contains("code_challenge_method=S256"));
        for scope in REQUIRED_SCOPES {
            assert!(query.contains(scope));
        }
    }

    #[test]
    fn validates_callback_state() {
        let callback =
            Url::parse("http://127.0.0.1:8888/callback?code=abc&state=expected").unwrap();
        assert_eq!(
            authorization_code(&callback, "expected").expect("valid callback"),
            "abc"
        );
        assert!(authorization_code(&callback, "different").is_err());
    }

    #[test]
    fn rejects_localhost_and_non_loopback_callbacks() {
        assert!(
            SpotifyOAuthConfig::new("abc123".to_owned(), "http://localhost:8888/callback").is_err()
        );
        assert!(
            SpotifyOAuthConfig::new("abc123".to_owned(), "https://example.com/callback").is_err()
        );
    }
}
