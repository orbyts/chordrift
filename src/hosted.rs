//! Production hosted-service assembly for the private beta.
//!
//! This module owns HTTPS-origin policy, OIDC login, browser-cookie bridging,
//! health checks, and the static contract workbench. It deliberately does not
//! expose a shell, SQL, provider URLs, database credentials, or provider
//! credentials. Provider mutation stays disabled until the deployment's real
//! maintenance adapter and read-only cutover gate are verified.

use std::{
    collections::{BTreeMap, HashMap},
    env,
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::{Query as AxumQuery, Request, State},
    http::{
        HeaderName, HeaderValue, StatusCode,
        header::{
            AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE, COOKIE, LOCATION, ORIGIN, SET_COOKIE,
        },
    },
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, TimeDelta, Utc};
use rand::RngCore as _;
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use sqlx::{PgPool, Row as _};
use tokio::sync::Mutex;
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    ChordriftError, Result, config,
    contract::{
        CAPABILITY_AUTHENTICATED_SERVICE_TRANSPORT, CAPABILITY_DURABLE_OPERATIONS,
        CAPABILITY_MAINTENANCE_TASK_SESSION, CAPABILITY_PRODUCT_IDENTITY,
        CAPABILITY_PROVIDER_CREDENTIAL_VAULT, CAPABILITY_REMOTE_CLI, CONTRACT_VERSION,
        CapabilityAvailability, ContractVersionRange, ErrorCode, ExcludedTrackView,
        ExcludedTracksView, LibraryPlaylistTrackView, LibraryPlaylistTracksView,
        LibraryPlaylistView, LibraryPlaylistsView, LibraryStateSource, LibraryTrackPlacementView,
        LibraryTrackView, ProviderConnectionView, ProviderConnectionsView, ResourceId,
        ServiceCompatibility,
    },
    db,
    http_transport::{AuthenticatedHttpTransport, BearerAuthenticator},
    identity::{
        ExternalIdentityVerifier, PRODUCT_SESSION_SCHEMA_VERSION, PostgresProductIdentityStore,
        ProductSessionAuthenticator, ProductSessionAuthority, SessionExchangeRequest,
        VerifiedExternalIdentity,
    },
    maintenance::{MaintenanceDecisionProjection, MaintenanceProjection},
    service::{AuthenticatedSubject, MaintenanceApplication, MaintenanceBackend},
};

const SESSION_COOKIE: &str = "chordrift_session";
const LOGIN_COOKIE: &str = "chordrift_login";
const LOGIN_TTL_MINUTES: i64 = 10;
const MAX_LOGIN_ATTEMPTS: usize = 128;
const INDEX_HTML: &str = include_str!("../web/index.html");
const APP_JS: &str = include_str!("../web/app.js");
const APP_CSS: &str = include_str!("../web/app.css");

/// Environment-backed deployment settings. Secret values are never included
/// in formatted errors, diagnostics, health responses, or structured logs.
struct HostedConfig {
    bind: SocketAddr,
    public_origin: Url,
    account_id: ResourceId,
    bootstrap_email: Option<String>,
    oidc_issuer: Url,
    oidc_authorization_url: Url,
    oidc_token_url: Url,
    oidc_userinfo_url: Url,
    oidc_client_id: String,
    oidc_client_secret: Zeroizing<String>,
}

impl HostedConfig {
    fn from_env() -> Result<Self> {
        let bind = required("CHORDRIFT_BIND")?
            .parse()
            .map_err(|_| configuration("CHORDRIFT_BIND must be a socket address"))?;
        let public_origin = https_url("CHORDRIFT_PUBLIC_ORIGIN")?;
        if public_origin.path() != "/"
            || public_origin.query().is_some()
            || public_origin.fragment().is_some()
        {
            return Err(configuration(
                "CHORDRIFT_PUBLIC_ORIGIN must contain only scheme and authority",
            ));
        }
        let account_id = Uuid::parse_str(&required("CHORDRIFT_ACCOUNT_ID")?)
            .map(ResourceId::from_uuid)
            .map_err(|_| configuration("CHORDRIFT_ACCOUNT_ID must be a UUID"))?;
        let bootstrap_email = env::var("CHORDRIFT_BOOTSTRAP_VERIFIED_EMAIL")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty());
        let oidc_issuer = https_url("CHORDRIFT_OIDC_ISSUER")?;
        let oidc_authorization_url = https_url("CHORDRIFT_OIDC_AUTHORIZATION_URL")?;
        let oidc_token_url = https_url("CHORDRIFT_OIDC_TOKEN_URL")?;
        let oidc_userinfo_url = https_url("CHORDRIFT_OIDC_USERINFO_URL")?;
        let oidc_client_id = required("CHORDRIFT_OIDC_CLIENT_ID")?;
        let oidc_client_secret = Zeroizing::new(required("CHORDRIFT_OIDC_CLIENT_SECRET")?);
        Ok(Self {
            bind,
            public_origin,
            account_id,
            bootstrap_email,
            oidc_issuer,
            oidc_authorization_url,
            oidc_token_url,
            oidc_userinfo_url,
            oidc_client_id,
            oidc_client_secret,
        })
    }

    fn callback_url(&self) -> String {
        self.public_origin
            .join("auth/callback")
            .expect("validated origin accepts a relative callback")
            .to_string()
    }
}

#[derive(Clone)]
struct OidcVerifier {
    issuer: String,
    userinfo_url: Url,
    http: reqwest::Client,
}

