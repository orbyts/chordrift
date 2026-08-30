//! Restart-safe authenticated background-operation persistence.
//!
//! A typed application command is durably accepted before a worker claims it.
//! Expiring leases prevent concurrent execution, ordered lifecycle events let
//! clients reconnect, and account-scoped idempotency survives process restarts.
//! Provider credentials never enter this boundary.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use sha2::{Digest as _, Sha256};
use sqlx::{PgPool, Postgres, Row as _, Transaction, types::Json};
use uuid::Uuid;

use crate::{
    contract::{
        CONTRACT_VERSION, CancellationOutcome, CancellationRequest, ClientError, CommandReceipt,
        CommandRequest, ErrorCode, OperationEvent, OperationEventsView, OperationHistoryView,
        OperationId, OperationState, OperationView, Progress, QueryResponse, RequestId, ResourceId,
        View,
    },
    service::{AuthenticatedSubject, ServiceClock, SystemServiceClock},
};

const MAX_WORKER_NAME_BYTES: usize = 160;
const MAX_LEASE_SECONDS: u64 = 60 * 60;

/// Bounded automatic retry policy attached when a command is accepted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperationRetryPolicy {
    /// Total worker attempts, including the first attempt.
    pub max_attempts: u16,
    /// Default delay before a recoverable operation may be claimed again.
    pub retry_delay: Duration,
}

impl OperationRetryPolicy {
    /// Creates a bounded retry policy.
    pub fn new(max_attempts: u16, retry_delay: Duration) -> Result<Self, ClientError> {
        if max_attempts == 0
            || max_attempts > 100
            || retry_delay.as_secs() > 86_400
            || retry_delay.subsec_nanos() != 0
        {
            return Err(ClientError::new(ErrorCode::InvalidRequest, false));
        }
        Ok(Self {
            max_attempts,
            retry_delay,
        })
    }
}

/// Result of accepting a typed command into durable storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurableAcceptance {
    /// Stable receipt returned for every identical idempotent replay.
    pub receipt: CommandReceipt,
    /// Whether an existing operation was reused instead of inserting work.
    pub replayed: bool,
}

/// One exclusively leased typed command ready for a trusted worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurableOperationLease {
    /// Durable operation being executed.
    pub operation_id: OperationId,
    /// Unforgeable lease generation used for every worker update.
    pub lease_id: ResourceId,
    /// Authenticated account/subject that originally submitted the command.
    pub subject: AuthenticatedSubject,
    /// Exact typed application command accepted by the service.
    pub request: CommandRequest,
    /// One-based execution attempt.
    pub attempt: u16,
    /// Maximum attempts allowed by policy.
    pub max_attempts: u16,
    /// Time after which another worker may recover the abandoned lease.
    pub lease_expires_at: DateTime<Utc>,
}

/// Opaque values prepared by the Rust queue before persistence.
pub struct NewDurableOperation {
    /// Generated operation receipt.
    pub receipt: CommandReceipt,
    /// Authenticated submitting subject.
    pub subject: AuthenticatedSubject,
    /// Exact accepted command envelope.
    pub request: CommandRequest,
    /// SHA-256 of the canonical command payload.
    pub command_fingerprint: [u8; 32],
    /// Retry policy fixed at acceptance.
    pub retry_policy: OperationRetryPolicy,
    /// Acceptance time.
    pub accepted_at: DateTime<Utc>,
}

/// Durable infrastructure seam used by the Rust operation queue.
#[async_trait]
pub trait DurableOperationStore: Send + Sync {
    /// Inserts a new command or returns its exact existing idempotent receipt.
    async fn accept(
        &self,
        operation: NewDurableOperation,
    ) -> Result<DurableAcceptance, ClientError>;

