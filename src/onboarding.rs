//! Provider-read-only onboarding session boundary.
//!
//! This module captures one selected immutable provider inventory and optional
//! extended-history evidence. It deliberately exposes no provider mutation and
//! does not read Chordrift collection, recipe, surface, Spin, or publication
//! intent. V020-07 may consume the captured inputs to build an audit; this
//! boundary stops before making those conclusions.

use std::{fmt, future::Future};

use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use sqlx::postgres::PgRow;
use storexa::Database;

use crate::{
    ChordriftError,
    application::ApplicationInvocation,
    contract::{
        CONTRACT_VERSION, ClientError, Command, CommandRequest, ErrorCode, IdempotencyKey,
        ResourceId,
    },
    domain::{
        AccountContext, CapabilityStatus, ChordriftAccountId, EvidenceCapability,
        OnboardingSessionId, ProviderCapability,
    },
};

/// A validated lowercase SHA-256 content fingerprint.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ContentFingerprint(String);

impl ContentFingerprint {
    /// Validates a lowercase hexadecimal SHA-256 fingerprint.
    pub fn new(value: impl Into<String>) -> Result<Self, OnboardingValueError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(OnboardingValueError::InvalidFingerprint);
        }
        Ok(Self(value))
    }

    /// Returns the hexadecimal fingerprint.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContentFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ContentFingerprint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

/// Invalid onboarding input value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnboardingValueError {
    /// A fingerprint was not exactly 64 lowercase hexadecimal characters.
    InvalidFingerprint,
    /// Evidence capabilities appeared more than once or in a noncanonical order.
    DuplicateOrUnorderedEvidence,
}

impl fmt::Display for OnboardingValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidFingerprint => "the onboarding fingerprint is invalid",
            Self::DuplicateOrUnorderedEvidence => {
                "onboarding evidence must be unique and canonically ordered"
            }
        })
    }
}

impl std::error::Error for OnboardingValueError {}

/// One immutable provider inventory selected for onboarding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OnboardingInventory {
    /// Existing immutable inventory checkpoint selected by the provider reader.
    pub checkpoint_id: ResourceId,
    /// Provider state fingerprint recorded by that checkpoint.
    pub state_fingerprint: ContentFingerprint,
    /// Provider items represented by the selected checkpoint.
    pub item_count: u64,
}

/// One immutable optional evidence source selected for onboarding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OnboardingEvidence {
    /// Provider-neutral evidence capability represented by this source.
    pub capability: EvidenceCapability,
    /// Content fingerprint of the selected evidence.
    pub content_fingerprint: ContentFingerprint,
    /// Evidence records represented by the selected source.
    pub record_count: u64,
}

/// Immutable result returned by the read-only provider port.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OnboardingInputs {
    /// Selected provider inventory.
    pub inventory: OnboardingInventory,
    evidence: Vec<OnboardingEvidence>,
}

impl OnboardingInputs {
    /// Creates canonical inputs when optional evidence is unique and ordered.
    pub fn new(
        inventory: OnboardingInventory,
        evidence: Vec<OnboardingEvidence>,
    ) -> Result<Self, OnboardingValueError> {
        if evidence
            .windows(2)
            .any(|pair| pair[0].capability >= pair[1].capability)
        {
            return Err(OnboardingValueError::DuplicateOrUnorderedEvidence);
        }
        Ok(Self {
            inventory,
            evidence,
        })
    }

    /// Returns the selected optional evidence in canonical order.
    #[must_use]
    pub fn evidence(&self) -> &[OnboardingEvidence] {
        &self.evidence
    }
}

#[derive(Deserialize)]
struct RawOnboardingInputs {
    inventory: OnboardingInventory,
    evidence: Vec<OnboardingEvidence>,
}

impl<'de> Deserialize<'de> for OnboardingInputs {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawOnboardingInputs::deserialize(deserializer)?;
        Self::new(raw.inventory, raw.evidence).map_err(D::Error::custom)
    }
}

/// Exact read selection derived from the onboarding command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OnboardingReadSelection {
    /// Whether one extended-history evidence source must also be read.
    pub include_extended_history: bool,
}

