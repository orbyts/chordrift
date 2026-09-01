use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chordrift::{
    client_transport::{ClientTransport, LocalDevelopmentClient, RemoteHttpClient},
    contract::{
        CONTRACT_VERSION, CancellationRequest, ClientCompatibility, ClientError, Command,
        CommandReceipt, CommandRequest, ContractVersion, ContractVersionRange, ErrorCode,
        IdempotencyKey, LibraryComparisonStatus, LibraryComparisonView,
        LibraryPlaylistComparisonView, MaintenanceChangeId, MaintenanceChangeKind,
        MaintenanceChangeView, MaintenanceDecision, MaintenanceProviderEffectKind,
        MaintenanceProviderEffectView, MaintenanceResolution, MaintenanceSessionId,
        MaintenanceSessionState, MaintenanceSessionView, MaintenanceSurfaceView,
        MaintenanceTrackView, OperationEventsView, OperationState, Query, QueryRequest,
        QueryResponse, RequestId, ResourceId, SchemaVersionRange, ServiceCompatibility,
        WaitingReason,
    },
    http_transport::{AuthenticatedHttpTransport, BearerAuthenticator, HttpRequestGate},
    maintenance::{MaintenanceDecisionProjection, MaintenanceProjection},
    service::{
        AuthenticatedSubject, ContractApplication, MaintenanceApplication, MaintenanceBackend,
        ServiceClock,
    },
};
use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use tokio::{net::TcpListener, sync::Mutex as AsyncMutex, task::JoinHandle};

#[derive(Clone)]
struct FixedClock(DateTime<Utc>);

impl ServiceClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

#[derive(Clone)]
struct Fixture {
    owner: AuthenticatedSubject,
    stranger: AuthenticatedSubject,
    provider_connection_id: ResourceId,
    session_id: MaintenanceSessionId,
    change_id: MaintenanceChangeId,
    initial: MaintenanceProjection,
    applied: MaintenanceProjection,
}

impl Fixture {
    fn new() -> Self {
        let owner = AuthenticatedSubject {
            subject_id: ResourceId::new(),
            account_id: ResourceId::new(),
        };
        let track = MaintenanceTrackView {
            track_id: ResourceId::new(),
            title: "Fixture Song".to_owned(),
            artists: vec!["Fixture Artist".to_owned()],
        };
        let surface = MaintenanceSurfaceView {
            surface_id: ResourceId::new(),
            name: "Old Vibe".to_owned(),
        };
        let change_id = MaintenanceChangeId::new();
        Self {
            owner,
            stranger: AuthenticatedSubject {
                subject_id: ResourceId::new(),
                account_id: ResourceId::new(),
            },
            provider_connection_id: ResourceId::new(),
            session_id: MaintenanceSessionId::new(),
            change_id,
            initial: MaintenanceProjection {
                provider_snapshot_id: ResourceId::new(),
                observed_changes: vec![MaintenanceChangeView {
                    change_id,
                    kind: MaintenanceChangeKind::Removal,
                    track: Some(track.clone()),
                    previous_surface: Some(surface.clone()),
                    current_surface: None,
                    summary: "Removed Fixture Song from Old Vibe".to_owned(),
                    resolution: None,
                    recommended_resolution: None,
                    recommendation_reason: None,
                }],
                provider_effects: vec![MaintenanceProviderEffectView {
                    effect_id: ResourceId::new(),
                    kind: MaintenanceProviderEffectKind::RemoveTrack,
                    track: Some(track),
                    surface: Some(surface),
                    summary: "Remove Fixture Song from Old Vibe".to_owned(),
                }],
                review_id: None,
            },
            applied: MaintenanceProjection {
                provider_snapshot_id: ResourceId::new(),
                observed_changes: Vec::new(),
                provider_effects: Vec::new(),
                review_id: None,
            },
        }
    }
}