    /// Recovers abandoned leases, then exclusively claims the next eligible command.
    async fn claim_next(
        &self,
        worker: &str,
        now: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DurableOperationLease>, ClientError>;

    /// Appends structured progress while retaining the active lease.
    async fn record_progress(
        &self,
        lease: &DurableOperationLease,
        progress: Progress,
        now: DateTime<Utc>,
    ) -> Result<(), ClientError>;

    /// Extends an active lease while preserving its exclusive generation.
    async fn renew_lease(
        &self,
        lease: &DurableOperationLease,
        now: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<(), ClientError>;

    /// Reports whether cooperative cancellation has been requested.
    async fn cancellation_requested(
        &self,
        lease: &DurableOperationLease,
        now: DateTime<Utc>,
    ) -> Result<bool, ClientError>;

    /// Completes a leased operation exactly once.
    async fn complete(
        &self,
        lease: &DurableOperationLease,
        result_id: Option<ResourceId>,
        now: DateTime<Utc>,
    ) -> Result<(), ClientError>;

    /// Records a retryable/recoverable failure or a terminal failure by policy.
    async fn fail(
        &self,
        lease: &DurableOperationLease,
        error: ClientError,
        now: DateTime<Utc>,
    ) -> Result<OperationState, ClientError>;

    /// Acknowledges cooperative cancellation at a safe worker checkpoint.
    async fn acknowledge_cancellation(
        &self,
        lease: &DurableOperationLease,
        now: DateTime<Utc>,
    ) -> Result<(), ClientError>;

    /// Requests cancellation using the authenticated receipt capability.
    async fn request_cancellation(
        &self,
        subject: AuthenticatedSubject,
        cancellation: CancellationRequest,
        now: DateTime<Utc>,
    ) -> Result<CancellationOutcome, ClientError>;

    /// Requeues expired leases or terminates them after the retry budget.
    async fn recover_expired(&self, now: DateTime<Utc>) -> Result<u64, ClientError>;

    /// Reads one authenticated operation view.
    async fn operation(
        &self,
        subject: AuthenticatedSubject,
        operation_id: OperationId,
    ) -> Result<OperationView, ClientError>;

    /// Reads ordered events after an operation-local cursor.
    async fn events(
        &self,
        subject: AuthenticatedSubject,
        operation_id: OperationId,
        after_sequence: Option<u64>,
    ) -> Result<OperationEventsView, ClientError>;

    /// Reads the authenticated subject's operation acceptance history.
    async fn history(
        &self,
        subject: AuthenticatedSubject,
    ) -> Result<OperationHistoryView, ClientError>;
}

/// Rust-owned durable command acceptance, worker, and reconnect boundary.
pub struct DurableOperationQueue<S> {
    store: Arc<S>,
    clock: Arc<dyn ServiceClock>,
}

impl<S> DurableOperationQueue<S>
where
    S: DurableOperationStore,
{
    /// Creates a queue using the production wall clock.
    pub fn new(store: Arc<S>) -> Self {
        Self::with_clock(store, Arc::new(SystemServiceClock))
    }

    /// Creates a queue with a deterministic clock.
    pub fn with_clock(store: Arc<S>, clock: Arc<dyn ServiceClock>) -> Self {
        Self { store, clock }
    }

    /// Durably accepts a typed command before any worker/provider action.
    pub async fn accept(
        &self,
        subject: AuthenticatedSubject,
        request: CommandRequest,
        retry_policy: OperationRetryPolicy,
    ) -> Result<DurableAcceptance, ClientError> {
        if request.contract_version != CONTRACT_VERSION {
            return Err(ClientError::new(ErrorCode::IncompatibleContract, false));
        }
        let command_fingerprint: [u8; 32] = Sha256::digest(
            serde_json::to_vec(&request.command)
                .map_err(|_| ClientError::new(ErrorCode::Internal, false))?,
        )
        .into();
        let receipt = CommandReceipt {
            contract_version: CONTRACT_VERSION,
            request_id: request.request_id,
            operation_id: OperationId::new(),
            cancellation_id: crate::contract::CancellationId::new(),
        };
        self.store
            .accept(NewDurableOperation {
                receipt,
                subject,
                request,
                command_fingerprint,
                retry_policy,
                accepted_at: self.clock.now(),
            })
            .await
    }

    /// Recovers expired work and claims one eligible command for a bounded lease.
    pub async fn claim_next(
        &self,
        worker: &str,
        lease_duration: Duration,
    ) -> Result<Option<DurableOperationLease>, ClientError> {
        validate_worker(worker)?;
        if lease_duration.is_zero()
            || lease_duration.as_secs() > MAX_LEASE_SECONDS
            || lease_duration.subsec_nanos() != 0
        {
            return Err(ClientError::new(ErrorCode::InvalidRequest, false));
        }
        let now = self.clock.now();
        self.store.recover_expired(now).await?;
        let seconds = i64::try_from(lease_duration.as_secs())
            .map_err(|_| ClientError::new(ErrorCode::InvalidRequest, false))?;
        self.store
            .claim_next(worker, now, now + TimeDelta::seconds(seconds))
            .await
    }

    /// Appends reconnectable structured progress for an active lease.
    pub async fn record_progress(
        &self,
        lease: &DurableOperationLease,
        progress: Progress,
    ) -> Result<(), ClientError> {
        self.store
            .record_progress(lease, progress, self.clock.now())
            .await
    }

    /// Heartbeats a long-running worker lease without adding a lifecycle event.
    pub async fn renew_lease(
        &self,
        lease: &DurableOperationLease,
        lease_duration: Duration,
    ) -> Result<(), ClientError> {
        if lease_duration.is_zero()
            || lease_duration.as_secs() > MAX_LEASE_SECONDS
            || lease_duration.subsec_nanos() != 0
        {
            return Err(invalid());
        }
        let now = self.clock.now();
        let seconds = i64::try_from(lease_duration.as_secs()).map_err(|_| invalid())?;
        self.store
            .renew_lease(lease, now, now + TimeDelta::seconds(seconds))
            .await
    }

    /// Reports whether a worker should stop at its next safe checkpoint.
    pub async fn cancellation_requested(
        &self,
        lease: &DurableOperationLease,
    ) -> Result<bool, ClientError> {
        self.store
            .cancellation_requested(lease, self.clock.now())
            .await
    }

    /// Records successful completion and clears the lease atomically.
    pub async fn complete(
        &self,
        lease: &DurableOperationLease,
        result_id: Option<ResourceId>,
    ) -> Result<(), ClientError> {
        self.store
            .complete(lease, result_id, self.clock.now())
            .await
    }

    /// Applies the persisted retry budget to one client-safe worker failure.
    pub async fn fail(
        &self,
        lease: &DurableOperationLease,
        error: ClientError,
    ) -> Result<OperationState, ClientError> {
        self.store.fail(lease, error, self.clock.now()).await
    }

    /// Acknowledges a pending cancellation at a safe worker checkpoint.
    pub async fn acknowledge_cancellation(
        &self,
        lease: &DurableOperationLease,
    ) -> Result<(), ClientError> {
        self.store
            .acknowledge_cancellation(lease, self.clock.now())
            .await
    }

    /// Requests cooperative cancellation without executing provider work.
    pub async fn request_cancellation(
        &self,
        subject: AuthenticatedSubject,
        cancellation: CancellationRequest,
    ) -> Result<CancellationOutcome, ClientError> {
        self.store
            .request_cancellation(subject, cancellation, self.clock.now())
            .await
    }

    /// Reads one operation using the same client-facing DTO as HTTP queries.
    pub async fn operation(
        &self,
        subject: AuthenticatedSubject,
        operation_id: OperationId,
    ) -> Result<OperationView, ClientError> {
        self.store.operation(subject, operation_id).await
    }

    /// Reads ordered operation events after a reconnect cursor.
    pub async fn events(
        &self,
        subject: AuthenticatedSubject,
        operation_id: OperationId,
        after_sequence: Option<u64>,
    ) -> Result<OperationEventsView, ClientError> {
        self.store
            .events(subject, operation_id, after_sequence)
            .await
    }

    /// Reads acceptance-ordered history for the authenticated subject/account.
    pub async fn history(
        &self,
        subject: AuthenticatedSubject,
    ) -> Result<OperationHistoryView, ClientError> {
        self.store.history(subject).await
    }
}

/// PostgreSQL implementation of restart-safe application work.
#[derive(Clone)]
pub struct PostgresDurableOperationStore {
    pool: PgPool,
}

impl PostgresDurableOperationStore {
    /// Creates a store over the hosted application pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Verifies migration 0050 before accepting durable work.
    pub async fn verify_schema(&self) -> Result<(), ClientError> {
        let ready: bool =
            sqlx::query_scalar("SELECT to_regclass('service_operations') IS NOT NULL")
                .fetch_one(&self.pool)
                .await
                .map_err(|_| unavailable())?;
        if ready {
            Ok(())
        } else {
            Err(ClientError::new(ErrorCode::DependencyUnavailable, false))
        }
    }
}

#[async_trait]
impl DurableOperationStore for PostgresDurableOperationStore {
    async fn accept(
        &self,
        operation: NewDurableOperation,
    ) -> Result<DurableAcceptance, ClientError> {
        let mut transaction = self.pool.begin().await.map_err(|_| unavailable())?;
        require_authority(&mut transaction, operation.subject).await?;
        let queued = OperationState::Queued;
        let payload = serde_json::to_value(&operation.request).map_err(|_| unavailable())?;
        let inserted: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO service_operations
             (id, chordrift_account_id, product_subject_id, request_id,
              cancellation_id, idempotency_key, command_fingerprint,
              command_payload, state_name, state_payload, max_attempts,
              retry_delay_seconds, next_attempt_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'queued', $9,
                     $10, $11, $12, $12, $12)
             ON CONFLICT (chordrift_account_id, product_subject_id, idempotency_key)
             DO NOTHING RETURNING id",
        )
        .bind(operation.receipt.operation_id.as_uuid())
        .bind(operation.subject.account_id.as_uuid())
        .bind(operation.subject.subject_id.as_uuid())
        .bind(operation.receipt.request_id.as_uuid())
        .bind(operation.receipt.cancellation_id.as_uuid())
        .bind(operation.request.idempotency_key.as_uuid())
        .bind(operation.command_fingerprint.as_slice())
        .bind(Json(payload))
        .bind(Json(&queued))
        .bind(i32::from(operation.retry_policy.max_attempts))
        .bind(i32::try_from(operation.retry_policy.retry_delay.as_secs()).map_err(|_| invalid())?)
        .bind(operation.accepted_at)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| unavailable())?;
        if inserted.is_some() {
            append_event(
                &mut transaction,
                operation.receipt.operation_id,
                1,
                &queued,
                operation.accepted_at,
            )
            .await?;
            transaction.commit().await.map_err(|_| unavailable())?;
            return Ok(DurableAcceptance {
                receipt: operation.receipt,
                replayed: false,
            });
        }
        let row = sqlx::query(
            "SELECT id, request_id, cancellation_id, command_fingerprint
             FROM service_operations
             WHERE chordrift_account_id = $1 AND product_subject_id = $2
               AND idempotency_key = $3",
        )
        .bind(operation.subject.account_id.as_uuid())
        .bind(operation.subject.subject_id.as_uuid())
        .bind(operation.request.idempotency_key.as_uuid())
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| unavailable())?;
        let fingerprint: Vec<u8> = row
            .try_get("command_fingerprint")
            .map_err(|_| unavailable())?;
        if fingerprint.as_slice() != operation.command_fingerprint {
            return Err(ClientError::new(ErrorCode::StateConflict, false));
        }
        let receipt = CommandReceipt {
            contract_version: CONTRACT_VERSION,
            request_id: RequestId::from_uuid(row.try_get("request_id").map_err(|_| unavailable())?),
            operation_id: OperationId::from_uuid(row.try_get("id").map_err(|_| unavailable())?),
            cancellation_id: crate::contract::CancellationId::from_uuid(
                row.try_get("cancellation_id").map_err(|_| unavailable())?,
            ),
        };
        transaction.commit().await.map_err(|_| unavailable())?;
        Ok(DurableAcceptance {
            receipt,
            replayed: true,
        })
    }

    async fn claim_next(
        &self,
        worker: &str,
        now: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<Option<DurableOperationLease>, ClientError> {
        let mut transaction = self.pool.begin().await.map_err(|_| unavailable())?;
        let row = sqlx::query(
            "SELECT operation.id, operation.chordrift_account_id,
                    operation.product_subject_id, operation.command_payload,
                    operation.attempt, operation.max_attempts
             FROM service_operations operation
             JOIN chordrift_accounts account
               ON account.id = operation.chordrift_account_id
             JOIN chordrift_account_memberships membership
               ON membership.chordrift_account_id = operation.chordrift_account_id
              AND membership.product_subject_id = operation.product_subject_id
             JOIN product_subjects subject
               ON subject.id = operation.product_subject_id
             WHERE operation.state_name IN ('queued', 'recoverable')
               AND operation.next_attempt_at <= $1
               AND operation.cancellation_requested_at IS NULL
               AND account.status = 'active' AND membership.status = 'active'
               AND subject.status = 'active'
             ORDER BY operation.next_attempt_at, operation.created_at, operation.id
             FOR UPDATE OF operation SKIP LOCKED LIMIT 1",
        )
        .bind(now)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| unavailable())?;
        let Some(row) = row else {
            transaction.commit().await.map_err(|_| unavailable())?;
            return Ok(None);
        };
        let operation_id = OperationId::from_uuid(row.try_get("id").map_err(|_| unavailable())?);
        let attempt: i32 = row.try_get("attempt").map_err(|_| unavailable())?;
        let attempt = attempt.saturating_add(1);
        let max_attempts: i32 = row.try_get("max_attempts").map_err(|_| unavailable())?;
        let lease_id = ResourceId::new();
        let state = OperationState::Running {
            progress: Some(
                Progress::new("claimed", 0, None, crate::contract::ProgressUnit::Steps)
                    .map_err(|_| unavailable())?,
            ),
        };
        let sequence = next_sequence(&mut transaction, operation_id).await?;
        sqlx::query(
            "UPDATE service_operations
             SET state_name = 'running', state_payload = $2, attempt = $3,
                 lease_id = $4, lease_owner = $5, lease_expires_at = $6,
                 updated_at = $7
             WHERE id = $1",
        )
        .bind(operation_id.as_uuid())
        .bind(Json(&state))
        .bind(attempt)
        .bind(lease_id.as_uuid())
        .bind(worker)
        .bind(lease_expires_at)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(|_| unavailable())?;
        append_event(&mut transaction, operation_id, sequence, &state, now).await?;
        let request: Json<CommandRequest> =
            row.try_get("command_payload").map_err(|_| unavailable())?;
        let lease = DurableOperationLease {
            operation_id,
            lease_id,
            subject: AuthenticatedSubject {
                account_id: ResourceId::from_uuid(
                    row.try_get("chordrift_account_id")
                        .map_err(|_| unavailable())?,
                ),
                subject_id: ResourceId::from_uuid(
                    row.try_get("product_subject_id")
                        .map_err(|_| unavailable())?,
                ),
            },
            request: request.0,
            attempt: u16::try_from(attempt).map_err(|_| unavailable())?,
            max_attempts: u16::try_from(max_attempts).map_err(|_| unavailable())?,
            lease_expires_at,
        };
        transaction.commit().await.map_err(|_| unavailable())?;
        Ok(Some(lease))
    }

    async fn record_progress(
        &self,
        lease: &DurableOperationLease,
        progress: Progress,
        now: DateTime<Utc>,
    ) -> Result<(), ClientError> {
        let state = OperationState::Running {
            progress: Some(progress),
        };
        update_leased_state(&self.pool, lease, &state, now, false, None).await
    }

    async fn renew_lease(
        &self,
        lease: &DurableOperationLease,
        now: DateTime<Utc>,
        lease_expires_at: DateTime<Utc>,
    ) -> Result<(), ClientError> {
        let result = sqlx::query(
            "UPDATE service_operations SET lease_expires_at = $3, updated_at = $4
             WHERE id = $1 AND lease_id = $2 AND state_name = 'running'
               AND lease_expires_at > $4 AND cancellation_requested_at IS NULL",
        )
        .bind(lease.operation_id.as_uuid())
        .bind(lease.lease_id.as_uuid())
        .bind(lease_expires_at)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|_| unavailable())?;
        if result.rows_affected() == 1 {
            Ok(())
        } else {
            Err(conflict())
        }
    }

    async fn cancellation_requested(
        &self,
        lease: &DurableOperationLease,
        now: DateTime<Utc>,
    ) -> Result<bool, ClientError> {
        let requested: Option<bool> = sqlx::query_scalar(
            "SELECT cancellation_requested_at IS NOT NULL
             FROM service_operations
             WHERE id = $1 AND lease_id = $2 AND state_name = 'running'
               AND lease_expires_at > $3",
        )
        .bind(lease.operation_id.as_uuid())
        .bind(lease.lease_id.as_uuid())
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| unavailable())?;
        requested.ok_or_else(conflict)
    }

    async fn complete(
        &self,
        lease: &DurableOperationLease,
        result_id: Option<ResourceId>,
        now: DateTime<Utc>,
    ) -> Result<(), ClientError> {
        let state = OperationState::Completed { result_id };
        update_leased_state(&self.pool, lease, &state, now, true, None).await
    }

    async fn fail(
        &self,
        lease: &DurableOperationLease,
        error: ClientError,
        now: DateTime<Utc>,
    ) -> Result<OperationState, ClientError> {
        let row = sqlx::query(
            "SELECT attempt, max_attempts, retry_delay_seconds
             FROM service_operations
             WHERE id = $1 AND lease_id = $2 AND state_name = 'running'
               AND lease_expires_at > $3",
        )
        .bind(lease.operation_id.as_uuid())
        .bind(lease.lease_id.as_uuid())
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(conflict)?;
        let attempt: i32 = row.try_get("attempt").map_err(|_| unavailable())?;
        let max_attempts: i32 = row.try_get("max_attempts").map_err(|_| unavailable())?;
        let default_delay: i32 = row
            .try_get("retry_delay_seconds")
            .map_err(|_| unavailable())?;
        if error.retryable && attempt < max_attempts {
            let delay = i64::from(
                error
                    .retry_after_seconds
                    .unwrap_or(u32::try_from(default_delay).map_err(|_| unavailable())?),
            );
            let state = OperationState::Recoverable { error };
            update_leased_state(
                &self.pool,
                lease,
                &state,
                now,
                false,
                Some(now + TimeDelta::seconds(delay)),
            )
            .await?;
            Ok(state)
        } else {
            let state = OperationState::Failed { error };
            update_leased_state(&self.pool, lease, &state, now, true, None).await?;
            Ok(state)
        }
    }

    async fn acknowledge_cancellation(
        &self,
        lease: &DurableOperationLease,
        now: DateTime<Utc>,
    ) -> Result<(), ClientError> {
        let requested: bool = sqlx::query_scalar(
            "SELECT cancellation_requested_at IS NOT NULL
             FROM service_operations
             WHERE id = $1 AND lease_id = $2 AND state_name = 'running'
               AND lease_expires_at > $3",
        )
        .bind(lease.operation_id.as_uuid())
        .bind(lease.lease_id.as_uuid())
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(conflict)?;
        if !requested {
            return Err(conflict());
        }
        update_leased_state(
            &self.pool,
            lease,
            &OperationState::Cancelled,
            now,
            true,
            None,
        )
        .await
    }

    async fn request_cancellation(
        &self,
        subject: AuthenticatedSubject,
        cancellation: CancellationRequest,
        now: DateTime<Utc>,
    ) -> Result<CancellationOutcome, ClientError> {
        let mut transaction = self.pool.begin().await.map_err(|_| unavailable())?;
        require_authority(&mut transaction, subject).await?;
        let row = sqlx::query(
            "SELECT state_name, cancellation_id
             FROM service_operations
             WHERE id = $1 AND chordrift_account_id = $2
               AND product_subject_id = $3 FOR UPDATE",
        )
        .bind(cancellation.operation_id.as_uuid())
        .bind(subject.account_id.as_uuid())
        .bind(subject.subject_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(|| ClientError::new(ErrorCode::ResourceNotFound, false))?;
        let cancellation_id: Uuid = row.try_get("cancellation_id").map_err(|_| unavailable())?;
        if cancellation_id != cancellation.cancellation_id.as_uuid() {
            return Err(ClientError::new(ErrorCode::PermissionDenied, false));
        }
        let state_name: String = row.try_get("state_name").map_err(|_| unavailable())?;
        if matches!(state_name.as_str(), "completed" | "failed" | "cancelled") {
            transaction.commit().await.map_err(|_| unavailable())?;
            return Ok(CancellationOutcome::TooLate);
        }
        if state_name == "running" {
            sqlx::query(
                "UPDATE service_operations
                 SET cancellation_requested_at = COALESCE(cancellation_requested_at, $2),
                     updated_at = $2 WHERE id = $1",
            )
            .bind(cancellation.operation_id.as_uuid())
            .bind(now)
            .execute(&mut *transaction)
            .await
            .map_err(|_| unavailable())?;
            transaction.commit().await.map_err(|_| unavailable())?;
            return Ok(CancellationOutcome::Requested);
        }
        let state = OperationState::Cancelled;
        let sequence = next_sequence(&mut transaction, cancellation.operation_id).await?;
        sqlx::query(
            "UPDATE service_operations
             SET state_name = 'cancelled', state_payload = $2,
                 cancellation_requested_at = $3, lease_id = NULL,
                 lease_owner = NULL, lease_expires_at = NULL,
                 updated_at = $3, finished_at = $3 WHERE id = $1",
        )
        .bind(cancellation.operation_id.as_uuid())
        .bind(Json(&state))
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(|_| unavailable())?;
        append_event(
            &mut transaction,
            cancellation.operation_id,
            sequence,
            &state,
            now,
        )
        .await?;
        transaction.commit().await.map_err(|_| unavailable())?;
        Ok(CancellationOutcome::Cancelled)
    }

    async fn recover_expired(&self, now: DateTime<Utc>) -> Result<u64, ClientError> {
        let mut transaction = self.pool.begin().await.map_err(|_| unavailable())?;
        let rows = sqlx::query(
            "SELECT id, attempt, max_attempts, retry_delay_seconds,
                    cancellation_requested_at IS NOT NULL AS cancel_requested
             FROM service_operations
             WHERE state_name = 'running' AND lease_expires_at <= $1
             ORDER BY lease_expires_at, id FOR UPDATE SKIP LOCKED",
        )
        .bind(now)
        .fetch_all(&mut *transaction)
        .await
        .map_err(|_| unavailable())?;
        for row in &rows {
            let operation_id =
                OperationId::from_uuid(row.try_get("id").map_err(|_| unavailable())?);
            let attempt: i32 = row.try_get("attempt").map_err(|_| unavailable())?;
            let max_attempts: i32 = row.try_get("max_attempts").map_err(|_| unavailable())?;
            let delay: i32 = row
                .try_get("retry_delay_seconds")
                .map_err(|_| unavailable())?;
            let cancel_requested: bool =
                row.try_get("cancel_requested").map_err(|_| unavailable())?;
            let state = if cancel_requested {
                OperationState::Cancelled
            } else if attempt < max_attempts {
                OperationState::Recoverable {
                    error: ClientError::new(ErrorCode::DependencyUnavailable, true),
                }
            } else {
                OperationState::Failed {
                    error: ClientError::new(ErrorCode::DependencyUnavailable, false),
                }
            };
            let terminal = state.is_terminal();
            let next_attempt_at = if matches!(state, OperationState::Recoverable { .. }) {
                now + TimeDelta::seconds(i64::from(delay))
            } else {
                now
            };
            let sequence = next_sequence(&mut transaction, operation_id).await?;
            sqlx::query(
                "UPDATE service_operations
                 SET state_name = $2, state_payload = $3, lease_id = NULL,
                     lease_owner = NULL, lease_expires_at = NULL,
                     next_attempt_at = $4, updated_at = $5,
                     finished_at = CASE WHEN $6 THEN $5 ELSE NULL END
                 WHERE id = $1",
            )
            .bind(operation_id.as_uuid())
            .bind(state_name(&state))
            .bind(Json(&state))
            .bind(next_attempt_at)
            .bind(now)
            .bind(terminal)
            .execute(&mut *transaction)
            .await
            .map_err(|_| unavailable())?;
            append_event(&mut transaction, operation_id, sequence, &state, now).await?;
        }
        let count = u64::try_from(rows.len()).map_err(|_| unavailable())?;
        transaction.commit().await.map_err(|_| unavailable())?;
        Ok(count)
    }

    async fn operation(
        &self,
        subject: AuthenticatedSubject,
        operation_id: OperationId,
    ) -> Result<OperationView, ClientError> {
        require_pool_authority(&self.pool, subject).await?;
        load_operation(&self.pool, subject, operation_id).await
    }

    async fn events(
        &self,
        subject: AuthenticatedSubject,
        operation_id: OperationId,
        after_sequence: Option<u64>,
    ) -> Result<OperationEventsView, ClientError> {
        require_pool_authority(&self.pool, subject).await?;
        load_operation(&self.pool, subject, operation_id).await?;
        let cursor = i64::try_from(after_sequence.unwrap_or(0)).map_err(|_| invalid())?;
        let rows = sqlx::query(
            "SELECT sequence, occurred_at, state_payload
             FROM service_operation_events
             WHERE operation_id = $1 AND sequence > $2 ORDER BY sequence",
        )
        .bind(operation_id.as_uuid())
        .bind(cursor)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| unavailable())?;
        let events = rows
            .into_iter()
            .map(|row| {
                let state: Json<OperationState> =
                    row.try_get("state_payload").map_err(|_| unavailable())?;
                let sequence: i64 = row.try_get("sequence").map_err(|_| unavailable())?;
                Ok(OperationEvent {
                    contract_version: CONTRACT_VERSION,
                    operation_id,
                    sequence: u64::try_from(sequence).map_err(|_| unavailable())?,
                    occurred_at: row.try_get("occurred_at").map_err(|_| unavailable())?,
                    state: state.0,
                })
            })
            .collect::<Result<Vec<_>, ClientError>>()?;
        Ok(OperationEventsView {
            operation_id,
            events,
        })
    }

    async fn history(
        &self,
        subject: AuthenticatedSubject,
    ) -> Result<OperationHistoryView, ClientError> {
        require_pool_authority(&self.pool, subject).await?;
        let rows = sqlx::query(
            "SELECT id, cancellation_id, state_payload
             FROM service_operations
             WHERE chordrift_account_id = $1 AND product_subject_id = $2
             ORDER BY created_at, id",
        )
        .bind(subject.account_id.as_uuid())
        .bind(subject.subject_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|_| unavailable())?;
        let operations = rows
            .into_iter()
            .map(|row| operation_view_from_row(&row))
            .collect::<Result<Vec<_>, ClientError>>()?;
        Ok(OperationHistoryView { operations })
    }
}

