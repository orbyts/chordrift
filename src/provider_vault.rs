//! Encrypted server-side provider credential vault.
//!
//! Clients retain only Chordrift product sessions. Provider OAuth adapters hand
//! refresh credentials directly to this Rust boundary, which encrypts them with
//! an external key ring before PostgreSQL persistence. Plaintext is leased only
//! to an authorized internal provider operation and is zeroized on drop.

use std::{collections::BTreeMap, env, sync::Arc};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use chrono::{DateTime, Utc};
use rand::RngCore as _;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row as _};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::{
    contract::{ClientError, ErrorCode, ResourceId},
    service::AuthenticatedSubject,
};

const ALGORITHM: &str = "xchacha20poly1305-v1";
const NONCE_BYTES: usize = 24;
const KEY_BYTES: usize = 32;
const MAX_SECRET_BYTES: usize = 16 * 1024;
const MAX_KEY_ID_BYTES: usize = 128;
const ACTIVE_KEY_ID_VARIABLE: &str = "CHORDRIFT_PROVIDER_VAULT_ACTIVE_KEY_ID";
const KEY_B64_VARIABLE: &str = "CHORDRIFT_PROVIDER_VAULT_KEY_B64";

/// Stable account-owned identity of one provider refresh credential.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderCredentialIdentity {
    /// Owning Chordrift account.
    pub account_id: ResourceId,
    /// Account-scoped provider connection.
    pub provider_account_id: ResourceId,
    /// Provider namespace such as `spotify`.
    pub provider: String,
}

impl ProviderCredentialIdentity {
    /// Creates a bounded provider credential identity.
    pub fn new(
        account_id: ResourceId,
        provider_account_id: ResourceId,
        provider: impl Into<String>,
    ) -> Result<Self, ClientError> {
        let provider = provider.into();
        if provider.trim().is_empty()
            || provider.len() > 64
            || !provider
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(ClientError::new(ErrorCode::InvalidRequest, false));
        }
        Ok(Self {
            account_id,
            provider_account_id,
            provider,
        })
    }
}

/// Plaintext OAuth refresh credential accepted only from a server-side provider adapter.
pub struct ProviderRefreshCredential {
    refresh_token: Zeroizing<String>,
    scopes: Vec<String>,
}

impl ProviderRefreshCredential {
    /// Creates a bounded secret and normalized scope set.
    pub fn new(
        refresh_token: impl Into<String>,
        scopes: impl IntoIterator<Item = String>,
    ) -> Result<Self, ClientError> {
        let refresh_token = refresh_token.into();
        if refresh_token.trim().is_empty() || refresh_token.len() > MAX_SECRET_BYTES {
            return Err(ClientError::new(ErrorCode::InvalidRequest, false));
        }
        let scopes = normalize_scopes(scopes)?;
        Ok(Self {
            refresh_token: Zeroizing::new(refresh_token),
            scopes,
        })
    }
}

/// Short-lived decrypted credential for one internal provider operation.
pub struct ProviderCredentialLease {
    refresh_token: Zeroizing<String>,
    scopes: Vec<String>,
    /// Durable encrypted credential revision used by this lease.
    pub revision_id: ResourceId,
    /// Monotonic account/provider credential generation.
    pub generation: u32,
}

impl ProviderCredentialLease {
    /// Exposes plaintext only to the internal provider adapter holding the lease.
    pub fn refresh_token(&self) -> &str {
        self.refresh_token.as_str()
    }

    /// Returns the normalized OAuth scopes stored with this credential.
    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }
}

/// Non-secret result of provisioning or rotating a provider credential.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderCredentialRevision {
    /// New immutable encrypted revision.
    pub revision_id: ResourceId,
    /// Monotonic generation for this provider connection.
    pub generation: u32,
    /// External encryption-key selector; never key material.
    pub key_id: String,
    /// Whether an older active revision was atomically superseded.
    pub rotated: bool,
    /// Creation time supplied by the authority.
    pub created_at: DateTime<Utc>,
}

/// Non-secret result of revoking the active provider credential.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderCredentialRevocation {
    /// Revoked encrypted revision.
    pub revision_id: ResourceId,
    /// Revoked generation.
    pub generation: u32,
    /// Revocation time.
    pub revoked_at: DateTime<Utc>,
}

/// Deployment-supplied encryption keys. Key material is never serializable,
/// debuggable, or persisted by Chordrift.
#[derive(Clone)]
pub struct ProviderVaultKeyring {
    active_key_id: String,
    keys: Arc<BTreeMap<String, Zeroizing<Vec<u8>>>>,
}