#[derive(Clone)]
struct VerifiedProfile {
    identity: VerifiedExternalIdentity,
    email: Option<String>,
    email_verified: bool,
}

#[derive(Deserialize)]
struct UserInfoResponse {
    sub: String,
    email: Option<String>,
    email_verified: Option<bool>,
}

impl OidcVerifier {
    async fn verify_profile(
        &self,
        credential: &str,
    ) -> std::result::Result<VerifiedProfile, crate::contract::ClientError> {
        let response = self
            .http
            .get(self.userinfo_url.clone())
            .bearer_auth(credential)
            .send()
            .await
            .map_err(|_| client_unavailable())?;
        if !response.status().is_success() {
            return Err(crate::contract::ClientError::new(
                ErrorCode::AuthenticationRequired,
                false,
            ));
        }
        let profile: UserInfoResponse = response.json().await.map_err(|_| {
            crate::contract::ClientError::new(ErrorCode::AuthenticationRequired, false)
        })?;
        let identity = VerifiedExternalIdentity::new(self.issuer.clone(), profile.sub)?;
        Ok(VerifiedProfile {
            identity,
            email: profile.email.map(|email| email.to_ascii_lowercase()),
            email_verified: profile.email_verified.unwrap_or(false),
        })
    }
}

#[async_trait]
impl ExternalIdentityVerifier for OidcVerifier {
    async fn verify(
        &self,
        credential: &str,
    ) -> std::result::Result<VerifiedExternalIdentity, crate::contract::ClientError> {
        Ok(self.verify_profile(credential).await?.identity)
    }
}

struct LoginAttempt {
    state: String,
    code_verifier: Zeroizing<String>,
    expires_at: DateTime<Utc>,
}

#[derive(Clone)]
struct HostedState {
    config: Arc<HostedConfig>,
    database_pool: PgPool,
    identity_store: Arc<PostgresProductIdentityStore>,
    session_authority: Arc<ProductSessionAuthority<OidcVerifier, PostgresProductIdentityStore>>,
    session_authenticator: Arc<ProductSessionAuthenticator<PostgresProductIdentityStore>>,
    oidc: Arc<OidcVerifier>,
    login_attempts: Arc<Mutex<HashMap<String, LoginAttempt>>>,
}

#[derive(Clone)]
struct BrowserBridge {
    public_origin: String,
}

#[derive(Clone)]
struct DeploymentMaintenanceBackend {
    pool: PgPool,
}

#[async_trait]
impl MaintenanceBackend for DeploymentMaintenanceBackend {
    async fn provider_connections(
        &mut self,
        subject: AuthenticatedSubject,
    ) -> std::result::Result<ProviderConnectionsView, crate::contract::ClientError> {
        let rows = sqlx::query(
            "SELECT account.id, account.provider, account.display_name,
                    inventory.captured_at AS observed_at,
                    EXISTS (
                        SELECT 1 FROM provider_credential_vault credential
                         WHERE credential.provider_account_id = account.id
                           AND credential.credential_kind = 'oauth_refresh'
                           AND credential.revoked_at IS NULL
                    ) AS credential_ready
               FROM provider_accounts account
               LEFT JOIN provider_current_inventories inventory
                 ON inventory.provider_account_id = account.id
              WHERE account.chordrift_account_id = $1
              ORDER BY account.provider, lower(COALESCE(account.display_name, account.account_label)), account.id",
        )
        .bind(subject.account_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|_| client_unavailable())?;
        let connections = rows
            .into_iter()
            .map(|row| {
                Ok(ProviderConnectionView {
                    provider_connection_id: ResourceId::from_uuid(
                        row.try_get("id").map_err(|_| client_unavailable())?,
                    ),
                    provider: row.try_get("provider").map_err(|_| client_unavailable())?,
                    display_name: row
                        .try_get("display_name")
                        .map_err(|_| client_unavailable())?,
                    observed_at: row
                        .try_get("observed_at")
                        .map_err(|_| client_unavailable())?,
                    credential_ready: row
                        .try_get("credential_ready")
                        .map_err(|_| client_unavailable())?,
                })
            })
            .collect::<std::result::Result<Vec<_>, crate::contract::ClientError>>()?;
        Ok(ProviderConnectionsView { connections })
    }