async fn update_leased_state(
    pool: &PgPool,
    lease: &DurableOperationLease,
    state: &OperationState,
    now: DateTime<Utc>,
    terminal: bool,
    next_attempt_at: Option<DateTime<Utc>>,
) -> Result<(), ClientError> {
    let mut transaction = pool.begin().await.map_err(|_| unavailable())?;
    let sequence = next_sequence_for_lease(&mut transaction, lease, now).await?;
    let release_lease = terminal || matches!(state, OperationState::Recoverable { .. });
    let result = sqlx::query(
        "UPDATE service_operations
         SET state_name = $3, state_payload = $4,
             lease_id = CASE WHEN $5 THEN NULL ELSE lease_id END,
             lease_owner = CASE WHEN $5 THEN NULL ELSE lease_owner END,
             lease_expires_at = CASE WHEN $5 THEN NULL ELSE lease_expires_at END,
             next_attempt_at = COALESCE($6, next_attempt_at), updated_at = $7,
             finished_at = CASE WHEN $8 THEN $7 ELSE NULL END
         WHERE id = $1 AND lease_id = $2 AND state_name = 'running'
           AND lease_expires_at > $7",
    )
    .bind(lease.operation_id.as_uuid())
    .bind(lease.lease_id.as_uuid())
    .bind(state_name(state))
    .bind(Json(state))
    .bind(release_lease)
    .bind(next_attempt_at)
    .bind(now)
    .bind(terminal)
    .execute(&mut *transaction)
    .await
    .map_err(|_| unavailable())?;
    if result.rows_affected() != 1 {
        return Err(conflict());
    }
    append_event(&mut transaction, lease.operation_id, sequence, state, now).await?;
    transaction.commit().await.map_err(|_| unavailable())
}

