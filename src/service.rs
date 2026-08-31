//! Authenticated, transport-neutral application service.
//!
//! HTTP and in-process clients call this service with the same typed contract
//! values. Client adapters never inspect provider deltas, assemble plans, or
//! choose workflow transitions. Provider/database implementations sit behind
//! [`MaintenanceBackend`] and are invoked only by this Rust authority.

use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::{
    contract::{
        CONTRACT_VERSION, CancellationId, ClientError, Command, CommandReceipt, CommandRequest,
        ErrorCode, ExcludedTracksView, IdempotencyKey, LibraryPlaylistTracksView,
        LibraryPlaylistsView, LibraryStateSource, LibraryTrackView, MaintenanceReviewId,
        MaintenanceSessionId, MaintenanceSessionState, MaintenanceSessionView, OperationEvent,
        OperationEventsView, OperationHistoryView, OperationId, OperationState, OperationView,
        Progress, ProgressUnit, ProviderConnectionsView, Query, QueryRequest, QueryResponse,
        RequestId, ResourceId, View, WaitingReason,
    },
    maintenance::{MaintenanceDecisionProjection, MaintenanceProjection, MaintenanceSessions},
};

/// Authenticated caller context established by a transport adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticatedSubject {
    /// Product identity authenticated by the transport.
    pub subject_id: ResourceId,
    /// Chordrift account selected by the authenticated session.
    pub account_id: ResourceId,
}

/// Infrastructure boundary used by the Rust maintenance authority.
#[async_trait]
pub trait MaintenanceBackend: Send {
    /// Lists provider connections owned by the authenticated account.
    async fn provider_connections(
        &mut self,
        _subject: AuthenticatedSubject,
    ) -> Result<ProviderConnectionsView, ClientError> {
        Err(ClientError::new(ErrorCode::CapabilityUnavailable, false))
    }

    /// Lists playlists from one explicit library state plane.
    async fn library_playlists(
        &mut self,
        _subject: AuthenticatedSubject,
        _provider_connection_id: ResourceId,
        _source: LibraryStateSource,
    ) -> Result<LibraryPlaylistsView, ClientError> {
        Err(ClientError::new(ErrorCode::CapabilityUnavailable, false))
    }

    /// Lists ordered tracks from one playlist and state plane.
    async fn library_playlist_tracks(
        &mut self,
        _subject: AuthenticatedSubject,
        _provider_connection_id: ResourceId,
        _playlist_id: &str,
        _source: LibraryStateSource,
    ) -> Result<LibraryPlaylistTracksView, ClientError> {
        Err(ClientError::new(ErrorCode::CapabilityUnavailable, false))
    }

    /// Reads one track's placements and personal listening evidence.
    async fn library_track(
        &mut self,
        _subject: AuthenticatedSubject,
        _provider_connection_id: ResourceId,
        _provider_track_id: &str,
    ) -> Result<LibraryTrackView, ClientError> {
        Err(ClientError::new(ErrorCode::CapabilityUnavailable, false))
    }

    /// Lists active reversible exclusions.
    async fn excluded_tracks(
        &mut self,
        _subject: AuthenticatedSubject,
        _provider_connection_id: ResourceId,
    ) -> Result<ExcludedTracksView, ClientError> {
        Err(ClientError::new(ErrorCode::CapabilityUnavailable, false))
    }

    /// Reports whether the authenticated account owns one provider connection.
    async fn owns_provider_connection(
        &self,
        subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
    ) -> bool;

    /// Obtains the newest complete provider projection for start or refresh.
    async fn observe(
        &mut self,
        subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
        current: Option<&MaintenanceSessionView>,
    ) -> Result<MaintenanceProjection, ClientError>;

    /// Persists accepted ambiguity decisions and their resulting review state.
    async fn record_decisions(
        &mut self,
        subject: AuthenticatedSubject,
        view: &MaintenanceSessionView,
    ) -> Result<MaintenanceDecisionProjection, ClientError>;

    /// Applies one exact authorized provider review and returns the observed result.
    async fn apply(
        &mut self,
        subject: AuthenticatedSubject,
        view: &MaintenanceSessionView,
    ) -> Result<MaintenanceProjection, ClientError>;
}

/// Clock seam used to make lifecycle conformance deterministic.
pub trait ServiceClock: Send + Sync {
    /// Returns the current service time.
    fn now(&self) -> DateTime<Utc>;
}

/// Production wall clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemServiceClock;