impl ProviderVaultKeyring {
    /// Loads the active deployment key from explicit environment settings.
    ///
    /// Key material remains outside PostgreSQL and is never included in an
    /// error or debug representation. Retained decrypt-only keys can be added
    /// through a later rotation-specific deployment setting.
    pub fn from_environment() -> Result<Self, ClientError> {
        let key_id = env::var(ACTIVE_KEY_ID_VARIABLE)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(vault_unavailable)?;
        let encoded = Zeroizing::new(
            env::var(KEY_B64_VARIABLE)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(vault_unavailable)?,
        );
        let key = Zeroizing::new(
            STANDARD
                .decode(encoded.trim())
                .map_err(|_| vault_unavailable())?,
        );
        Self::new(key_id.clone(), [(key_id, key.to_vec())])
    }

    /// Creates a key ring with one active key and optional retained decrypt-only keys.
    pub fn new(
        active_key_id: impl Into<String>,
        keys: impl IntoIterator<Item = (String, Vec<u8>)>,
    ) -> Result<Self, ClientError> {
        let active_key_id = active_key_id.into();
        validate_key_id(&active_key_id)?;
        let mut normalized = BTreeMap::new();
        for (key_id, key) in keys {
            validate_key_id(&key_id)?;
            if key.len() != KEY_BYTES || normalized.contains_key(&key_id) {
                return Err(ClientError::new(ErrorCode::InvalidRequest, false));
            }
            normalized.insert(key_id, Zeroizing::new(key));
        }
        if !normalized.contains_key(&active_key_id) {
            return Err(ClientError::new(ErrorCode::InvalidRequest, false));
        }
        Ok(Self {
            active_key_id,
            keys: Arc::new(normalized),
        })
    }

    fn active_key(&self) -> (&str, &[u8]) {
        (
            &self.active_key_id,
            self.keys[&self.active_key_id].as_slice(),
        )
    }

    fn key(&self, key_id: &str) -> Result<&[u8], ClientError> {
        self.keys
            .get(key_id)
            .map(|key| key.as_slice())
            .ok_or_else(vault_unavailable)
    }
}

#[derive(Serialize)]
struct PayloadRef<'a> {
    refresh_token: &'a str,
    scopes: &'a [String],
}

#[derive(Deserialize)]
struct OwnedPayload {
    refresh_token: String,
    scopes: Vec<String>,
}

impl Drop for OwnedPayload {
    fn drop(&mut self) {
        self.refresh_token.zeroize();
        self.scopes.zeroize();
    }
}

#[derive(Serialize)]
struct AssociatedData<'a> {
    schema_version: u16,
    revision_id: Uuid,
    account_id: Uuid,
    provider_account_id: Uuid,
    provider: &'a str,
    credential_kind: &'static str,
    algorithm: &'static str,
    key_id: &'a str,
}

/// Opaque encrypted envelope passed to a persistence adapter.
///
/// Its fields remain private so adapters cannot accidentally redefine the
/// authenticated encryption contract.
#[derive(Clone)]
pub struct NewEncryptedCredential {
    /// Immutable credential revision identifier bound into the AEAD metadata.
    pub revision_id: ResourceId,
    /// Account and provider connection bound into the AEAD metadata.
    pub identity: ProviderCredentialIdentity,
    /// Stable authenticated-encryption algorithm identifier.
    pub algorithm: &'static str,
    /// External key-ring selector, never key material.
    pub key_id: String,
    /// Unique non-secret XChaCha20 nonce.
    pub nonce: [u8; NONCE_BYTES],
    /// Authenticated ciphertext containing the refresh token and scopes.
    pub ciphertext: Vec<u8>,
    /// Authority-supplied creation time.
    pub created_at: DateTime<Utc>,
}

/// Opaque encrypted envelope returned by a persistence adapter.
///
/// Its fields remain private so callers can obtain plaintext only through an
/// authorized [`ProviderCredentialVault::lease`] operation.
#[derive(Clone)]
pub struct EncryptedCredentialRecord {
    /// Immutable credential revision identifier bound into the AEAD metadata.
    pub revision_id: ResourceId,
    /// Account and provider connection bound into the AEAD metadata.
    pub identity: ProviderCredentialIdentity,
    /// Monotonic provider-connection credential generation.
    pub generation: u32,
    /// Stable authenticated-encryption algorithm identifier.
    pub algorithm: String,
    /// External key-ring selector, never key material.
    pub key_id: String,
    /// Unique non-secret XChaCha20 nonce.
    pub nonce: [u8; NONCE_BYTES],
    /// Authenticated ciphertext containing the refresh token and scopes.
    pub ciphertext: Vec<u8>,
}

