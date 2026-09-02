//! Product identities, account authorization, and revocable bearer sessions.
//!
//! An external identity provider verifies its own credential behind
//! [`ExternalIdentityVerifier`]. Chordrift then issues a random opaque session
//! token, persists only its digest, and resolves every request through current
//! subject, account, membership, expiry, and revocation state.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    response::{IntoResponse, Response},
    routing::{delete, post},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, TimeDelta, Utc};
use rand::RngCore as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    contract::{ClientError, ErrorCode, ResourceId},
    http_transport::{BearerAuthenticator, MAX_CONTRACT_BODY_BYTES, error_response},
    service::{AuthenticatedSubject, ServiceClock, SystemServiceClock},
};

const TOKEN_PREFIX: &str = "chd_session_";
const TOKEN_BYTES: usize = 32;
/// Current JSON schema for product-session exchange and grants.
pub const PRODUCT_SESSION_SCHEMA_VERSION: u16 = 1;

/// Identity claim returned only after an external verifier accepts a credential.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedExternalIdentity {
    /// Stable issuer namespace, normally the verifier's HTTPS issuer.
    pub issuer: String,
    /// Stable subject within that issuer.
    pub subject: String,
}

/// Optional presentation metadata verified by the configured identity provider.
/// These fields never participate in authentication or authorization decisions.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExternalIdentityProfile {
    /// Human-readable account label suitable for a signed-in UI.
    pub display_name: Option<String>,
    /// Sanitized HTTPS profile-image URL suitable for a signed-in UI.
    pub avatar_url: Option<String>,
}

impl VerifiedExternalIdentity {
    /// Creates a bounded, nonempty issuer/subject identity.
    pub fn new(issuer: impl Into<String>, subject: impl Into<String>) -> Result<Self, ClientError> {
        let issuer = issuer.into();
        let subject = subject.into();
        if issuer.trim().is_empty()
            || subject.trim().is_empty()
            || issuer.len() > 512
            || subject.len() > 512
        {
            return Err(ClientError::new(ErrorCode::AuthenticationRequired, false));
        }
        Ok(Self { issuer, subject })
    }
}

/// Verifies an opaque upstream identity credential without exposing its format
/// to Chordrift's domain or HTTP clients.
#[async_trait]
pub trait ExternalIdentityVerifier: Send + Sync {
    /// Returns stable claims only when signature, issuer, audience, expiry, and
    /// provider-specific revocation checks succeed.
    async fn verify(&self, credential: &str) -> Result<VerifiedExternalIdentity, ClientError>;
}

/// Request used to exchange a verified external identity for a Chordrift session.
#[derive(Deserialize)]
pub struct SessionExchangeRequest {
    /// Exact product-session schema understood by the client.
    pub schema_version: u16,
    /// Existing Chordrift account the client wants to enter.
    pub account_id: ResourceId,
}

/// Plaintext bearer grant returned exactly once after session creation.
#[derive(Deserialize, Serialize)]
pub struct SessionGrant {
    /// Product-session response schema.
    pub schema_version: u16,
    /// Standard HTTP authorization scheme used with the access token.
    pub token_type: String,
    /// Opaque Chordrift session token; clients must store it as a secret.
    pub access_token: String,
    /// Stable session identity used for account security views later.
    pub session_id: ResourceId,
    /// Authenticated product subject.
    pub subject_id: ResourceId,
    /// Authorized Chordrift account.
    pub account_id: ResourceId,
    /// Absolute session expiry.
    pub expires_at: DateTime<Utc>,
}

/// Digest and metadata supplied atomically to the persistence boundary.
pub struct NewProductSession {
    /// New session identity.
    pub session_id: ResourceId,
    /// Requested account.
    pub account_id: ResourceId,
    /// SHA-256 digest of a 256-bit random bearer token.
    pub token_sha256: [u8; 32],
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Expiry time.
    pub expires_at: DateTime<Utc>,
}

/// Durable product identity/session repository.
#[async_trait]
pub trait ProductIdentityStore: Send + Sync {
    /// Atomically validates the identity/account binding and inserts a session.
    async fn create_session(
        &self,
        identity: &VerifiedExternalIdentity,
        session: &NewProductSession,
    ) -> Result<AuthenticatedSubject, ClientError>;

