//! Thin client transports for the versioned Chordrift application contract.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use tokio::sync::Mutex;
use url::Url;
use zeroize::Zeroizing;

use crate::{
    contract::{
        ClientCompatibility, ClientError, CommandReceipt, CommandRequest, NegotiatedCompatibility,
        QueryRequest, QueryResponse, ServiceCompatibility, negotiate,
    },
    service::{AuthenticatedSubject, ContractApplication},
};

/// Failure returned by a client transport without exposing response bodies or secrets.
#[derive(Debug, thiserror::Error)]
pub enum ClientTransportError {
    /// The service URL is invalid or is not HTTP(S).
    #[error("invalid Chordrift service URL")]
    InvalidUrl,
    /// The network exchange failed before a typed response was available.
    #[error("Chordrift service request failed")]
    Network(#[source] reqwest::Error),
    /// The service returned a stable client-safe application error.
    #[error("Chordrift service error: {}", error.message())]
    Service {
        /// Stable secret-free application failure.
        error: ClientError,
    },
    /// The service returned a non-contract response.
    #[error("Chordrift service returned an invalid contract response")]
    InvalidResponse,
    /// Local and service compatibility do not overlap.
    #[error("the Chordrift client and service are incompatible")]
    Incompatible,
}

/// Command/query/compatibility boundary shared by remote and local clients.
#[async_trait]
pub trait ClientTransport: Send + Sync {
    /// Negotiates one compatible contract and capability view.
    async fn negotiate(
        &self,
        offer: ClientCompatibility,
    ) -> Result<NegotiatedCompatibility, ClientTransportError>;
    /// Submits one typed idempotent command.
    async fn command(
        &self,
        request: CommandRequest,
    ) -> Result<CommandReceipt, ClientTransportError>;
    /// Submits one typed query.
    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, ClientTransportError>;
}

/// Authenticated HTTPS client used by installed CLI and future UI wrappers.
pub struct RemoteHttpClient {
    client: Client,
    base_url: Url,
    bearer: Zeroizing<String>,
}

impl RemoteHttpClient {
    /// Creates a remote client. Plain HTTP is accepted only for loopback testing.
    pub fn new(base_url: &str, bearer: String) -> Result<Self, ClientTransportError> {
        let base_url = Url::parse(base_url).map_err(|_| ClientTransportError::InvalidUrl)?;
        let loopback = base_url
            .host_str()
            .is_some_and(|host| host == "localhost" || host == "127.0.0.1" || host == "::1");
        if base_url.scheme() != "https" && !(base_url.scheme() == "http" && loopback) {
            return Err(ClientTransportError::InvalidUrl);
        }
        if bearer.is_empty() {
            return Err(ClientTransportError::InvalidResponse);
        }
        Ok(Self {
            client: Client::new(),
            base_url,
            bearer: Zeroizing::new(bearer),
        })
    }

    async fn post<I, O>(&self, path: &str, value: &I) -> Result<O, ClientTransportError>
    where
        I: serde::Serialize + Sync,
        O: serde::de::DeserializeOwned,
    {
        let url = self
            .base_url
            .join(path)
            .map_err(|_| ClientTransportError::InvalidUrl)?;
        let response = self
            .client
            .post(url)
            .bearer_auth(self.bearer.as_str())
            .json(value)
            .send()
            .await
            .map_err(ClientTransportError::Network)?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(ClientTransportError::Network)?;
        if status.is_success() {
            serde_json::from_slice(&bytes).map_err(|_| ClientTransportError::InvalidResponse)
        } else if is_contract_error_status(status) {
            let error = serde_json::from_slice::<ClientError>(&bytes)
                .map_err(|_| ClientTransportError::InvalidResponse)?;
            Err(ClientTransportError::Service { error })
        } else {
            Err(ClientTransportError::InvalidResponse)
        }
    }
}

fn is_contract_error_status(status: StatusCode) -> bool {
    status.is_client_error() || status.is_server_error()
}

#[async_trait]
impl ClientTransport for RemoteHttpClient {
    async fn negotiate(
        &self,
        offer: ClientCompatibility,
    ) -> Result<NegotiatedCompatibility, ClientTransportError> {
        self.post("v1/compatibility", &offer).await
    }
    async fn command(
        &self,
        request: CommandRequest,
    ) -> Result<CommandReceipt, ClientTransportError> {
        self.post("v1/commands", &request).await
    }
    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, ClientTransportError> {
        self.post("v1/queries", &request).await
    }
}

/// Explicit in-process development transport over the identical typed contract.
pub struct LocalDevelopmentClient {
    application: Arc<Mutex<Box<dyn ContractApplication>>>,
    subject: AuthenticatedSubject,
    compatibility: ServiceCompatibility,
}

impl LocalDevelopmentClient {
    /// Creates a local client. Production wrappers should use [`RemoteHttpClient`].
    pub fn new(
        application: Arc<Mutex<Box<dyn ContractApplication>>>,
        subject: AuthenticatedSubject,
        compatibility: ServiceCompatibility,
    ) -> Self {
        Self {
            application,
            subject,
            compatibility,
        }
    }
}

#[async_trait]
impl ClientTransport for LocalDevelopmentClient {
    async fn negotiate(
        &self,
        offer: ClientCompatibility,
    ) -> Result<NegotiatedCompatibility, ClientTransportError> {
        negotiate(&offer, &self.compatibility).map_err(|_| ClientTransportError::Incompatible)
    }
    async fn command(
        &self,
        request: CommandRequest,
    ) -> Result<CommandReceipt, ClientTransportError> {
        self.application
            .lock()
            .await
            .command(self.subject, request)
            .await
            .map_err(|error| ClientTransportError::Service { error })
    }
    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, ClientTransportError> {
        self.application
            .lock()
            .await
            .query(self.subject, request)
            .await
            .map_err(|error| ClientTransportError::Service { error })
    }
}