struct FakeBackend {
    owner: AuthenticatedSubject,
    provider_connection_id: ResourceId,
    observations: VecDeque<MaintenanceProjection>,
    applied: MaintenanceProjection,
    observe_error: Option<ClientError>,
    trace: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl MaintenanceBackend for FakeBackend {
    async fn owns_provider_connection(
        &self,
        subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
    ) -> bool {
        subject == self.owner && provider_connection_id == self.provider_connection_id
    }

    async fn library_comparison(
        &mut self,
        _subject: AuthenticatedSubject,
        _provider_connection_id: ResourceId,
    ) -> Result<LibraryComparisonView, ClientError> {
        Ok(LibraryComparisonView {
            provider_state_at: None,
            chordrift_state_at: None,
            aligned_playlists: 0,
            differing_playlists: 1,
            playlists: vec![LibraryPlaylistComparisonView {
                provider_playlist_id: Some("playlist-a".to_owned()),
                chordrift_playlist_id: Some("model-a".to_owned()),
                name: "Fixture".to_owned(),
                provider_track_count: 5,
                chordrift_track_count: 4,
                provider_unresolved_track_count: 0,
                chordrift_unresolved_track_count: 0,
                provider_only_track_count: 1,
                chordrift_only_track_count: 0,
                shared_track_count: 4,
                order_matches: None,
                status: LibraryComparisonStatus::MembershipDiffers,
                explanation: "1 provider-only and 0 Chordrift-only membership(s).".to_owned(),
            }],
        })
    }

    async fn observe(
        &mut self,
        _subject: AuthenticatedSubject,
        _provider_connection_id: ResourceId,
        _current: Option<&MaintenanceSessionView>,
    ) -> Result<MaintenanceProjection, ClientError> {
        self.trace
            .lock()
            .unwrap()
            .push("observe_provider".to_owned());
        if let Some(error) = self.observe_error {
            return Err(error);
        }
        self.observations
            .pop_front()
            .ok_or_else(|| ClientError::new(ErrorCode::DependencyUnavailable, true))
    }

    async fn record_decisions(
        &mut self,
        _subject: AuthenticatedSubject,
        view: &MaintenanceSessionView,
    ) -> Result<MaintenanceDecisionProjection, ClientError> {
        self.trace
            .lock()
            .unwrap()
            .push("record_decisions".to_owned());
        Ok(MaintenanceDecisionProjection {
            provider_effects: view.provider_effects.clone(),
            review_id: view.review_id,
        })
    }

    async fn apply(
        &mut self,
        _subject: AuthenticatedSubject,
        _view: &MaintenanceSessionView,
    ) -> Result<MaintenanceProjection, ClientError> {
        self.trace.lock().unwrap().push("apply_provider".to_owned());
        Ok(self.applied.clone())
    }
}

fn application(fixture: &Fixture) -> (SharedApplication, Arc<Mutex<Vec<String>>>) {
    application_with_observations(fixture, VecDeque::from([fixture.initial.clone()]))
}

fn application_with_observations(
    fixture: &Fixture,
    observations: VecDeque<MaintenanceProjection>,
) -> (SharedApplication, Arc<Mutex<Vec<String>>>) {
    let trace = Arc::new(Mutex::new(Vec::new()));
    let backend = FakeBackend {
        owner: fixture.owner,
        provider_connection_id: fixture.provider_connection_id,
        observations,
        applied: fixture.applied.clone(),
        observe_error: None,
        trace: trace.clone(),
    };
    let clock: Arc<dyn ServiceClock> = Arc::new(FixedClock(
        "2026-08-30T00:00:00Z".parse().expect("fixed time"),
    ));
    let application: Box<dyn ContractApplication> =
        Box::new(MaintenanceApplication::with_clock(backend, clock));
    (Arc::new(AsyncMutex::new(application)), trace)
}

type SharedApplication = Arc<AsyncMutex<Box<dyn ContractApplication>>>;

#[derive(Clone)]
struct TestAuthenticator {
    owner: AuthenticatedSubject,
    stranger: AuthenticatedSubject,
}

#[async_trait]
impl BearerAuthenticator for TestAuthenticator {
    async fn authenticate(&self, token: &str) -> Result<AuthenticatedSubject, ClientError> {
        match token {
            "owner-token" => Ok(self.owner),
            "stranger-token" => Ok(self.stranger),
            _ => Err(ClientError::new(ErrorCode::AuthenticationRequired, false)),
        }
    }
}

struct RejectAllRequests;

impl HttpRequestGate for RejectAllRequests {
    fn check(&self, _subject: AuthenticatedSubject) -> Result<(), ClientError> {
        let mut error = ClientError::new(ErrorCode::RateLimited, true);
        error.retry_after_seconds = Some(2);
        Err(error)
    }
}

#[derive(Clone, Debug)]
struct TransportFailure {
    status: Option<StatusCode>,
    error: ClientError,
}

trait TestTransport {
    async fn command(&self, request: CommandRequest) -> Result<CommandReceipt, TransportFailure>;

    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, TransportFailure>;
}

#[derive(Clone)]
struct InProcessTransport {
    application: SharedApplication,
    subject: AuthenticatedSubject,
}

impl TestTransport for InProcessTransport {
    async fn command(&self, request: CommandRequest) -> Result<CommandReceipt, TransportFailure> {
        self.application
            .lock()
            .await
            .command(self.subject, request)
            .await
            .map_err(|error| TransportFailure {
                status: None,
                error,
            })
    }

    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, TransportFailure> {
        self.application
            .lock()
            .await
            .query(self.subject, request)
            .await
            .map_err(|error| TransportFailure {
                status: None,
                error,
            })
    }
}

#[derive(Clone)]
struct HttpClientTransport {
    client: reqwest::Client,
    base_url: String,
    token: &'static str,
}

impl TestTransport for HttpClientTransport {
    async fn command(&self, request: CommandRequest) -> Result<CommandReceipt, TransportFailure> {
        let response = self
            .client
            .post(format!("{}/v1/commands", self.base_url))
            .bearer_auth(self.token)
            .json(&request)
            .send()
            .await
            .expect("HTTP command completes");
        let status = response.status();
        if status.is_success() {
            Ok(response.json().await.expect("command receipt JSON"))
        } else {
            Err(TransportFailure {
                status: Some(status),
                error: response.json().await.expect("client error JSON"),
            })
        }
    }