    async fn library_playlists(
        &mut self,
        _subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
        source: LibraryStateSource,
    ) -> std::result::Result<LibraryPlaylistsView, crate::contract::ClientError> {
        match source {
            LibraryStateSource::ProviderObservation => {
                let state_at = sqlx::query_scalar(
                    "SELECT captured_at FROM provider_current_inventories WHERE provider_account_id = $1",
                )
                .bind(provider_connection_id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(|_| client_unavailable())?;
                let rows = sqlx::query(
                    "SELECT spotify_playlist_id, name, total_items, signal_class, role
                       FROM current_spotify_playlists
                      WHERE provider_account_id = $1
                      ORDER BY lower(name), spotify_playlist_id",
                )
                .bind(provider_connection_id.as_uuid())
                .fetch_all(&self.pool)
                .await
                .map_err(|_| client_unavailable())?;
                let playlists = rows
                    .into_iter()
                    .map(|row| {
                        let playlist_id: String = row
                            .try_get("spotify_playlist_id")
                            .map_err(|_| client_unavailable())?;
                        let count: i32 = row
                            .try_get("total_items")
                            .map_err(|_| client_unavailable())?;
                        Ok(LibraryPlaylistView {
                            provider_playlist_id: Some(playlist_id.clone()),
                            playlist_id,
                            name: row.try_get("name").map_err(|_| client_unavailable())?,
                            track_count: u64::try_from(count).map_err(|_| client_unavailable())?,
                            signal_class: row
                                .try_get("signal_class")
                                .map_err(|_| client_unavailable())?,
                            role: row.try_get("role").map_err(|_| client_unavailable())?,
                        })
                    })
                    .collect::<std::result::Result<Vec<_>, crate::contract::ClientError>>()?;
                Ok(LibraryPlaylistsView {
                    source,
                    state_at,
                    playlists,
                })
            }
            LibraryStateSource::ChordriftModel => {
                let generation = sqlx::query(
                    "SELECT id, created_at FROM playlist_generations
                      WHERE provider_account_id = $1
                        AND status IN ('proposed', 'approved', 'published')
                      ORDER BY CASE status WHEN 'proposed' THEN 0 WHEN 'approved' THEN 1 ELSE 2 END,
                               created_at DESC, id DESC LIMIT 1",
                )
                .bind(provider_connection_id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(|_| client_unavailable())?
                .ok_or_else(|| {
                    crate::contract::ClientError::new(ErrorCode::ResourceNotFound, false)
                })?;
                let generation_id: Uuid =
                    generation.try_get("id").map_err(|_| client_unavailable())?;
                let state_at = Some(
                    generation
                        .try_get("created_at")
                        .map_err(|_| client_unavailable())?,
                );
                let rows = sqlx::query(
                    "SELECT concept.stable_key,
                            COALESCE(name_revision.name, playlist.name) AS name,
                            provider.provider_playlist_id,
                            count(membership.id)::bigint AS track_count
                       FROM playlists playlist
                       JOIN playlist_concepts concept ON concept.id = playlist.concept_id
                       LEFT JOIN playlist_name_revisions name_revision
                         ON name_revision.playlist_id = playlist.id AND name_revision.selected
                       LEFT JOIN provider_playlists provider
                         ON provider.concept_id = concept.id AND provider.provider = 'spotify'
                       LEFT JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
                      WHERE playlist.generation_id = $1 AND playlist.archived_at IS NULL
                      GROUP BY concept.stable_key, COALESCE(name_revision.name, playlist.name),
                               provider.provider_playlist_id
                      ORDER BY lower(COALESCE(name_revision.name, playlist.name)), concept.stable_key",
                )
                .bind(generation_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|_| client_unavailable())?;
                let playlists = rows
                    .into_iter()
                    .map(|row| {
                        let count: i64 = row
                            .try_get("track_count")
                            .map_err(|_| client_unavailable())?;
                        Ok(LibraryPlaylistView {
                            playlist_id: row
                                .try_get("stable_key")
                                .map_err(|_| client_unavailable())?,
                            name: row.try_get("name").map_err(|_| client_unavailable())?,
                            provider_playlist_id: row
                                .try_get("provider_playlist_id")
                                .map_err(|_| client_unavailable())?,
                            track_count: u64::try_from(count).map_err(|_| client_unavailable())?,
                            signal_class: Some("canonical".to_owned()),
                            role: Some("managed".to_owned()),
                        })
                    })
                    .collect::<std::result::Result<Vec<_>, crate::contract::ClientError>>()?;
                Ok(LibraryPlaylistsView {
                    source,
                    state_at,
                    playlists,
                })
            }
        }
    }

    async fn library_playlist_tracks(
        &mut self,
        _subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
        playlist_id: &str,
        source: LibraryStateSource,
    ) -> std::result::Result<LibraryPlaylistTracksView, crate::contract::ClientError> {
        let (name, state_at, rows) = match source {
            LibraryStateSource::ProviderObservation => {
                let header = sqlx::query(
                    "SELECT name, inventory.captured_at
                       FROM current_spotify_playlists playlist
                       JOIN provider_current_inventories inventory
                         ON inventory.provider_account_id = playlist.provider_account_id
                      WHERE playlist.provider_account_id = $1 AND playlist.spotify_playlist_id = $2",
                )
                .bind(provider_connection_id.as_uuid())
                .bind(playlist_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|_| client_unavailable())?
                .ok_or_else(|| crate::contract::ClientError::new(ErrorCode::ResourceNotFound, false))?;
                let rows = sqlx::query(
                    "SELECT membership.position, provider_track.provider_track_id, track.title,
                            COALESCE(string_agg(artist.name, ', ' ORDER BY track_artist.position), '') AS artists,
                            album.title AS album
                       FROM provider_current_inventories inventory
                       JOIN provider_current_playlists current_playlist
                         ON current_playlist.provider_account_id = inventory.provider_account_id
                       JOIN provider_playlists provider_playlist
                         ON provider_playlist.id = current_playlist.provider_playlist_id
                       JOIN provider_playlist_revision_tracks membership
                         ON membership.revision_id = current_playlist.revision_id
                       JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
                       JOIN tracks track ON track.id = provider_track.track_id
                       LEFT JOIN albums album ON album.id = track.album_id
                       LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
                       LEFT JOIN artists artist ON artist.id = track_artist.artist_id
                      WHERE inventory.provider_account_id = $1
                        AND provider_playlist.provider_playlist_id = $2
                      GROUP BY membership.position, provider_track.provider_track_id, track.title, album.title
                      ORDER BY membership.position",
                )
                .bind(provider_connection_id.as_uuid())
                .bind(playlist_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|_| client_unavailable())?;
                (
                    header.try_get("name").map_err(|_| client_unavailable())?,
                    Some(
                        header
                            .try_get("captured_at")
                            .map_err(|_| client_unavailable())?,
                    ),
                    rows,
                )
            }
            LibraryStateSource::ChordriftModel => {
                let header = sqlx::query(
                    "SELECT playlist.id, generation.created_at,
                            COALESCE(name_revision.name, playlist.name) AS name
                       FROM playlist_generations generation
                       JOIN playlists playlist ON playlist.generation_id = generation.id
                       JOIN playlist_concepts concept ON concept.id = playlist.concept_id
                       LEFT JOIN playlist_name_revisions name_revision
                         ON name_revision.playlist_id = playlist.id AND name_revision.selected
                      WHERE generation.provider_account_id = $1
                        AND generation.status IN ('proposed', 'approved', 'published')
                        AND concept.stable_key = $2 AND playlist.archived_at IS NULL
                      ORDER BY CASE generation.status WHEN 'proposed' THEN 0 WHEN 'approved' THEN 1 ELSE 2 END,
                               generation.created_at DESC, generation.id DESC LIMIT 1",
                )
                .bind(provider_connection_id.as_uuid())
                .bind(playlist_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|_| client_unavailable())?
                .ok_or_else(|| crate::contract::ClientError::new(ErrorCode::ResourceNotFound, false))?;
                let model_playlist_id: Uuid =
                    header.try_get("id").map_err(|_| client_unavailable())?;
                let rows = sqlx::query(
                    "SELECT membership.position, provider.provider_track_id, track.title,
                            COALESCE(string_agg(artist.name, ', ' ORDER BY track_artist.position), '') AS artists,
                            album.title AS album
                       FROM playlist_tracks membership
                       JOIN tracks track ON track.id = membership.track_id
                       JOIN provider_tracks provider
                         ON provider.track_id = track.id AND provider.provider = 'spotify'
                       LEFT JOIN albums album ON album.id = track.album_id
                       LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
                       LEFT JOIN artists artist ON artist.id = track_artist.artist_id
                      WHERE membership.playlist_id = $1
                      GROUP BY membership.position, provider.provider_track_id, track.title, album.title
                      ORDER BY membership.position",
                )
                .bind(model_playlist_id)
                .fetch_all(&self.pool)
                .await
                .map_err(|_| client_unavailable())?;
                (
                    header.try_get("name").map_err(|_| client_unavailable())?,
                    Some(
                        header
                            .try_get("created_at")
                            .map_err(|_| client_unavailable())?,
                    ),
                    rows,
                )
            }
        };
        let tracks = rows
            .into_iter()
            .map(|row| {
                let position: i32 = row.try_get("position").map_err(|_| client_unavailable())?;
                Ok(LibraryPlaylistTrackView {
                    position: u64::try_from(position + 1).map_err(|_| client_unavailable())?,
                    provider_track_id: row
                        .try_get("provider_track_id")
                        .map_err(|_| client_unavailable())?,
                    title: row.try_get("title").map_err(|_| client_unavailable())?,
                    artists: row.try_get("artists").map_err(|_| client_unavailable())?,
                    album: row.try_get("album").map_err(|_| client_unavailable())?,
                })
            })
            .collect::<std::result::Result<Vec<_>, crate::contract::ClientError>>()?;
        Ok(LibraryPlaylistTracksView {
            source,
            playlist_id: playlist_id.to_owned(),
            name,
            state_at,
            tracks,
        })
    }

    async fn library_track(
        &mut self,
        _subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
        provider_track_id: &str,
    ) -> std::result::Result<LibraryTrackView, crate::contract::ClientError> {
        let row = sqlx::query(
            "SELECT track.id AS track_id, track.title,
                    COALESCE(string_agg(artist.name, ', ' ORDER BY track_artist.position), '') AS artists,
                    album.title AS album
               FROM provider_tracks provider
               JOIN tracks track ON track.id = provider.track_id
               LEFT JOIN albums album ON album.id = track.album_id
               LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
               LEFT JOIN artists artist ON artist.id = track_artist.artist_id
              WHERE provider.provider = 'spotify' AND provider.provider_track_id = $1
              GROUP BY track.id, track.title, album.title",
        )
        .bind(provider_track_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| client_unavailable())?
        .ok_or_else(|| crate::contract::ClientError::new(ErrorCode::ResourceNotFound, false))?;
        let track_id: Uuid = row.try_get("track_id").map_err(|_| client_unavailable())?;
        let statistics = sqlx::query(
            "SELECT event_count, play_count, total_ms_played, last_played_at
               FROM account_listening_track_statistics
              WHERE provider_account_id = $1 AND provider_track_id = $2",
        )
        .bind(provider_connection_id.as_uuid())
        .bind(provider_track_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| client_unavailable())?;
        let saved = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (
                SELECT 1 FROM provider_current_inventories inventory
                JOIN provider_saved_track_revision_tracks saved_track
                  ON saved_track.revision_id = inventory.saved_track_revision_id
                JOIN provider_tracks provider ON provider.id = saved_track.provider_track_id
                WHERE inventory.provider_account_id = $1 AND provider.provider_track_id = $2
            )",
        )
        .bind(provider_connection_id.as_uuid())
        .bind(provider_track_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| client_unavailable())?;
        let exclusion_reason = sqlx::query_scalar(
            "SELECT exclusion_reason FROM excluded_tracks
              WHERE provider_account_id = $1 AND track_id = $2 AND restored_at IS NULL
                AND source_provider <> 'chordrift_forget'
              ORDER BY excluded_at DESC, id DESC LIMIT 1",
        )
        .bind(provider_connection_id.as_uuid())
        .bind(track_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| client_unavailable())?;
        let placement_rows = sqlx::query(
            "WITH provider_placements AS (
                SELECT provider_playlist.provider_playlist_id AS playlist_id,
                       current_playlist.name, membership.position + 1 AS position,
                       'provider_observation'::text AS source
                  FROM provider_current_inventories inventory
                  JOIN provider_current_playlists current_playlist
                    ON current_playlist.provider_account_id = inventory.provider_account_id
                  JOIN provider_playlists provider_playlist
                    ON provider_playlist.id = current_playlist.provider_playlist_id
                  JOIN provider_playlist_revision_tracks membership
                    ON membership.revision_id = current_playlist.revision_id
                  JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
                 WHERE inventory.provider_account_id = $1 AND provider_track.track_id = $2
            ), model_placements AS (
                SELECT concept.stable_key AS playlist_id,
                       COALESCE(name_revision.name, playlist.name) AS name,
                       membership.position + 1 AS position,
                       'chordrift_model'::text AS source
                  FROM playlist_generations generation
                  JOIN playlists playlist ON playlist.generation_id = generation.id
                  JOIN playlist_concepts concept ON concept.id = playlist.concept_id
                  JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
                  LEFT JOIN playlist_name_revisions name_revision
                    ON name_revision.playlist_id = playlist.id AND name_revision.selected
                 WHERE generation.id = (
                    SELECT id FROM playlist_generations
                     WHERE provider_account_id = $1
                       AND status IN ('proposed', 'approved', 'published')
                     ORDER BY CASE status WHEN 'proposed' THEN 0 WHEN 'approved' THEN 1 ELSE 2 END,
                              created_at DESC, id DESC LIMIT 1
                 ) AND membership.track_id = $2
            ) SELECT * FROM provider_placements UNION ALL SELECT * FROM model_placements
              ORDER BY source, lower(name), position",
        )
        .bind(provider_connection_id.as_uuid())
        .bind(track_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| client_unavailable())?;
        let placements = placement_rows
            .into_iter()
            .map(|placement| {
                let position: i32 = placement
                    .try_get("position")
                    .map_err(|_| client_unavailable())?;
                let source: String = placement
                    .try_get("source")
                    .map_err(|_| client_unavailable())?;
                Ok(LibraryTrackPlacementView {
                    playlist_id: placement
                        .try_get("playlist_id")
                        .map_err(|_| client_unavailable())?,
                    name: placement
                        .try_get("name")
                        .map_err(|_| client_unavailable())?,
                    position: u64::try_from(position).map_err(|_| client_unavailable())?,
                    source: if source == "provider_observation" {
                        LibraryStateSource::ProviderObservation
                    } else {
                        LibraryStateSource::ChordriftModel
                    },
                })
            })
            .collect::<std::result::Result<Vec<_>, crate::contract::ClientError>>()?;
        let (event_count, play_count, total_ms_played, last_played_at) =
            if let Some(statistics) = statistics {
                let events: i64 = statistics
                    .try_get("event_count")
                    .map_err(|_| client_unavailable())?;
                let plays: i64 = statistics
                    .try_get("play_count")
                    .map_err(|_| client_unavailable())?;
                let duration: i64 = statistics
                    .try_get("total_ms_played")
                    .map_err(|_| client_unavailable())?;
                (
                    u64::try_from(events).map_err(|_| client_unavailable())?,
                    u64::try_from(plays).map_err(|_| client_unavailable())?,
                    u64::try_from(duration).map_err(|_| client_unavailable())?,
                    Some(
                        statistics
                            .try_get("last_played_at")
                            .map_err(|_| client_unavailable())?,
                    ),
                )
            } else {
                (0, 0, 0, None)
            };
        Ok(LibraryTrackView {
            provider_track_id: provider_track_id.to_owned(),
            title: row.try_get("title").map_err(|_| client_unavailable())?,
            artists: row.try_get("artists").map_err(|_| client_unavailable())?,
            album: row.try_get("album").map_err(|_| client_unavailable())?,
            play_count,
            event_count,
            total_ms_played,
            last_played_at,
            saved,
            exclusion_reason,
            placements,
        })
    }

