//! Durable wrapper-neutral maintenance-session persistence.
//!
//! This store contains typed task projections and immutable revision events.
//! It does not interpret provider changes or execute provider writes; those
//! remain Rust application-core responsibilities.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row as _, Transaction, types::Json};

use crate::{
    contract::{
        ClientError, ErrorCode, MaintenanceDecision, MaintenanceReviewId, MaintenanceSessionId,
        MaintenanceSessionState, MaintenanceSessionView, OperationId, ResourceId,
    },
    maintenance::{MaintenanceDecisionProjection, MaintenanceProjection, MaintenanceWorkflow},
    service::AuthenticatedSubject,
};

/// Durable reason for one accepted maintenance revision.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceTransition {
    /// Initial projection was accepted.
    Started,
    /// A newer complete provider observation was folded into the session.
    Refreshed,
    /// Human ambiguity decisions were accepted.
    Resolved,
    /// One exact immutable provider-effect review was authorized.
    Authorized,
    /// Exact provider execution began.
    Applying,
    /// Provider execution completed and verification began.
    Verifying,
    /// A complete provider observation verified the authorized effects.
    Verified,
    /// Work stopped safely and can be resumed.
    Recoverable,
}

impl MaintenanceTransition {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Refreshed => "refreshed",
            Self::Resolved => "resolved",
            Self::Authorized => "authorized",
            Self::Applying => "applying",
            Self::Verifying => "verifying",
            Self::Verified => "verified",
            Self::Recoverable => "recoverable",
        }
    }
}

/// Restart-safe maintenance session and its ownership boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurableMaintenanceSession {
    /// Chordrift subject and account that own the task.
    pub subject: AuthenticatedSubject,
    /// Account-owned provider connection being maintained.
    pub provider_connection_id: ResourceId,
    /// Current typed application projection.
    pub view: MaintenanceSessionView,
}

/// PostgreSQL persistence for current sessions and immutable revision history.
#[derive(Clone)]
pub struct PostgresMaintenanceSessionStore {
    pool: PgPool,
}

/// Rust-owned durable transition authority used by hosted workers.
#[derive(Clone)]
pub struct DurableMaintenanceAuthority {
    store: PostgresMaintenanceSessionStore,
}

impl DurableMaintenanceAuthority {
    /// Creates an authority over the migration-0051 session store.
    pub fn new(store: PostgresMaintenanceSessionStore) -> Self {
        Self { store }
    }

    /// Starts one restart-safe task from a complete interpreted projection.
    pub async fn start(
        &self,
        subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
        session_id: MaintenanceSessionId,
        projection: MaintenanceProjection,
        source_operation_id: Option<OperationId>,
        occurred_at: DateTime<Utc>,
    ) -> Result<MaintenanceSessionView, ClientError> {
        let view = MaintenanceWorkflow::new(session_id, projection)
            .map_err(|error| error.client_error())?
            .view();
        self.store
            .create(
                subject,
                provider_connection_id,
                &view,
                source_operation_id,
                occurred_at,
            )
            .await?;
        Ok(view)
    }

    /// Rebases cumulative provider intent onto one newer complete projection.
    pub async fn refresh(
        &self,
        subject: AuthenticatedSubject,
        session_id: MaintenanceSessionId,
        expected_revision: u64,
        projection: MaintenanceProjection,
        source_operation_id: Option<OperationId>,
        occurred_at: DateTime<Utc>,
    ) -> Result<MaintenanceSessionView, ClientError> {
        let current = self.store.load(subject, session_id).await?;
        let mut workflow =
            MaintenanceWorkflow::from_view(current.view).map_err(|error| error.client_error())?;
        let view = workflow
            .rebase(expected_revision, projection)
            .map_err(|error| error.client_error())?;
        self.store
            .replace(
                subject,
                expected_revision,
                &view,
                MaintenanceTransition::Refreshed,
                source_operation_id,
                occurred_at,
            )
            .await?;
        Ok(view)
    }