    async fn query(&self, request: QueryRequest) -> Result<QueryResponse, TransportFailure> {
        let response = self
            .client
            .post(format!("{}/v1/queries", self.base_url))
            .bearer_auth(self.token)
            .json(&request)
            .send()
            .await
            .expect("HTTP query completes");
        let status = response.status();
        if status.is_success() {
            Ok(response.json().await.expect("query response JSON"))
        } else {
            Err(TransportFailure {
                status: Some(status),
                error: response.json().await.expect("client error JSON"),
            })
        }
    }
}

struct TestServer {
    base_url: String,
    task: JoinHandle<()>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn server(fixture: &Fixture, application: SharedApplication) -> TestServer {
    server_with_transport(
        fixture,
        AuthenticatedHttpTransport::new(
            application,
            Arc::new(TestAuthenticator {
                owner: fixture.owner,
                stranger: fixture.stranger,
            }),
        ),
    )
    .await
}

async fn server_with_transport(
    _fixture: &Fixture,
    transport: AuthenticatedHttpTransport,
) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("loopback listener");
    let address = listener.local_addr().expect("listener address");
    let task = tokio::spawn(async move {
        axum::serve(listener, transport.router())
            .await
            .expect("test HTTP server");
    });
    TestServer {
        base_url: format!("http://{address}"),
        task,
    }
}

fn service_compatibility() -> ServiceCompatibility {
    ServiceCompatibility {
        contract_versions: ContractVersionRange::exact(CONTRACT_VERSION),
        schema_version: 50,
        features: Default::default(),
        provider_capabilities: Default::default(),
        evidence_capabilities: Default::default(),
    }
}

fn client_offer() -> ClientCompatibility {
    ClientCompatibility {
        contract_versions: ContractVersionRange::exact(CONTRACT_VERSION),
        schema_versions: SchemaVersionRange::new(47, 50).expect("valid schema range"),
        requested_features: Vec::new(),
    }
}

#[tokio::test]
async fn shipped_remote_client_and_explicit_local_transport_share_the_contract() {
    let fixture = Fixture::new();
    let (remote_application, _) = application(&fixture);
    let server = server_with_transport(
        &fixture,
        AuthenticatedHttpTransport::new(
            remote_application,
            Arc::new(TestAuthenticator {
                owner: fixture.owner,
                stranger: fixture.stranger,
            }),
        )
        .with_compatibility(service_compatibility()),
    )
    .await;
    let remote = RemoteHttpClient::new(&server.base_url, "owner-token".to_owned())
        .expect("loopback remote client");
    let negotiated = remote
        .negotiate(client_offer())
        .await
        .expect("remote compatibility");
    assert_eq!(negotiated.contract_version, CONTRACT_VERSION);
    assert_eq!(negotiated.schema_version, 50);

    let remote_receipt = remote
        .command(command_request(
            Command::StartMaintenance {
                session_id: fixture.session_id,
                provider_connection_id: fixture.provider_connection_id,
            },
            IdempotencyKey::new(),
        ))
        .await
        .expect("remote command");
    let remote_view = remote
        .query(query_request(Query::Operation {
            operation_id: remote_receipt.operation_id,
        }))
        .await
        .expect("remote query");

    let local_fixture = Fixture::new();
    let (local_application, _) = application(&local_fixture);
    let local = LocalDevelopmentClient::new(
        local_application,
        local_fixture.owner,
        service_compatibility(),
    );
    assert_eq!(
        local
            .negotiate(client_offer())
            .await
            .expect("local compatibility"),
        negotiated
    );
    let local_receipt = local
        .command(command_request(
            Command::StartMaintenance {
                session_id: local_fixture.session_id,
                provider_connection_id: local_fixture.provider_connection_id,
            },
            IdempotencyKey::new(),
        ))
        .await
        .expect("local command");
    let local_view = local
        .query(query_request(Query::Operation {
            operation_id: local_receipt.operation_id,
        }))
        .await
        .expect("local query");
    assert_eq!(
        serde_json::to_value(remote_view).expect("remote JSON")["type"],
        serde_json::to_value(local_view).expect("local JSON")["type"]
    );
    let remote_comparison = remote
        .query(query_request(Query::LibraryComparison {
            provider_connection_id: fixture.provider_connection_id,
        }))
        .await
        .expect("remote comparison");
    let local_comparison = local
        .query(query_request(Query::LibraryComparison {
            provider_connection_id: local_fixture.provider_connection_id,
        }))
        .await
        .expect("local comparison");
    assert_eq!(
        serde_json::to_value(remote_comparison).expect("remote comparison JSON")["view"]["value"],
        serde_json::to_value(local_comparison).expect("local comparison JSON")["view"]["value"]
    );

    assert!(RemoteHttpClient::new("http://example.com", "secret".to_owned()).is_err());
}

fn command_request(command: Command, key: IdempotencyKey) -> CommandRequest {
    CommandRequest {
        contract_version: CONTRACT_VERSION,
        request_id: RequestId::new(),
        idempotency_key: key,
        command,
    }
}

fn query_request(query: Query) -> QueryRequest {
    QueryRequest {
        contract_version: CONTRACT_VERSION,
        request_id: RequestId::new(),
        query,
    }
}

fn maintenance(response: QueryResponse) -> MaintenanceSessionView {
    let QueryResponse::MaintenanceSession(view) = response else {
        panic!("expected maintenance session");
    };
    view.value
}

fn events(response: QueryResponse) -> OperationEventsView {
    let QueryResponse::OperationEvents(view) = response else {
        panic!("expected operation events");
    };
    view.value
}

#[derive(Debug, Eq, PartialEq)]
struct ScenarioResult {
    initial_state: MaintenanceSessionState,
    ready_state: MaintenanceSessionState,
    final_state: MaintenanceSessionState,
    start_event_states: Vec<OperationState>,
}

async fn exercise_full_flow(transport: &impl TestTransport, fixture: &Fixture) -> ScenarioResult {
    let start_key = IdempotencyKey::new();
    let start = command_request(
        Command::StartMaintenance {
            session_id: fixture.session_id,
            provider_connection_id: fixture.provider_connection_id,
        },
        start_key,
    );
    let receipt = transport
        .command(start.clone())
        .await
        .expect("start succeeds");
    let replay = transport
        .command(start)
        .await
        .expect("idempotent replay succeeds");
    assert_eq!(replay, receipt);

    let start_events = events(
        transport
            .query(query_request(Query::OperationEvents {
                operation_id: receipt.operation_id,
                after_sequence: None,
            }))
            .await
            .expect("events query succeeds"),
    );
    assert_eq!(
        start_events
            .events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );

    let initial = maintenance(
        transport
            .query(query_request(Query::MaintenanceSession {
                session_id: fixture.session_id,
            }))
            .await
            .expect("reconnect query succeeds"),
    );
    let resolve = command_request(
        Command::ResolveMaintenance {
            session_id: fixture.session_id,
            expected_revision: initial.revision,
            decisions: vec![MaintenanceDecision {
                change_id: fixture.change_id,
                resolution: MaintenanceResolution::Exclude,
            }],
        },
        IdempotencyKey::new(),
    );
    transport
        .command(resolve)
        .await
        .expect("resolution succeeds");
    let ready = maintenance(
        transport
            .query(query_request(Query::MaintenanceSession {
                session_id: fixture.session_id,
            }))
            .await
            .expect("ready query succeeds"),
    );

    let stale = transport
        .command(command_request(
            Command::AuthorizeMaintenance {
                session_id: fixture.session_id,
                expected_revision: initial.revision,
                review_id: ready.review_id.expect("review exists"),
            },
            IdempotencyKey::new(),
        ))
        .await
        .expect_err("stale revision fails");
    assert_eq!(stale.error.code, ErrorCode::StateConflict);

    transport
        .command(command_request(
            Command::AuthorizeMaintenance {
                session_id: fixture.session_id,
                expected_revision: ready.revision,
                review_id: ready.review_id.expect("review exists"),
            },
            IdempotencyKey::new(),
        ))
        .await
        .expect("authorization succeeds");
    let final_view = maintenance(
        transport
            .query(query_request(Query::MaintenanceSession {
                session_id: fixture.session_id,
            }))
            .await
            .expect("final reconnect query succeeds"),
    );
    ScenarioResult {
        initial_state: initial.state,
        ready_state: ready.state,
        final_state: final_view.state,
        start_event_states: start_events
            .events
            .into_iter()
            .map(|event| event.state)
            .collect(),
    }
}

#[tokio::test]
async fn in_process_and_authenticated_http_have_identical_outcomes_and_provider_traces() {
    let local_fixture = Fixture::new();
    let (local_application, local_trace) = application(&local_fixture);
    let local = InProcessTransport {
        application: local_application,
        subject: local_fixture.owner,
    };
    let local_result = exercise_full_flow(&local, &local_fixture).await;

    let http_fixture = Fixture::new();
    let (http_application, http_trace) = application(&http_fixture);
    let server = server(&http_fixture, http_application).await;
    let http = HttpClientTransport {
        client: reqwest::Client::new(),
        base_url: server.base_url.clone(),
        token: "owner-token",
    };
    let http_result = exercise_full_flow(&http, &http_fixture).await;

    assert_eq!(local_result, http_result);
    assert_eq!(
        local_result.initial_state,
        MaintenanceSessionState::NeedsDecision
    );
    assert_eq!(
        local_result.ready_state,
        MaintenanceSessionState::ReadyForAuthorization
    );
    assert_eq!(local_result.final_state, MaintenanceSessionState::InSync);
    assert!(matches!(
        &local_result.start_event_states[..],
        [
            OperationState::Queued,
            OperationState::Running { .. },
            OperationState::Waiting {
                reason: WaitingReason::Consent
            }
        ]
    ));
    assert_eq!(
        *local_trace.lock().unwrap(),
        ["observe_provider", "record_decisions", "apply_provider"]
    );
    assert_eq!(*local_trace.lock().unwrap(), *http_trace.lock().unwrap());
}

#[tokio::test]
async fn http_requires_authentication_and_denies_cross_subject_resources() {
    let fixture = Fixture::new();
    let (application, _) = application(&fixture);
    let server = server(&fixture, application).await;
    let client = reqwest::Client::new();
    let request = command_request(
        Command::StartMaintenance {
            session_id: fixture.session_id,
            provider_connection_id: fixture.provider_connection_id,
        },
        IdempotencyKey::new(),
    );
    let unauthenticated = client
        .post(format!("{}/v1/commands", server.base_url))
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);
    let error: ClientError = unauthenticated.json().await.unwrap();
    assert_eq!(error.code, ErrorCode::AuthenticationRequired);