/// Persistence boundary for encrypted provider credentials. Implementations
/// never receive plaintext or encryption key material.
#[async_trait]
pub trait ProviderCredentialStore: Send + Sync {
    /// Atomically supersedes any active envelope and stores this revision.
    async fn rotate(
        &self,
        subject: AuthenticatedSubject,
        credential: NewEncryptedCredential,
    ) -> Result<ProviderCredentialRevision, ClientError>;

    /// Loads the active envelope after rechecking current account access.
    async fn load_active(
        &self,
        subject: AuthenticatedSubject,
        identity: &ProviderCredentialIdentity,
    ) -> Result<EncryptedCredentialRecord, ClientError>;

    /// Revokes the active envelope without contacting the provider.
    async fn revoke(
        &self,
        subject: AuthenticatedSubject,
        identity: &ProviderCredentialIdentity,
        reason: &str,
        revoked_at: DateTime<Utc>,
    ) -> Result<ProviderCredentialRevocation, ClientError>;
}

/// Rust-owned credential encryption, leasing, rotation, and revocation authority.
pub struct ProviderCredentialVault<S> {
    store: S,
    keyring: ProviderVaultKeyring,
}

impl<S> ProviderCredentialVault<S>
where
    S: ProviderCredentialStore,
{
    /// Creates a vault over an encrypted persistence store and external key ring.
    pub fn new(store: S, keyring: ProviderVaultKeyring) -> Self {
        Self { store, keyring }
    }

    /// Encrypts and atomically provisions or rotates one refresh credential.
    pub async fn rotate(
        &self,
        subject: AuthenticatedSubject,
        identity: ProviderCredentialIdentity,
        credential: &ProviderRefreshCredential,
        created_at: DateTime<Utc>,
    ) -> Result<ProviderCredentialRevision, ClientError> {
        require_account(subject, &identity)?;
        let revision_id = ResourceId::new();
        let (key_id, key) = self.keyring.active_key();
        let aad = associated_data(revision_id, &identity, key_id)?;
        let mut plaintext = Zeroizing::new(
            serde_json::to_vec(&PayloadRef {
                refresh_token: credential.refresh_token.as_str(),
                scopes: &credential.scopes,
            })
            .map_err(|_| vault_unavailable())?,
        );
        let mut nonce = [0_u8; NONCE_BYTES];
        rand::rng().fill_bytes(&mut nonce);
        let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| vault_unavailable())?;
        let nonce_array = XNonce::from(nonce);
        let ciphertext = cipher
            .encrypt(
                &nonce_array,
                Payload {
                    msg: plaintext.as_slice(),
                    aad: &aad,
                },
            )
            .map_err(|_| vault_unavailable())?;
        plaintext.zeroize();
        self.store
            .rotate(
                subject,
                NewEncryptedCredential {
                    revision_id,
                    identity,
                    algorithm: ALGORITHM,
                    key_id: key_id.to_owned(),
                    nonce,
                    ciphertext,
                    created_at,
                },
            )
            .await
    }

    /// Decrypts the active revision into a short-lived internal lease.
    pub async fn lease(
        &self,
        subject: AuthenticatedSubject,
        identity: &ProviderCredentialIdentity,
    ) -> Result<ProviderCredentialLease, ClientError> {
        require_account(subject, identity)?;
        let record = self.store.load_active(subject, identity).await?;
        if record.algorithm != ALGORITHM || record.identity != *identity {
            return Err(vault_unavailable());
        }
        let aad = associated_data(record.revision_id, identity, &record.key_id)?;
        let cipher = XChaCha20Poly1305::new_from_slice(self.keyring.key(&record.key_id)?)
            .map_err(|_| vault_unavailable())?;
        let nonce_array = XNonce::from(record.nonce);
        let plaintext = Zeroizing::new(
            cipher
                .decrypt(
                    &nonce_array,
                    Payload {
                        msg: &record.ciphertext,
                        aad: &aad,
                    },
                )
                .map_err(|_| vault_unavailable())?,
        );
        let mut payload: OwnedPayload =
            serde_json::from_slice(&plaintext).map_err(|_| vault_unavailable())?;
        let refresh_token = Zeroizing::new(std::mem::take(&mut payload.refresh_token));
        let scopes = std::mem::take(&mut payload.scopes);
        Ok(ProviderCredentialLease {
            refresh_token,
            scopes,
            revision_id: record.revision_id,
            generation: record.generation,
        })
    }

    /// Revokes the active ciphertext without contacting the provider.
    pub async fn revoke(
        &self,
        subject: AuthenticatedSubject,
        identity: &ProviderCredentialIdentity,
        reason: &str,
        revoked_at: DateTime<Utc>,
    ) -> Result<ProviderCredentialRevocation, ClientError> {
        require_account(subject, identity)?;
        let reason = reason.trim();
        if reason.is_empty() || reason.len() > 300 {
            return Err(ClientError::new(ErrorCode::InvalidRequest, false));
        }
        self.store
            .revoke(subject, identity, reason, revoked_at)
            .await
    }
}