    /// Persists one complete set of ambiguity decisions and recomputed review.
    #[allow(clippy::too_many_arguments)]
    pub async fn resolve(
        &self,
        subject: AuthenticatedSubject,
        session_id: MaintenanceSessionId,
        expected_revision: u64,
        decisions: Vec<MaintenanceDecision>,
        projection: MaintenanceDecisionProjection,
        source_operation_id: Option<OperationId>,
        occurred_at: DateTime<Utc>,
    ) -> Result<MaintenanceSessionView, ClientError> {
        let current = self.store.load(subject, session_id).await?;
        let mut workflow =
            MaintenanceWorkflow::from_view(current.view).map_err(|error| error.client_error())?;
        let view = workflow
            .resolve(expected_revision, decisions, projection)
            .map_err(|error| error.client_error())?;
        self.store
            .replace(
                subject,
                expected_revision,
                &view,
                MaintenanceTransition::Resolved,
                source_operation_id,
                occurred_at,
            )
            .await?;
        Ok(view)
    }

    /// Persists exact-review authorization without executing provider effects.
    pub async fn authorize(
        &self,
        subject: AuthenticatedSubject,
        session_id: MaintenanceSessionId,
        expected_revision: u64,
        review_id: MaintenanceReviewId,
        source_operation_id: Option<OperationId>,
        occurred_at: DateTime<Utc>,
    ) -> Result<MaintenanceSessionView, ClientError> {
        let current = self.store.load(subject, session_id).await?;
        let mut workflow =
            MaintenanceWorkflow::from_view(current.view).map_err(|error| error.client_error())?;
        let view = workflow
            .authorize(expected_revision, review_id)
            .map_err(|error| error.client_error())?;
        self.store
            .replace(
                subject,
                expected_revision,
                &view,
                MaintenanceTransition::Authorized,
                source_operation_id,
                occurred_at,
            )
            .await?;
        Ok(view)
    }

    /// Persists a server-owned execution transition without changing the
    /// immutable reviewed effects.
    pub async fn mark_execution_state(
        &self,
        subject: AuthenticatedSubject,
        session_id: MaintenanceSessionId,
        expected_revision: u64,
        state: MaintenanceSessionState,
        source_operation_id: Option<OperationId>,
        occurred_at: DateTime<Utc>,
    ) -> Result<MaintenanceSessionView, ClientError> {
        let current = self.store.load(subject, session_id).await?;
        let mut workflow =
            MaintenanceWorkflow::from_view(current.view).map_err(|error| error.client_error())?;
        if workflow.view().revision != expected_revision {
            return Err(conflict());
        }
        let view = workflow
            .mark_execution_state(state)
            .map_err(|error| error.client_error())?;
        let transition = match state {
            MaintenanceSessionState::Applying => MaintenanceTransition::Applying,
            MaintenanceSessionState::Verifying => MaintenanceTransition::Verifying,
            MaintenanceSessionState::Recoverable => MaintenanceTransition::Recoverable,
            _ => return Err(invalid()),
        };
        self.store
            .replace(
                subject,
                expected_revision,
                &view,
                transition,
                source_operation_id,
                occurred_at,
            )
            .await?;
        Ok(view)
    }

    /// Consumes one authorized review only after a fresh provider observation
    /// verifies the exact effects.
    pub async fn complete_verification(
        &self,
        subject: AuthenticatedSubject,
        session_id: MaintenanceSessionId,
        expected_revision: u64,
        projection: MaintenanceProjection,
        source_operation_id: Option<OperationId>,
        occurred_at: DateTime<Utc>,
    ) -> Result<MaintenanceSessionView, ClientError> {
        let current = self.store.load(subject, session_id).await?;
        let mut workflow =
            MaintenanceWorkflow::from_view(current.view).map_err(|error| error.client_error())?;
        if workflow.view().revision != expected_revision {
            return Err(conflict());
        }
        let view = workflow
            .complete_verification(projection)
            .map_err(|error| error.client_error())?;
        self.store
            .replace(
                subject,
                expected_revision,
                &view,
                MaintenanceTransition::Verified,
                source_operation_id,
                occurred_at,
            )
            .await?;
        Ok(view)
    }
}