impl ServiceClock for SystemServiceClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Shared application interface consumed by local and HTTP transports.
#[async_trait]
pub trait ContractApplication: Send {
    /// Executes one authenticated command.
    async fn command(
        &mut self,
        subject: AuthenticatedSubject,
        request: CommandRequest,
    ) -> Result<CommandReceipt, ClientError>;

    /// Executes one authenticated query.
    async fn query(
        &mut self,
        subject: AuthenticatedSubject,
        request: QueryRequest,
    ) -> Result<QueryResponse, ClientError>;
}

#[derive(Clone)]
struct CachedCommand {
    fingerprint: [u8; 32],
    outcome: Result<CommandReceipt, ClientError>,
}

#[derive(Clone)]
struct OperationRecord {
    subject: AuthenticatedSubject,
    cancellation_id: CancellationId,
    events: Vec<OperationEvent>,
}

impl OperationRecord {
    fn view(&self, operation_id: OperationId) -> OperationView {
        OperationView {
            operation_id,
            cancellation_id: self.cancellation_id,
            state: self
                .events
                .last()
                .expect("an accepted operation always has an event")
                .state
                .clone(),
        }
    }
}

/// Rust-owned command/query authority for ordinary maintenance.
pub struct MaintenanceApplication<B> {
    backend: B,
    sessions: MaintenanceSessions,
    session_owners: BTreeMap<MaintenanceSessionId, AuthenticatedSubject>,
    session_connections: BTreeMap<MaintenanceSessionId, ResourceId>,
    operations: BTreeMap<OperationId, OperationRecord>,
    operation_order: Vec<OperationId>,
    idempotency: BTreeMap<(ResourceId, ResourceId, IdempotencyKey), CachedCommand>,
    clock: Arc<dyn ServiceClock>,
}