/// PostgreSQL encrypted-envelope store. It rechecks current product subject,
/// membership, account, and provider ownership on every operation.
#[derive(Clone)]
pub struct PostgresProviderCredentialStore {
    pool: PgPool,
}

impl PostgresProviderCredentialStore {
    /// Creates the PostgreSQL store over the application-owned pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Verifies migration 0049 before enabling provider-backed hosted work.
    pub async fn verify_schema(&self) -> Result<(), ClientError> {
        let ready: bool =
            sqlx::query_scalar("SELECT to_regclass('provider_credential_vault') IS NOT NULL")
                .fetch_one(&self.pool)
                .await
                .map_err(|_| dependency_unavailable())?;
        if ready {
            Ok(())
        } else {
            Err(ClientError::new(ErrorCode::DependencyUnavailable, false))
        }
    }
}

#[async_trait]
impl ProviderCredentialStore for PostgresProviderCredentialStore {
    async fn rotate(
        &self,
        subject: AuthenticatedSubject,
        credential: NewEncryptedCredential,
    ) -> Result<ProviderCredentialRevision, ClientError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| dependency_unavailable())?;
        require_postgres_authority(&mut transaction, subject, &credential.identity, true).await?;
        // Serialize every generation change on the stable provider account,
        // including reconnects made after the active envelope was revoked.
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM provider_accounts WHERE id = $1 FOR UPDATE")
            .bind(credential.identity.provider_account_id.as_uuid())
            .fetch_one(&mut *transaction)
            .await
            .map_err(|_| dependency_unavailable())?;
        let previous: Option<(Uuid, i32, Option<DateTime<Utc>>)> = sqlx::query_as(
            "SELECT id, generation, revoked_at FROM provider_credential_vault
             WHERE provider_account_id = $1 AND credential_kind = 'oauth_refresh'
             ORDER BY generation DESC LIMIT 1",
        )
        .bind(credential.identity.provider_account_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| dependency_unavailable())?;
        let generation = previous
            .as_ref()
            .map_or(1_i32, |(_, generation, _)| generation.saturating_add(1));
        let rotated = previous
            .as_ref()
            .is_some_and(|(_, _, revoked_at)| revoked_at.is_none());
        if let Some((previous_id, _, None)) = previous {
            sqlx::query(
                "UPDATE provider_credential_vault
                 SET revoked_at = $2, revoked_by_subject_id = $3,
                     revocation_reason = 'rotated'
                 WHERE id = $1 AND revoked_at IS NULL",
            )
            .bind(previous_id)
            .bind(credential.created_at)
            .bind(subject.subject_id.as_uuid())
            .execute(&mut *transaction)
            .await
            .map_err(|_| dependency_unavailable())?;
        }
        sqlx::query(
            "INSERT INTO provider_credential_vault
             (id, chordrift_account_id, provider_account_id, provider,
              credential_kind, generation, algorithm, key_id, nonce,
              ciphertext, created_by_subject_id, created_at)
             VALUES ($1, $2, $3, $4, 'oauth_refresh', $5, $6, $7, $8,
                     $9, $10, $11)",
        )
        .bind(credential.revision_id.as_uuid())
        .bind(credential.identity.account_id.as_uuid())
        .bind(credential.identity.provider_account_id.as_uuid())
        .bind(&credential.identity.provider)
        .bind(generation)
        .bind(credential.algorithm)
        .bind(&credential.key_id)
        .bind(credential.nonce.as_slice())
        .bind(&credential.ciphertext)
        .bind(subject.subject_id.as_uuid())
        .bind(credential.created_at)
        .execute(&mut *transaction)
        .await
        .map_err(|_| ClientError::new(ErrorCode::StateConflict, true))?;
        transaction
            .commit()
            .await
            .map_err(|_| dependency_unavailable())?;
        Ok(ProviderCredentialRevision {
            revision_id: credential.revision_id,
            generation: u32::try_from(generation).map_err(|_| vault_unavailable())?,
            key_id: credential.key_id,
            rotated,
            created_at: credential.created_at,
        })
    }

    async fn load_active(
        &self,
        subject: AuthenticatedSubject,
        identity: &ProviderCredentialIdentity,
    ) -> Result<EncryptedCredentialRecord, ClientError> {
        let row = sqlx::query(
            "SELECT vault.id, vault.generation, vault.algorithm, vault.key_id,
                    vault.nonce, vault.ciphertext
             FROM provider_credential_vault vault
             JOIN provider_accounts provider_account
               ON provider_account.id = vault.provider_account_id
              AND provider_account.chordrift_account_id = vault.chordrift_account_id
              AND provider_account.provider = vault.provider
             JOIN chordrift_accounts account ON account.id = vault.chordrift_account_id
             JOIN chordrift_account_memberships membership
               ON membership.chordrift_account_id = account.id
              AND membership.product_subject_id = $1
             JOIN product_subjects subject ON subject.id = membership.product_subject_id
             WHERE vault.chordrift_account_id = $2
               AND vault.provider_account_id = $3 AND vault.provider = $4
               AND vault.credential_kind = 'oauth_refresh'
               AND vault.revoked_at IS NULL
               AND subject.status = 'active' AND membership.status = 'active'
               AND account.status = 'active'",
        )
        .bind(subject.subject_id.as_uuid())
        .bind(identity.account_id.as_uuid())
        .bind(identity.provider_account_id.as_uuid())
        .bind(&identity.provider)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| dependency_unavailable())?
        .ok_or_else(|| ClientError::new(ErrorCode::PermissionDenied, false))?;
        let nonce: Vec<u8> = row.try_get("nonce").map_err(|_| vault_unavailable())?;
        let nonce: [u8; NONCE_BYTES] = nonce.try_into().map_err(|_| vault_unavailable())?;
        let generation: i32 = row.try_get("generation").map_err(|_| vault_unavailable())?;
        Ok(EncryptedCredentialRecord {
            revision_id: ResourceId::from_uuid(row.try_get("id").map_err(|_| vault_unavailable())?),
            identity: identity.clone(),
            generation: u32::try_from(generation).map_err(|_| vault_unavailable())?,
            algorithm: row.try_get("algorithm").map_err(|_| vault_unavailable())?,
            key_id: row.try_get("key_id").map_err(|_| vault_unavailable())?,
            nonce,
            ciphertext: row.try_get("ciphertext").map_err(|_| vault_unavailable())?,
        })
    }

    async fn revoke(
        &self,
        subject: AuthenticatedSubject,
        identity: &ProviderCredentialIdentity,
        reason: &str,
        revoked_at: DateTime<Utc>,
    ) -> Result<ProviderCredentialRevocation, ClientError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| dependency_unavailable())?;
        require_postgres_authority(&mut transaction, subject, identity, true).await?;
        let row = sqlx::query(
            "UPDATE provider_credential_vault
             SET revoked_at = $4, revoked_by_subject_id = $5,
                 revocation_reason = $6
             WHERE chordrift_account_id = $1 AND provider_account_id = $2
               AND provider = $3 AND credential_kind = 'oauth_refresh'
               AND revoked_at IS NULL
             RETURNING id, generation",
        )
        .bind(identity.account_id.as_uuid())
        .bind(identity.provider_account_id.as_uuid())
        .bind(&identity.provider)
        .bind(revoked_at)
        .bind(subject.subject_id.as_uuid())
        .bind(reason)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| dependency_unavailable())?
        .ok_or_else(|| ClientError::new(ErrorCode::StateConflict, false))?;
        transaction
            .commit()
            .await
            .map_err(|_| dependency_unavailable())?;
        let generation: i32 = row.try_get("generation").map_err(|_| vault_unavailable())?;
        Ok(ProviderCredentialRevocation {
            revision_id: ResourceId::from_uuid(row.try_get("id").map_err(|_| vault_unavailable())?),
            generation: u32::try_from(generation).map_err(|_| vault_unavailable())?,
            revoked_at,
        })
    }
}

