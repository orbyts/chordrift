//! Account-owned provider connection lifecycle behind every client skin.
//!
//! OAuth transport obtains a provider identity and refresh credential; this
//! authority alone decides whether that identity resumes an existing
//! connection, creates an isolated connection, or conflicts with ownership.

use chrono::Utc;
use sqlx::{PgPool, Row as _};
use uuid::Uuid;

use crate::{
    contract::{ClientError, ErrorCode, ResourceId},
    provider_vault::{
        PostgresProviderCredentialStore, ProviderCredentialIdentity, ProviderCredentialVault,
        ProviderRefreshCredential,
    },
    service::AuthenticatedSubject,
};

/// PostgreSQL and encrypted-vault implementation of provider lifecycle rules.
pub struct PostgresProviderConnectionAuthority {
    pool: PgPool,
    vault: ProviderCredentialVault<PostgresProviderCredentialStore>,
}

impl PostgresProviderConnectionAuthority {
    /// Creates one account-scoped provider connection authority.
    pub fn new(
        pool: PgPool,
        vault: ProviderCredentialVault<PostgresProviderCredentialStore>,
    ) -> Self {
        Self { pool, vault }
    }

    /// Returns whether one Spotify connection is owned by the caller.
    pub async fn owns_spotify_connection(
        &self,
        subject: AuthenticatedSubject,
        connection_id: ResourceId,
    ) -> bool {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                 SELECT 1 FROM provider_accounts
                  WHERE id = $1 AND chordrift_account_id = $2 AND provider = 'spotify'
             )",
        )
        .bind(connection_id.as_uuid())
        .bind(subject.account_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .unwrap_or(false)
    }

    /// Resolves a stable Spotify identity and encrypts its refresh credential.
    /// A pinned reconnect can only resume that exact existing connection.
    pub async fn connect_spotify(
        &self,
        subject: AuthenticatedSubject,
        expected_connection_id: Option<ResourceId>,
        external_account_id: &str,
        display_name: Option<&str>,
        credential: &ProviderRefreshCredential,
    ) -> Result<ResourceId, ClientError> {
        if external_account_id.trim().is_empty() {
            return Err(ClientError::new(ErrorCode::InvalidRequest, false));
        }
        let existing = sqlx::query(
            "SELECT id, chordrift_account_id
               FROM provider_accounts
              WHERE provider = 'spotify' AND provider_account_id = $1",
        )
        .bind(external_account_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| unavailable())?;
        let existing_identity = existing
            .map(|row| {
                Ok((
                    ResourceId::from_uuid(
                        row.try_get::<Uuid, _>("chordrift_account_id")
                            .map_err(|_| unavailable())?,
                    ),
                    ResourceId::from_uuid(row.try_get::<Uuid, _>("id").map_err(|_| unavailable())?),
                ))
            })
            .transpose()?;
        let connection_id = if let Some(id) =
            resolve_spotify_identity(subject, expected_connection_id, existing_identity)?
        {
            id
        } else {
            let id = ResourceId::new();
            sqlx::query(
                "INSERT INTO provider_accounts
                    (id, provider, provider_account_id, account_label, display_name,
                     chordrift_account_id, last_authenticated_at, created_at, updated_at)
                 VALUES ($1, 'spotify', $2, $3, $4, $5, now(), now(), now())",
            )
            .bind(id.as_uuid())
            .bind(external_account_id)
            .bind(format!("spotify-{}", id.as_uuid()))
            .bind(display_name)
            .bind(subject.account_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|_| ClientError::new(ErrorCode::StateConflict, true))?;
            id
        };
        sqlx::query(
            "UPDATE provider_accounts
                SET display_name = $1, last_authenticated_at = now(), updated_at = now()
              WHERE id = $2 AND chordrift_account_id = $3",
        )
        .bind(display_name)
        .bind(connection_id.as_uuid())
        .bind(subject.account_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|_| unavailable())?;
        let identity =
            ProviderCredentialIdentity::new(subject.account_id, connection_id, "spotify")?;
        self.vault
            .rotate(subject, identity, credential, Utc::now())
            .await?;
        Ok(connection_id)
    }

    /// Revokes Spotify access while retaining all provider history and intent.
    pub async fn disconnect_spotify(
        &self,
        subject: AuthenticatedSubject,
        connection_id: ResourceId,
    ) -> Result<(), ClientError> {
        if !self.owns_spotify_connection(subject, connection_id).await {
            return Err(ClientError::new(ErrorCode::ResourceNotFound, false));
        }
        let active = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                 SELECT 1 FROM provider_credential_vault
                  WHERE provider_account_id = $1 AND credential_kind = 'oauth_refresh'
                    AND revoked_at IS NULL
             )",
        )
        .bind(connection_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|_| unavailable())?;
        if active {
            let identity =
                ProviderCredentialIdentity::new(subject.account_id, connection_id, "spotify")?;
            self.vault
                .revoke(subject, &identity, "provider disconnected", Utc::now())
                .await?;
        }
        Ok(())
    }
}

fn resolve_spotify_identity(
    subject: AuthenticatedSubject,
    expected_connection_id: Option<ResourceId>,
    existing: Option<(ResourceId, ResourceId)>,
) -> Result<Option<ResourceId>, ClientError> {
    match existing {
        Some((owner, id))
            if owner == subject.account_id
                && expected_connection_id.is_none_or(|expected| expected == id) =>
        {
            Ok(Some(id))
        }
        Some(_) => Err(ClientError::new(ErrorCode::StateConflict, false)),
        None if expected_connection_id.is_some() => {
            Err(ClientError::new(ErrorCode::StateConflict, false))
        }
        None => Ok(None),
    }
}

fn unavailable() -> ClientError {
    ClientError::new(ErrorCode::DependencyUnavailable, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnects_in_place_and_never_crosses_accounts() {
        let account = ResourceId::new();
        let other_account = ResourceId::new();
        let connection = ResourceId::new();
        let subject = AuthenticatedSubject {
            subject_id: ResourceId::new(),
            account_id: account,
        };
        assert_eq!(
            resolve_spotify_identity(subject, Some(connection), Some((account, connection))),
            Ok(Some(connection))
        );
        assert_eq!(
            resolve_spotify_identity(subject, None, Some((account, connection))),
            Ok(Some(connection))
        );
        assert_eq!(
            resolve_spotify_identity(subject, None, Some((other_account, connection)))
                .expect_err("cross-account identity must fail")
                .code,
            ErrorCode::StateConflict
        );
        assert_eq!(
            resolve_spotify_identity(subject, Some(connection), None)
                .expect_err("pinned reconnect cannot create")
                .code,
            ErrorCode::StateConflict
        );
        assert_eq!(resolve_spotify_identity(subject, None, None), Ok(None));
    }
}