    async fn excluded_tracks(
        &mut self,
        _subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
    ) -> std::result::Result<ExcludedTracksView, crate::contract::ClientError> {
        let rows = sqlx::query(
            "SELECT min(provider.provider_track_id) AS provider_track_id, track.title,
                    COALESCE(string_agg(DISTINCT artist.name, ', '), '') AS artists,
                    exclusion.exclusion_reason, exclusion.excluded_at,
                    current_playlist.name AS previous_playlist
               FROM excluded_tracks exclusion
               JOIN tracks track ON track.id = exclusion.track_id
               JOIN provider_tracks provider
                 ON provider.track_id = track.id AND provider.provider = 'spotify'
               LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
               LEFT JOIN artists artist ON artist.id = track_artist.artist_id
               LEFT JOIN provider_playlists previous
                 ON previous.id = exclusion.source_provider_playlist_id
               LEFT JOIN current_spotify_playlists current_playlist
                 ON current_playlist.provider_account_id = exclusion.provider_account_id
                AND current_playlist.provider_playlist_id = previous.id
              WHERE exclusion.provider_account_id = $1 AND exclusion.restored_at IS NULL
                AND exclusion.source_provider <> 'chordrift_forget'
              GROUP BY exclusion.id, track.id, track.title, exclusion.exclusion_reason,
                       exclusion.excluded_at, current_playlist.name
              ORDER BY exclusion.excluded_at DESC, exclusion.id DESC",
        )
        .bind(provider_connection_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|_| client_unavailable())?;
        let tracks = rows
            .into_iter()
            .map(|row| {
                Ok(ExcludedTrackView {
                    provider_track_id: row
                        .try_get("provider_track_id")
                        .map_err(|_| client_unavailable())?,
                    title: row.try_get("title").map_err(|_| client_unavailable())?,
                    artists: row.try_get("artists").map_err(|_| client_unavailable())?,
                    reason: row
                        .try_get("exclusion_reason")
                        .map_err(|_| client_unavailable())?,
                    excluded_at: row
                        .try_get("excluded_at")
                        .map_err(|_| client_unavailable())?,
                    previous_playlist: row
                        .try_get("previous_playlist")
                        .map_err(|_| client_unavailable())?,
                })
            })
            .collect::<std::result::Result<Vec<_>, crate::contract::ClientError>>()?;
        Ok(ExcludedTracksView { tracks })
    }

