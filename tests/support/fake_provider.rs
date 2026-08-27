use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    future,
    num::NonZeroU8,
};

use chordrift::{
    Result as ChordriftResult,
    application::ApplicationInvocation,
    contract::{
        CONTRACT_VERSION, CancellationId, CancellationOutcome, CancellationRequest, ClientError,
        Command, CommandReceipt, CommandRequest, ErrorCode, IdempotencyKey, OperationId,
        OperationState, ResourceId,
    },
    domain::{
        AccountContext, CapabilityStatus, ChordriftAccountId, ProviderCapability,
        ProviderConnectionId, ProviderConnectionIdentity, ProviderTrackId,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FakeProviderOutcome {
    Inventory,
    TemporarilyUnavailable,
}

pub(crate) struct FakeProvider {
    connection: ProviderConnectionIdentity,
    inventory: Vec<ProviderTrackId>,
    outcomes: RefCell<VecDeque<FakeProviderOutcome>>,
    calls: Cell<u8>,
}

impl FakeProvider {
    pub(crate) fn new(
        connection: ProviderConnectionIdentity,
        inventory: Vec<ProviderTrackId>,
        outcomes: impl IntoIterator<Item = FakeProviderOutcome>,
    ) -> Self {
        assert!(
            inventory
                .iter()
                .all(|track| { track.provider() == connection.provider_account_id.provider() })
        );
        Self {
            connection,
            inventory,
            outcomes: RefCell::new(outcomes.into_iter().collect()),
            calls: Cell::new(0),
        }
    }

    pub(crate) fn calls(&self) -> u8 {
        self.calls.get()
    }

    fn observe(&self, context: &AccountContext) -> Result<Vec<ProviderTrackId>, ClientError> {
        if context.account_id() != self.connection.account_id
            || context.provider_connection() != &self.connection
        {
            return Err(ClientError::new(ErrorCode::PermissionDenied, false));
        }
        if context
            .provider_capabilities()
            .status(ProviderCapability::LibraryInventoryRead)
            == CapabilityStatus::Unavailable
        {
            return Err(ClientError::new(ErrorCode::CapabilityUnavailable, false));
        }

        self.calls.set(self.calls.get() + 1);
        match self
            .outcomes
            .borrow_mut()
            .pop_front()
            .unwrap_or(FakeProviderOutcome::Inventory)
        {
            FakeProviderOutcome::Inventory => Ok(self.inventory.clone()),
            FakeProviderOutcome::TemporarilyUnavailable => {
                let mut error = ClientError::new(ErrorCode::DependencyUnavailable, true);
                error.retry_after_seconds = Some(1);
                Err(error)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct SubmissionKey {
    account_id: ChordriftAccountId,
    idempotency_key: IdempotencyKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RequestFingerprint {
    provider_connection_id: ProviderConnectionId,
    seed: u64,
}

#[derive(Clone, Debug)]
struct FakeOperation {
    context: AccountContext,
    cancellation_id: CancellationId,
    fingerprint: RequestFingerprint,
    retry_limit: NonZeroU8,
    attempts: u8,
    cancellation_requested: bool,
    state: OperationState,
    preview: Option<Vec<ProviderTrackId>>,
}

#[derive(Default)]
struct HarnessState {
    submissions: BTreeMap<SubmissionKey, OperationId>,
    operations: BTreeMap<OperationId, FakeOperation>,
    accepted: usize,
}

pub(crate) struct FakeApplication<'a> {
    providers: BTreeMap<ProviderConnectionId, &'a FakeProvider>,
    state: RefCell<HarnessState>,
}

impl<'a> FakeApplication<'a> {
    pub(crate) fn new(providers: impl IntoIterator<Item = &'a FakeProvider>) -> Self {
        Self {
            providers: providers
                .into_iter()
                .map(|provider| (provider.connection.connection_id, provider))
                .collect(),
            state: RefCell::new(HarnessState::default()),
        }
    }

    pub(crate) fn submit(
        &self,
        context: AccountContext,
        request: CommandRequest,
        seed: u64,
        retry_limit: NonZeroU8,
    ) -> Result<CommandReceipt, ClientError> {
        let Command::ObserveProvider {
            provider_connection_id,
        } = request.command
        else {
            return Err(ClientError::new(ErrorCode::InvalidRequest, false));
        };
        if request.contract_version != CONTRACT_VERSION {
            return Err(ClientError::new(ErrorCode::IncompatibleContract, false));
        }
        let selected_connection = context.provider_connection().connection_id;
        if provider_connection_id.as_uuid() != selected_connection.as_uuid() {
            return Err(ClientError::new(ErrorCode::PermissionDenied, false));
        }

        let key = SubmissionKey {
            account_id: context.account_id(),
            idempotency_key: request.idempotency_key,
        };
        let fingerprint = RequestFingerprint {
            provider_connection_id: selected_connection,
            seed,
        };
        let mut state = self.state.borrow_mut();
        if let Some(operation_id) = state.submissions.get(&key).copied() {
            let existing = state
                .operations
                .get(&operation_id)
                .expect("submission always has an operation");
            if existing.fingerprint != fingerprint {
                return Err(ClientError::new(ErrorCode::StateConflict, false));
            }
            return Ok(CommandReceipt {
                contract_version: CONTRACT_VERSION,
                request_id: request.request_id,
                operation_id,
                cancellation_id: existing.cancellation_id,
            });
        }

        let operation_id = OperationId::new();
        let cancellation_id = CancellationId::new();
        state.submissions.insert(key, operation_id);
        state.operations.insert(
            operation_id,
            FakeOperation {
                context,
                cancellation_id,
                fingerprint,
                retry_limit,
                attempts: 0,
                cancellation_requested: false,
                state: OperationState::Queued,
                preview: None,
            },
        );
        state.accepted += 1;
        Ok(CommandReceipt {
            contract_version: CONTRACT_VERSION,
            request_id: request.request_id,
            operation_id,
            cancellation_id,
        })
    }

    pub(crate) fn request_cancellation(
        &self,
        request: CancellationRequest,
    ) -> Result<CancellationOutcome, ClientError> {
        let mut state = self.state.borrow_mut();
        let operation = state
            .operations
            .get_mut(&request.operation_id)
            .ok_or_else(|| ClientError::new(ErrorCode::ResourceNotFound, false))?;
        if operation.cancellation_id != request.cancellation_id {
            return Err(ClientError::new(ErrorCode::PermissionDenied, false));
        }
        if operation.state.is_terminal() {
            return Ok(CancellationOutcome::TooLate);
        }
        operation.cancellation_requested = true;
        Ok(CancellationOutcome::Requested)
    }

    pub(crate) fn advance(&self, operation_id: OperationId) -> Result<OperationState, ClientError> {
        let mut state = self.state.borrow_mut();
        let operation = state
            .operations
            .get_mut(&operation_id)
            .ok_or_else(|| ClientError::new(ErrorCode::ResourceNotFound, false))?;
        if operation.state.is_terminal() {
            return Ok(operation.state.clone());
        }
        if operation.cancellation_requested {
            operation.state = OperationState::Cancelled;
            return Ok(operation.state.clone());
        }

        let connection_id = operation.context.provider_connection().connection_id;
        let provider = self
            .providers
            .get(&connection_id)
            .ok_or_else(|| ClientError::new(ErrorCode::ResourceNotFound, false))?;
        if operation
            .context
            .provider_capabilities()
            .status(ProviderCapability::LibraryInventoryRead)
            == CapabilityStatus::Unavailable
        {
            operation.state = OperationState::Failed {
                error: ClientError::new(ErrorCode::CapabilityUnavailable, false),
            };
            return Ok(operation.state.clone());
        }

        operation.attempts += 1;
        match provider.observe(&operation.context) {
            Ok(inventory) => {
                let preview = deterministic_preview(inventory, operation.fingerprint.seed);
                operation.preview = Some(preview);
                operation.state = OperationState::Completed { result_id: None };
            }
            Err(error) if error.retryable && operation.attempts < operation.retry_limit.get() => {
                operation.state = OperationState::Recoverable { error };
            }
            Err(error) => operation.state = OperationState::Failed { error },
        }
        Ok(operation.state.clone())
    }

    pub(crate) fn advance_invocation(
        &self,
        operation_id: OperationId,
    ) -> FakeAdvanceInvocation<'_, 'a> {
        FakeAdvanceInvocation {
            application: self,
            operation_id,
        }
    }

    pub(crate) fn preview(&self, operation_id: OperationId) -> Option<Vec<ProviderTrackId>> {
        self.state
            .borrow()
            .operations
            .get(&operation_id)
            .and_then(|operation| operation.preview.clone())
    }

    pub(crate) fn accepted(&self) -> usize {
        self.state.borrow().accepted
    }
}

pub(crate) struct FakeAdvanceInvocation<'application, 'provider> {
    application: &'application FakeApplication<'provider>,
    operation_id: OperationId,
}

impl ApplicationInvocation for FakeAdvanceInvocation<'_, '_> {
    type Output = std::result::Result<OperationState, ClientError>;

    fn execute(self) -> impl std::future::Future<Output = ChordriftResult<Self::Output>> {
        future::ready(Ok(self.application.advance(self.operation_id)))
    }
}

pub(crate) fn observe_request(
    connection_id: ProviderConnectionId,
    idempotency_key: IdempotencyKey,
) -> CommandRequest {
    CommandRequest {
        contract_version: CONTRACT_VERSION,
        request_id: Default::default(),
        idempotency_key,
        command: Command::ObserveProvider {
            provider_connection_id: ResourceId::from_uuid(connection_id.as_uuid()),
        },
    }
}

fn deterministic_preview(mut inventory: Vec<ProviderTrackId>, seed: u64) -> Vec<ProviderTrackId> {
    inventory.sort();
    if !inventory.is_empty() {
        let offset = (seed % inventory.len() as u64) as usize;
        inventory.rotate_left(offset);
    }
    inventory
}