    /// Creates a second session for an already authenticated active subject.
    async fn create_delegated_session(
        &self,
        subject: AuthenticatedSubject,
        session: &NewProductSession,
    ) -> Result<AuthenticatedSubject, ClientError>;

    /// Resolves a session digest through all current authorization state.
    async fn authenticate_session(
        &self,
        token_sha256: [u8; 32],
        now: DateTime<Utc>,
    ) -> Result<AuthenticatedSubject, ClientError>;

    /// Revokes one current session. Unknown or already-revoked sessions fail closed.
    async fn revoke_session(
        &self,
        token_sha256: [u8; 32],
        now: DateTime<Utc>,
    ) -> Result<(), ClientError>;
}

/// Session lifetime policy. Rotation and refresh can be added without changing
/// the bearer-authentication contract.
#[derive(Clone, Copy, Debug)]
pub struct ProductSessionPolicy {
    ttl: Duration,
}

impl ProductSessionPolicy {
    /// Creates a positive session TTL no longer than 90 days.
    pub fn new(ttl: Duration) -> Result<Self, ClientError> {
        if ttl.is_zero() || ttl > Duration::from_secs(90 * 24 * 60 * 60) {
            return Err(ClientError::new(ErrorCode::InvalidRequest, false));
        }
        Ok(Self { ttl })
    }

    fn expires_at(self, now: DateTime<Utc>) -> Result<DateTime<Utc>, ClientError> {
        let delta = TimeDelta::from_std(self.ttl)
            .map_err(|_| ClientError::new(ErrorCode::Internal, false))?;
        now.checked_add_signed(delta)
            .ok_or_else(|| ClientError::new(ErrorCode::Internal, false))
    }
}

impl Default for ProductSessionPolicy {
    fn default() -> Self {
        Self {
            ttl: Duration::from_secs(30 * 24 * 60 * 60),
        }
    }
}

/// Issues and revokes Chordrift sessions after external identity verification.
pub struct ProductSessionAuthority<V, S> {
    verifier: Arc<V>,
    store: Arc<S>,
    clock: Arc<dyn ServiceClock>,
    policy: ProductSessionPolicy,
}

/// HTTP session exchange and revocation endpoints. These routes issue
/// Chordrift credentials only; application commands remain on the typed V021-01
/// transport.
pub struct ProductSessionHttpTransport<V, S> {
    authority: Arc<ProductSessionAuthority<V, S>>,
}

impl<V, S> Clone for ProductSessionHttpTransport<V, S> {
    fn clone(&self) -> Self {
        Self {
            authority: self.authority.clone(),
        }
    }
}

impl<V, S> ProductSessionHttpTransport<V, S>
where
    V: ExternalIdentityVerifier + 'static,
    S: ProductIdentityStore + 'static,
{
    /// Creates the narrow product-session HTTP adapter.
    pub fn new(authority: Arc<ProductSessionAuthority<V, S>>) -> Self {
        Self { authority }
    }

    /// Returns routes that can be merged with [`crate::http_transport::AuthenticatedHttpTransport`].
    pub fn router(self) -> Router {
        Router::new()
            .route("/v1/sessions", post(exchange_session::<V, S>))
            .route("/v1/sessions/current", delete(revoke_session::<V, S>))
            .with_state(self.authority)
    }
}

async fn exchange_session<V, S>(
    State(authority): State<Arc<ProductSessionAuthority<V, S>>>,
    request: Request,
) -> Response
where
    V: ExternalIdentityVerifier + 'static,
    S: ProductIdentityStore + 'static,
{
    let identity_credential = match request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|credential| !credential.is_empty())
    {
        Some(credential) => credential.to_owned(),
        None => return error_response(ClientError::new(ErrorCode::AuthenticationRequired, false)),
    };
    let body = match to_bytes(request.into_body(), MAX_CONTRACT_BODY_BYTES).await {
        Ok(body) => body,
        Err(_) => return error_response(ClientError::new(ErrorCode::InvalidRequest, false)),
    };
    let request = match serde_json::from_slice::<SessionExchangeRequest>(&body) {
        Ok(request) => request,
        Err(_) => return error_response(ClientError::new(ErrorCode::InvalidRequest, false)),
    };
    match authority.exchange(&identity_credential, request).await {
        Ok(grant) => (StatusCode::CREATED, Json(grant)).into_response(),
        Err(error) => error_response(error),
    }
}