impl<B> MaintenanceApplication<B>
where
    B: MaintenanceBackend,
{
    /// Creates an authority using the production wall clock.
    pub fn new(backend: B) -> Self {
        Self::with_clock(backend, Arc::new(SystemServiceClock))
    }

    /// Creates an authority with an explicit clock for deterministic tests.
    pub fn with_clock(backend: B, clock: Arc<dyn ServiceClock>) -> Self {
        Self {
            backend,
            sessions: MaintenanceSessions::new(),
            session_owners: BTreeMap::new(),
            session_connections: BTreeMap::new(),
            operations: BTreeMap::new(),
            operation_order: Vec::new(),
            idempotency: BTreeMap::new(),
            clock,
        }
    }

    async fn execute_command(
        &mut self,
        subject: AuthenticatedSubject,
        request_id: RequestId,
        command: Command,
    ) -> Result<CommandReceipt, ClientError> {
        match command {
            Command::StartMaintenance {
                session_id,
                provider_connection_id,
            } => {
                if self.session_owners.contains_key(&session_id) {
                    return Err(ClientError::new(ErrorCode::StateConflict, false));
                }
                if !self
                    .backend
                    .owns_provider_connection(subject, provider_connection_id)
                    .await
                {
                    return Err(ClientError::new(ErrorCode::PermissionDenied, false));
                }
                let projection = self
                    .backend
                    .observe(subject, provider_connection_id, None)
                    .await?;
                let view = self
                    .sessions
                    .start(
                        &Command::StartMaintenance {
                            session_id,
                            provider_connection_id,
                        },
                        projection,
                    )
                    .map_err(|error| error.client_error())?;
                self.session_owners.insert(session_id, subject);
                self.session_connections
                    .insert(session_id, provider_connection_id);
                Ok(self.accept_operation(subject, request_id, operation_states_for_view(&view)))
            }
            Command::RefreshMaintenance {
                session_id,
                expected_revision,
            } => {
                self.require_session_owner(subject, session_id)?;
                let current = self.session_view(session_id)?;
                if current.revision != expected_revision {
                    return Err(ClientError::new(ErrorCode::StateConflict, false));
                }
                let provider_connection_id = *self
                    .session_connections
                    .get(&session_id)
                    .ok_or_else(|| ClientError::new(ErrorCode::ResourceNotFound, false))?;
                let projection = self
                    .backend
                    .observe(subject, provider_connection_id, Some(&current))
                    .await?;
                let view = self
                    .sessions
                    .refresh(
                        &Command::RefreshMaintenance {
                            session_id,
                            expected_revision,
                        },
                        projection,
                    )
                    .map_err(|error| error.client_error())?;
                Ok(self.accept_operation(subject, request_id, operation_states_for_view(&view)))
            }
            Command::ResolveMaintenance {
                session_id,
                expected_revision,
                decisions,
            } => {
                self.require_session_owner(subject, session_id)?;
                let current = self.session_view(session_id)?;
                let next_review_id =
                    (!current.provider_effects.is_empty()).then(MaintenanceReviewId::new);
                let initial_projection = MaintenanceDecisionProjection {
                    provider_effects: current.provider_effects.clone(),
                    review_id: next_review_id,
                };
                let mut candidate_sessions = self.sessions.clone();
                let candidate = candidate_sessions
                    .execute(
                        &Command::ResolveMaintenance {
                            session_id,
                            expected_revision,
                            decisions: decisions.clone(),
                        },
                        Some(initial_projection),
                    )
                    .map_err(|error| error.client_error())?;
                let decision_projection =
                    self.backend.record_decisions(subject, &candidate).await?;
                let mut next_sessions = self.sessions.clone();
                let view = next_sessions
                    .execute(
                        &Command::ResolveMaintenance {
                            session_id,
                            expected_revision,
                            decisions,
                        },
                        Some(decision_projection),
                    )
                    .map_err(|error| error.client_error())?;
                self.sessions = next_sessions;
                Ok(self.accept_operation(subject, request_id, operation_states_for_view(&view)))
            }
            Command::AuthorizeMaintenance {
                session_id,
                expected_revision,
                review_id,
            } => {
                self.require_session_owner(subject, session_id)?;
                let mut next_sessions = self.sessions.clone();
                let authorized = next_sessions
                    .execute(
                        &Command::AuthorizeMaintenance {
                            session_id,
                            expected_revision,
                            review_id,
                        },
                        None,
                    )
                    .map_err(|error| error.client_error())?;
                next_sessions
                    .mark_execution_state(session_id, MaintenanceSessionState::Applying)
                    .map_err(|error| error.client_error())?;
                let observed = self.backend.apply(subject, &authorized).await?;
                let verifying = next_sessions
                    .mark_execution_state(session_id, MaintenanceSessionState::Verifying)
                    .map_err(|error| error.client_error())?;
                let final_view = next_sessions
                    .refresh(
                        &Command::RefreshMaintenance {
                            session_id,
                            expected_revision: verifying.revision,
                        },
                        observed,
                    )
                    .map_err(|error| error.client_error())?;
                self.sessions = next_sessions;
                Ok(self.accept_operation(
                    subject,
                    request_id,
                    vec![
                        OperationState::Queued,
                        running("apply_provider", 0, Some(1)),
                        running("verify_provider", 0, Some(1)),
                        OperationState::Completed {
                            result_id: Some(ResourceId::from_uuid(final_view.session_id.as_uuid())),
                        },
                    ],
                ))
            }
            Command::CancelOperation(cancellation) => {
                let target = self
                    .operations
                    .get_mut(&cancellation.operation_id)
                    .ok_or_else(|| ClientError::new(ErrorCode::ResourceNotFound, false))?;
                if target.subject != subject {
                    return Err(ClientError::new(ErrorCode::PermissionDenied, false));
                }
                if target.cancellation_id != cancellation.cancellation_id {
                    return Err(ClientError::new(ErrorCode::PermissionDenied, false));
                }
                if target
                    .events
                    .last()
                    .is_some_and(|event| event.state.is_terminal())
                {
                    return Err(ClientError::new(ErrorCode::StateConflict, false));
                }
                let sequence = target.events.len() as u64 + 1;
                target.events.push(OperationEvent {
                    contract_version: CONTRACT_VERSION,
                    operation_id: cancellation.operation_id,
                    sequence,
                    occurred_at: self.clock.now(),
                    state: OperationState::Cancelled,
                });
                Ok(self.accept_operation(
                    subject,
                    request_id,
                    vec![
                        OperationState::Queued,
                        OperationState::Completed { result_id: None },
                    ],
                ))
            }
            _ => Err(ClientError::new(ErrorCode::InvalidRequest, false)),
        }
    }

    fn accept_operation(
        &mut self,
        subject: AuthenticatedSubject,
        request_id: RequestId,
        states: Vec<OperationState>,
    ) -> CommandReceipt {
        let operation_id = OperationId::new();
        let cancellation_id = CancellationId::new();
        let events = states
            .into_iter()
            .enumerate()
            .map(|(index, state)| OperationEvent {
                contract_version: CONTRACT_VERSION,
                operation_id,
                sequence: index as u64 + 1,
                occurred_at: self.clock.now(),
                state,
            })
            .collect();
        self.operations.insert(
            operation_id,
            OperationRecord {
                subject,
                cancellation_id,
                events,
            },
        );
        self.operation_order.push(operation_id);
        CommandReceipt {
            contract_version: CONTRACT_VERSION,
            request_id,
            operation_id,
            cancellation_id,
        }
    }

    fn require_session_owner(
        &self,
        subject: AuthenticatedSubject,
        session_id: MaintenanceSessionId,
    ) -> Result<(), ClientError> {
        match self.session_owners.get(&session_id) {
            Some(owner) if *owner == subject => Ok(()),
            Some(_) => Err(ClientError::new(ErrorCode::PermissionDenied, false)),
            None => Err(ClientError::new(ErrorCode::ResourceNotFound, false)),
        }
    }

    fn session_view(
        &self,
        session_id: MaintenanceSessionId,
    ) -> Result<MaintenanceSessionView, ClientError> {
        self.sessions
            .query(&Query::MaintenanceSession { session_id })
            .map_err(|error| error.client_error())
    }

    fn query_operation(
        &self,
        subject: AuthenticatedSubject,
        operation_id: OperationId,
    ) -> Result<&OperationRecord, ClientError> {
        let record = self
            .operations
            .get(&operation_id)
            .ok_or_else(|| ClientError::new(ErrorCode::ResourceNotFound, false))?;
        if record.subject != subject {
            return Err(ClientError::new(ErrorCode::PermissionDenied, false));
        }
        Ok(record)
    }
}