    let owner = HttpClientTransport {
        client: client.clone(),
        base_url: server.base_url.clone(),
        token: "owner-token",
    };
    owner.command(request).await.expect("owner starts session");
    let stranger = HttpClientTransport {
        client,
        base_url: server.base_url.clone(),
        token: "stranger-token",
    };
    let denied = stranger
        .query(query_request(Query::MaintenanceSession {
            session_id: fixture.session_id,
        }))
        .await
        .expect_err("cross-subject query fails");
    assert_eq!(denied.status, Some(StatusCode::FORBIDDEN));
    assert_eq!(denied.error.code, ErrorCode::PermissionDenied);
}

#[tokio::test]
async fn http_cancellation_and_event_cursor_are_reconnectable_and_ordered() {
    let fixture = Fixture::new();
    let (application, _) = application(&fixture);
    let server = server(&fixture, application).await;
    let http = HttpClientTransport {
        client: reqwest::Client::new(),
        base_url: server.base_url.clone(),
        token: "owner-token",
    };
    let start = http
        .command(command_request(
            Command::StartMaintenance {
                session_id: fixture.session_id,
                provider_connection_id: fixture.provider_connection_id,
            },
            IdempotencyKey::new(),
        ))
        .await
        .expect("start succeeds");
    let cancel_key = IdempotencyKey::new();
    let cancel = command_request(
        Command::CancelOperation(CancellationRequest {
            operation_id: start.operation_id,
            cancellation_id: start.cancellation_id,
        }),
        cancel_key,
    );
    let first = http.command(cancel.clone()).await.expect("cancel succeeds");
    let replay = http.command(cancel).await.expect("cancel replay succeeds");
    assert_eq!(first, replay);

    let after_waiting = events(
        http.query(query_request(Query::OperationEvents {
            operation_id: start.operation_id,
            after_sequence: Some(3),
        }))
        .await
        .expect("event cursor succeeds"),
    );
    assert_eq!(after_waiting.events.len(), 1);
    assert_eq!(after_waiting.events[0].sequence, 4);
    assert_eq!(after_waiting.events[0].state, OperationState::Cancelled);
}

