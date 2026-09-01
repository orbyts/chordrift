//! Production hosted-service assembly for the private beta.
//!
//! This module owns HTTPS-origin policy, OIDC login, browser-cookie bridging,
//! health checks, provider OAuth transport, and the static contract workbench.
//! It deliberately does not expose a shell, SQL, client-supplied provider URLs,
//! database credentials, or provider credentials. Provider writes remain
//! bounded by the separate exact-review maintenance authority.

use std::{
    collections::{BTreeMap, HashMap},
    env,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use axum::{
    Form, Json, Router,
    extract::{Path as AxumPath, Query as AxumQuery, Request, State},
    http::{
        HeaderMap, HeaderName, HeaderValue, StatusCode,
        header::{
            AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE, COOKIE, LOCATION, ORIGIN, REFERER,
            SET_COOKIE,
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
        CapabilityAvailability, Command, CommandReceipt, CommandRequest, ContractVersionRange,
        ErrorCode, ExcludedTrackView, ExcludedTracksView, LibraryComparisonView,
        LibraryPlaylistTrackView, LibraryPlaylistTracksView, LibraryPlaylistView,
        LibraryPlaylistsView, LibraryStateSource, LibraryTrackPlacementView, LibraryTrackView,
        ProviderConnectionView, ProviderConnectionsView, Query, QueryRequest, QueryResponse,
        ResourceId, ServiceCompatibility, View,
    },
    db,
    durable_operations::{
        DurableOperationQueue, OperationRetryPolicy, PostgresDurableOperationStore,
    },
    http_transport::{AuthenticatedHttpTransport, BearerAuthenticator},
    identity::{
        ExternalIdentityVerifier, PRODUCT_SESSION_SCHEMA_VERSION, PostgresProductIdentityStore,
        ProductSessionAuthenticator, ProductSessionAuthority, SessionExchangeRequest,
        VerifiedExternalIdentity,
    },
    maintenance::{MaintenanceDecisionProjection, MaintenanceProjection},
    maintenance_interpretation::surface as maintenance_surface,
    maintenance_store::PostgresMaintenanceSessionStore,
    provider_connections::PostgresProviderConnectionAuthority,
    provider_vault::{
        PostgresProviderCredentialStore, ProviderCredentialVault, ProviderVaultKeyring,
    },
    providers::spotify::{begin_hosted_authorization, complete_hosted_authorization},
    service::{
        AuthenticatedSubject, ContractApplication, MaintenanceApplication, MaintenanceBackend,
    },
};

const SESSION_COOKIE: &str = "chordrift_session";
const LOGIN_COOKIE: &str = "chordrift_login";
const SPOTIFY_LOGIN_COOKIE: &str = "chordrift_spotify_login";
const LOGIN_TTL_MINUTES: i64 = 10;
const MAX_LOGIN_ATTEMPTS: usize = 128;
const CLI_LOGIN_TTL_MINUTES: i64 = 5;
const INDEX_HTML: &str = include_str!("../web/index.html");
const APP_JS: &str = include_str!("../web/app.js");
const LIBRARY_EXPLORER_JS: &str = include_str!("../web/library-explorer.js");
const MAINTENANCE_DECISIONS_JS: &str = include_str!("../web/maintenance-decisions.js");
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
    spotify_client_id: String,
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
        let spotify_client_id = required("CHORDRIFT_SPOTIFY_CLIENT_ID")?;
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
            spotify_client_id,
        })
    }

    fn callback_url(&self) -> String {
        self.public_origin
            .join("auth/callback")
            .expect("validated origin accepts a relative callback")
            .to_string()
    }

    fn spotify_callback_url(&self) -> Url {
        self.public_origin
            .join("providers/spotify/callback")
            .expect("validated origin accepts a relative callback")
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

struct SpotifyLoginAttempt {
    state: String,
    code_verifier: Zeroizing<String>,
    subject: AuthenticatedSubject,
    expected_provider_connection_id: Option<ResourceId>,
    expires_at: DateTime<Utc>,
}

struct CliLoginAttempt {
    subject: AuthenticatedSubject,
    redirect_uri: Url,
    client_state: String,
    code_challenge: String,
    consent_token_sha256: [u8; 32],
    approved: bool,
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
    spotify_login_attempts: Arc<Mutex<HashMap<String, SpotifyLoginAttempt>>>,
    cli_login_attempts: Arc<Mutex<HashMap<String, CliLoginAttempt>>>,
    provider_connections: Arc<PostgresProviderConnectionAuthority>,
}

#[derive(Clone)]
struct BrowserBridge {
    public_origin: String,
}

#[derive(Clone)]
struct DeploymentMaintenanceBackend {
    pool: PgPool,
}

struct DeploymentApplication {
    pool: PgPool,
    reads: MaintenanceApplication<DeploymentMaintenanceBackend>,
    operations: DurableOperationQueue<PostgresDurableOperationStore>,
    maintenance_sessions: PostgresMaintenanceSessionStore,
}

impl DeploymentApplication {
    fn new(pool: PgPool, operations: DurableOperationQueue<PostgresDurableOperationStore>) -> Self {
        Self {
            reads: MaintenanceApplication::new(DeploymentMaintenanceBackend { pool: pool.clone() }),
            pool: pool.clone(),
            operations,
            maintenance_sessions: PostgresMaintenanceSessionStore::new(pool),
        }
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
}

#[async_trait]
impl ContractApplication for DeploymentApplication {
    async fn command(
        &mut self,
        subject: AuthenticatedSubject,
        request: CommandRequest,
    ) -> std::result::Result<CommandReceipt, crate::contract::ClientError> {
        if let Command::CancelOperation(cancellation) = request.command {
            self.operations
                .request_cancellation(subject, cancellation)
                .await?;
            return Ok(CommandReceipt {
                contract_version: CONTRACT_VERSION,
                request_id: request.request_id,
                operation_id: cancellation.operation_id,
                cancellation_id: cancellation.cancellation_id,
            });
        }
        let provider_connection_id = match &request.command {
            Command::ObserveProvider {
                provider_connection_id,
            }
            | Command::StartMaintenance {
                provider_connection_id,
                ..
            } => Some(*provider_connection_id),
            _ => None,
        };
        if let Some(provider_connection_id) = provider_connection_id {
            if !self
                .owns_provider_connection(subject, provider_connection_id)
                .await
            {
                return Err(crate::contract::ClientError::new(
                    ErrorCode::PermissionDenied,
                    false,
                ));
            }
            return self
                .operations
                .accept(
                    subject,
                    request,
                    OperationRetryPolicy::new(3, Duration::from_secs(5))
                        .expect("static hosted retry policy is valid"),
                )
                .await
                .map(|accepted| accepted.receipt);
        }
        if matches!(
            &request.command,
            Command::RefreshMaintenance { .. } | Command::ResolveMaintenance { .. }
        ) {
            let session_id = match &request.command {
                Command::RefreshMaintenance { session_id, .. }
                | Command::ResolveMaintenance { session_id, .. } => *session_id,
                _ => unreachable!("maintenance command shape already matched"),
            };
            self.maintenance_sessions.load(subject, session_id).await?;
            return self
                .operations
                .accept(
                    subject,
                    request,
                    OperationRetryPolicy::new(3, Duration::from_secs(5))
                        .expect("static hosted retry policy is valid"),
                )
                .await
                .map(|accepted| accepted.receipt);
        }
        if let Command::AuthorizeMaintenance {
            session_id,
            expected_revision,
            review_id,
        } = &request.command
        {
            let current = self.maintenance_sessions.load(subject, *session_id).await?;
            if current.view.revision != *expected_revision
                || current.view.state
                    != crate::contract::MaintenanceSessionState::ReadyForAuthorization
                || current.view.review_id != Some(*review_id)
            {
                return Err(crate::contract::ClientError::new(
                    ErrorCode::StateConflict,
                    false,
                ));
            }
            return self
                .operations
                .accept(
                    subject,
                    request,
                    OperationRetryPolicy::new(3, Duration::from_secs(5))
                        .expect("static hosted retry policy is valid"),
                )
                .await
                .map(|accepted| accepted.receipt);
        }
        self.reads.command(subject, request).await
    }

    async fn query(
        &mut self,
        subject: AuthenticatedSubject,
        request: QueryRequest,
    ) -> std::result::Result<QueryResponse, crate::contract::ClientError> {
        let generated_at = Utc::now();
        match request.query {
            Query::Operation { operation_id } => {
                let operation = self.operations.operation(subject, operation_id).await?;
                Ok(crate::durable_operations::operation_query_response(
                    request.request_id,
                    generated_at,
                    operation,
                ))
            }
            Query::OperationHistory { account_id } => {
                if account_id != subject.account_id {
                    return Err(crate::contract::ClientError::new(
                        ErrorCode::PermissionDenied,
                        false,
                    ));
                }
                Ok(QueryResponse::OperationHistory(View {
                    contract_version: CONTRACT_VERSION,
                    request_id: request.request_id,
                    generated_at,
                    value: self.operations.history(subject).await?,
                }))
            }
            Query::OperationEvents {
                operation_id,
                after_sequence,
            } => Ok(QueryResponse::OperationEvents(View {
                contract_version: CONTRACT_VERSION,
                request_id: request.request_id,
                generated_at,
                value: self
                    .operations
                    .events(subject, operation_id, after_sequence)
                    .await?,
            })),
            Query::MaintenanceSession { session_id } => {
                let durable = self.maintenance_sessions.load(subject, session_id).await?;
                Ok(QueryResponse::MaintenanceSession(View {
                    contract_version: CONTRACT_VERSION,
                    request_id: request.request_id,
                    generated_at,
                    value: durable.view,
                }))
            }
            query => {
                self.reads
                    .query(subject, QueryRequest { query, ..request })
                    .await
            }
        }
    }
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
                            maintenance_surface: maintenance_surface(
                                &row.try_get::<String, _>("name")
                                    .map_err(|_| client_unavailable())?,
                            ),
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
                            maintenance_surface: maintenance_surface(
                                &row.try_get::<String, _>("name")
                                    .map_err(|_| client_unavailable())?,
                            ),
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
                            album.title AS album,
                            COALESCE(statistics.play_count, 0) AS play_count,
                            statistics.last_played_at
                       FROM provider_current_inventories inventory
                       JOIN provider_current_playlists current_playlist
                         ON current_playlist.provider_account_id = inventory.provider_account_id
                       JOIN provider_playlists provider_playlist
                         ON provider_playlist.id = current_playlist.provider_playlist_id
                       JOIN provider_playlist_revision_tracks membership
                         ON membership.revision_id = current_playlist.revision_id
                       JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
                       JOIN tracks track ON track.id = provider_track.track_id
                       LEFT JOIN account_listening_track_statistics statistics
                         ON statistics.provider_account_id = inventory.provider_account_id
                        AND statistics.provider_track_id = provider_track.provider_track_id
                       LEFT JOIN albums album ON album.id = track.album_id
                       LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
                       LEFT JOIN artists artist ON artist.id = track_artist.artist_id
                      WHERE inventory.provider_account_id = $1
                        AND provider_playlist.provider_playlist_id = $2
                      GROUP BY membership.position, provider_track.provider_track_id, track.title,
                               album.title, statistics.play_count, statistics.last_played_at
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
                            album.title AS album,
                            COALESCE(statistics.play_count, 0) AS play_count,
                            statistics.last_played_at
                       FROM playlist_tracks membership
                       JOIN tracks track ON track.id = membership.track_id
                       JOIN provider_tracks provider
                         ON provider.track_id = track.id AND provider.provider = 'spotify'
                       LEFT JOIN account_listening_track_statistics statistics
                         ON statistics.provider_account_id = $2
                        AND statistics.provider_track_id = provider.provider_track_id
                       LEFT JOIN albums album ON album.id = track.album_id
                       LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
                       LEFT JOIN artists artist ON artist.id = track_artist.artist_id
                      WHERE membership.playlist_id = $1
                      GROUP BY membership.position, provider.provider_track_id, track.title,
                               album.title, statistics.play_count, statistics.last_played_at
                      ORDER BY membership.position",
                )
                .bind(model_playlist_id)
                .bind(provider_connection_id.as_uuid())
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
                    play_count: u64::try_from(
                        row.try_get::<i64, _>("play_count")
                            .map_err(|_| client_unavailable())?,
                    )
                    .map_err(|_| client_unavailable())?,
                    last_played_at: row
                        .try_get("last_played_at")
                        .map_err(|_| client_unavailable())?,
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

    async fn library_comparison(
        &mut self,
        _subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
    ) -> std::result::Result<LibraryComparisonView, crate::contract::ClientError> {
        crate::library_comparison::query(&self.pool, provider_connection_id).await
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
            ) SELECT * FROM (
                  SELECT * FROM provider_placements
                  UNION ALL
                  SELECT * FROM model_placements
              ) placement
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
                    album.title AS album,
                    COALESCE(max(statistics.play_count), 0) AS play_count,
                    COALESCE(max(statistics.event_count), 0) AS event_count,
                    max(statistics.last_played_at) AS last_played_at,
                    exclusion.exclusion_reason, exclusion.excluded_at,
                    current_playlist.name AS previous_playlist
               FROM excluded_tracks exclusion
               JOIN tracks track ON track.id = exclusion.track_id
               JOIN provider_tracks provider
                 ON provider.track_id = track.id AND provider.provider = 'spotify'
               LEFT JOIN account_listening_track_statistics statistics
                 ON statistics.provider_account_id = exclusion.provider_account_id
                AND statistics.provider_track_id = provider.provider_track_id
               LEFT JOIN albums album ON album.id = track.album_id
               LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
               LEFT JOIN artists artist ON artist.id = track_artist.artist_id
               LEFT JOIN provider_playlists previous
                 ON previous.id = exclusion.source_provider_playlist_id
               LEFT JOIN current_spotify_playlists current_playlist
                 ON current_playlist.provider_account_id = exclusion.provider_account_id
                AND current_playlist.provider_playlist_id = previous.id
              WHERE exclusion.provider_account_id = $1 AND exclusion.restored_at IS NULL
                AND exclusion.source_provider <> 'chordrift_forget'
              GROUP BY exclusion.id, track.id, track.title, album.title,
                       exclusion.exclusion_reason, exclusion.excluded_at, current_playlist.name
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
                    album: row.try_get("album").map_err(|_| client_unavailable())?,
                    play_count: u64::try_from(
                        row.try_get::<i64, _>("play_count")
                            .map_err(|_| client_unavailable())?,
                    )
                    .map_err(|_| client_unavailable())?,
                    event_count: u64::try_from(
                        row.try_get::<i64, _>("event_count")
                            .map_err(|_| client_unavailable())?,
                    )
                    .map_err(|_| client_unavailable())?,
                    last_played_at: row
                        .try_get("last_played_at")
                        .map_err(|_| client_unavailable())?,
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
    let provider_store = PostgresProviderCredentialStore::new(pool.clone());
    provider_store
        .verify_schema()
        .await
        .map_err(|_| configuration("hosted provider credential schema is not ready"))?;
    let provider_keyring = ProviderVaultKeyring::from_environment()
        .map_err(|_| configuration("hosted provider credential key is not ready"))?;
    let provider_connections = Arc::new(PostgresProviderConnectionAuthority::new(
        pool.clone(),
        ProviderCredentialVault::new(provider_store, provider_keyring),
    ));
    let operation_store = Arc::new(PostgresDurableOperationStore::new(pool.clone()));
    operation_store
        .verify_schema()
        .await
        .map_err(|_| configuration("durable operation schema is not ready"))?;
    PostgresMaintenanceSessionStore::new(pool.clone())
        .verify_schema()
        .await
        .map_err(|_| configuration("durable maintenance schema is not ready"))?;
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
        spotify_login_attempts: Arc::new(Mutex::new(HashMap::new())),
        cli_login_attempts: Arc::new(Mutex::new(HashMap::new())),
        provider_connections,
    };

    let application = DeploymentApplication::new(pool, DurableOperationQueue::new(operation_store));
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
        .route(
            "/assets/library-explorer.js",
            get(library_explorer_javascript),
        )
        .route(
            "/assets/maintenance-decisions.js",
            get(maintenance_decisions_javascript),
        )
        .route("/assets/app.css", get(stylesheet))
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/auth/login", get(login))
        .route("/auth/callback", get(callback))
        .route("/auth/session", get(session_status))
        .route("/auth/logout", post(logout))
        .route("/auth/cli/authorize", get(cli_authorize))
        .route("/auth/cli/approve", post(cli_approve))
        .route("/auth/cli/exchange", post(cli_exchange))
        .route("/providers/spotify/connect", get(spotify_connect))
        .route("/providers/spotify/callback", get(spotify_callback))
        .route(
            "/providers/spotify/{provider_connection_id}/disconnect",
            post(spotify_disconnect),
        )
        .with_state(state)
        .merge(typed)
        .layer(middleware::from_fn(security_headers))
        .layer(middleware::from_fn(request_observability));

    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn request_observability(request: Request, next: Next) -> Response {
    let started = Instant::now();
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
        .map_or_else(random_url_token, str::to_owned);
    let method = request.method().as_str().to_owned();
    let path = request.uri().path().to_owned();
    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
    eprintln!(
        "{}",
        request_log_line(
            &request_id,
            &method,
            &path,
            response.status().as_u16(),
            started.elapsed().as_millis(),
        )
    );
    response
}

fn request_log_line(
    request_id: &str,
    method: &str,
    path: &str,
    status: u16,
    elapsed_ms: u128,
) -> String {
    serde_json::json!({
        "event": "http_request",
        "request_id": request_id,
        "method": method,
        "path": path,
        "status": status,
        "elapsed_ms": elapsed_ms,
    })
    .to_string()
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
        schema_version: 51,
        features: BTreeMap::from([
            (
                CAPABILITY_AUTHENTICATED_SERVICE_TRANSPORT.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_PRODUCT_IDENTITY.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_PROVIDER_CREDENTIAL_VAULT.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_DURABLE_OPERATIONS.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_REMOTE_CLI.to_owned(),
                CapabilityAvailability::Available,
            ),
            (
                CAPABILITY_MAINTENANCE_TASK_SESSION.to_owned(),
                CapabilityAvailability::Available,
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

async fn library_explorer_javascript() -> Response {
    static_asset(LIBRARY_EXPLORER_JS, "text/javascript; charset=utf-8")
}

async fn maintenance_decisions_javascript() -> Response {
    static_asset(MAINTENANCE_DECISIONS_JS, "text/javascript; charset=utf-8")
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
            "provider_writes": true,
            "provider_write_scope": "exact_review_only",
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

#[derive(Deserialize)]
struct CliAuthorizeQuery {
    redirect_uri: String,
    state: String,
    code_challenge: String,
    code_challenge_method: String,
}

#[derive(Deserialize)]
struct CliApproveForm {
    flow_id: String,
    consent_token: String,
}

#[derive(Deserialize, Serialize)]
/// One-time PKCE exchange submitted by the installed CLI after browser consent.
pub struct CliSessionExchangeRequest {
    /// Product-session schema understood by the CLI.
    pub schema_version: u16,
    /// Single-use authorization code returned only to the loopback listener.
    pub code: String,
    /// Exact loopback callback bound by the authorization request.
    pub redirect_uri: String,
    /// PKCE verifier retained only by the initiating CLI process.
    pub code_verifier: String,
}

async fn cli_authorize(
    State(state): State<HostedState>,
    AxumQuery(query): AxumQuery<CliAuthorizeQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(subject) = authenticated_browser_subject(&state, &headers).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Html("Sign in to Chordrift in this browser, then run the CLI login again."),
        )
            .into_response();
    };
    let Ok(redirect_uri) = Url::parse(&query.redirect_uri) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !valid_cli_redirect(&redirect_uri)
        || query.code_challenge_method != "S256"
        || query.state.is_empty()
        || query.state.len() > 256
        || URL_SAFE_NO_PAD
            .decode(query.code_challenge.as_bytes())
            .ok()
            .is_none_or(|bytes| bytes.len() != 32)
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let flow_id = random_url_token();
    let consent_token = random_url_token();
    let consent_token_sha256 = Sha256::digest(consent_token.as_bytes()).into();
    {
        let mut attempts = state.cli_login_attempts.lock().await;
        attempts.retain(|_, attempt| attempt.expires_at > Utc::now());
        if attempts.len() >= MAX_LOGIN_ATTEMPTS {
            return StatusCode::TOO_MANY_REQUESTS.into_response();
        }
        attempts.insert(
            flow_id.clone(),
            CliLoginAttempt {
                subject,
                redirect_uri,
                client_state: query.state,
                code_challenge: query.code_challenge,
                consent_token_sha256,
                approved: false,
                expires_at: Utc::now() + TimeDelta::minutes(CLI_LOGIN_TTL_MINUTES),
            },
        );
    }
    Html(format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Authorize Chordrift CLI</title></head><body><main><h1>Connect the Chordrift CLI?</h1><p>This creates a separate revocable Chordrift session on this computer. It does not share Spotify or database credentials.</p><form method=\"post\" action=\"/auth/cli/approve\"><input type=\"hidden\" name=\"flow_id\" value=\"{flow_id}\"><input type=\"hidden\" name=\"consent_token\" value=\"{consent_token}\"><button type=\"submit\">Authorize CLI</button></form></main></body></html>"
    ))
    .into_response()
}

async fn cli_approve(
    State(state): State<HostedState>,
    headers: HeaderMap,
    Form(form): Form<CliApproveForm>,
) -> Response {
    let Some(subject) = authenticated_browser_subject(&state, &headers).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let mut attempts = state.cli_login_attempts.lock().await;
    let Some(attempt) = attempts.get(&form.flow_id) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if !valid_cli_consent(attempt, subject, &form.consent_token, Utc::now()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut attempt = attempts
        .remove(&form.flow_id)
        .expect("validated CLI login attempt remains present");
    drop(attempts);
    attempt.approved = true;
    let code = random_url_token();
    let mut redirect = attempt.redirect_uri.clone();
    redirect
        .query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &attempt.client_state);
    state.cli_login_attempts.lock().await.insert(code, attempt);
    (
        StatusCode::SEE_OTHER,
        [(LOCATION, redirect.as_str().to_owned())],
    )
        .into_response()
}

fn valid_cli_consent(
    attempt: &CliLoginAttempt,
    subject: AuthenticatedSubject,
    consent_token: &str,
    now: DateTime<Utc>,
) -> bool {
    let consent_token_sha256: [u8; 32] = Sha256::digest(consent_token.as_bytes()).into();
    attempt.expires_at > now
        && attempt.subject == subject
        && !attempt.approved
        && consent_token_sha256 == attempt.consent_token_sha256
}

async fn cli_exchange(
    State(state): State<HostedState>,
    Json(request): Json<CliSessionExchangeRequest>,
) -> Response {
    if request.schema_version != PRODUCT_SESSION_SCHEMA_VERSION {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(attempt) = state.cli_login_attempts.lock().await.remove(&request.code) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let verifier_challenge =
        URL_SAFE_NO_PAD.encode(Sha256::digest(request.code_verifier.as_bytes()));
    if !attempt.approved
        || attempt.expires_at <= Utc::now()
        || attempt.redirect_uri.as_str() != request.redirect_uri
        || verifier_challenge != attempt.code_challenge
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match state.session_authority.delegate(attempt.subject).await {
        Ok(grant) => (StatusCode::CREATED, Json(grant)).into_response(),
        Err(_) => StatusCode::FORBIDDEN.into_response(),
    }
}

fn valid_cli_redirect(url: &Url) -> bool {
    url.scheme() == "http"
        && matches!(url.host_str(), Some("127.0.0.1" | "::1" | "[::1]"))
        && url.port().is_some()
        && url.path() == "/callback"
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}

#[derive(Deserialize)]
struct SpotifyConnectQuery {
    provider_connection_id: Option<ResourceId>,
}

async fn spotify_connect(
    State(state): State<HostedState>,
    AxumQuery(query): AxumQuery<SpotifyConnectQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(subject) = authenticated_browser_subject(&state, &headers).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if let Some(connection_id) = query.provider_connection_id
        && !state
            .provider_connections
            .owns_spotify_connection(subject, connection_id)
            .await
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let callback = state.config.spotify_callback_url();
    let authorization = match begin_hosted_authorization(&state.config.spotify_client_id, &callback)
    {
        Ok(value) => value,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let flow_id = random_url_token();
    {
        let mut attempts = state.spotify_login_attempts.lock().await;
        attempts.retain(|_, attempt| attempt.expires_at > Utc::now());
        if attempts.len() >= MAX_LOGIN_ATTEMPTS {
            return StatusCode::TOO_MANY_REQUESTS.into_response();
        }
        attempts.insert(
            flow_id.clone(),
            SpotifyLoginAttempt {
                state: authorization.state,
                code_verifier: authorization.code_verifier,
                subject,
                expected_provider_connection_id: query.provider_connection_id,
                expires_at: Utc::now() + TimeDelta::minutes(LOGIN_TTL_MINUTES),
            },
        );
    }
    redirect_with_cookie(
        authorization.authorization_url.as_str(),
        &format!(
            "{SPOTIFY_LOGIN_COOKIE}={flow_id}; Path=/providers/spotify; HttpOnly; Secure; SameSite=Lax; Max-Age={}",
            LOGIN_TTL_MINUTES * 60
        ),
    )
}

#[derive(Deserialize)]
struct SpotifyCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn spotify_callback(
    State(state): State<HostedState>,
    AxumQuery(query): AxumQuery<SpotifyCallbackQuery>,
    headers: HeaderMap,
) -> Response {
    if query.error.is_some() {
        return spotify_redirect(&state, "cancelled");
    }
    let Some(flow_id) = cookie(&headers, SPOTIFY_LOGIN_COOKIE) else {
        return auth_failure();
    };
    let Some(attempt) = state.spotify_login_attempts.lock().await.remove(&flow_id) else {
        return auth_failure();
    };
    let Some(subject) = authenticated_browser_subject(&state, &headers).await else {
        return auth_failure();
    };
    if subject != attempt.subject
        || attempt.expires_at <= Utc::now()
        || query.state.as_deref() != Some(attempt.state.as_str())
    {
        return auth_failure();
    }
    let Some(code) = query.code else {
        return auth_failure();
    };
    let authorization = match complete_hosted_authorization(
        &state.config.spotify_client_id,
        &state.config.spotify_callback_url(),
        &code,
        attempt.code_verifier.as_str(),
    )
    .await
    {
        Ok(value) => value,
        Err(_) => return spotify_redirect(&state, "failed"),
    };
    if state
        .provider_connections
        .connect_spotify(
            subject,
            attempt.expected_provider_connection_id,
            &authorization.account_id,
            authorization.display_name.as_deref(),
            &authorization.credential,
        )
        .await
        .is_err()
    {
        return spotify_redirect(&state, "failed");
    }
    spotify_redirect(&state, "connected")
}

async fn spotify_disconnect(
    State(state): State<HostedState>,
    AxumPath(provider_connection_id): AxumPath<ResourceId>,
    headers: HeaderMap,
) -> Response {
    if !same_origin(&state, &headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(subject) = authenticated_browser_subject(&state, &headers).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if state
        .provider_connections
        .disconnect_spotify(subject, provider_connection_id)
        .await
        .is_err()
    {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    spotify_redirect(&state, "disconnected")
}

async fn authenticated_browser_subject(
    state: &HostedState,
    headers: &HeaderMap,
) -> Option<AuthenticatedSubject> {
    let token = cookie(headers, SESSION_COOKIE)?;
    state.session_authenticator.authenticate(&token).await.ok()
}

fn same_origin(state: &HostedState, headers: &HeaderMap) -> bool {
    same_origin_for(&state.config.public_origin, headers)
}

fn same_origin_for(public_origin: &Url, headers: &HeaderMap) -> bool {
    let exact_origin = headers
        .get(ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|origin| {
            origin.trim_end_matches('/') == public_origin.as_str().trim_end_matches('/')
        });
    let exact_referer = headers
        .get(REFERER)
        .and_then(|value| value.to_str().ok())
        .and_then(|referer| Url::parse(referer).ok())
        .is_some_and(|referer| referer.origin() == public_origin.origin());
    exact_origin
        || exact_referer
        || headers
            .get("x-chordrift-browser")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == "1")
}

fn spotify_redirect(state: &HostedState, outcome: &str) -> Response {
    let location = format!("{}?spotify={outcome}", state.config.public_origin);
    redirect_with_cookie(
        &location,
        &format!(
            "{SPOTIFY_LOGIN_COOKIE}=; Path=/providers/spotify; HttpOnly; Secure; SameSite=Lax; Max-Age=0"
        ),
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
    fn deployment_manifest_exposes_durable_record_only_maintenance() {
        let compatibility = deployment_compatibility();
        assert_eq!(
            compatibility.features.get(CAPABILITY_DURABLE_OPERATIONS),
            Some(&CapabilityAvailability::Available)
        );
        assert_eq!(
            compatibility
                .features
                .get(CAPABILITY_PROVIDER_CREDENTIAL_VAULT),
            Some(&CapabilityAvailability::Available)
        );
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
            Some(&CapabilityAvailability::Available)
        );
        assert_eq!(
            compatibility.features.get(CAPABILITY_REMOTE_CLI),
            Some(&CapabilityAvailability::Available)
        );
        assert!(compatibility.provider_capabilities.is_empty());
    }

    #[test]
    fn cli_login_accepts_only_exact_loopback_callbacks() {
        assert!(valid_cli_redirect(
            &Url::parse("http://127.0.0.1:43117/callback").unwrap()
        ));
        assert!(valid_cli_redirect(
            &Url::parse("http://[::1]:43117/callback").unwrap()
        ));
        for rejected in [
            "https://127.0.0.1:43117/callback",
            "http://localhost:43117/callback",
            "http://127.0.0.1/callback",
            "http://127.0.0.1:43117/other",
            "http://127.0.0.1:43117/callback?next=https://attacker.example",
            "http://user@127.0.0.1:43117/callback",
            "http://attacker.example:43117/callback",
        ] {
            assert!(
                !valid_cli_redirect(&Url::parse(rejected).unwrap()),
                "{rejected}"
            );
        }
    }

    #[test]
    fn request_logs_are_built_only_from_controlled_fields() {
        let line = request_log_line("req_1", "POST", "/v1/commands", 202, 17);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&line).unwrap()["request_id"],
            "req_1"
        );
        assert!(!line.contains("Authorization"));
        assert!(!line.contains("chd_session_"));
        assert!(!line.contains("postgresql://"));
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
    fn browser_mutations_accept_exact_origin_referer_or_non_simple_wrapper_header() {
        let state = HostedConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            public_origin: Url::parse("https://chordrift.example/").unwrap(),
            oidc_issuer: Url::parse("https://identity.example/").unwrap(),
            oidc_authorization_url: Url::parse("https://identity.example/authorize").unwrap(),
            oidc_token_url: Url::parse("https://identity.example/token").unwrap(),
            oidc_userinfo_url: Url::parse("https://identity.example/userinfo").unwrap(),
            oidc_client_id: "client".to_owned(),
            oidc_client_secret: Zeroizing::new("secret".to_owned()),
            spotify_client_id: "spotify-client".to_owned(),
            bootstrap_email: None,
            account_id: ResourceId::new(),
        };
        let mut origin = HeaderMap::new();
        origin.insert(
            ORIGIN,
            HeaderValue::from_static("https://chordrift.example"),
        );
        assert!(same_origin_for(&state.public_origin, &origin));

        let mut referer = HeaderMap::new();
        referer.insert(
            REFERER,
            HeaderValue::from_static("https://chordrift.example/auth/cli/authorize?flow=opaque"),
        );
        assert!(same_origin_for(&state.public_origin, &referer));

        for rejected in [
            "https://attacker.example/auth/cli/authorize",
            "https://chordrift.example.attacker.invalid/auth/cli/authorize",
            "http://chordrift.example/auth/cli/authorize",
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(REFERER, HeaderValue::from_str(rejected).unwrap());
            assert!(
                !same_origin_for(&state.public_origin, &headers),
                "{rejected}"
            );
        }

        let mut wrapper = HeaderMap::new();
        wrapper.insert("x-chordrift-browser", HeaderValue::from_static("1"));
        assert!(same_origin_for(&state.public_origin, &wrapper));
        assert!(!same_origin_for(&state.public_origin, &HeaderMap::new()));
    }

    #[test]
    fn cli_consent_token_supports_headerless_browsers_without_accepting_forgery() {
        let subject = AuthenticatedSubject {
            subject_id: ResourceId::new(),
            account_id: ResourceId::new(),
        };
        let now = Utc::now();
        let consent_token = "one-time-consent";
        let mut attempt = CliLoginAttempt {
            subject,
            redirect_uri: Url::parse("http://127.0.0.1:43117/callback").unwrap(),
            client_state: "client-state".to_owned(),
            code_challenge: URL_SAFE_NO_PAD.encode([7_u8; 32]),
            consent_token_sha256: Sha256::digest(consent_token.as_bytes()).into(),
            approved: false,
            expires_at: now + TimeDelta::minutes(5),
        };

        assert!(valid_cli_consent(&attempt, subject, consent_token, now));
        assert!(!valid_cli_consent(&attempt, subject, "forged", now));
        assert!(!valid_cli_consent(
            &attempt,
            AuthenticatedSubject {
                subject_id: ResourceId::new(),
                account_id: subject.account_id,
            },
            consent_token,
            now,
        ));
        assert!(!valid_cli_consent(
            &attempt,
            subject,
            consent_token,
            attempt.expires_at,
        ));
        attempt.approved = true;
        assert!(!valid_cli_consent(&attempt, subject, consent_token, now));
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