    async fn owns_provider_connection(
        &self,
        subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
    ) -> bool {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                 SELECT 1 FROM provider_accounts
                  WHERE chordrift_account_id = $1 AND id = $2
             )",
        )
        .bind(subject.account_id.as_uuid())
        .bind(provider_connection_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .unwrap_or(false)
    }

    async fn observe(
        &mut self,
        _subject: AuthenticatedSubject,
        _provider_connection_id: ResourceId,
        _current: Option<&crate::contract::MaintenanceSessionView>,
    ) -> std::result::Result<MaintenanceProjection, crate::contract::ClientError> {
        // Honest deployment boundary: do not pretend the legacy script is a
        // hosted adapter. V021-06 keeps this unavailable until the real
        // provider/database adapter lands and passes the read-only cutover gate.
        Err(client_unavailable())
    }

    async fn record_decisions(
        &mut self,
        _subject: AuthenticatedSubject,
        _view: &crate::contract::MaintenanceSessionView,
    ) -> std::result::Result<MaintenanceDecisionProjection, crate::contract::ClientError> {
        Err(client_unavailable())
    }

    async fn apply(
        &mut self,
        _subject: AuthenticatedSubject,
        _view: &crate::contract::MaintenanceSessionView,
    ) -> std::result::Result<MaintenanceProjection, crate::contract::ClientError> {
        Err(client_unavailable())
    }
}

