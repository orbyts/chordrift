use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use axum::http::StatusCode;
use chordrift::{
    contract::{
        CONTRACT_VERSION, ClientError, CommandReceipt, CommandRequest, ErrorCode,
        OperationHistoryView, Query, QueryRequest, QueryResponse, RequestId, ResourceId, View,
    },
    http_transport::{AuthenticatedHttpTransport, BearerAuthenticator},
    identity::{
        ExternalIdentityVerifier, NewProductSession, ProductIdentityStore,
        ProductSessionAuthenticator, ProductSessionAuthority, ProductSessionHttpTransport,
        ProductSessionPolicy, SessionGrant, VerifiedExternalIdentity,
    },
    service::{AuthenticatedSubject, ContractApplication, ServiceClock},
};
use chrono::{DateTime, TimeDelta, Utc};
use tokio::{net::TcpListener, sync::Mutex as AsyncMutex};

#[derive(Clone)]
struct AdjustableClock(Arc<Mutex<DateTime<Utc>>>);

impl AdjustableClock {
    fn new(now: DateTime<Utc>) -> Self {
        Self(Arc::new(Mutex::new(now)))
    }

    fn advance(&self, delta: TimeDelta) {
        let mut now = self.0.lock().unwrap();
        *now = now.checked_add_signed(delta).unwrap();
    }
}

impl ServiceClock for AdjustableClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

struct FakeVerifier {
    identities: BTreeMap<String, VerifiedExternalIdentity>,
}

#[async_trait]
impl ExternalIdentityVerifier for FakeVerifier {
    async fn verify(&self, credential: &str) -> Result<VerifiedExternalIdentity, ClientError> {
        self.identities
            .get(credential)
            .cloned()
            .ok_or_else(|| ClientError::new(ErrorCode::AuthenticationRequired, false))
    }
}

#[derive(Clone)]
struct Binding {
    subject_id: ResourceId,
    account_id: ResourceId,
    identity_active: bool,
    subject_active: bool,
    membership_active: bool,
    account_active: bool,
}

#[derive(Clone)]
struct StoredSession {
    subject: AuthenticatedSubject,
    expires_at: DateTime<Utc>,
    revoked: bool,
}

#[derive(Default)]
struct MemoryState {
    bindings: BTreeMap<(String, String, ResourceId), Binding>,
    sessions: BTreeMap<[u8; 32], StoredSession>,
}

#[derive(Default)]
struct MemoryIdentityStore(Mutex<MemoryState>);

impl MemoryIdentityStore {
    fn bind(&self, issuer: &str, external: &str, binding: Binding) {
        self.0.lock().unwrap().bindings.insert(
            (issuer.to_owned(), external.to_owned(), binding.account_id),
            binding,
        );
    }

    fn set_membership_active(&self, account_id: ResourceId, active: bool) {
        for binding in self.0.lock().unwrap().bindings.values_mut() {
            if binding.account_id == account_id {
                binding.membership_active = active;
            }
        }
    }

    fn set_account_active(&self, account_id: ResourceId, active: bool) {
        for binding in self.0.lock().unwrap().bindings.values_mut() {
            if binding.account_id == account_id {
                binding.account_active = active;
            }
        }
    }

    fn set_subject_active(&self, subject_id: ResourceId, active: bool) {
        for binding in self.0.lock().unwrap().bindings.values_mut() {
            if binding.subject_id == subject_id {
                binding.subject_active = active;
            }
        }
    }

    fn set_identity_active(&self, subject_id: ResourceId, active: bool) {
        for binding in self.0.lock().unwrap().bindings.values_mut() {
            if binding.subject_id == subject_id {
                binding.identity_active = active;
            }
        }
    }
}