/// Provider port available to the onboarding application boundary.
///
/// There is intentionally no mutation method on this trait.
pub trait OnboardingProviderReader {
    /// Reads one selected inventory and the explicitly selected evidence.
    fn read_onboarding_inputs(
        &self,
        context: &AccountContext,
        selection: OnboardingReadSelection,
    ) -> impl Future<Output = Result<OnboardingInputs, ClientError>>;
}

/// Durable, content-addressed onboarding input capture.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OnboardingSession {
    /// Immutable session identity.
    pub id: OnboardingSessionId,
    /// Owning Chordrift account.
    pub account_id: ChordriftAccountId,
    /// Selected provider connection represented as a contract resource.
    pub provider_connection_id: ResourceId,
    /// Deterministic fingerprint of every selected input and capability snapshot.
    pub input_fingerprint: ContentFingerprint,
    /// Whether optional extended-history evidence was selected.
    pub include_extended_history: bool,
    /// Always true for the V020-06 boundary.
    pub ignored_existing_intent: bool,
    /// Immutable input manifest persisted with the session.
    pub input_manifest: Value,
    /// Immutable provenance describing how the boundary produced its result.
    pub output_provenance: Value,
}

/// Failure from the onboarding application boundary.
#[derive(Debug)]
pub enum OnboardingError {
    /// A client-safe validation, capability, provider, or ownership failure.
    Client(ClientError),
    /// Infrastructure failed while persisting the immutable session.
    Infrastructure(ChordriftError),
}

impl OnboardingError {
    /// Returns the stable client-safe representation of this failure.
    #[must_use]
    pub fn client_error(&self) -> ClientError {
        match self {
            Self::Client(error) => *error,
            Self::Infrastructure(_) => ClientError::new(ErrorCode::Internal, false),
        }
    }
}

impl fmt::Display for OnboardingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client(error) => formatter.write_str(error.message()),
            Self::Infrastructure(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for OnboardingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Client(_) => None,
            Self::Infrastructure(error) => Some(error),
        }
    }
}

impl From<ChordriftError> for OnboardingError {
    fn from(error: ChordriftError) -> Self {
        Self::Infrastructure(error)
    }
}

impl From<sqlx::Error> for OnboardingError {
    fn from(error: sqlx::Error) -> Self {
        Self::Infrastructure(ChordriftError::from(error))
    }
}

/// PostgreSQL-backed onboarding application boundary.
pub struct OnboardingSessionBoundary<'database> {
    database: &'database Database,
}

impl<'database> OnboardingSessionBoundary<'database> {
    /// Creates a boundary over Chordrift's existing database connection.
    #[must_use]
    pub const fn new(database: &'database Database) -> Self {
        Self { database }
    }

    /// Captures one provider-read-only onboarding session.
    ///
    /// Identical immutable inputs return the same session. This method reads no
    /// Chordrift intent tables and performs no provider mutation.
    pub async fn create<P>(
        &self,
        context: &AccountContext,
        request: &CommandRequest,
        provider: &P,
    ) -> Result<OnboardingSession, OnboardingError>
    where
        P: OnboardingProviderReader,
    {
        let include_extended_history = validate_request(context, request)?;
        self.validate_provider_owner(context).await?;
        if let Some(existing) = self
            .find_by_idempotency_key(context, request.idempotency_key)
            .await?
        {
            if existing.provider_connection_id.as_uuid()
                != context.provider_connection().connection_id.as_uuid()
                || existing.include_extended_history != include_extended_history
            {
                return Err(client_error(ErrorCode::StateConflict));
            }
            return Ok(existing);
        }
        validate_capabilities(context, include_extended_history)?;
        let selection = OnboardingReadSelection {
            include_extended_history,
        };
        let inputs = provider
            .read_onboarding_inputs(context, selection)
            .await
            .map_err(OnboardingError::Client)?;
        validate_selected_evidence(&inputs, include_extended_history)?;
        self.persist(
            context,
            request.idempotency_key,
            include_extended_history,
            &inputs,
        )
        .await
    }