#[tokio::test]
async fn http_rejects_malformed_and_incompatible_requests_without_internal_text() {
    let fixture = Fixture::new();
    let (application, _) = application(&fixture);
    let server = server(&fixture, application).await;
    let client = reqwest::Client::new();
    let malformed = client
        .post(format!("{}/v1/commands", server.base_url))
        .bearer_auth("owner-token")
        .body("{not-json")
        .send()
        .await
        .unwrap();
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    let body = malformed.text().await.unwrap();
    assert!(!body.contains("serde"));
    assert!(!body.contains("expected"));
    assert!(!body.contains("token"));

    let http = HttpClientTransport {
        client,
        base_url: server.base_url.clone(),
        token: "owner-token",
    };
    let mut incompatible = command_request(
        Command::StartMaintenance {
            session_id: fixture.session_id,
            provider_connection_id: fixture.provider_connection_id,
        },
        IdempotencyKey::new(),
    );
    incompatible.contract_version = ContractVersion::new(2, 0);
    let failure = http
        .command(incompatible)
        .await
        .expect_err("incompatible contract fails");
    assert_eq!(failure.status, Some(StatusCode::BAD_REQUEST));
    assert_eq!(failure.error.code, ErrorCode::IncompatibleContract);
}

#[tokio::test]
async fn http_rejects_idempotency_collisions_without_repeating_provider_work() {
    let fixture = Fixture::new();
    let (application, trace) = application(&fixture);
    let server = server(&fixture, application).await;
    let http = HttpClientTransport {
        client: reqwest::Client::new(),
        base_url: server.base_url.clone(),
        token: "owner-token",
    };
    let key = IdempotencyKey::new();
    http.command(command_request(
        Command::StartMaintenance {
            session_id: fixture.session_id,
            provider_connection_id: fixture.provider_connection_id,
        },
        key,
    ))
    .await
    .expect("first command succeeds");
    let collision = http
        .command(command_request(
            Command::RefreshMaintenance {
                session_id: fixture.session_id,
                expected_revision: 1,
            },
            key,
        ))
        .await
        .expect_err("same key with a different command fails");
    assert_eq!(collision.status, Some(StatusCode::CONFLICT));
    assert_eq!(collision.error.code, ErrorCode::StateConflict);
    assert_eq!(*trace.lock().unwrap(), ["observe_provider"]);
}