async fn require_postgres_authority(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    subject: AuthenticatedSubject,
    identity: &ProviderCredentialIdentity,
    owner_required: bool,
) -> Result<(), ClientError> {
    let authorized: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM provider_accounts provider_account
             JOIN chordrift_accounts account
               ON account.id = provider_account.chordrift_account_id
             JOIN chordrift_account_memberships membership
               ON membership.chordrift_account_id = account.id
              AND membership.product_subject_id = $1
             JOIN product_subjects subject ON subject.id = membership.product_subject_id
             WHERE provider_account.chordrift_account_id = $2
               AND provider_account.id = $3 AND provider_account.provider = $4
               AND subject.status = 'active' AND membership.status = 'active'
               AND account.status = 'active'
               AND ($5 = FALSE OR membership.role = 'owner'))",
    )
    .bind(subject.subject_id.as_uuid())
    .bind(identity.account_id.as_uuid())
    .bind(identity.provider_account_id.as_uuid())
    .bind(&identity.provider)
    .bind(owner_required)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|_| dependency_unavailable())?;
    if authorized {
        Ok(())
    } else {
        Err(ClientError::new(ErrorCode::PermissionDenied, false))
    }
}

fn associated_data(
    revision_id: ResourceId,
    identity: &ProviderCredentialIdentity,
    key_id: &str,
) -> Result<Vec<u8>, ClientError> {
    serde_json::to_vec(&AssociatedData {
        schema_version: 1,
        revision_id: revision_id.as_uuid(),
        account_id: identity.account_id.as_uuid(),
        provider_account_id: identity.provider_account_id.as_uuid(),
        provider: &identity.provider,
        credential_kind: "oauth_refresh",
        algorithm: ALGORITHM,
        key_id,
    })
    .map_err(|_| vault_unavailable())
}