    async fn validate_provider_owner(
        &self,
        context: &AccountContext,
    ) -> Result<(), OnboardingError> {
        let connection = context.provider_connection();
        let provider_owner = sqlx::query(
            "SELECT chordrift_account_id, provider, provider_account_id
               FROM provider_accounts WHERE id = $1",
        )
        .bind(connection.connection_id.as_uuid())
        .fetch_optional(self.database.pool())
        .await
        .map_err(ChordriftError::from)?;
        let Some(provider_owner) = provider_owner else {
            return Err(client_error(ErrorCode::ResourceNotFound));
        };
        if provider_owner.try_get::<uuid::Uuid, _>("chordrift_account_id")?
            != context.account_id().as_uuid()
            || provider_owner.try_get::<String, _>("provider")?
                != connection.provider_account_id.provider().as_str()
            || provider_owner.try_get::<String, _>("provider_account_id")?
                != connection.provider_account_id.value()
        {
            return Err(client_error(ErrorCode::PermissionDenied));
        }
        Ok(())
    }

    async fn find_by_idempotency_key(
        &self,
        context: &AccountContext,
        idempotency_key: IdempotencyKey,
    ) -> Result<Option<OnboardingSession>, OnboardingError> {
        let row = sqlx::query(
            "SELECT id, chordrift_account_id, provider_account_id, input_fingerprint,
                    include_extended_history, ignore_existing_intent,
                    input_manifest, output_provenance
               FROM onboarding_sessions
              WHERE chordrift_account_id = $1
                AND output_provenance ->> 'boundary' = 'onboarding_input_capture'
                AND output_provenance ->> 'idempotency_key' = $2
              ORDER BY created_at, id
              LIMIT 1",
        )
        .bind(context.account_id().as_uuid())
        .bind(idempotency_key.to_string())
        .fetch_optional(self.database.pool())
        .await
        .map_err(ChordriftError::from)?;
        row.map(session_from_row).transpose()
    }

    async fn persist(
        &self,
        context: &AccountContext,
        idempotency_key: IdempotencyKey,
        include_extended_history: bool,
        inputs: &OnboardingInputs,
    ) -> Result<OnboardingSession, OnboardingError> {
        let connection = context.provider_connection();
        let account_id = context.account_id().as_uuid();
        let provider_account_id = connection.connection_id.as_uuid();
        let checkpoint_id = inputs.inventory.checkpoint_id.as_uuid();
        let provider_namespace = connection.provider_account_id.provider().as_str();
        let provider_owned_account_id = connection.provider_account_id.value();

        let input_manifest = json!({
            "schema_version": 1,
            "account_id": account_id,
            "provider_connection_id": provider_account_id,
            "provider_namespace": provider_namespace,
            "provider_account_id": provider_owned_account_id,
            "inventory": inputs.inventory,
            "evidence": inputs.evidence(),
            "include_extended_history": include_extended_history,
            "ignore_existing_intent": true,
            "provider_capabilities": context.provider_capabilities(),
            "evidence_capabilities": context.evidence_capabilities(),
        });
        let input_fingerprint = ContentFingerprint::new(hex_sha256(
            &serde_json::to_vec(&input_manifest).map_err(ChordriftError::from)?,
        ))
        .expect("SHA-256 formatting is valid");
        let output_provenance = json!({
            "schema_version": 1,
            "boundary": "onboarding_input_capture",
            "idempotency_key": idempotency_key,
            "input_fingerprint": input_fingerprint,
            "provider_reads": if include_extended_history {
                vec!["inventory", "extended_history"]
            } else {
                vec!["inventory"]
            },
            "chordrift_intent_read": false,
            "provider_write_requested": false,
            "next_boundary": "inventory_only_audit",
        });

        let mut transaction = self
            .database
            .pool()
            .begin()
            .await
            .map_err(ChordriftError::from)?;
        let provider_owner = sqlx::query(
            "SELECT chordrift_account_id, provider, provider_account_id
               FROM provider_accounts WHERE id = $1 FOR SHARE",
        )
        .bind(provider_account_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(ChordriftError::from)?;
        let Some(provider_owner) = provider_owner else {
            return Err(client_error(ErrorCode::ResourceNotFound));
        };
        if provider_owner.try_get::<uuid::Uuid, _>("chordrift_account_id")? != account_id
            || provider_owner.try_get::<String, _>("provider")? != provider_namespace
            || provider_owner.try_get::<String, _>("provider_account_id")?
                != provider_owned_account_id
        {
            return Err(client_error(ErrorCode::PermissionDenied));
        }

        let checkpoint_matches: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM provider_inventory_checkpoints
                  WHERE provider_account_id = $1 AND id = $2 AND state_sha256 = $3
               )",
        )
        .bind(provider_account_id)
        .bind(checkpoint_id)
        .bind(inputs.inventory.state_fingerprint.as_str())
        .fetch_one(&mut *transaction)
        .await
        .map_err(ChordriftError::from)?;
        if !checkpoint_matches {
            return Err(client_error(ErrorCode::StateConflict));
        }