#[async_trait]
impl<B> ContractApplication for MaintenanceApplication<B>
where
    B: MaintenanceBackend,
{
    async fn command(
        &mut self,
        subject: AuthenticatedSubject,
        request: CommandRequest,
    ) -> Result<CommandReceipt, ClientError> {
        if request.contract_version != CONTRACT_VERSION {
            return Err(ClientError::new(ErrorCode::IncompatibleContract, false));
        }
        let fingerprint: [u8; 32] = Sha256::digest(
            serde_json::to_vec(&request.command)
                .map_err(|_| ClientError::new(ErrorCode::Internal, false))?,
        )
        .into();
        let key = (
            subject.subject_id,
            subject.account_id,
            request.idempotency_key,
        );
        if let Some(cached) = self.idempotency.get(&key) {
            if cached.fingerprint != fingerprint {
                return Err(ClientError::new(ErrorCode::StateConflict, false));
            }
            return cached.outcome.clone();
        }
        let outcome = self
            .execute_command(subject, request.request_id, request.command)
            .await;
        self.idempotency.insert(
            key,
            CachedCommand {
                fingerprint,
                outcome: outcome.clone(),
            },
        );
        outcome
    }

    async fn query(
        &mut self,
        subject: AuthenticatedSubject,
        request: QueryRequest,
    ) -> Result<QueryResponse, ClientError> {
        if request.contract_version != CONTRACT_VERSION {
            return Err(ClientError::new(ErrorCode::IncompatibleContract, false));
        }
        let generated_at = self.clock.now();
        match request.query {
            Query::ProviderConnections => {
                let value = self.backend.provider_connections(subject).await?;
                Ok(QueryResponse::ProviderConnections(View {
                    contract_version: CONTRACT_VERSION,
                    request_id: request.request_id,
                    generated_at,
                    value,
                }))
            }
            Query::LibraryPlaylists {
                provider_connection_id,
                source,
            } => {
                if !self
                    .backend
                    .owns_provider_connection(subject, provider_connection_id)
                    .await
                {
                    return Err(ClientError::new(ErrorCode::PermissionDenied, false));
                }
                let value = self
                    .backend
                    .library_playlists(subject, provider_connection_id, source)
                    .await?;
                Ok(QueryResponse::LibraryPlaylists(View {
                    contract_version: CONTRACT_VERSION,
                    request_id: request.request_id,
                    generated_at,
                    value,
                }))
            }
            Query::LibraryPlaylistTracks {
                provider_connection_id,
                playlist_id,
                source,
            } => {
                if !self
                    .backend
                    .owns_provider_connection(subject, provider_connection_id)
                    .await
                {
                    return Err(ClientError::new(ErrorCode::PermissionDenied, false));
                }
                let value = self
                    .backend
                    .library_playlist_tracks(subject, provider_connection_id, &playlist_id, source)
                    .await?;
                Ok(QueryResponse::LibraryPlaylistTracks(View {
                    contract_version: CONTRACT_VERSION,
                    request_id: request.request_id,
                    generated_at,
                    value,
                }))
            }
            Query::LibraryTrack {
                provider_connection_id,
                provider_track_id,
            } => {
                if !self
                    .backend
                    .owns_provider_connection(subject, provider_connection_id)
                    .await
                {
                    return Err(ClientError::new(ErrorCode::PermissionDenied, false));
                }
                let value = self
                    .backend
                    .library_track(subject, provider_connection_id, &provider_track_id)
                    .await?;
                Ok(QueryResponse::LibraryTrack(View {
                    contract_version: CONTRACT_VERSION,
                    request_id: request.request_id,
                    generated_at,
                    value,
                }))
            }
            Query::ExcludedTracks {
                provider_connection_id,
            } => {
                if !self
                    .backend
                    .owns_provider_connection(subject, provider_connection_id)
                    .await
                {
                    return Err(ClientError::new(ErrorCode::PermissionDenied, false));
                }
                let value = self
                    .backend
                    .excluded_tracks(subject, provider_connection_id)
                    .await?;
                Ok(QueryResponse::ExcludedTracks(View {
                    contract_version: CONTRACT_VERSION,
                    request_id: request.request_id,
                    generated_at,
                    value,
                }))
            }
            Query::MaintenanceSession { session_id } => {
                self.require_session_owner(subject, session_id)?;
                Ok(QueryResponse::MaintenanceSession(View {
                    contract_version: CONTRACT_VERSION,
                    request_id: request.request_id,
                    generated_at,
                    value: self.session_view(session_id)?,
                }))
            }
            Query::Operation { operation_id } => {
                let record = self.query_operation(subject, operation_id)?;
                Ok(QueryResponse::Operation(View {
                    contract_version: CONTRACT_VERSION,
                    request_id: request.request_id,
                    generated_at,
                    value: record.view(operation_id),
                }))
            }
            Query::OperationHistory { account_id } => {
                if account_id != subject.account_id {
                    return Err(ClientError::new(ErrorCode::PermissionDenied, false));
                }
                let operations = self
                    .operation_order
                    .iter()
                    .filter_map(|operation_id| {
                        self.operations.get(operation_id).and_then(|record| {
                            (record.subject == subject).then(|| record.view(*operation_id))
                        })
                    })
                    .collect();
                Ok(QueryResponse::OperationHistory(View {
                    contract_version: CONTRACT_VERSION,
                    request_id: request.request_id,
                    generated_at,
                    value: OperationHistoryView { operations },
                }))
            }
            Query::OperationEvents {
                operation_id,
                after_sequence,
            } => {
                let record = self.query_operation(subject, operation_id)?;
                let cursor = after_sequence.unwrap_or(0);
                let events = record
                    .events
                    .iter()
                    .filter(|event| event.sequence > cursor)
                    .cloned()
                    .collect();
                Ok(QueryResponse::OperationEvents(View {
                    contract_version: CONTRACT_VERSION,
                    request_id: request.request_id,
                    generated_at,
                    value: OperationEventsView {
                        operation_id,
                        events,
                    },
                }))
            }
            _ => Err(ClientError::new(ErrorCode::InvalidRequest, false)),
        }
    }
}

fn operation_states_for_view(view: &MaintenanceSessionView) -> Vec<OperationState> {
    let final_state = match view.state {
        MaintenanceSessionState::NeedsDecision => OperationState::Waiting {
            reason: WaitingReason::Consent,
        },
        MaintenanceSessionState::ReadyForAuthorization => OperationState::Waiting {
            reason: WaitingReason::Authorization,
        },
        MaintenanceSessionState::InSync => OperationState::Completed {
            result_id: Some(ResourceId::from_uuid(view.session_id.as_uuid())),
        },
        _ => OperationState::Running { progress: None },
    };
    vec![
        OperationState::Queued,
        running("maintenance", 0, Some(1)),
        final_state,
    ]
}

fn running(phase: &str, completed: u64, total: Option<u64>) -> OperationState {
    OperationState::Running {
        progress: Some(
            Progress::new(phase, completed, total, ProgressUnit::Steps)
                .expect("service progress is valid"),
        ),
    }
}
