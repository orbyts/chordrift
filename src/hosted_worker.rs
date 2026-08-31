//! Durable hosted provider-operation worker.
//!
//! The API persists typed commands before this process claims them. Only this
//! trusted server process may lease encrypted provider credentials. It calls
//! Rust provider/database adapters directly and never invokes the Chordrift
//! CLI, a shell, arbitrary SQL supplied by a client, or a client-supplied URL.

use std::{env, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::Utc;
use sqlx::Row as _;
use storexa::Database;

use crate::{
    ChordriftError, Result, config,
    contract::{
        ClientError, Command, ErrorCode, MaintenanceDecision, MaintenanceSessionId, OperationId,
        Progress, ProgressUnit, ResourceId,
    },
    db,
    durable_operations::{
        DurableOperationLease, DurableOperationQueue, PostgresDurableOperationStore,
    },
    maintenance::MaintenanceDecisionProjection,
    maintenance_interpretation::PostgresMaintenanceInterpreter,
    maintenance_store::{DurableMaintenanceAuthority, PostgresMaintenanceSessionStore},
    provider_vault::{
        PostgresProviderCredentialStore, ProviderCredentialIdentity, ProviderCredentialVault,
        ProviderVaultKeyring,
    },
    providers::spotify,
    service::AuthenticatedSubject,
};

const DEFAULT_POLL_MILLISECONDS: u64 = 750;
const LEASE_DURATION: Duration = Duration::from_secs(120);

/// Provider-side work accepted by the durable hosted command boundary.
#[async_trait]
pub trait HostedProviderExecutor: Send + Sync {
    /// Reads and persists one complete provider observation.
    async fn observe(
        &self,
        subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
    ) -> std::result::Result<ResourceId, ClientError>;

    /// Starts a durable record-only maintenance interpretation.
    async fn start_maintenance(
        &self,
        _subject: AuthenticatedSubject,
        _operation_id: OperationId,
        _session_id: MaintenanceSessionId,
        _provider_connection_id: ResourceId,
    ) -> std::result::Result<ResourceId, ClientError> {
        Err(ClientError::new(ErrorCode::CapabilityUnavailable, false))
    }

    /// Rebases a durable session onto the newest already-observed state.
    async fn refresh_maintenance(
        &self,
        _subject: AuthenticatedSubject,
        _operation_id: OperationId,
        _session_id: MaintenanceSessionId,
        _expected_revision: u64,
    ) -> std::result::Result<ResourceId, ClientError> {
        Err(ClientError::new(ErrorCode::CapabilityUnavailable, false))
    }

    /// Persists explicit ambiguity decisions without provider effects.
    async fn resolve_maintenance(
        &self,
        _subject: AuthenticatedSubject,
        _operation_id: OperationId,
        _session_id: MaintenanceSessionId,
        _expected_revision: u64,
        _decisions: Vec<MaintenanceDecision>,
    ) -> std::result::Result<ResourceId, ClientError> {
        Err(ClientError::new(ErrorCode::CapabilityUnavailable, false))
    }
}

/// Production Spotify/Neon executor using an encrypted credential vault.
pub struct SpotifyObservationExecutor {
    database: Database,
    vault: ProviderCredentialVault<PostgresProviderCredentialStore>,
    sessions: PostgresMaintenanceSessionStore,
}

impl SpotifyObservationExecutor {
    /// Builds the executor after the caller verifies schemas 0049 through 0051.
    pub fn new(
        database: Database,
        vault: ProviderCredentialVault<PostgresProviderCredentialStore>,
    ) -> Self {
        let sessions = PostgresMaintenanceSessionStore::new(database.pool().clone());
        Self {
            database,
            vault,
            sessions,
        }
    }
}

#[async_trait]
impl HostedProviderExecutor for SpotifyObservationExecutor {
    async fn observe(
        &self,
        subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
    ) -> std::result::Result<ResourceId, ClientError> {
        let row = sqlx::query(
            "SELECT account.account_label, account.provider,
                    account.provider_account_id
               FROM provider_accounts account
              WHERE account.id = $1 AND account.chordrift_account_id = $2",
        )
        .bind(provider_connection_id.as_uuid())
        .bind(subject.account_id.as_uuid())
        .fetch_optional(self.database.pool())
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(|| ClientError::new(ErrorCode::PermissionDenied, false))?;
        let provider: String = row.try_get("provider").map_err(|_| unavailable())?;
        if provider != "spotify" {
            return Err(ClientError::new(ErrorCode::CapabilityUnavailable, false));
        }
        let account_label: String = row.try_get("account_label").map_err(|_| unavailable())?;
        let stable_provider_id: String = row
            .try_get("provider_account_id")
            .map_err(|_| unavailable())?;
        let identity =
            ProviderCredentialIdentity::new(subject.account_id, provider_connection_id, provider)?;
        let lease = self.vault.lease(subject, &identity).await?;
        let (session, rotated) =
            spotify::hosted_session(lease.refresh_token(), lease.scopes(), &stable_provider_id)
                .await
                .map_err(provider_error)?;
        if let Some(rotated) = rotated.as_ref() {
            self.vault
                .rotate(subject, identity, rotated, Utc::now())
                .await?;
        }
        let report = spotify::import_hosted(&account_label, &self.database, session)
            .await
            .map_err(provider_error)?;
        Ok(ResourceId::from_uuid(report.snapshot_id))
    }

    async fn start_maintenance(
        &self,
        subject: AuthenticatedSubject,
        operation_id: OperationId,
        session_id: MaintenanceSessionId,
        provider_connection_id: ResourceId,
    ) -> std::result::Result<ResourceId, ClientError> {
        self.observe(subject, provider_connection_id).await?;
        let projection = PostgresMaintenanceInterpreter::new(&self.database)
            .project(subject, provider_connection_id, None)
            .await?;
        DurableMaintenanceAuthority::new(self.sessions.clone())
            .start(
                subject,
                provider_connection_id,
                session_id,
                projection,
                Some(operation_id),
                Utc::now(),
            )
            .await?;
        Ok(ResourceId::from_uuid(session_id.as_uuid()))
    }

    async fn refresh_maintenance(
        &self,
        subject: AuthenticatedSubject,
        operation_id: OperationId,
        session_id: MaintenanceSessionId,
        expected_revision: u64,
    ) -> std::result::Result<ResourceId, ClientError> {
        let current = self.sessions.load(subject, session_id).await?;
        let observed_snapshot = self
            .observe(subject, current.provider_connection_id)
            .await?;
        if observed_snapshot == current.view.provider_snapshot_id {
            return Ok(ResourceId::from_uuid(session_id.as_uuid()));
        }
        let projection = PostgresMaintenanceInterpreter::new(&self.database)
            .project(subject, current.provider_connection_id, Some(&current.view))
            .await?;
        DurableMaintenanceAuthority::new(self.sessions.clone())
            .refresh(
                subject,
                session_id,
                expected_revision,
                projection,
                Some(operation_id),
                Utc::now(),
            )
            .await?;
        Ok(ResourceId::from_uuid(session_id.as_uuid()))
    }

    async fn resolve_maintenance(
        &self,
        subject: AuthenticatedSubject,
        operation_id: OperationId,
        session_id: MaintenanceSessionId,
        expected_revision: u64,
        decisions: Vec<MaintenanceDecision>,
    ) -> std::result::Result<ResourceId, ClientError> {
        DurableMaintenanceAuthority::new(self.sessions.clone())
            .resolve(
                subject,
                session_id,
                expected_revision,
                decisions,
                MaintenanceDecisionProjection {
                    provider_effects: Vec::new(),
                    review_id: None,
                },
                Some(operation_id),
                Utc::now(),
            )
            .await?;
        Ok(ResourceId::from_uuid(session_id.as_uuid()))
    }
}

/// Runs the separate provider worker until process shutdown.
pub async fn run_from_env() -> Result<()> {
    let worker_name = required("CHORDRIFT_WORKER_NAME")?;
    let database = db::connect(config::database_config_from_env()?).await?;
    db::require_schema_through(&database, 51).await?;
    let pool = database.pool().clone();
    let credential_store = PostgresProviderCredentialStore::new(pool.clone());
    credential_store
        .verify_schema()
        .await
        .map_err(|_| configuration("hosted provider credential schema is not ready"))?;
    let keyring = ProviderVaultKeyring::from_environment()
        .map_err(|_| configuration("hosted provider credential key is not ready"))?;
    let operation_store = Arc::new(PostgresDurableOperationStore::new(pool));
    operation_store
        .verify_schema()
        .await
        .map_err(|_| configuration("durable operation schema is not ready"))?;
    PostgresMaintenanceSessionStore::new(database.pool().clone())
        .verify_schema()
        .await
        .map_err(|_| configuration("durable maintenance schema is not ready"))?;
    let queue = DurableOperationQueue::new(operation_store);
    let executor = SpotifyObservationExecutor::new(
        database,
        ProviderCredentialVault::new(credential_store, keyring),
    );
    let mut shutdown = Box::pin(shutdown_signal());
    loop {
        tokio::select! {
            _ = &mut shutdown => return Ok(()),
            outcome = run_once(&queue, &executor, &worker_name) => {
                if outcome.map_err(|_| configuration("durable worker queue is unavailable"))? {
                    continue;
                }
                tokio::time::sleep(Duration::from_millis(DEFAULT_POLL_MILLISECONDS)).await;
            }
        }
    }
}

/// Claims and executes at most one durable command. Returns whether work was claimed.
pub async fn run_once<S, E>(
    queue: &DurableOperationQueue<S>,
    executor: &E,
    worker_name: &str,
) -> std::result::Result<bool, ClientError>
where
    S: crate::durable_operations::DurableOperationStore,
    E: HostedProviderExecutor,
{
    let Some(lease) = queue.claim_next(worker_name, LEASE_DURATION).await? else {
        return Ok(false);
    };
    if queue.cancellation_requested(&lease).await? {
        queue.acknowledge_cancellation(&lease).await?;
        return Ok(true);
    }
    queue
        .record_progress(
            &lease,
            Progress::new(
                progress_phase(&lease.request.command),
                0,
                Some(1),
                ProgressUnit::Steps,
            )
            .expect("static worker progress is valid"),
        )
        .await?;
    let mut work = Box::pin(dispatch(executor, &lease));
    let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
    heartbeat.tick().await;
    let outcome = loop {
        tokio::select! {
            result = &mut work => break result,
            _ = heartbeat.tick() => {
                if queue.cancellation_requested(&lease).await? {
                    drop(work);
                    queue.acknowledge_cancellation(&lease).await?;
                    return Ok(true);
                }
                queue.renew_lease(&lease, LEASE_DURATION).await?;
            }
        }
    };
    match outcome {
        Ok(result_id) => queue.complete(&lease, Some(result_id)).await?,
        Err(error) => {
            queue.fail(&lease, error).await?;
        }
    }
    Ok(true)
}

async fn dispatch<E: HostedProviderExecutor>(
    executor: &E,
    lease: &DurableOperationLease,
) -> std::result::Result<ResourceId, ClientError> {
    match &lease.request.command {
        Command::ObserveProvider {
            provider_connection_id,
        } => {
            executor
                .observe(lease.subject, *provider_connection_id)
                .await
        }
        Command::StartMaintenance {
            session_id,
            provider_connection_id,
        } => {
            executor
                .start_maintenance(
                    lease.subject,
                    lease.operation_id,
                    *session_id,
                    *provider_connection_id,
                )
                .await
        }
        Command::RefreshMaintenance {
            session_id,
            expected_revision,
        } => {
            executor
                .refresh_maintenance(
                    lease.subject,
                    lease.operation_id,
                    *session_id,
                    *expected_revision,
                )
                .await
        }
        Command::ResolveMaintenance {
            session_id,
            expected_revision,
            decisions,
        } => {
            executor
                .resolve_maintenance(
                    lease.subject,
                    lease.operation_id,
                    *session_id,
                    *expected_revision,
                    decisions.clone(),
                )
                .await
        }
        _ => Err(ClientError::new(ErrorCode::InvalidRequest, false)),
    }
}

fn progress_phase(command: &Command) -> &'static str {
    match command {
        Command::ObserveProvider { .. } => "observe_provider",
        Command::StartMaintenance { .. } => "start_maintenance",
        Command::RefreshMaintenance { .. } => "refresh_maintenance",
        Command::ResolveMaintenance { .. } => "resolve_maintenance",
        _ => "unsupported",
    }
}