/// Starts the private-beta HTTP authority from explicit environment settings.
pub async fn run_from_env() -> Result<()> {
    let config = Arc::new(HostedConfig::from_env()?);
    let database = db::connect(config::database_config_from_env()?).await?;
    let pool = database.pool().clone();
    let identity_store = Arc::new(PostgresProductIdentityStore::new(pool.clone()));
    identity_store
        .verify_schema()
        .await
        .map_err(|_| configuration("hosted identity schema is not ready"))?;

    let http = reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(15))
        .build()?;
    let oidc = Arc::new(OidcVerifier {
        issuer: config.oidc_issuer.as_str().trim_end_matches('/').to_owned(),
        userinfo_url: config.oidc_userinfo_url.clone(),
        http,
    });
    let session_authority = Arc::new(ProductSessionAuthority::new(
        Arc::clone(&oidc),
        Arc::clone(&identity_store),
    ));
    let session_authenticator = Arc::new(ProductSessionAuthenticator::new(Arc::clone(
        &identity_store,
    )));
    let state = HostedState {
        config: Arc::clone(&config),
        database_pool: pool.clone(),
        identity_store,
        session_authority,
        session_authenticator: Arc::clone(&session_authenticator),
        oidc,
        login_attempts: Arc::new(Mutex::new(HashMap::new())),
    };

    let application = MaintenanceApplication::new(DeploymentMaintenanceBackend { pool });
    let compatibility = deployment_compatibility();
    let typed = AuthenticatedHttpTransport::new(
        Arc::new(Mutex::new(Box::new(application))),
        session_authenticator,
    )
    .with_compatibility(compatibility)
    .router()
    .layer(middleware::from_fn_with_state(
        BrowserBridge {
            public_origin: config
                .public_origin
                .as_str()
                .trim_end_matches('/')
                .to_owned(),
        },
        browser_cookie_to_bearer,
    ));

    let router = Router::new()
        .route("/", get(index))
        .route("/assets/app.js", get(javascript))
        .route("/assets/app.css", get(stylesheet))
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/auth/login", get(login))
        .route("/auth/callback", get(callback))
        .route("/auth/session", get(session_status))
        .route("/auth/logout", post(logout))
        .with_state(state)
        .merge(typed)
        .layer(middleware::from_fn(security_headers));

    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    add_security_headers(&mut response);
    response
}

