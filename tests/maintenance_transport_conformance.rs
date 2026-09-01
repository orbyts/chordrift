use chordrift::{
    contract::{
        CONTRACT_VERSION, CancellationId, Command, CommandReceipt, CommandRequest, ErrorCode,
        IdempotencyKey, MaintenanceChangeId, MaintenanceChangeKind, MaintenanceChangeView,
        MaintenanceDecision, MaintenanceProviderEffectKind, MaintenanceProviderEffectView,
        MaintenanceResolution, MaintenanceReviewId, MaintenanceSessionId, MaintenanceSessionState,
        MaintenanceSessionView, MaintenanceSurfaceView, MaintenanceTrackView, OperationId, Query,
        QueryRequest, RequestId, ResourceId, View,
    },
    maintenance::{MaintenanceDecisionProjection, MaintenanceProjection, MaintenanceSessions},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct ScenarioFixture {
    provider_connection_id: ResourceId,
    session_id: MaintenanceSessionId,
    next_review_id: MaintenanceReviewId,
    projection: MaintenanceProjection,
    refreshed_projection: Option<MaintenanceProjection>,
    generated_at: DateTime<Utc>,
}

impl ScenarioFixture {
    fn ambiguous_removal() -> Self {
        let track = MaintenanceTrackView {
            track_id: ResourceId::new(),
            title: "Fixture Song".to_owned(),
            artists: vec!["Fixture Artist".to_owned()],
        };
        let surface = MaintenanceSurfaceView {
            surface_id: ResourceId::new(),
            name: "Old Vibe".to_owned(),
        };
        let refreshed_projection = MaintenanceProjection {
            provider_snapshot_id: ResourceId::new(),
            observed_changes: vec![MaintenanceChangeView {
                change_id: MaintenanceChangeId::new(),
                kind: MaintenanceChangeKind::Reorder,
                track: None,
                previous_surface: None,
                current_surface: Some(surface.clone()),
                summary: "Accepted the newest complete provider state".to_owned(),
                resolution: Some(MaintenanceResolution::KeepObserved),
                recommended_resolution: None,
                recommendation_reason: None,
            }],
            provider_effects: Vec::new(),
            review_id: None,
        };
        Self {
            provider_connection_id: ResourceId::new(),
            session_id: MaintenanceSessionId::new(),
            next_review_id: MaintenanceReviewId::new(),
            projection: MaintenanceProjection {
                provider_snapshot_id: ResourceId::new(),
                observed_changes: vec![MaintenanceChangeView {
                    change_id: MaintenanceChangeId::new(),
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
            refreshed_projection: Some(refreshed_projection),
            generated_at: "2026-08-30T00:00:00Z".parse().expect("fixed time"),
        }
    }

    fn cumulative_provider_order() -> Self {
        let surface = MaintenanceSurfaceView {
            surface_id: ResourceId::new(),
            name: "Celluloid Mehfil".to_owned(),
        };
        Self {
            provider_connection_id: ResourceId::new(),
            session_id: MaintenanceSessionId::new(),
            next_review_id: MaintenanceReviewId::new(),
            projection: MaintenanceProjection {
                provider_snapshot_id: ResourceId::new(),
                observed_changes: vec![MaintenanceChangeView {
                    change_id: MaintenanceChangeId::new(),
                    kind: MaintenanceChangeKind::Reorder,
                    track: None,
                    previous_surface: None,
                    current_surface: Some(surface),
                    summary: "Accepted current Spotify order".to_owned(),
                    resolution: Some(MaintenanceResolution::KeepObserved),
                    recommended_resolution: None,
                    recommendation_reason: None,
                }],
                provider_effects: Vec::new(),
                review_id: None,
            },
            refreshed_projection: None,
            generated_at: "2026-08-30T00:00:00Z".parse().expect("fixed time"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
enum WireRequest {
    Command(CommandRequest),
    Query(QueryRequest),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
enum WireResponse {
    Command(CommandReceipt),
    Maintenance(View<MaintenanceSessionView>),
    Error(chordrift::contract::ClientError),
}

#[derive(Clone)]
struct ScenarioApplication {
    fixture: ScenarioFixture,
    sessions: MaintenanceSessions,
    trace: Vec<String>,
}

impl ScenarioApplication {
    fn new(fixture: ScenarioFixture) -> Self {
        Self {
            fixture,
            sessions: MaintenanceSessions::new(),
            trace: Vec::new(),
        }
    }

    fn handle(&mut self, request: WireRequest) -> WireResponse {
        match request {
            WireRequest::Command(request) => {
                let result = match request.command {
                    Command::StartMaintenance {
                        session_id,
                        provider_connection_id,
                    } if session_id == self.fixture.session_id
                        && provider_connection_id == self.fixture.provider_connection_id =>
                    {
                        self.trace.push("observe_provider".to_owned());
                        self.sessions
                            .start(
                                &Command::StartMaintenance {
                                    session_id,
                                    provider_connection_id,
                                },
                                self.fixture.projection.clone(),
                            )
                            .map(|_| ())
                            .map_err(|error| error.client_error())
                    }
                    Command::ResolveMaintenance {
                        session_id,
                        expected_revision,
                        decisions,
                    } if session_id == self.fixture.session_id => {
                        self.trace.push("record_decisions".to_owned());
                        self.sessions
                            .execute(
                                &Command::ResolveMaintenance {
                                    session_id,
                                    expected_revision,
                                    decisions,
                                },
                                Some(MaintenanceDecisionProjection {
                                    provider_effects: self
                                        .fixture
                                        .projection
                                        .provider_effects
                                        .clone(),
                                    review_id: Some(self.fixture.next_review_id),
                                }),
                            )
                            .map(|_| ())
                            .map_err(|error| error.client_error())
                    }
                    Command::RefreshMaintenance {
                        session_id,
                        expected_revision,
                    } if session_id == self.fixture.session_id => {
                        self.trace.push("observe_provider".to_owned());
                        self.sessions
                            .refresh(
                                &Command::RefreshMaintenance {
                                    session_id,
                                    expected_revision,
                                },
                                self.fixture
                                    .refreshed_projection
                                    .clone()
                                    .expect("scenario has a refresh projection"),
                            )
                            .map(|_| ())
                            .map_err(|error| error.client_error())
                    }
                    Command::AuthorizeMaintenance {
                        session_id,
                        expected_revision,
                        review_id,
                    } if session_id == self.fixture.session_id => {
                        let result = self
                            .sessions
                            .execute(
                                &Command::AuthorizeMaintenance {
                                    session_id,
                                    expected_revision,
                                    review_id,
                                },
                                None,
                            )
                            .map(|_| ())
                            .map_err(|error| error.client_error());
                        if result.is_ok() {
                            self.trace.push("authorize_review".to_owned());
                        }
                        result
                    }
                    _ => Err(chordrift::contract::ClientError::new(
                        ErrorCode::InvalidRequest,
                        false,
                    )),
                };
                match result {
                    Ok(()) => WireResponse::Command(CommandReceipt {
                        contract_version: CONTRACT_VERSION,
                        request_id: request.request_id,
                        operation_id: OperationId::new(),
                        cancellation_id: CancellationId::new(),
                    }),
                    Err(error) => WireResponse::Error(error),
                }
            }
            WireRequest::Query(request) => match request.query {
                Query::MaintenanceSession { session_id }
                    if session_id == self.fixture.session_id =>
                {
                    match self
                        .sessions
                        .query(&Query::MaintenanceSession { session_id })
                    {
                        Ok(value) => WireResponse::Maintenance(View {
                            contract_version: CONTRACT_VERSION,
                            request_id: request.request_id,
                            generated_at: self.fixture.generated_at,
                            value,
                        }),
                        Err(error) => WireResponse::Error(error.client_error()),
                    }
                }
                _ => WireResponse::Error(chordrift::contract::ClientError::new(
                    ErrorCode::InvalidRequest,
                    false,
                )),
            },
        }
    }
}

trait ScenarioTransport {
    fn send(&mut self, request: WireRequest) -> WireResponse;
    fn trace(&self) -> &[String];
}

struct InProcessTransport {
    application: ScenarioApplication,
}

impl ScenarioTransport for InProcessTransport {
    fn send(&mut self, request: WireRequest) -> WireResponse {
        self.application.handle(request)
    }

    fn trace(&self) -> &[String] {
        &self.application.trace
    }
}

struct JsonLoopbackTransport {
    application: ScenarioApplication,
}

impl ScenarioTransport for JsonLoopbackTransport {
    fn send(&mut self, request: WireRequest) -> WireResponse {
        let encoded = serde_json::to_vec(&request).expect("web request serializes");
        let decoded = serde_json::from_slice(&encoded).expect("service decodes request");
        let response = self.application.handle(decoded);
        let encoded = serde_json::to_vec(&response).expect("service response serializes");
        serde_json::from_slice(&encoded).expect("web client decodes response")
    }

    fn trace(&self) -> &[String] {
        &self.application.trace
    }
}

fn command(command: Command) -> WireRequest {
    WireRequest::Command(CommandRequest {
        contract_version: CONTRACT_VERSION,
        request_id: RequestId::new(),
        idempotency_key: IdempotencyKey::new(),
        command,
    })
}

fn query(session_id: MaintenanceSessionId) -> WireRequest {
    WireRequest::Query(QueryRequest {
        contract_version: CONTRACT_VERSION,
        request_id: RequestId::new(),
        query: Query::MaintenanceSession { session_id },
    })
}

fn session_view(response: WireResponse) -> MaintenanceSessionView {
    let WireResponse::Maintenance(view) = response else {
        panic!("expected maintenance view");
    };
    view.value
}

fn exercise_ambiguous_flow(
    transport: &mut impl ScenarioTransport,
    fixture: &ScenarioFixture,
) -> MaintenanceSessionView {
    assert!(matches!(
        transport.send(command(Command::StartMaintenance {
            session_id: fixture.session_id,
            provider_connection_id: fixture.provider_connection_id,
        })),
        WireResponse::Command(_)
    ));
    let initial = session_view(transport.send(query(fixture.session_id)));
    assert_eq!(initial.state, MaintenanceSessionState::NeedsDecision);
    let change_id = initial.observed_changes[0].change_id;

    assert!(matches!(
        transport.send(command(Command::ResolveMaintenance {
            session_id: fixture.session_id,
            expected_revision: initial.revision,
            decisions: vec![MaintenanceDecision {
                change_id,
                resolution: MaintenanceResolution::Exclude,
            }],
        })),
        WireResponse::Command(_)
    ));
    let ready = session_view(transport.send(query(fixture.session_id)));
    assert_eq!(ready.state, MaintenanceSessionState::ReadyForAuthorization);
    assert_eq!(ready.review_id, Some(fixture.next_review_id));

    let stale = transport.send(command(Command::AuthorizeMaintenance {
        session_id: fixture.session_id,
        expected_revision: initial.revision,
        review_id: fixture.next_review_id,
    }));
    let WireResponse::Error(stale) = stale else {
        panic!("stale web command must fail");
    };
    assert_eq!(stale.code, ErrorCode::StateConflict);

    assert!(matches!(
        transport.send(command(Command::AuthorizeMaintenance {
            session_id: fixture.session_id,
            expected_revision: ready.revision,
            review_id: fixture.next_review_id,
        })),
        WireResponse::Command(_)
    ));
    session_view(transport.send(query(fixture.session_id)))
}

#[test]
fn in_process_and_json_web_calls_have_identical_workflow_outcomes() {
    let fixture = ScenarioFixture::ambiguous_removal();
    let mut local = InProcessTransport {
        application: ScenarioApplication::new(fixture.clone()),
    };
    let mut web = JsonLoopbackTransport {
        application: ScenarioApplication::new(fixture.clone()),
    };

    let local_view = exercise_ambiguous_flow(&mut local, &fixture);
    let web_view = exercise_ambiguous_flow(&mut web, &fixture);
    assert_eq!(local_view, web_view);
    assert_eq!(local_view.state, MaintenanceSessionState::Authorized);
    assert_eq!(local.trace(), web.trace());
    assert_eq!(
        local.trace(),
        &["observe_provider", "record_decisions", "authorize_review"]
    );
}

#[test]
fn serialized_web_call_accepts_record_only_provider_order_without_authorization() {
    let fixture = ScenarioFixture::cumulative_provider_order();
    let mut web = JsonLoopbackTransport {
        application: ScenarioApplication::new(fixture.clone()),
    };
    assert!(matches!(
        web.send(command(Command::StartMaintenance {
            session_id: fixture.session_id,
            provider_connection_id: fixture.provider_connection_id,
        })),
        WireResponse::Command(_)
    ));
    let view = session_view(web.send(query(fixture.session_id)));
    assert_eq!(view.state, MaintenanceSessionState::InSync);
    assert!(view.provider_effects.is_empty());
    assert_eq!(web.trace(), &["observe_provider"]);
}

#[test]
fn serialized_refresh_rebases_to_newest_provider_state_and_invalidates_old_review() {
    let fixture = ScenarioFixture::ambiguous_removal();
    let mut web = JsonLoopbackTransport {
        application: ScenarioApplication::new(fixture.clone()),
    };
    assert!(matches!(
        web.send(command(Command::StartMaintenance {
            session_id: fixture.session_id,
            provider_connection_id: fixture.provider_connection_id,
        })),
        WireResponse::Command(_)
    ));
    let initial = session_view(web.send(query(fixture.session_id)));
    let change_id = initial.observed_changes[0].change_id;
    assert!(matches!(
        web.send(command(Command::ResolveMaintenance {
            session_id: fixture.session_id,
            expected_revision: initial.revision,
            decisions: vec![MaintenanceDecision {
                change_id,
                resolution: MaintenanceResolution::Exclude,
            }],
        })),
        WireResponse::Command(_)
    ));
    let reviewed = session_view(web.send(query(fixture.session_id)));

    assert!(matches!(
        web.send(command(Command::RefreshMaintenance {
            session_id: fixture.session_id,
            expected_revision: reviewed.revision,
        })),
        WireResponse::Command(_)
    ));
    let refreshed = session_view(web.send(query(fixture.session_id)));
    assert_eq!(refreshed.state, MaintenanceSessionState::InSync);
    assert_eq!(refreshed.review_id, None);
    assert_eq!(refreshed.provider_effects, Vec::new());

    let stale_authorization = web.send(command(Command::AuthorizeMaintenance {
        session_id: fixture.session_id,
        expected_revision: refreshed.revision,
        review_id: fixture.next_review_id,
    }));
    let WireResponse::Error(error) = stale_authorization else {
        panic!("authorization for a superseded review must fail");
    };
    assert_eq!(error.code, ErrorCode::StateConflict);
    assert_eq!(
        web.trace(),
        &["observe_provider", "record_decisions", "observe_provider"]
    );
}