async fn revoke_session<V, S>(
    State(authority): State<Arc<ProductSessionAuthority<V, S>>>,
    request: Request,
) -> Response
where
    V: ExternalIdentityVerifier + 'static,
    S: ProductIdentityStore + 'static,
{
    let value = match request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    {
        Some(value) => value,
        None => return error_response(ClientError::new(ErrorCode::AuthenticationRequired, false)),
    };
    let token = match value
        .strip_prefix("Bearer ")
        .filter(|token| !token.is_empty())
    {
        Some(token) => token,
        None => return error_response(ClientError::new(ErrorCode::AuthenticationRequired, false)),
    };
    match authority.revoke(token).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => error_response(error),
    }
}

impl<V, S> ProductSessionAuthority<V, S>
where
    V: ExternalIdentityVerifier,
    S: ProductIdentityStore,
{
    /// Creates a production-clock authority with the default 30-day lifetime.
    pub fn new(verifier: Arc<V>, store: Arc<S>) -> Self {
        Self::with_clock_and_policy(
            verifier,
            store,
            Arc::new(SystemServiceClock),
            ProductSessionPolicy::default(),
        )
    }

    /// Creates an authority with deterministic clock and explicit policy.
    pub fn with_clock_and_policy(
        verifier: Arc<V>,
        store: Arc<S>,
        clock: Arc<dyn ServiceClock>,
        policy: ProductSessionPolicy,
    ) -> Self {
        Self {
            verifier,
            store,
            clock,
            policy,
        }
    }

    /// Exchanges one verified external credential for a persisted Chordrift session.
    pub async fn exchange(
        &self,
        identity_credential: &str,
        request: SessionExchangeRequest,
    ) -> Result<SessionGrant, ClientError> {
        if request.schema_version != PRODUCT_SESSION_SCHEMA_VERSION {
            return Err(ClientError::new(ErrorCode::IncompatibleContract, false));
        }
        let identity = self.verifier.verify(identity_credential).await?;
        let (access_token, session) = self.new_session(request.account_id)?;
        let subject = self.store.create_session(&identity, &session).await?;
        Ok(session_grant(access_token, &session, subject))
    }

    /// Issues a distinct revocable session after an existing Chordrift session
    /// has authenticated the same active subject and account.
    pub async fn delegate(
        &self,
        subject: AuthenticatedSubject,
    ) -> Result<SessionGrant, ClientError> {
        let (access_token, session) = self.new_session(subject.account_id)?;
        let delegated = self
            .store
            .create_delegated_session(subject, &session)
            .await?;
        Ok(session_grant(access_token, &session, delegated))
    }

    /// Revokes the supplied current Chordrift bearer token.
    pub async fn revoke(&self, token: &str) -> Result<(), ClientError> {
        if !is_product_token(token) {
            return Err(ClientError::new(ErrorCode::AuthenticationRequired, false));
        }
        self.store
            .revoke_session(token_digest(token), self.clock.now())
            .await
    }

    fn new_session(
        &self,
        account_id: ResourceId,
    ) -> Result<(String, NewProductSession), ClientError> {
        let now = self.clock.now();
        let expires_at = self.policy.expires_at(now)?;
        let mut secret = [0_u8; TOKEN_BYTES];
        rand::rng().fill_bytes(&mut secret);
        let access_token = format!("{TOKEN_PREFIX}{}", URL_SAFE_NO_PAD.encode(secret));
        let session = NewProductSession {
            session_id: ResourceId::new(),
            account_id,
            token_sha256: token_digest(&access_token),
            created_at: now,
            expires_at,
        };
        Ok((access_token, session))
    }
}

fn session_grant(
    access_token: String,
    session: &NewProductSession,
    subject: AuthenticatedSubject,
) -> SessionGrant {
    SessionGrant {
        schema_version: PRODUCT_SESSION_SCHEMA_VERSION,
        token_type: "Bearer".to_owned(),
        access_token,
        session_id: session.session_id,
        subject_id: subject.subject_id,
        account_id: subject.account_id,
        expires_at: session.expires_at,
    }
}