fn require_account(
    subject: AuthenticatedSubject,
    identity: &ProviderCredentialIdentity,
) -> Result<(), ClientError> {
    if subject.account_id == identity.account_id {
        Ok(())
    } else {
        Err(ClientError::new(ErrorCode::PermissionDenied, false))
    }
}

fn normalize_scopes(scopes: impl IntoIterator<Item = String>) -> Result<Vec<String>, ClientError> {
    let mut normalized = scopes
        .into_iter()
        .map(|scope| scope.trim().to_owned())
        .collect::<Vec<_>>();
    if normalized.len() > 128
        || normalized
            .iter()
            .any(|scope| scope.is_empty() || scope.len() > 256)
    {
        return Err(ClientError::new(ErrorCode::InvalidRequest, false));
    }
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn validate_key_id(key_id: &str) -> Result<(), ClientError> {
    if key_id.trim().is_empty()
        || key_id.len() > MAX_KEY_ID_BYTES
        || !key_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/'))
    {
        return Err(ClientError::new(ErrorCode::InvalidRequest, false));
    }
    Ok(())
}

fn vault_unavailable() -> ClientError {
    ClientError::new(ErrorCode::DependencyUnavailable, false)
}

fn dependency_unavailable() -> ClientError {
    ClientError::new(ErrorCode::DependencyUnavailable, true)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct MemoryState {
        active: Option<EncryptedCredentialRecord>,
        history: Vec<EncryptedCredentialRecord>,
        tamper_ciphertext: bool,
        substitute_identity: Option<ProviderCredentialIdentity>,
    }

    struct MemoryStore {
        owner_id: ResourceId,
        member_id: ResourceId,
        state: Mutex<MemoryState>,
    }

    impl MemoryStore {
        fn new(owner_id: ResourceId, member_id: ResourceId) -> Self {
            Self {
                owner_id,
                member_id,
                state: Mutex::new(MemoryState::default()),
            }
        }

        fn active_ciphertext(&self) -> Vec<u8> {
            self.state
                .lock()
                .expect("memory store lock")
                .active
                .as_ref()
                .expect("active credential")
                .ciphertext
                .clone()
        }

        fn set_tamper_ciphertext(&self) {
            self.state
                .lock()
                .expect("memory store lock")
                .tamper_ciphertext = true;
        }

        fn set_substitute_identity(&self, identity: ProviderCredentialIdentity) {
            self.state
                .lock()
                .expect("memory store lock")
                .substitute_identity = Some(identity);
        }
    }

    #[async_trait]
    impl ProviderCredentialStore for Arc<MemoryStore> {
        async fn rotate(
            &self,
            subject: AuthenticatedSubject,
            credential: NewEncryptedCredential,
        ) -> Result<ProviderCredentialRevision, ClientError> {
            if subject.subject_id != self.owner_id {
                return Err(ClientError::new(ErrorCode::PermissionDenied, false));
            }
            let mut state = self.state.lock().expect("memory store lock");
            let generation = state
                .active
                .iter()
                .chain(state.history.iter())
                .map(|row| row.generation)
                .max()
                .map_or(1, |generation| generation.saturating_add(1));
            let rotated = state.active.is_some();
            if let Some(previous) = state.active.take() {
                state.history.push(previous);
            }
            state.active = Some(EncryptedCredentialRecord {
                revision_id: credential.revision_id,
                identity: credential.identity,
                generation,
                algorithm: credential.algorithm.to_owned(),
                key_id: credential.key_id.clone(),
                nonce: credential.nonce,
                ciphertext: credential.ciphertext,
            });
            Ok(ProviderCredentialRevision {
                revision_id: credential.revision_id,
                generation,
                key_id: credential.key_id,
                rotated,
                created_at: credential.created_at,
            })
        }

        async fn load_active(
            &self,
            subject: AuthenticatedSubject,
            _identity: &ProviderCredentialIdentity,
        ) -> Result<EncryptedCredentialRecord, ClientError> {
            if subject.subject_id != self.owner_id && subject.subject_id != self.member_id {
                return Err(ClientError::new(ErrorCode::PermissionDenied, false));
            }
            let state = self.state.lock().expect("memory store lock");
            let mut record = state
                .active
                .clone()
                .ok_or_else(|| ClientError::new(ErrorCode::PermissionDenied, false))?;
            if state.tamper_ciphertext {
                record.ciphertext[0] ^= 0x80;
            }
            if let Some(identity) = &state.substitute_identity {
                record.identity = identity.clone();
            }
            Ok(record)
        }

        async fn revoke(
            &self,
            subject: AuthenticatedSubject,
            _identity: &ProviderCredentialIdentity,
            _reason: &str,
            revoked_at: DateTime<Utc>,
        ) -> Result<ProviderCredentialRevocation, ClientError> {
            if subject.subject_id != self.owner_id {
                return Err(ClientError::new(ErrorCode::PermissionDenied, false));
            }
            let mut state = self.state.lock().expect("memory store lock");
            let record = state
                .active
                .take()
                .ok_or_else(|| ClientError::new(ErrorCode::StateConflict, false))?;
            let result = ProviderCredentialRevocation {
                revision_id: record.revision_id,
                generation: record.generation,
                revoked_at,
            };
            state.history.push(record);
            Ok(result)
        }
    }

    fn fixture() -> (
        Arc<MemoryStore>,
        ProviderCredentialVault<Arc<MemoryStore>>,
        AuthenticatedSubject,
        AuthenticatedSubject,
        ProviderCredentialIdentity,
    ) {
        let account_id = ResourceId::new();
        let owner_id = ResourceId::new();
        let member_id = ResourceId::new();
        let owner = AuthenticatedSubject {
            subject_id: owner_id,
            account_id,
        };
        let member = AuthenticatedSubject {
            subject_id: member_id,
            account_id,
        };
        let identity = ProviderCredentialIdentity::new(account_id, ResourceId::new(), "spotify")
            .expect("valid identity");
        let store = Arc::new(MemoryStore::new(owner_id, member_id));
        let keyring = ProviderVaultKeyring::new(
            "primary-2026-08",
            [("primary-2026-08".to_owned(), vec![7; KEY_BYTES])],
        )
        .expect("valid key ring");
        let vault = ProviderCredentialVault::new(Arc::clone(&store), keyring);
        (store, vault, owner, member, identity)
    }

    fn credential(token: &str) -> ProviderRefreshCredential {
        ProviderRefreshCredential::new(
            token,
            [
                "playlist-read-private".to_owned(),
                "playlist-modify-private".to_owned(),
            ],
        )
        .expect("valid credential")
    }

    #[tokio::test]
    async fn encrypts_round_trips_and_allows_member_lease_without_member_rotation() {
        let (store, vault, owner, member, identity) = fixture();
        let secret = "refresh-token-that-must-never-enter-postgres";
        let revision = vault
            .rotate(owner, identity.clone(), &credential(secret), Utc::now())
            .await
            .expect("owner rotates");
        assert_eq!(revision.generation, 1);
        assert!(!revision.rotated);
        assert!(
            !store
                .active_ciphertext()
                .windows(secret.len())
                .any(|window| window == secret.as_bytes())
        );

        let lease = vault
            .lease(member, &identity)
            .await
            .expect("active member leases");
        assert_eq!(lease.refresh_token(), secret);
        assert_eq!(lease.generation, 1);
        assert_eq!(lease.scopes().len(), 2);

        let error = vault
            .rotate(
                member,
                identity,
                &credential("member-cannot-write"),
                Utc::now(),
            )
            .await
            .expect_err("member rotation denied");
        assert_eq!(error.code, ErrorCode::PermissionDenied);
    }

    #[tokio::test]
    async fn rotation_uses_active_key_and_retained_key_decrypts_old_envelope() {
        let (store, first_vault, owner, _, identity) = fixture();
        let first = first_vault
            .rotate(
                owner,
                identity.clone(),
                &credential("old-secret"),
                Utc::now(),
            )
            .await
            .expect("first revision");
        assert_eq!(first.key_id, "primary-2026-08");

        let rotating_keyring = ProviderVaultKeyring::new(
            "primary-2026-09",
            [
                ("primary-2026-08".to_owned(), vec![7; KEY_BYTES]),
                ("primary-2026-09".to_owned(), vec![9; KEY_BYTES]),
            ],
        )
        .expect("rotating key ring");
        let rotating_vault = ProviderCredentialVault::new(Arc::clone(&store), rotating_keyring);
        assert_eq!(
            rotating_vault
                .lease(owner, &identity)
                .await
                .expect("old key retained")
                .refresh_token(),
            "old-secret"
        );
        let second = rotating_vault
            .rotate(
                owner,
                identity.clone(),
                &credential("new-secret"),
                Utc::now(),
            )
            .await
            .expect("second revision");
        assert_eq!(second.generation, 2);
        assert_eq!(second.key_id, "primary-2026-09");
        assert!(second.rotated);
        assert_eq!(
            rotating_vault
                .lease(owner, &identity)
                .await
                .expect("new lease")
                .refresh_token(),
            "new-secret"
        );
    }

    #[tokio::test]
    async fn tampering_identity_substitution_and_missing_keys_fail_closed() {
        let (store, vault, owner, _, identity) = fixture();
        vault
            .rotate(
                owner,
                identity.clone(),
                &credential("sealed-secret"),
                Utc::now(),
            )
            .await
            .expect("stored");
        store.set_tamper_ciphertext();
        let error = vault
            .lease(owner, &identity)
            .await
            .err()
            .expect("tampering rejected");
        assert_eq!(error.code, ErrorCode::DependencyUnavailable);

        let (store, vault, owner, _, identity) = fixture();
        vault
            .rotate(
                owner,
                identity.clone(),
                &credential("bound-secret"),
                Utc::now(),
            )
            .await
            .expect("stored");
        store.set_substitute_identity(
            ProviderCredentialIdentity::new(identity.account_id, ResourceId::new(), "spotify")
                .expect("alternate identity"),
        );
        let error = vault
            .lease(owner, &identity)
            .await
            .err()
            .expect("identity substitution rejected");
        assert_eq!(error.code, ErrorCode::DependencyUnavailable);

        let missing_keyring =
            ProviderVaultKeyring::new("other-key", [("other-key".to_owned(), vec![3; KEY_BYTES])])
                .expect("valid but unrelated key ring");
        let missing_key_vault = ProviderCredentialVault::new(store, missing_keyring);
        let error = missing_key_vault
            .lease(owner, &identity)
            .await
            .err()
            .expect("unknown key rejected");
        assert_eq!(error.code, ErrorCode::DependencyUnavailable);
    }

    #[tokio::test]
    async fn tenant_mismatch_and_revocation_fail_closed_without_provider_work() {
        let (_store, vault, owner, _, identity) = fixture();
        vault
            .rotate(
                owner,
                identity.clone(),
                &credential("revocable-secret"),
                Utc::now(),
            )
            .await
            .expect("stored");
        let other_subject = AuthenticatedSubject {
            subject_id: owner.subject_id,
            account_id: ResourceId::new(),
        };
        let error = vault
            .lease(other_subject, &identity)
            .await
            .err()
            .expect("cross-account lease denied before storage");
        assert_eq!(error.code, ErrorCode::PermissionDenied);

        let revoked = vault
            .revoke(owner, &identity, "provider disconnected", Utc::now())
            .await
            .expect("owner revokes");
        assert_eq!(revoked.generation, 1);
        let error = vault
            .lease(owner, &identity)
            .await
            .err()
            .expect("revoked credential unavailable");
        assert_eq!(error.code, ErrorCode::PermissionDenied);

        let reconnected = vault
            .rotate(
                owner,
                identity.clone(),
                &credential("new-session-after-reconnect"),
                Utc::now(),
            )
            .await
            .expect("reconnect advances beyond revoked history");
        assert_eq!(reconnected.generation, 2);
        assert!(!reconnected.rotated);
        assert_eq!(
            vault
                .lease(owner, &identity)
                .await
                .expect("reconnected credential active")
                .refresh_token(),
            "new-session-after-reconnect"
        );
    }
}