fn add_security_headers(response: &mut Response) {
    let headers = response.headers_mut();
    headers.insert(
        HeaderName::from_static("strict-transport-security"),
        HeaderValue::from_static("max-age=31536000"),
    );
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(
            "default-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'; object-src 'none'",
        ),
    );
}

/// Checks the loopback liveness endpoint for container orchestration without
/// requiring curl, a shell, or another executable in the runtime image.
pub async fn healthcheck_from_env() -> Result<()> {
    let bind = env::var("CHORDRIFT_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());
    let address: SocketAddr = bind
        .parse()
        .map_err(|_| configuration("CHORDRIFT_BIND must be a socket address"))?;
    let url = format!("http://127.0.0.1:{}/health/live", address.port());
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?
        .get(url)
        .send()
        .await?;
    if response.status() == StatusCode::OK {
        Ok(())
    } else {
        Err(configuration("hosted service liveness check failed"))
    }
}

fn deployment_compatibility() -> ServiceCompatibility {
    ServiceCompatibility {
        contract_versions: ContractVersionRange::exact(CONTRACT_VERSION),
        schema_version: 50,
        features: BTreeMap::from([
            (
                CAPABILITY_AUTHENTICATED_SERVICE_TRANSPORT.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_PRODUCT_IDENTITY.to_owned(),
                CapabilityAvailability::Available,
            ),
            // These implementations exist, but remain unavailable in the
            // deployment manifest until production assembly is complete.
            (
                CAPABILITY_PROVIDER_CREDENTIAL_VAULT.to_owned(),
                CapabilityAvailability::Unavailable,
            ),
            (
                CAPABILITY_DURABLE_OPERATIONS.to_owned(),
                CapabilityAvailability::Unavailable,
            ),
            (
                CAPABILITY_REMOTE_CLI.to_owned(),
                CapabilityAvailability::Unavailable,
            ),
            (
                CAPABILITY_MAINTENANCE_TASK_SESSION.to_owned(),
                CapabilityAvailability::Unavailable,
            ),
        ]),
        provider_capabilities: BTreeMap::new(),
        evidence_capabilities: BTreeMap::new(),
    }
}

async fn browser_cookie_to_bearer(
    State(bridge): State<BrowserBridge>,
    mut request: Request,
    next: Next,
) -> Response {
    if request.headers().contains_key(AUTHORIZATION) {
        return next.run(request).await;
    }
    let browser_marker = request
        .headers()
        .get("x-chordrift-browser")
        .and_then(|value| value.to_str().ok());
    let origin = request
        .headers()
        .get(ORIGIN)
        .and_then(|value| value.to_str().ok());
    if browser_marker != Some("1") || origin != Some(bridge.public_origin.as_str()) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(token) = cookie(request.headers(), SESSION_COOKIE) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Ok(value) = HeaderValue::from_str(&format!("Bearer {token}")) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    request.headers_mut().insert(AUTHORIZATION, value);
    next.run(request).await
}

async fn index() -> Response {
    let mut response = Html(INDEX_HTML).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn javascript() -> Response {
    static_asset(APP_JS, "text/javascript; charset=utf-8")
}

async fn stylesheet() -> Response {
    static_asset(APP_CSS, "text/css; charset=utf-8")
}

fn static_asset(body: &'static str, content_type: &'static str) -> Response {
    let mut response = body.into_response();
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    // Private-beta clients must not retain a stale contract wrapper across a
    // service rollout. Versioned immutable caching can return after the beta
    // establishes an asset-manifest build step.
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

async fn live() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "live",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn ready(State(state): State<HostedState>) -> Response {
    let database_ready = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.database_pool)
        .await
        .is_ok();
    let schema_ready = state.identity_store.verify_schema().await.is_ok();
    let status = if database_ready && schema_ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(serde_json::json!({
            "status": if status == StatusCode::OK { "ready" } else { "not_ready" },
            "database": database_ready,
            "identity_schema": schema_ready,
            "provider_writes": false,
        })),
    )
        .into_response()
}

async fn login(State(state): State<HostedState>) -> Response {
    let flow_id = random_url_token();
    let csrf_state = random_url_token();
    let code_verifier = Zeroizing::new(random_url_token());
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
    {
        let mut attempts = state.login_attempts.lock().await;
        attempts.retain(|_, attempt| attempt.expires_at > Utc::now());
        if attempts.len() >= MAX_LOGIN_ATTEMPTS {
            return StatusCode::TOO_MANY_REQUESTS.into_response();
        }
        attempts.insert(
            flow_id.clone(),
            LoginAttempt {
                state: csrf_state.clone(),
                code_verifier,
                expires_at: Utc::now() + TimeDelta::minutes(LOGIN_TTL_MINUTES),
            },
        );
    }
    let mut authorization = state.config.oidc_authorization_url.clone();
    authorization
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &state.config.oidc_client_id)
        .append_pair("redirect_uri", &state.config.callback_url())
        .append_pair("scope", "openid profile email")
        .append_pair("state", &csrf_state)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256");
    redirect_with_cookie(
        authorization.as_str(),
        &format!(
            "{LOGIN_COOKIE}={flow_id}; Path=/auth; HttpOnly; Secure; SameSite=Lax; Max-Age={}",
            LOGIN_TTL_MINUTES * 60
        ),
    )
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

async fn callback(
    State(state): State<HostedState>,
    AxumQuery(query): AxumQuery<CallbackQuery>,
    request: Request,
) -> Response {
    let Some(flow_id) = cookie(request.headers(), LOGIN_COOKIE) else {
        return auth_failure();
    };
    let Some(attempt) = state.login_attempts.lock().await.remove(&flow_id) else {
        return auth_failure();
    };
    if attempt.expires_at <= Utc::now() || attempt.state != query.state {
        return auth_failure();
    }
    let token = match state
        .oidc
        .http
        .post(state.config.oidc_token_url.clone())
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", state.config.oidc_client_id.as_str()),
            ("client_secret", state.config.oidc_client_secret.as_str()),
            ("code", query.code.as_str()),
            ("redirect_uri", state.config.callback_url().as_str()),
            ("code_verifier", attempt.code_verifier.as_str()),
        ])
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {
            match response.json::<TokenResponse>().await {
                Ok(token) => Zeroizing::new(token.access_token),
                Err(_) => return auth_failure(),
            }
        }
        _ => return auth_failure(),
    };
    let profile = match state.oidc.verify_profile(&token).await {
        Ok(profile) => profile,
        Err(_) => return auth_failure(),
    };
    if let Some(expected_email) = &state.config.bootstrap_email {
        if !profile.email_verified || profile.email.as_deref() != Some(expected_email.as_str()) {
            return StatusCode::FORBIDDEN.into_response();
        }
        if state
            .identity_store
            .provision_account_owner(&profile.identity, state.config.account_id)
            .await
            .is_err()
        {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    let grant = match state
        .session_authority
        .exchange(
            &token,
            SessionExchangeRequest {
                schema_version: PRODUCT_SESSION_SCHEMA_VERSION,
                account_id: state.config.account_id,
            },
        )
        .await
    {
        Ok(grant) => grant,
        Err(_) => return StatusCode::FORBIDDEN.into_response(),
    };
    redirect_with_cookie(
        state.config.public_origin.as_str(),
        &format!(
            "{SESSION_COOKIE}={}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age={}",
            grant.access_token,
            (grant.expires_at - Utc::now()).num_seconds().max(0)
        ),
    )
}

async fn session_status(State(state): State<HostedState>, request: Request) -> Response {
    let Some(token) = cookie(request.headers(), SESSION_COOKIE) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match state.session_authenticator.authenticate(&token).await {
        Ok(subject) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "signed_in": true,
                "subject_id": subject.subject_id,
                "account_id": subject.account_id,
            })),
        )
            .into_response(),
        Err(_) => StatusCode::UNAUTHORIZED.into_response(),
    }
}