/// Bearer authenticator backed by durable Chordrift product sessions.
pub struct ProductSessionAuthenticator<S> {
    store: Arc<S>,
    clock: Arc<dyn ServiceClock>,
}

impl<S> ProductSessionAuthenticator<S>
where
    S: ProductIdentityStore,
{
    /// Creates an authenticator using the production wall clock.
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store,
            clock: Arc::new(SystemServiceClock),
        }
    }

    /// Creates a deterministic authenticator for conformance tests.
    pub fn with_clock(store: Arc<S>, clock: Arc<dyn ServiceClock>) -> Self {
        Self { store, clock }
    }
}

#[async_trait]
impl<S> BearerAuthenticator for ProductSessionAuthenticator<S>
where
    S: ProductIdentityStore,
{
    async fn authenticate(&self, token: &str) -> Result<AuthenticatedSubject, ClientError> {
        if !is_product_token(token) {
            return Err(ClientError::new(ErrorCode::AuthenticationRequired, false));
        }
        self.store
            .authenticate_session(token_digest(token), self.clock.now())
            .await
    }
}

/// PostgreSQL implementation of the product identity/session repository.
#[derive(Clone)]
pub struct PostgresProductIdentityStore {
    pool: PgPool,
}

impl PostgresProductIdentityStore {
    /// Creates a repository over Chordrift's application-owned pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Verifies the hosted identity schema before accepting traffic.
    pub async fn verify_schema(&self) -> Result<(), ClientError> {
        let ready: bool = sqlx::query_scalar(
            "SELECT to_regclass('product_subjects') IS NOT NULL
                AND to_regclass('product_external_identities') IS NOT NULL
                AND to_regclass('chordrift_account_memberships') IS NOT NULL
                AND to_regclass('product_sessions') IS NOT NULL
                AND EXISTS (
                    SELECT 1 FROM information_schema.columns
                    WHERE table_schema = current_schema()
                      AND table_name = 'product_external_identities'
                      AND column_name = 'profile_verified_at'
                )",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|_| ClientError::new(ErrorCode::DependencyUnavailable, true))?;
        if ready {
            Ok(())
        } else {
            Err(ClientError::new(ErrorCode::DependencyUnavailable, false))
        }
    }

    /// Trusted bootstrap operation that binds the first active owner of an
    /// existing Chordrift account. This is intentionally not exposed as a
    /// public HTTP endpoint; registration policy belongs to the selected
    /// product identity provider and deployment.
    pub async fn provision_account_owner(
        &self,
        identity: &VerifiedExternalIdentity,
        account_id: ResourceId,
    ) -> Result<AuthenticatedSubject, ClientError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| ClientError::new(ErrorCode::DependencyUnavailable, true))?;
        let account_status: Option<String> =
            sqlx::query_scalar("SELECT status FROM chordrift_accounts WHERE id = $1 FOR UPDATE")
                .bind(account_id.as_uuid())
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|_| ClientError::new(ErrorCode::DependencyUnavailable, true))?;
        if account_status.as_deref() != Some("active") {
            return Err(ClientError::new(ErrorCode::PermissionDenied, false));
        }

        let existing_identity: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT product_subject_id, status
             FROM product_external_identities
             WHERE issuer = $1 AND external_subject = $2",
        )
        .bind(&identity.issuer)
        .bind(&identity.subject)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| ClientError::new(ErrorCode::DependencyUnavailable, true))?;
        let subject_id = match existing_identity {
            Some((subject_id, status)) if status == "active" => subject_id,
            Some(_) => return Err(ClientError::new(ErrorCode::PermissionDenied, false)),
            None => {
                let subject_id: Uuid =
                    sqlx::query_scalar("INSERT INTO product_subjects DEFAULT VALUES RETURNING id")
                        .fetch_one(&mut *transaction)
                        .await
                        .map_err(|_| ClientError::new(ErrorCode::DependencyUnavailable, true))?;
                sqlx::query(
                    "INSERT INTO product_external_identities
                     (issuer, external_subject, product_subject_id)
                     VALUES ($1, $2, $3)",
                )
                .bind(&identity.issuer)
                .bind(&identity.subject)
                .bind(subject_id)
                .execute(&mut *transaction)
                .await
                .map_err(|_| ClientError::new(ErrorCode::DependencyUnavailable, true))?;
                subject_id
            }
        };

        let active_owner: Option<Uuid> = sqlx::query_scalar(
            "SELECT product_subject_id FROM chordrift_account_memberships
             WHERE chordrift_account_id = $1 AND role = 'owner' AND status = 'active'",
        )
        .bind(account_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| ClientError::new(ErrorCode::DependencyUnavailable, true))?;
        if active_owner.is_some_and(|owner| owner != subject_id) {
            return Err(ClientError::new(ErrorCode::PermissionDenied, false));
        }
        sqlx::query(
            "INSERT INTO chordrift_account_memberships
             (chordrift_account_id, product_subject_id, role, status)
             VALUES ($1, $2, 'owner', 'active')
             ON CONFLICT (chordrift_account_id, product_subject_id)
             DO UPDATE SET role = 'owner', status = 'active', updated_at = now()",
        )
        .bind(account_id.as_uuid())
        .bind(subject_id)
        .execute(&mut *transaction)
        .await
        .map_err(|_| ClientError::new(ErrorCode::DependencyUnavailable, true))?;
        transaction
            .commit()
            .await
            .map_err(|_| ClientError::new(ErrorCode::DependencyUnavailable, true))?;
        Ok(AuthenticatedSubject {
            subject_id: ResourceId::from_uuid(subject_id),
            account_id,
        })
    }

    /// Refreshes non-authoritative presentation metadata for one active
    /// external identity after UserInfo verification succeeds.
    pub async fn update_external_profile(
        &self,
        identity: &VerifiedExternalIdentity,
        profile: &ExternalIdentityProfile,
    ) -> Result<(), ClientError> {
        let updated: Option<Uuid> = sqlx::query_scalar(
            "UPDATE product_external_identities
             SET display_name = $3,
                 avatar_url = $4,
                 profile_verified_at = now(),
                 updated_at = now()
             WHERE issuer = $1
               AND external_subject = $2
               AND status = 'active'
             RETURNING product_subject_id",
        )
        .bind(&identity.issuer)
        .bind(&identity.subject)
        .bind(&profile.display_name)
        .bind(&profile.avatar_url)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ClientError::new(ErrorCode::DependencyUnavailable, true))?;
        updated
            .map(|_| ())
            .ok_or_else(|| ClientError::new(ErrorCode::PermissionDenied, false))
    }

    /// Returns presentation metadata for an authenticated subject. Only an
    /// active identity can contribute profile fields.
    pub async fn subject_profile(
        &self,
        subject_id: ResourceId,
    ) -> Result<ExternalIdentityProfile, ClientError> {
        let profile: Option<(Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT display_name, avatar_url
             FROM product_external_identities
             WHERE product_subject_id = $1
               AND status = 'active'
             ORDER BY profile_verified_at DESC NULLS LAST, updated_at DESC
             LIMIT 1",
        )
        .bind(subject_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ClientError::new(ErrorCode::DependencyUnavailable, true))?;
        let (display_name, avatar_url) = profile.unwrap_or((None, None));
        Ok(ExternalIdentityProfile {
            display_name,
            avatar_url,
        })
    }
}