        let provider_capabilities =
            serde_json::to_value(context.provider_capabilities()).map_err(ChordriftError::from)?;
        let evidence_capabilities =
            serde_json::to_value(context.evidence_capabilities()).map_err(ChordriftError::from)?;
        let capability_observation_id: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO provider_capability_observations
                 (chordrift_account_id, provider_account_id, provider_capabilities,
                  evidence_capabilities, input_fingerprint)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (provider_account_id, input_fingerprint)
                 WHERE input_fingerprint IS NOT NULL
             DO UPDATE SET input_fingerprint = EXCLUDED.input_fingerprint
             RETURNING id",
        )
        .bind(account_id)
        .bind(provider_account_id)
        .bind(provider_capabilities)
        .bind(evidence_capabilities)
        .bind(input_fingerprint.as_str())
        .fetch_one(&mut *transaction)
        .await
        .map_err(ChordriftError::from)?;

        let inserted_id: Option<uuid::Uuid> = sqlx::query_scalar(
            "INSERT INTO onboarding_sessions
                 (chordrift_account_id, provider_account_id,
                  provider_inventory_checkpoint_id, capability_observation_id,
                  include_extended_history, ignore_existing_intent, status,
                  input_fingerprint, input_manifest, output_provenance)
             VALUES ($1, $2, $3, $4, $5, TRUE, 'created', $6, $7, $8)
             ON CONFLICT (chordrift_account_id, provider_account_id, input_fingerprint)
                 WHERE input_fingerprint IS NOT NULL
             DO NOTHING
             RETURNING id",
        )
        .bind(account_id)
        .bind(provider_account_id)
        .bind(checkpoint_id)
        .bind(capability_observation_id)
        .bind(include_extended_history)
        .bind(input_fingerprint.as_str())
        .bind(&input_manifest)
        .bind(&output_provenance)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(ChordriftError::from)?;

        let session_id = match inserted_id {
            Some(id) => id,
            None => sqlx::query_scalar(
                "SELECT id FROM onboarding_sessions
                  WHERE chordrift_account_id = $1 AND provider_account_id = $2
                    AND input_fingerprint = $3",
            )
            .bind(account_id)
            .bind(provider_account_id)
            .bind(input_fingerprint.as_str())
            .fetch_one(&mut *transaction)
            .await
            .map_err(ChordriftError::from)?,
        };
        let session_row = sqlx::query(
            "SELECT id, chordrift_account_id, provider_account_id, input_fingerprint,
                    include_extended_history, ignore_existing_intent,
                    input_manifest, output_provenance
               FROM onboarding_sessions WHERE id = $1",
        )
        .bind(session_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(ChordriftError::from)?;
        let session = session_from_row(session_row)?;
        transaction.commit().await.map_err(ChordriftError::from)?;

        Ok(session)
    }

    /// Wraps this boundary call for execution through [`crate::application::ApplicationFacade`].
    #[must_use]
    pub const fn invocation<'request, P>(
        &'request self,
        context: &'request AccountContext,
        request: &'request CommandRequest,
        provider: &'request P,
    ) -> CreateOnboardingSessionInvocation<'request, 'database, P> {
        CreateOnboardingSessionInvocation {
            boundary: self,
            context,
            request,
            provider,
        }
    }
}

/// One onboarding call submitted through the shared application facade.
pub struct CreateOnboardingSessionInvocation<'request, 'database, P> {
    boundary: &'request OnboardingSessionBoundary<'database>,
    context: &'request AccountContext,
    request: &'request CommandRequest,
    provider: &'request P,
}