#[async_trait]
impl ProductIdentityStore for MemoryIdentityStore {
    async fn create_session(
        &self,
        identity: &VerifiedExternalIdentity,
        session: &NewProductSession,
    ) -> Result<AuthenticatedSubject, ClientError> {
        let mut state = self.0.lock().unwrap();
        let binding = state
            .bindings
            .get(&(
                identity.issuer.clone(),
                identity.subject.clone(),
                session.account_id,
            ))
            .filter(|binding| {
                binding.identity_active
                    && binding.subject_active
                    && binding.membership_active
                    && binding.account_active
            })
            .cloned()
            .ok_or_else(|| ClientError::new(ErrorCode::PermissionDenied, false))?;
        let subject = AuthenticatedSubject {
            subject_id: binding.subject_id,
            account_id: binding.account_id,
        };
        state.sessions.insert(
            session.token_sha256,
            StoredSession {
                subject,
                expires_at: session.expires_at,
                revoked: false,
            },
        );
        Ok(subject)
    }

    async fn authenticate_session(
        &self,
        token_sha256: [u8; 32],
        now: DateTime<Utc>,
    ) -> Result<AuthenticatedSubject, ClientError> {
        let state = self.0.lock().unwrap();
        let session = state
            .sessions
            .get(&token_sha256)
            .filter(|session| !session.revoked && session.expires_at > now)
            .ok_or_else(|| ClientError::new(ErrorCode::AuthenticationRequired, false))?;
        let binding_active = state.bindings.values().any(|binding| {
            binding.subject_id == session.subject.subject_id
                && binding.account_id == session.subject.account_id
                && binding.subject_active
                && binding.membership_active
                && binding.account_active
        });
        if !binding_active {
            return Err(ClientError::new(ErrorCode::AuthenticationRequired, false));
        }
        Ok(session.subject)
    }

    async fn revoke_session(
        &self,
        token_sha256: [u8; 32],
        _now: DateTime<Utc>,
    ) -> Result<(), ClientError> {
        let mut state = self.0.lock().unwrap();
        let session = state
            .sessions
            .get_mut(&token_sha256)
            .filter(|session| !session.revoked)
            .ok_or_else(|| ClientError::new(ErrorCode::AuthenticationRequired, false))?;
        session.revoked = true;
        Ok(())
    }
}

#[derive(Default)]
struct AuthorizationProbe {
    subjects: Arc<Mutex<Vec<AuthenticatedSubject>>>,
}

#[async_trait]
impl ContractApplication for AuthorizationProbe {
    async fn command(
        &mut self,
        _subject: AuthenticatedSubject,
        _request: CommandRequest,
    ) -> Result<CommandReceipt, ClientError> {
        Err(ClientError::new(ErrorCode::InvalidRequest, false))
    }

    async fn query(
        &mut self,
        subject: AuthenticatedSubject,
        request: QueryRequest,
    ) -> Result<QueryResponse, ClientError> {
        self.subjects.lock().unwrap().push(subject);
        match request.query {
            Query::OperationHistory { account_id } if account_id == subject.account_id => {
                Ok(QueryResponse::OperationHistory(View {
                    contract_version: CONTRACT_VERSION,
                    request_id: request.request_id,
                    generated_at: Utc::now(),
                    value: OperationHistoryView {
                        operations: Vec::new(),
                    },
                }))
            }
            Query::OperationHistory { .. } => {
                Err(ClientError::new(ErrorCode::PermissionDenied, false))
            }
            _ => Err(ClientError::new(ErrorCode::InvalidRequest, false)),
        }
    }
}