async fn shutdown_signal() {
    let interrupt = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install termination handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = interrupt => {}, _ = terminate => {} }
}

fn provider_error(error: ChordriftError) -> ClientError {
    match error {
        ChordriftError::Configuration(_) => ClientError::new(ErrorCode::StateConflict, false),
        _ => unavailable(),
    }
}

fn unavailable() -> ClientError {
    ClientError::new(ErrorCode::DependencyUnavailable, true)
}

fn configuration(message: &str) -> ChordriftError {
    ChordriftError::Configuration(message.to_owned())
}

fn required(name: &str) -> Result<String> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| configuration(&format!("{name} is required")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::CancellationId;
    use crate::contract::{
        CONTRACT_VERSION, CommandRequest, IdempotencyKey, OperationId, RequestId,
    };
    use crate::durable_operations::DurableOperationLease;

    struct FakeExecutor;

    #[async_trait]
    impl HostedProviderExecutor for FakeExecutor {
        async fn observe(
            &self,
            _subject: AuthenticatedSubject,
            provider_connection_id: ResourceId,
        ) -> std::result::Result<ResourceId, ClientError> {
            Ok(provider_connection_id)
        }

        async fn start_maintenance(
            &self,
            _subject: AuthenticatedSubject,
            _operation_id: OperationId,
            session_id: MaintenanceSessionId,
            _provider_connection_id: ResourceId,
        ) -> std::result::Result<ResourceId, ClientError> {
            Ok(ResourceId::from_uuid(session_id.as_uuid()))
        }

        async fn refresh_maintenance(
            &self,
            _subject: AuthenticatedSubject,
            _operation_id: OperationId,
            session_id: MaintenanceSessionId,
            _expected_revision: u64,
        ) -> std::result::Result<ResourceId, ClientError> {
            Ok(ResourceId::from_uuid(session_id.as_uuid()))
        }

        async fn resolve_maintenance(
            &self,
            _subject: AuthenticatedSubject,
            _operation_id: OperationId,
            session_id: MaintenanceSessionId,
            _expected_revision: u64,
            _decisions: Vec<MaintenanceDecision>,
        ) -> std::result::Result<ResourceId, ClientError> {
            Ok(ResourceId::from_uuid(session_id.as_uuid()))
        }
    }

    fn lease(command: Command) -> DurableOperationLease {
        DurableOperationLease {
            operation_id: OperationId::new(),
            lease_id: ResourceId::new(),
            subject: AuthenticatedSubject {
                subject_id: ResourceId::new(),
                account_id: ResourceId::new(),
            },
            request: CommandRequest {
                contract_version: CONTRACT_VERSION,
                request_id: RequestId::new(),
                idempotency_key: IdempotencyKey::new(),
                command,
            },
            attempt: 1,
            max_attempts: 3,
            lease_expires_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn worker_dispatches_only_typed_observation() {
        let provider_connection_id = ResourceId::new();
        let result = dispatch(
            &FakeExecutor,
            &lease(Command::ObserveProvider {
                provider_connection_id,
            }),
        )
        .await
        .expect("typed observation is accepted");
        assert_eq!(result, provider_connection_id);

        let cancellation = Command::CancelOperation(crate::contract::CancellationRequest {
            operation_id: OperationId::new(),
            cancellation_id: CancellationId::new(),
        });
        assert_eq!(
            dispatch(&FakeExecutor, &lease(cancellation))
                .await
                .expect_err("worker rejects unsupported command")
                .code,
            ErrorCode::InvalidRequest
        );
    }

    #[tokio::test]
    async fn worker_dispatches_record_only_maintenance_commands() {
        let session_id = MaintenanceSessionId::new();
        for command in [
            Command::StartMaintenance {
                session_id,
                provider_connection_id: ResourceId::new(),
            },
            Command::RefreshMaintenance {
                session_id,
                expected_revision: 1,
            },
            Command::ResolveMaintenance {
                session_id,
                expected_revision: 1,
                decisions: Vec::new(),
            },
        ] {
            assert_eq!(
                dispatch(&FakeExecutor, &lease(command)).await.unwrap(),
                ResourceId::from_uuid(session_id.as_uuid())
            );
        }
    }
}