async fn next_sequence_for_lease(
    transaction: &mut Transaction<'_, Postgres>,
    lease: &DurableOperationLease,
    now: DateTime<Utc>,
) -> Result<i64, ClientError> {
    let sequence: Option<i64> = sqlx::query_scalar(
        "SELECT COALESCE(max(event.sequence), 0) + 1
         FROM service_operation_events event
         JOIN service_operations operation ON operation.id = event.operation_id
         WHERE operation.id = $1 AND operation.lease_id = $2
           AND operation.state_name = 'running' AND operation.lease_expires_at > $3",
    )
    .bind(lease.operation_id.as_uuid())
    .bind(lease.lease_id.as_uuid())
    .bind(now)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|_| unavailable())?;
    sequence.ok_or_else(conflict)
}

async fn next_sequence(
    transaction: &mut Transaction<'_, Postgres>,
    operation_id: OperationId,
) -> Result<i64, ClientError> {
    sqlx::query_scalar(
        "SELECT COALESCE(max(sequence), 0) + 1
         FROM service_operation_events WHERE operation_id = $1",
    )
    .bind(operation_id.as_uuid())
    .fetch_one(&mut **transaction)
    .await
    .map_err(|_| unavailable())
}

async fn append_event(
    transaction: &mut Transaction<'_, Postgres>,
    operation_id: OperationId,
    sequence: i64,
    state: &OperationState,
    occurred_at: DateTime<Utc>,
) -> Result<(), ClientError> {
    sqlx::query(
        "INSERT INTO service_operation_events
         (operation_id, sequence, occurred_at, state_name, state_payload)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(operation_id.as_uuid())
    .bind(sequence)
    .bind(occurred_at)
    .bind(state_name(state))
    .bind(Json(state))
    .execute(&mut **transaction)
    .await
    .map_err(|_| unavailable())?;
    Ok(())
}

async fn load_operation(
    pool: &PgPool,
    subject: AuthenticatedSubject,
    operation_id: OperationId,
) -> Result<OperationView, ClientError> {
    let row = sqlx::query(
        "SELECT id, cancellation_id, state_payload FROM service_operations
         WHERE id = $1 AND chordrift_account_id = $2 AND product_subject_id = $3",
    )
    .bind(operation_id.as_uuid())
    .bind(subject.account_id.as_uuid())
    .bind(subject.subject_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(|_| unavailable())?
    .ok_or_else(|| ClientError::new(ErrorCode::ResourceNotFound, false))?;
    operation_view_from_row(&row)
}

fn operation_view_from_row(row: &sqlx::postgres::PgRow) -> Result<OperationView, ClientError> {
    let state: Json<OperationState> = row.try_get("state_payload").map_err(|_| unavailable())?;
    Ok(OperationView {
        operation_id: OperationId::from_uuid(row.try_get("id").map_err(|_| unavailable())?),
        cancellation_id: crate::contract::CancellationId::from_uuid(
            row.try_get("cancellation_id").map_err(|_| unavailable())?,
        ),
        state: state.0,
    })
}

async fn require_pool_authority(
    pool: &PgPool,
    subject: AuthenticatedSubject,
) -> Result<(), ClientError> {
    let mut transaction = pool.begin().await.map_err(|_| unavailable())?;
    require_authority(&mut transaction, subject).await?;
    transaction.commit().await.map_err(|_| unavailable())
}

async fn require_authority(
    transaction: &mut Transaction<'_, Postgres>,
    caller: AuthenticatedSubject,
) -> Result<(), ClientError> {
    let authorized: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM chordrift_accounts account
             JOIN chordrift_account_memberships membership
               ON membership.chordrift_account_id = account.id
              AND membership.product_subject_id = $1
             JOIN product_subjects subject ON subject.id = membership.product_subject_id
             WHERE account.id = $2 AND account.status = 'active'
               AND membership.status = 'active' AND subject.status = 'active')",
    )
    .bind(caller.subject_id.as_uuid())
    .bind(caller.account_id.as_uuid())
    .fetch_one(&mut **transaction)
    .await
    .map_err(|_| unavailable())?;
    if authorized {
        Ok(())
    } else {
        Err(ClientError::new(ErrorCode::PermissionDenied, false))
    }
}