impl PostgresMaintenanceSessionStore {
    /// Creates a store over the hosted application pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Verifies additive migration 0051 before hosted maintenance is enabled.
    pub async fn verify_schema(&self) -> Result<(), ClientError> {
        let ready: bool = sqlx::query_scalar(
            "SELECT to_regclass('maintenance_sessions') IS NOT NULL
                 AND to_regclass('maintenance_session_events') IS NOT NULL",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|_| unavailable())?;
        ready
            .then_some(())
            .ok_or_else(|| ClientError::new(ErrorCode::DependencyUnavailable, false))
    }

    /// Creates one session and its immutable revision-one event atomically.
    pub async fn create(
        &self,
        subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
        view: &MaintenanceSessionView,
        source_operation_id: Option<OperationId>,
        occurred_at: DateTime<Utc>,
    ) -> Result<(), ClientError> {
        MaintenanceWorkflow::from_view(view.clone()).map_err(|_| invalid())?;
        if view.revision != 1 {
            return Err(invalid());
        }
        let mut transaction = self.pool.begin().await.map_err(|_| unavailable())?;
        require_authority(&mut transaction, subject, provider_connection_id).await?;
        let result = sqlx::query(
            "INSERT INTO maintenance_sessions
             (id, chordrift_account_id, product_subject_id, provider_account_id,
              revision, state_name, view_payload, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(view.session_id.as_uuid())
        .bind(subject.account_id.as_uuid())
        .bind(subject.subject_id.as_uuid())
        .bind(provider_connection_id.as_uuid())
        .bind(i64::try_from(view.revision).map_err(|_| invalid())?)
        .bind(state_name(view.state))
        .bind(Json(view))
        .bind(occurred_at)
        .execute(&mut *transaction)
        .await
        .map_err(|_| unavailable())?;
        if result.rows_affected() != 1 {
            return Err(conflict());
        }
        append_event(
            &mut transaction,
            view,
            MaintenanceTransition::Started,
            source_operation_id,
            occurred_at,
        )
        .await?;
        transaction.commit().await.map_err(|_| unavailable())
    }

    /// Loads one session only through its exact subject/account ownership.
    pub async fn load(
        &self,
        subject: AuthenticatedSubject,
        session_id: MaintenanceSessionId,
    ) -> Result<DurableMaintenanceSession, ClientError> {
        let row = sqlx::query(
            "SELECT provider_account_id, view_payload
               FROM maintenance_sessions
              WHERE id = $1 AND chordrift_account_id = $2
                AND product_subject_id = $3",
        )
        .bind(session_id.as_uuid())
        .bind(subject.account_id.as_uuid())
        .bind(subject.subject_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(not_found)?;
        let Json(view): Json<MaintenanceSessionView> =
            row.try_get("view_payload").map_err(|_| unavailable())?;
        MaintenanceWorkflow::from_view(view.clone()).map_err(|_| unavailable())?;
        Ok(DurableMaintenanceSession {
            subject,
            provider_connection_id: ResourceId::from_uuid(
                row.try_get("provider_account_id")
                    .map_err(|_| unavailable())?,
            ),
            view,
        })
    }

    /// Replaces the current projection with one exact next revision and appends
    /// the corresponding immutable event in the same transaction.
    pub async fn replace(
        &self,
        subject: AuthenticatedSubject,
        expected_revision: u64,
        view: &MaintenanceSessionView,
        transition: MaintenanceTransition,
        source_operation_id: Option<OperationId>,
        occurred_at: DateTime<Utc>,
    ) -> Result<(), ClientError> {
        MaintenanceWorkflow::from_view(view.clone()).map_err(|_| invalid())?;
        if view.revision != expected_revision.saturating_add(1) {
            return Err(invalid());
        }
        let mut transaction = self.pool.begin().await.map_err(|_| unavailable())?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT revision FROM maintenance_sessions
              WHERE id = $1 AND chordrift_account_id = $2
                AND product_subject_id = $3 FOR UPDATE",
        )
        .bind(view.session_id.as_uuid())
        .bind(subject.account_id.as_uuid())
        .bind(subject.subject_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| unavailable())?;
        let Some(current) = current else {
            return Err(not_found());
        };
        if u64::try_from(current).map_err(|_| unavailable())? != expected_revision {
            return Err(conflict());
        }
        sqlx::query(
            "UPDATE maintenance_sessions
                SET revision = $2, state_name = $3, view_payload = $4, updated_at = $5
              WHERE id = $1",
        )
        .bind(view.session_id.as_uuid())
        .bind(i64::try_from(view.revision).map_err(|_| invalid())?)
        .bind(state_name(view.state))
        .bind(Json(view))
        .bind(occurred_at)
        .execute(&mut *transaction)
        .await
        .map_err(|_| unavailable())?;
        append_event(
            &mut transaction,
            view,
            transition,
            source_operation_id,
            occurred_at,
        )
        .await?;
        transaction.commit().await.map_err(|_| unavailable())
    }
}

async fn require_authority(
    transaction: &mut Transaction<'_, Postgres>,
    subject: AuthenticatedSubject,
    provider_connection_id: ResourceId,
) -> Result<(), ClientError> {
    let authorized: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1
               FROM chordrift_account_memberships membership
               JOIN chordrift_accounts account
                 ON account.id = membership.chordrift_account_id
               JOIN product_subjects subject
                 ON subject.id = membership.product_subject_id
               JOIN provider_accounts provider
                 ON provider.chordrift_account_id = account.id
              WHERE membership.chordrift_account_id = $1
                AND membership.product_subject_id = $2
                AND provider.id = $3
                AND membership.status = 'active'
                AND account.status = 'active' AND subject.status = 'active')",
    )
    .bind(subject.account_id.as_uuid())
    .bind(subject.subject_id.as_uuid())
    .bind(provider_connection_id.as_uuid())
    .fetch_one(&mut **transaction)
    .await
    .map_err(|_| unavailable())?;
    authorized
        .then_some(())
        .ok_or_else(|| ClientError::new(ErrorCode::PermissionDenied, false))
}

async fn append_event(
    transaction: &mut Transaction<'_, Postgres>,
    view: &MaintenanceSessionView,
    transition: MaintenanceTransition,
    source_operation_id: Option<OperationId>,
    occurred_at: DateTime<Utc>,
) -> Result<(), ClientError> {
    sqlx::query(
        "INSERT INTO maintenance_session_events
         (maintenance_session_id, revision, transition_name,
          source_operation_id, view_payload, occurred_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(view.session_id.as_uuid())
    .bind(i64::try_from(view.revision).map_err(|_| invalid())?)
    .bind(transition.as_str())
    .bind(source_operation_id.map(|id| id.as_uuid()))
    .bind(Json(view))
    .bind(occurred_at)
    .execute(&mut **transaction)
    .await
    .map_err(|_| unavailable())?;
    Ok(())
}

const fn state_name(state: MaintenanceSessionState) -> &'static str {
    match state {
        MaintenanceSessionState::Reconciling => "reconciling",
        MaintenanceSessionState::NeedsDecision => "needs_decision",
        MaintenanceSessionState::ReadyForAuthorization => "ready_for_authorization",
        MaintenanceSessionState::Authorized => "authorized",
        MaintenanceSessionState::Applying => "applying",
        MaintenanceSessionState::Verifying => "verifying",
        MaintenanceSessionState::InSync => "in_sync",
        MaintenanceSessionState::Recoverable => "recoverable",
    }
}

fn invalid() -> ClientError {
    ClientError::new(ErrorCode::InvalidRequest, false)
}

fn conflict() -> ClientError {
    ClientError::new(ErrorCode::StateConflict, false)
}

fn not_found() -> ClientError {
    ClientError::new(ErrorCode::ResourceNotFound, false)
}

fn unavailable() -> ClientError {
    ClientError::new(ErrorCode::DependencyUnavailable, true)
}
