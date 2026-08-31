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
        HeaderValue, StatusCode,
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
use sqlx::PgPool;
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
        CapabilityAvailability, ContractVersionRange, ErrorCode, ResourceId, ServiceCompatibility,
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
        .merge(typed);

    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
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
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );
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
}