fn state_name(state: &OperationState) -> &'static str {
    match state {
        OperationState::Queued => "queued",
        OperationState::Running { .. } => "running",
        OperationState::Waiting { .. } => "waiting",
        OperationState::Completed { .. } => "completed",
        OperationState::Failed { .. } => "failed",
        OperationState::Cancelled => "cancelled",
        OperationState::Recoverable { .. } => "recoverable",
    }
}

fn validate_worker(worker: &str) -> Result<(), ClientError> {
    if worker.trim().is_empty()
        || worker.len() > MAX_WORKER_NAME_BYTES
        || worker.chars().any(char::is_control)
    {
        Err(invalid())
    } else {
        Ok(())
    }
}

fn invalid() -> ClientError {
    ClientError::new(ErrorCode::InvalidRequest, false)
}

fn conflict() -> ClientError {
    ClientError::new(ErrorCode::StateConflict, false)
}

fn unavailable() -> ClientError {
    ClientError::new(ErrorCode::DependencyUnavailable, true)
}

/// Builds the existing typed operation query response from durable storage.
pub fn operation_query_response(
    request_id: RequestId,
    generated_at: DateTime<Utc>,
    operation: OperationView,
) -> QueryResponse {
    QueryResponse::Operation(View {
        contract_version: CONTRACT_VERSION,
        request_id,
        generated_at,
        value: operation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_policy_is_bounded_and_uses_integral_seconds() {
        assert!(OperationRetryPolicy::new(1, Duration::ZERO).is_ok());
        assert!(OperationRetryPolicy::new(100, Duration::from_secs(86_400)).is_ok());
        assert_eq!(
            OperationRetryPolicy::new(0, Duration::ZERO)
                .expect_err("zero attempts are invalid")
                .code,
            ErrorCode::InvalidRequest
        );
        assert!(OperationRetryPolicy::new(101, Duration::ZERO).is_err());
        assert!(OperationRetryPolicy::new(1, Duration::from_millis(1)).is_err());
        assert!(OperationRetryPolicy::new(1, Duration::from_secs(86_401)).is_err());
    }

    #[test]
    fn worker_identity_is_bounded_and_safe_for_diagnostics() {
        assert!(validate_worker("worker-us-west-2/03").is_ok());
        assert!(validate_worker("").is_err());
        assert!(validate_worker("worker\nsecret").is_err());
        assert!(validate_worker(&"w".repeat(MAX_WORKER_NAME_BYTES + 1)).is_err());
    }

    #[test]
    fn persisted_state_names_are_stable() {
        assert_eq!(state_name(&OperationState::Queued), "queued");
        assert_eq!(
            state_name(&OperationState::Recoverable {
                error: ClientError::new(ErrorCode::DependencyUnavailable, true),
            }),
            "recoverable"
        );
        assert_eq!(state_name(&OperationState::Cancelled), "cancelled");
    }
}