struct Server {
    base_url: String,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn query(account_id: ResourceId) -> QueryRequest {
    QueryRequest {
        contract_version: CONTRACT_VERSION,
        request_id: RequestId::new(),
        query: Query::OperationHistory { account_id },
    }
}

#[tokio::test]
async fn real_http_exchanges_uses_and_revokes_a_persisted_product_session() {
    let now = DateTime::parse_from_rfc3339("2026-08-30T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let clock = Arc::new(AdjustableClock::new(now));
    let store = Arc::new(MemoryIdentityStore::default());
    let subject_id = ResourceId::new();
    let account_id = ResourceId::new();
    let other_account = ResourceId::new();
    store.bind(
        "https://identity.example",
        "person-a",
        Binding {
            subject_id,
            account_id,
            identity_active: true,
            subject_active: true,
            membership_active: true,
            account_active: true,
        },
    );
    let verifier = Arc::new(FakeVerifier {
        identities: BTreeMap::from([(
            "upstream-a".to_owned(),
            VerifiedExternalIdentity::new("https://identity.example", "person-a").unwrap(),
        )]),
    });
    let authority = Arc::new(ProductSessionAuthority::with_clock_and_policy(
        verifier,
        store.clone(),
        clock.clone(),
        ProductSessionPolicy::new(Duration::from_secs(3600)).unwrap(),
    ));
    let authenticator: Arc<dyn BearerAuthenticator> = Arc::new(
        ProductSessionAuthenticator::with_clock(store.clone(), clock.clone()),
    );
    let probe = AuthorizationProbe::default();
    let observed_subjects = probe.subjects.clone();
    let application: Arc<AsyncMutex<Box<dyn ContractApplication>>> =
        Arc::new(AsyncMutex::new(Box::new(probe)));
    let router = AuthenticatedHttpTransport::new(application, authenticator)
        .router()
        .merge(ProductSessionHttpTransport::new(authority).router());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let server = Server {
        base_url: format!("http://{address}"),
        task,
    };
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/v1/sessions", server.base_url))
        .bearer_auth("upstream-a")
        .json(&serde_json::json!({
            "schema_version": 1,
            "account_id": account_id,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let grant: SessionGrant = response.json().await.unwrap();
    assert_eq!(grant.schema_version, 1);
    assert_eq!(grant.token_type, "Bearer");
    assert!(grant.access_token.starts_with("chd_session_"));
    assert_eq!(grant.subject_id, subject_id);
    assert_eq!(grant.account_id, account_id);
    assert_eq!(store.0.lock().unwrap().sessions.len(), 1);

    let second_response = client
        .post(format!("{}/v1/sessions", server.base_url))
        .bearer_auth("upstream-a")
        .json(&serde_json::json!({
            "schema_version": 1,
            "account_id": account_id,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(second_response.status(), StatusCode::CREATED);
    let second_grant: SessionGrant = second_response.json().await.unwrap();
    assert_ne!(second_grant.access_token, grant.access_token);
    assert_eq!(store.0.lock().unwrap().sessions.len(), 2);

    let allowed = client
        .post(format!("{}/v1/queries", server.base_url))
        .bearer_auth(&grant.access_token)
        .json(&query(account_id))
        .send()
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);
    assert_eq!(
        observed_subjects.lock().unwrap().as_slice(),
        &[AuthenticatedSubject {
            subject_id,
            account_id,
        }]
    );

    let cross_account = client
        .post(format!("{}/v1/queries", server.base_url))
        .bearer_auth(&grant.access_token)
        .json(&query(other_account))
        .send()
        .await
        .unwrap();
    assert_eq!(cross_account.status(), StatusCode::FORBIDDEN);

    let revoked = client
        .delete(format!("{}/v1/sessions/current", server.base_url))
        .bearer_auth(&grant.access_token)
        .send()
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);
    let after_revoke = client
        .post(format!("{}/v1/queries", server.base_url))
        .bearer_auth(&grant.access_token)
        .json(&query(account_id))
        .send()
        .await
        .unwrap();
    assert_eq!(after_revoke.status(), StatusCode::UNAUTHORIZED);
    let other_session_survives = client
        .post(format!("{}/v1/queries", server.base_url))
        .bearer_auth(&second_grant.access_token)
        .json(&query(account_id))
        .send()
        .await
        .unwrap();
    assert_eq!(other_session_survives.status(), StatusCode::OK);
}

#[tokio::test]
async fn tenant_matrix_fails_closed_for_wrong_unknown_expired_and_revoked_authority() {
    let now = DateTime::parse_from_rfc3339("2026-08-30T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let clock = Arc::new(AdjustableClock::new(now));
    let store = Arc::new(MemoryIdentityStore::default());
    let account_a = ResourceId::new();
    let account_b = ResourceId::new();
    let subject_a = ResourceId::new();
    for (external, subject_id, account_id) in [
        ("person-a", subject_a, account_a),
        ("person-b", ResourceId::new(), account_b),
    ] {
        store.bind(
            "issuer",
            external,
            Binding {
                subject_id,
                account_id,
                identity_active: true,
                subject_active: true,
                membership_active: true,
                account_active: true,
            },
        );
    }
    let verifier = Arc::new(FakeVerifier {
        identities: BTreeMap::from([
            (
                "credential-a".to_owned(),
                VerifiedExternalIdentity::new("issuer", "person-a").unwrap(),
            ),
            (
                "credential-b".to_owned(),
                VerifiedExternalIdentity::new("issuer", "person-b").unwrap(),
            ),
        ]),
    });
    let authority = ProductSessionAuthority::with_clock_and_policy(
        verifier,
        store.clone(),
        clock.clone(),
        ProductSessionPolicy::new(Duration::from_secs(60)).unwrap(),
    );
    let incompatible = authority
        .exchange(
            "credential-a",
            chordrift::identity::SessionExchangeRequest {
                schema_version: 99,
                account_id: account_a,
            },
        )
        .await;
    assert!(matches!(
        incompatible,
        Err(ClientError {
            code: ErrorCode::IncompatibleContract,
            ..
        })
    ));
    let wrong_account = authority
        .exchange(
            "credential-a",
            chordrift::identity::SessionExchangeRequest {
                schema_version: 1,
                account_id: account_b,
            },
        )
        .await;
    let wrong_account = match wrong_account {
        Ok(_) => panic!("one tenant cannot enter another tenant"),
        Err(error) => error,
    };
    assert_eq!(wrong_account.code, ErrorCode::PermissionDenied);

    let grant = authority
        .exchange(
            "credential-a",
            chordrift::identity::SessionExchangeRequest {
                schema_version: 1,
                account_id: account_a,
            },
        )
        .await
        .unwrap();
    let authenticator = ProductSessionAuthenticator::with_clock(store.clone(), clock.clone());
    assert_eq!(
        authenticator
            .authenticate(&grant.access_token)
            .await
            .unwrap(),
        AuthenticatedSubject {
            subject_id: subject_a,
            account_id: account_a,
        }
    );
    let guessed = authenticator
        .authenticate("chd_session_not-a-real-token")
        .await
        .expect_err("a guessed token fails");
    assert_eq!(guessed.code, ErrorCode::AuthenticationRequired);

    store.set_membership_active(account_a, false);
    assert_eq!(
        authenticator
            .authenticate(&grant.access_token)
            .await
            .expect_err("membership revocation is immediate")
            .code,
        ErrorCode::AuthenticationRequired
    );
    store.set_membership_active(account_a, true);
    store.set_account_active(account_a, false);
    assert_eq!(
        authenticator
            .authenticate(&grant.access_token)
            .await
            .expect_err("account suspension is immediate")
            .code,
        ErrorCode::AuthenticationRequired
    );
    store.set_account_active(account_a, true);
    store.set_subject_active(subject_a, false);
    assert_eq!(
        authenticator
            .authenticate(&grant.access_token)
            .await
            .expect_err("subject suspension is immediate")
            .code,
        ErrorCode::AuthenticationRequired
    );
    store.set_subject_active(subject_a, true);
    store.set_identity_active(subject_a, false);
    let revoked_identity = authority
        .exchange(
            "credential-a",
            chordrift::identity::SessionExchangeRequest {
                schema_version: 1,
                account_id: account_a,
            },
        )
        .await;
    assert!(matches!(
        revoked_identity,
        Err(ClientError {
            code: ErrorCode::PermissionDenied,
            ..
        })
    ));
    store.set_identity_active(subject_a, true);
    clock.advance(TimeDelta::seconds(61));
    assert_eq!(
        authenticator
            .authenticate(&grant.access_token)
            .await
            .expect_err("expired session fails")
            .code,
        ErrorCode::AuthenticationRequired
    );
}