#[async_trait]
impl ProductIdentityStore for PostgresProductIdentityStore {
    async fn create_session(
        &self,
        identity: &VerifiedExternalIdentity,
        session: &NewProductSession,
    ) -> Result<AuthenticatedSubject, ClientError> {
        let subject_id: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO product_sessions
                (id, product_subject_id, chordrift_account_id, token_sha256,
                 created_at, expires_at)
             SELECT $1, identity.product_subject_id, membership.chordrift_account_id,
                    $2, $3, $4
             FROM product_external_identities identity
             JOIN product_subjects subject
               ON subject.id = identity.product_subject_id
             JOIN chordrift_account_memberships membership
               ON membership.product_subject_id = subject.id
             JOIN chordrift_accounts account
               ON account.id = membership.chordrift_account_id
             WHERE identity.issuer = $5
               AND identity.external_subject = $6
               AND identity.status = 'active'
               AND subject.status = 'active'
               AND membership.status = 'active'
               AND account.status = 'active'
               AND membership.chordrift_account_id = $7
             RETURNING product_subject_id",
        )
        .bind(session.session_id.as_uuid())
        .bind(session.token_sha256.as_slice())
        .bind(session.created_at)
        .bind(session.expires_at)
        .bind(&identity.issuer)
        .bind(&identity.subject)
        .bind(session.account_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ClientError::new(ErrorCode::DependencyUnavailable, true))?;
        let subject_id =
            subject_id.ok_or_else(|| ClientError::new(ErrorCode::PermissionDenied, false))?;
        Ok(AuthenticatedSubject {
            subject_id: ResourceId::from_uuid(subject_id),
            account_id: session.account_id,
        })
    }

    async fn create_delegated_session(
        &self,
        subject: AuthenticatedSubject,
        session: &NewProductSession,
    ) -> Result<AuthenticatedSubject, ClientError> {
        if session.account_id != subject.account_id {
            return Err(ClientError::new(ErrorCode::PermissionDenied, false));
        }
        let inserted: Option<(Uuid, Uuid)> = sqlx::query_as(
            "INSERT INTO product_sessions
                (id, product_subject_id, chordrift_account_id, token_sha256,
                 created_at, expires_at)
             SELECT $1, subject.id, membership.chordrift_account_id,
                    $2, $3, $4
             FROM product_subjects subject
             JOIN chordrift_account_memberships membership
               ON membership.product_subject_id = subject.id
             JOIN chordrift_accounts account
               ON account.id = membership.chordrift_account_id
             WHERE subject.id = $5
               AND membership.chordrift_account_id = $6
               AND subject.status = 'active'
               AND membership.status = 'active'
               AND account.status = 'active'
             RETURNING product_subject_id, chordrift_account_id",
        )
        .bind(session.session_id.as_uuid())
        .bind(session.token_sha256.as_slice())
        .bind(session.created_at)
        .bind(session.expires_at)
        .bind(subject.subject_id.as_uuid())
        .bind(subject.account_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ClientError::new(ErrorCode::DependencyUnavailable, true))?;
        let (subject_id, account_id) =
            inserted.ok_or_else(|| ClientError::new(ErrorCode::PermissionDenied, false))?;
        Ok(AuthenticatedSubject {
            subject_id: ResourceId::from_uuid(subject_id),
            account_id: ResourceId::from_uuid(account_id),
        })
    }

    async fn authenticate_session(
        &self,
        token_sha256: [u8; 32],
        now: DateTime<Utc>,
    ) -> Result<AuthenticatedSubject, ClientError> {
        let row: Option<(Uuid, Uuid)> = sqlx::query_as(
            "SELECT session.product_subject_id, session.chordrift_account_id
             FROM product_sessions session
             JOIN product_subjects subject ON subject.id = session.product_subject_id
             JOIN chordrift_account_memberships membership
               ON membership.chordrift_account_id = session.chordrift_account_id
              AND membership.product_subject_id = session.product_subject_id
             JOIN chordrift_accounts account ON account.id = session.chordrift_account_id
             WHERE session.token_sha256 = $1
               AND session.revoked_at IS NULL
               AND session.expires_at > $2
               AND subject.status = 'active'
               AND membership.status = 'active'
               AND account.status = 'active'",
        )
        .bind(token_sha256.as_slice())
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ClientError::new(ErrorCode::DependencyUnavailable, true))?;
        let (subject_id, account_id) =
            row.ok_or_else(|| ClientError::new(ErrorCode::AuthenticationRequired, false))?;
        Ok(AuthenticatedSubject {
            subject_id: ResourceId::from_uuid(subject_id),
            account_id: ResourceId::from_uuid(account_id),
        })
    }

    async fn revoke_session(
        &self,
        token_sha256: [u8; 32],
        now: DateTime<Utc>,
    ) -> Result<(), ClientError> {
        let revoked: Option<Uuid> = sqlx::query_scalar(
            "UPDATE product_sessions SET revoked_at = $2
             WHERE token_sha256 = $1 AND revoked_at IS NULL
             RETURNING id",
        )
        .bind(token_sha256.as_slice())
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ClientError::new(ErrorCode::DependencyUnavailable, true))?;
        revoked
            .map(|_| ())
            .ok_or_else(|| ClientError::new(ErrorCode::AuthenticationRequired, false))
    }
}

fn token_digest(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

fn is_product_token(token: &str) -> bool {
    token.starts_with(TOKEN_PREFIX) && token.len() == TOKEN_PREFIX.len() + 43
}