impl<P> ApplicationInvocation for CreateOnboardingSessionInvocation<'_, '_, P>
where
    P: OnboardingProviderReader,
{
    type Output = Result<OnboardingSession, OnboardingError>;

    async fn execute(self) -> crate::Result<Self::Output> {
        Ok(self
            .boundary
            .create(self.context, self.request, self.provider)
            .await)
    }
}

fn validate_request(
    context: &AccountContext,
    request: &CommandRequest,
) -> Result<bool, OnboardingError> {
    if request.contract_version != CONTRACT_VERSION {
        return Err(client_error(ErrorCode::IncompatibleContract));
    }
    let Command::CreateOnboardingSession {
        account_id,
        include_extended_history,
    } = request.command
    else {
        return Err(client_error(ErrorCode::InvalidRequest));
    };
    if account_id.as_uuid() != context.account_id().as_uuid() {
        return Err(client_error(ErrorCode::PermissionDenied));
    }
    Ok(include_extended_history)
}

fn validate_capabilities(
    context: &AccountContext,
    include_extended_history: bool,
) -> Result<(), OnboardingError> {
    if context
        .provider_capabilities()
        .status(ProviderCapability::LibraryInventoryRead)
        == CapabilityStatus::Unavailable
        || context
            .evidence_capabilities()
            .status(EvidenceCapability::CurrentInventory)
            == CapabilityStatus::Unavailable
        || (include_extended_history
            && context
                .evidence_capabilities()
                .status(EvidenceCapability::ExtendedPlaybackHistory)
                == CapabilityStatus::Unavailable)
    {
        return Err(client_error(ErrorCode::CapabilityUnavailable));
    }
    Ok(())
}

fn validate_selected_evidence(
    inputs: &OnboardingInputs,
    include_extended_history: bool,
) -> Result<(), OnboardingError> {
    let selected_extended = inputs
        .evidence()
        .iter()
        .filter(|evidence| evidence.capability == EvidenceCapability::ExtendedPlaybackHistory)
        .count();
    if inputs.evidence().len() != selected_extended
        || selected_extended != usize::from(include_extended_history)
    {
        return Err(client_error(ErrorCode::InvalidRequest));
    }
    Ok(())
}

fn client_error(code: ErrorCode) -> OnboardingError {
    OnboardingError::Client(ClientError::new(code, false))
}

fn session_from_row(row: PgRow) -> Result<OnboardingSession, OnboardingError> {
    let input_fingerprint: String = row.try_get("input_fingerprint")?;
    Ok(OnboardingSession {
        id: OnboardingSessionId::from_uuid(row.try_get("id")?),
        account_id: ChordriftAccountId::from_uuid(row.try_get("chordrift_account_id")?),
        provider_connection_id: ResourceId::from_uuid(row.try_get("provider_account_id")?),
        input_fingerprint: ContentFingerprint::new(input_fingerprint)
            .map_err(|_| client_error(ErrorCode::Internal))?,
        include_extended_history: row.try_get("include_extended_history")?,
        ignored_existing_intent: row.try_get("ignore_existing_intent")?,
        input_manifest: row.try_get("input_manifest")?,
        output_provenance: row.try_get("output_provenance")?,
    })
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_and_evidence_order_are_validated_during_deserialization() {
        assert!(ContentFingerprint::new("a".repeat(64)).is_ok());
        assert_eq!(
            ContentFingerprint::new("A".repeat(64)),
            Err(OnboardingValueError::InvalidFingerprint)
        );
        assert!(serde_json::from_str::<ContentFingerprint>("\"short\"").is_err());

        let inventory = OnboardingInventory {
            checkpoint_id: ResourceId::new(),
            state_fingerprint: ContentFingerprint::new("b".repeat(64)).expect("valid hash"),
            item_count: 1,
        };
        let duplicate = OnboardingEvidence {
            capability: EvidenceCapability::ExtendedPlaybackHistory,
            content_fingerprint: ContentFingerprint::new("c".repeat(64)).expect("valid hash"),
            record_count: 1,
        };
        assert_eq!(
            OnboardingInputs::new(inventory, vec![duplicate.clone(), duplicate]),
            Err(OnboardingValueError::DuplicateOrUnorderedEvidence)
        );
    }
}