#[tokio::test]
async fn http_rejects_stale_refresh_before_another_provider_read() {
    let fixture = Fixture::new();
    let (application, trace) = application(&fixture);
    let server = server(&fixture, application).await;
    let http = HttpClientTransport {
        client: reqwest::Client::new(),
        base_url: server.base_url.clone(),
        token: "owner-token",
    };
    http.command(command_request(
        Command::StartMaintenance {
            session_id: fixture.session_id,
            provider_connection_id: fixture.provider_connection_id,
        },
        IdempotencyKey::new(),
    ))
    .await
    .unwrap();
    let failure = http
        .command(command_request(
            Command::RefreshMaintenance {
                session_id: fixture.session_id,
                expected_revision: 99,
            },
            IdempotencyKey::new(),
        ))
        .await
        .expect_err("stale refresh fails");
    assert_eq!(failure.error.code, ErrorCode::StateConflict);
    assert_eq!(*trace.lock().unwrap(), ["observe_provider"]);
}

#[tokio::test]
async fn http_applies_authenticated_request_budget_before_application_work() {
    let fixture = Fixture::new();
    let (application, trace) = application(&fixture);
    let transport = AuthenticatedHttpTransport::new(
        application,
        Arc::new(TestAuthenticator {
            owner: fixture.owner,
            stranger: fixture.stranger,
        }),
    )
    .with_request_gate(Arc::new(RejectAllRequests));
    let server = server_with_transport(&fixture, transport).await;
    let response = reqwest::Client::new()
        .post(format!("{}/v1/commands", server.base_url))
        .bearer_auth("owner-token")
        .json(&command_request(
            Command::StartMaintenance {
                session_id: fixture.session_id,
                provider_connection_id: fixture.provider_connection_id,
            },
            IdempotencyKey::new(),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let error: ClientError = response.json().await.unwrap();
    assert_eq!(error.code, ErrorCode::RateLimited);
    assert_eq!(error.retry_after_seconds, Some(2));
    assert!(trace.lock().unwrap().is_empty());
}

#[tokio::test]
async fn http_refresh_accepts_newest_provider_order_and_invalidates_older_review() {
    let fixture = Fixture::new();
    let provider_order = MaintenanceProjection {
        provider_snapshot_id: ResourceId::new(),
        observed_changes: vec![MaintenanceChangeView {
            change_id: MaintenanceChangeId::new(),
            kind: MaintenanceChangeKind::Reorder,
            track: None,
            previous_surface: None,
            current_surface: Some(MaintenanceSurfaceView {
                surface_id: ResourceId::new(),
                name: "Celluloid Mehfil".to_owned(),
            }),
            summary: "Accepted current provider order".to_owned(),
            resolution: Some(MaintenanceResolution::KeepObserved),
            recommended_resolution: None,
            recommendation_reason: None,
        }],
        provider_effects: Vec::new(),
        review_id: None,
    };
    let (application, trace) = application_with_observations(
        &fixture,
        VecDeque::from([fixture.initial.clone(), provider_order]),
    );
    let server = server(&fixture, application).await;
    let http = HttpClientTransport {
        client: reqwest::Client::new(),
        base_url: server.base_url.clone(),
        token: "owner-token",
    };
    http.command(command_request(
        Command::StartMaintenance {
            session_id: fixture.session_id,
            provider_connection_id: fixture.provider_connection_id,
        },
        IdempotencyKey::new(),
    ))
    .await
    .unwrap();
    let initial = maintenance(
        http.query(query_request(Query::MaintenanceSession {
            session_id: fixture.session_id,
        }))
        .await
        .unwrap(),
    );
    http.command(command_request(
        Command::ResolveMaintenance {
            session_id: fixture.session_id,
            expected_revision: initial.revision,
            decisions: vec![MaintenanceDecision {
                change_id: fixture.change_id,
                resolution: MaintenanceResolution::Exclude,
            }],
        },
        IdempotencyKey::new(),
    ))
    .await
    .unwrap();
    let reviewed = maintenance(
        http.query(query_request(Query::MaintenanceSession {
            session_id: fixture.session_id,
        }))
        .await
        .unwrap(),
    );
    let old_review = reviewed.review_id.unwrap();
    http.command(command_request(
        Command::RefreshMaintenance {
            session_id: fixture.session_id,
            expected_revision: reviewed.revision,
        },
        IdempotencyKey::new(),
    ))
    .await
    .unwrap();
    let refreshed = maintenance(
        http.query(query_request(Query::MaintenanceSession {
            session_id: fixture.session_id,
        }))
        .await
        .unwrap(),
    );
    assert_eq!(refreshed.state, MaintenanceSessionState::InSync);
    assert!(refreshed.provider_effects.is_empty());
    assert_eq!(refreshed.review_id, None);
    let stale = http
        .command(command_request(
            Command::AuthorizeMaintenance {
                session_id: fixture.session_id,
                expected_revision: refreshed.revision,
                review_id: old_review,
            },
            IdempotencyKey::new(),
        ))
        .await
        .expect_err("superseded review cannot be authorized");
    assert_eq!(stale.error.code, ErrorCode::StateConflict);
    assert_eq!(
        *trace.lock().unwrap(),
        ["observe_provider", "record_decisions", "observe_provider"]
    );
}

#[tokio::test]
async fn http_preserves_capability_failure_as_structured_service_response() {
    let fixture = Fixture::new();
    let trace = Arc::new(Mutex::new(Vec::new()));
    let backend = FakeBackend {
        owner: fixture.owner,
        provider_connection_id: fixture.provider_connection_id,
        observations: VecDeque::new(),
        applied: fixture.applied.clone(),
        observe_error: Some(ClientError::new(ErrorCode::CapabilityUnavailable, false)),
        trace: trace.clone(),
    };
    let application: Box<dyn ContractApplication> = Box::new(MaintenanceApplication::new(backend));
    let server = server(&fixture, Arc::new(AsyncMutex::new(application))).await;
    let http = HttpClientTransport {
        client: reqwest::Client::new(),
        base_url: server.base_url.clone(),
        token: "owner-token",
    };
    let failure = http
        .command(command_request(
            Command::StartMaintenance {
                session_id: fixture.session_id,
                provider_connection_id: fixture.provider_connection_id,
            },
            IdempotencyKey::new(),
        ))
        .await
        .expect_err("missing provider capability fails visibly");
    assert_eq!(failure.status, Some(StatusCode::UNPROCESSABLE_ENTITY));
    assert_eq!(failure.error.code, ErrorCode::CapabilityUnavailable);
    assert_eq!(*trace.lock().unwrap(), ["observe_provider"]);
}
