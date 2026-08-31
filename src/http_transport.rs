//! Authenticated HTTP adapter for the transport-neutral application contract.
//!
//! This adapter accepts typed command/query envelopes only. It intentionally
//! has no route that executes a CLI command, shell script, SQL statement, or
//! provider request supplied by a client.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    response::{IntoResponse, Response},
    routing::post,
};
use tokio::sync::Mutex;

use crate::{
    contract::{
        CONTRACT_VERSION, ClientCompatibility, ClientError, CommandRequest, ContractVersionRange,
        ErrorCategory, ErrorCode, QueryRequest, ServiceCompatibility, negotiate,
    },
    service::{AuthenticatedSubject, ContractApplication},
};

/// Maximum accepted command or query envelope size.
pub const MAX_CONTRACT_BODY_BYTES: usize = 1024 * 1024;

/// Authentication seam implemented by V021-02 product sessions later.
#[async_trait]
pub trait BearerAuthenticator: Send + Sync {
    /// Resolves one opaque bearer credential to client-safe subject context.
    async fn authenticate(&self, token: &str) -> Result<AuthenticatedSubject, ClientError>;
}

/// Deployment-supplied request-budget check applied after authentication.
pub trait HttpRequestGate: Send + Sync {
    /// Accepts the request or returns a client-safe rate-limit error.
    fn check(&self, subject: AuthenticatedSubject) -> Result<(), ClientError>;
}

#[derive(Clone, Copy, Debug, Default)]
struct AllowAllRequestGate;

impl HttpRequestGate for AllowAllRequestGate {
    fn check(&self, _subject: AuthenticatedSubject) -> Result<(), ClientError> {
        Ok(())
    }
}

#[derive(Clone)]
struct HttpState {
    application: Arc<Mutex<Box<dyn ContractApplication>>>,
    authenticator: Arc<dyn BearerAuthenticator>,
    request_gate: Arc<dyn HttpRequestGate>,
    compatibility: ServiceCompatibility,
}

/// Builds an authenticated HTTP router over one Rust application authority.
#[derive(Clone)]
pub struct AuthenticatedHttpTransport {
    state: HttpState,
}

impl AuthenticatedHttpTransport {
    /// Creates a transport around one shared application service.
    pub fn new(
        application: Arc<Mutex<Box<dyn ContractApplication>>>,
        authenticator: Arc<dyn BearerAuthenticator>,
    ) -> Self {
        Self {
            state: HttpState {
                application,
                authenticator,
                request_gate: Arc::new(AllowAllRequestGate),
                compatibility: ServiceCompatibility {
                    contract_versions: ContractVersionRange::exact(CONTRACT_VERSION),
                    schema_version: 0,
                    features: Default::default(),
                    provider_capabilities: Default::default(),
                    evidence_capabilities: Default::default(),
                },
            },
        }
    }

    /// Replaces the permissive development gate with a deployment policy.
    #[must_use]
    pub fn with_request_gate(mut self, request_gate: Arc<dyn HttpRequestGate>) -> Self {
        self.state.request_gate = request_gate;
        self
    }

    /// Declares the exact compatibility and capability view of this deployment.
    #[must_use]
    pub fn with_compatibility(mut self, compatibility: ServiceCompatibility) -> Self {
        self.state.compatibility = compatibility;
        self
    }

    /// Returns routes suitable for a loopback test server or later deployment.
    pub fn router(self) -> Router {
        Router::new()
            .route("/v1/compatibility", post(compatibility))
            .route("/v1/commands", post(command))
            .route("/v1/queries", post(query))
            .with_state(self.state)
    }
}

async fn compatibility(State(state): State<HttpState>, request: Request) -> Response {
    let subject = match authenticate(&state, request.headers()).await {
        Ok(subject) => subject,
        Err(error) => return error_response(error),
    };
    if let Err(error) = state.request_gate.check(subject) {
        return error_response(error);
    }
    let body = match to_bytes(request.into_body(), MAX_CONTRACT_BODY_BYTES).await {
        Ok(body) => body,
        Err(_) => return error_response(ClientError::new(ErrorCode::InvalidRequest, false)),
    };
    let offer = match serde_json::from_slice::<ClientCompatibility>(&body) {
        Ok(offer) => offer,
        Err(_) => return error_response(ClientError::new(ErrorCode::InvalidRequest, false)),
    };
    match negotiate(&offer, &state.compatibility) {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(_) => error_response(ClientError::new(ErrorCode::IncompatibleContract, false)),
    }
}

async fn command(State(state): State<HttpState>, request: Request) -> Response {
    let subject = match authenticate(&state, request.headers()).await {
        Ok(subject) => subject,
        Err(error) => return error_response(error),
    };
    if let Err(error) = state.request_gate.check(subject) {
        return error_response(error);
    }
    let body = match to_bytes(request.into_body(), MAX_CONTRACT_BODY_BYTES).await {
        Ok(body) => body,
        Err(_) => return error_response(ClientError::new(ErrorCode::InvalidRequest, false)),
    };
    let request = match serde_json::from_slice::<CommandRequest>(&body) {
        Ok(request) => request,
        Err(_) => return error_response(ClientError::new(ErrorCode::InvalidRequest, false)),
    };
    let outcome = state
        .application
        .lock()
        .await
        .command(subject, request)
        .await;
    match outcome {
        Ok(receipt) => (StatusCode::ACCEPTED, Json(receipt)).into_response(),
        Err(error) => error_response(error),
    }
}

async fn query(State(state): State<HttpState>, request: Request) -> Response {
    let subject = match authenticate(&state, request.headers()).await {
        Ok(subject) => subject,
        Err(error) => return error_response(error),
    };
    if let Err(error) = state.request_gate.check(subject) {
        return error_response(error);
    }
    let body = match to_bytes(request.into_body(), MAX_CONTRACT_BODY_BYTES).await {
        Ok(body) => body,
        Err(_) => return error_response(ClientError::new(ErrorCode::InvalidRequest, false)),
    };
    let request = match serde_json::from_slice::<QueryRequest>(&body) {
        Ok(request) => request,
        Err(_) => return error_response(ClientError::new(ErrorCode::InvalidRequest, false)),
    };
    let outcome = state.application.lock().await.query(subject, request).await;
    match outcome {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => error_response(error),
    }
}

async fn authenticate(
    state: &HttpState,
    headers: &HeaderMap,
) -> Result<AuthenticatedSubject, ClientError> {
    let value = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ClientError::new(ErrorCode::AuthenticationRequired, false))?;
    let token = value
        .strip_prefix("Bearer ")
        .filter(|token| !token.is_empty())
        .ok_or_else(|| ClientError::new(ErrorCode::AuthenticationRequired, false))?;
    state.authenticator.authenticate(token).await
}

pub(crate) fn error_response(error: ClientError) -> Response {
    let status = match error.code.category() {
        ErrorCategory::InvalidRequest => StatusCode::BAD_REQUEST,
        ErrorCategory::Authorization => {
            if error.code == ErrorCode::AuthenticationRequired {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::FORBIDDEN
            }
        }
        ErrorCategory::NotFound => StatusCode::NOT_FOUND,
        ErrorCategory::Conflict => StatusCode::CONFLICT,
        ErrorCategory::Unsupported => StatusCode::UNPROCESSABLE_ENTITY,
        ErrorCategory::Unavailable => {
            if error.code == ErrorCode::RateLimited {
                StatusCode::TOO_MANY_REQUESTS
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            }
        }
        ErrorCategory::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(error)).into_response()
}