async fn logout(State(state): State<HostedState>, request: Request) -> Response {
    if let Some(token) = cookie(request.headers(), SESSION_COOKIE) {
        let _ = state.session_authority.revoke(&token).await;
    }
    redirect_with_cookie(
        state.config.public_origin.as_str(),
        &format!("{SESSION_COOKIE}=; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=0"),
    )
}

fn cookie(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|entry| entry.trim().split_once('='))
        .find_map(|(key, value)| (key == name && !value.is_empty()).then(|| value.to_owned()))
}

fn redirect_with_cookie(location: &str, cookie: &str) -> Response {
    let Ok(location) = HeaderValue::from_str(location) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let Ok(cookie) = HeaderValue::from_str(cookie) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let mut response = StatusCode::SEE_OTHER.into_response();
    response.headers_mut().insert(LOCATION, location);
    response.headers_mut().append(SET_COOKIE, cookie);
    response
}

fn auth_failure() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "authentication_failed" })),
    )
        .into_response()
}

fn random_url_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn required(name: &str) -> Result<String> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| configuration(format!("required setting {name} is missing")))
}

fn https_url(name: &str) -> Result<Url> {
    let url = Url::parse(&required(name)?)
        .map_err(|_| configuration(format!("{name} must be a valid URL")))?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err(configuration(format!("{name} must be an HTTPS URL")));
    }
    Ok(url)
}

fn configuration(message: impl Into<String>) -> ChordriftError {
    ChordriftError::Configuration(message.into())
}

fn client_unavailable() -> crate::contract::ClientError {
    crate::contract::ClientError::new(ErrorCode::DependencyUnavailable, true)
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deployment_manifest_is_honest_before_real_adapter_is_wired() {
        let compatibility = deployment_compatibility();
        assert_eq!(
            compatibility
                .features
                .get(CAPABILITY_AUTHENTICATED_SERVICE_TRANSPORT),
            Some(&CapabilityAvailability::Available)
        );
        assert_eq!(
            compatibility
                .features
                .get(CAPABILITY_MAINTENANCE_TASK_SESSION),
            Some(&CapabilityAvailability::Unavailable)
        );
        assert!(compatibility.provider_capabilities.is_empty());
    }

    #[test]
    fn cookie_parser_does_not_match_prefixes() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_static("not_chordrift_session=x; chordrift_session=expected"),
        );
        assert_eq!(
            cookie(&headers, SESSION_COOKIE).as_deref(),
            Some("expected")
        );
    }

    #[test]
    fn hosted_responses_enforce_wrapper_independent_browser_policy() {
        let mut response = StatusCode::OK.into_response();
        add_security_headers(&mut response);
        assert_eq!(
            response
                .headers()
                .get("content-security-policy")
                .and_then(|value| value.to_str().ok()),
            Some(
                "default-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'; object-src 'none'"
            )
        );
        assert_eq!(
            response
                .headers()
                .get("strict-transport-security")
                .and_then(|value| value.to_str().ok()),
            Some("max-age=31536000")
        );
    }
}
