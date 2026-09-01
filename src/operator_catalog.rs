//! Host-local catalog resolution for an explicit migration cohort.
//!
//! This module never writes Spotify. It leases the already-encrypted hosted
//! credential only inside the trusted host process, performs bounded catalog
//! reads, and returns privacy-minimized metadata for an operator review.

use chrono::Utc;
use serde::Serialize;
use sqlx::Row as _;
use storexa::Database;

use crate::{
    ChordriftError, Result,
    contract::ResourceId,
    provider_vault::{
        PostgresProviderCredentialStore, ProviderCredentialIdentity, ProviderCredentialVault,
        ProviderVaultKeyring,
    },
    providers::spotify,
    service::AuthenticatedSubject,
};

/// One requested provider identity and its currently available catalog facts.
#[derive(Clone, Debug, Serialize)]
pub struct CatalogTrackResolution {
    /// Requested Spotify track ID.
    pub requested_spotify_id: String,
    /// Whether Spotify currently returned a playable catalog track object.
    pub available: bool,
    /// Current Spotify track ID, which may differ after catalog relinking.
    pub resolved_spotify_id: Option<String>,
    /// Current track title.
    pub title: Option<String>,
    /// Ordered credited artist names.
    pub artists: Vec<String>,
    /// Current album title.
    pub album: Option<String>,
    /// Provider release date at its available precision.
    pub release_date: Option<String>,
    /// Recording ISRC when supplied by Spotify.
    pub isrc: Option<String>,
    /// Track duration in milliseconds.
    pub duration_ms: Option<i32>,
    /// Provider URL retained for attribution and operator review.
    pub spotify_url: Option<String>,
}

/// Resolves at most 500 unique Spotify track IDs through the hosted vault.
pub async fn resolve_hosted_catalog_tracks(
    database: &Database,
    account_label: &str,
    track_ids: &[String],
) -> Result<Vec<CatalogTrackResolution>> {
    validate_track_ids(track_ids)?;
    let row = sqlx::query(
        "SELECT account.id AS provider_account_id,
                account.chordrift_account_id, account.provider_account_id AS stable_provider_id,
                membership.product_subject_id
           FROM provider_accounts account
           JOIN chordrift_account_memberships membership
             ON membership.chordrift_account_id=account.chordrift_account_id
            AND membership.role='owner' AND membership.status='active'
           JOIN product_subjects subject
             ON subject.id=membership.product_subject_id AND subject.status='active'
          WHERE account.provider='spotify' AND account.account_label=$1
          ORDER BY membership.created_at, membership.product_subject_id LIMIT 1",
    )
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(
            "Spotify account has no active hosted owner credential boundary".to_owned(),
        )
    })?;
    let provider_account_id = ResourceId::from_uuid(row.try_get("provider_account_id")?);
    let account_id = ResourceId::from_uuid(row.try_get("chordrift_account_id")?);
    let subject = AuthenticatedSubject {
        subject_id: ResourceId::from_uuid(row.try_get("product_subject_id")?),
        account_id,
    };
    let identity = ProviderCredentialIdentity::new(account_id, provider_account_id, "spotify")
        .map_err(|_| operator_credential_error())?;
    let store = PostgresProviderCredentialStore::new(database.pool().clone());
    let keyring = ProviderVaultKeyring::from_environment().map_err(|_| {
        ChordriftError::Configuration("hosted provider credential key is not ready".to_owned())
    })?;
    let vault = ProviderCredentialVault::new(store, keyring);
    let lease = vault
        .lease(subject, &identity)
        .await
        .map_err(|_| operator_credential_error())?;
    let stable_provider_id: String = row.try_get("stable_provider_id")?;
    let (session, rotated) =
        spotify::hosted_session(lease.refresh_token(), lease.scopes(), &stable_provider_id).await?;
    if let Some(rotated) = rotated.as_ref() {
        vault
            .rotate(subject, identity, rotated, Utc::now())
            .await
            .map_err(|_| operator_credential_error())?;
    }

    let mut resolved = Vec::with_capacity(track_ids.len());
    for chunk in track_ids.chunks(50) {
        let tracks = session.client.catalog_tracks(chunk).await?;
        for (requested_spotify_id, track) in chunk.iter().zip(tracks) {
            resolved.push(match track {
                Some(track) => CatalogTrackResolution {
                    requested_spotify_id: requested_spotify_id.clone(),
                    available: !track.is_local && track.kind == "track",
                    resolved_spotify_id: track.id,
                    title: Some(track.name),
                    artists: track
                        .artists
                        .into_iter()
                        .map(|artist| artist.name)
                        .collect(),
                    album: track.album.as_ref().map(|album| album.name.clone()),
                    release_date: track.album.and_then(|album| album.release_date),
                    isrc: track.external_ids.isrc,
                    duration_ms: track.duration_ms,
                    spotify_url: track.external_urls.spotify().map(str::to_owned),
                },
                None => CatalogTrackResolution {
                    requested_spotify_id: requested_spotify_id.clone(),
                    available: false,
                    resolved_spotify_id: None,
                    title: None,
                    artists: Vec::new(),
                    album: None,
                    release_date: None,
                    isrc: None,
                    duration_ms: None,
                    spotify_url: None,
                },
            });
        }
    }
    Ok(resolved)
}

fn validate_track_ids(track_ids: &[String]) -> Result<()> {
    if track_ids.is_empty() || track_ids.len() > 500 {
        return Err(ChordriftError::Configuration(
            "catalog cohort must contain between 1 and 500 track IDs".to_owned(),
        ));
    }
    let mut normalized = track_ids.to_vec();
    normalized.sort();
    if normalized.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(ChordriftError::Configuration(
            "catalog cohort must not repeat Spotify track IDs".to_owned(),
        ));
    }
    if normalized
        .iter()
        .any(|track_id| track_id.trim().is_empty() || track_id.contains(','))
    {
        return Err(ChordriftError::Configuration(
            "catalog cohort contains an invalid Spotify track ID".to_owned(),
        ));
    }
    Ok(())
}

fn operator_credential_error() -> ChordriftError {
    ChordriftError::Configuration(
        "hosted provider credential is unavailable to the catalog audit".to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::validate_track_ids;

    #[test]
    fn cohort_requires_unique_bounded_track_ids() {
        assert!(validate_track_ids(&["one".to_owned(), "two".to_owned()]).is_ok());
        assert!(validate_track_ids(&[]).is_err());
        assert!(validate_track_ids(&["same".to_owned(), "same".to_owned()]).is_err());
        assert!(validate_track_ids(&["bad,id".to_owned()]).is_err());
        assert!(validate_track_ids(&vec!["id".to_owned(); 501]).is_err());
    }
}
