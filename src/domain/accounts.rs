use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use super::{
    ChordriftAccountId, DomainValueError, EvidenceCapabilities, ProviderCapabilities,
    ProviderConnectionIdentity,
};

/// Explicit account and selected-provider context for application work.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AccountContext {
    account_id: ChordriftAccountId,
    provider_connection: ProviderConnectionIdentity,
    provider_capabilities: ProviderCapabilities,
    evidence_capabilities: EvidenceCapabilities,
}

impl AccountContext {
    /// Creates a context only when ownership and capability identities agree.
    pub fn new(
        account_id: ChordriftAccountId,
        provider_connection: ProviderConnectionIdentity,
        provider_capabilities: ProviderCapabilities,
        evidence_capabilities: EvidenceCapabilities,
    ) -> Result<Self, DomainValueError> {
        if provider_connection.account_id != account_id {
            return Err(DomainValueError::OwnershipMismatch);
        }
        if provider_capabilities.provider_connection_id != provider_connection.connection_id {
            return Err(DomainValueError::ProviderConnectionMismatch);
        }
        Ok(Self {
            account_id,
            provider_connection,
            provider_capabilities,
            evidence_capabilities,
        })
    }

    /// Returns the Chordrift ownership boundary.
    #[must_use]
    pub const fn account_id(&self) -> ChordriftAccountId {
        self.account_id
    }

    /// Returns the selected provider connection identity.
    #[must_use]
    pub const fn provider_connection(&self) -> &ProviderConnectionIdentity {
        &self.provider_connection
    }

    /// Returns the provider capability snapshot.
    #[must_use]
    pub const fn provider_capabilities(&self) -> &ProviderCapabilities {
        &self.provider_capabilities
    }

    /// Returns the evidence capability snapshot.
    #[must_use]
    pub const fn evidence_capabilities(&self) -> &EvidenceCapabilities {
        &self.evidence_capabilities
    }
}

#[derive(Deserialize)]
struct RawAccountContext {
    account_id: ChordriftAccountId,
    provider_connection: ProviderConnectionIdentity,
    provider_capabilities: ProviderCapabilities,
    evidence_capabilities: EvidenceCapabilities,
}

impl<'de> Deserialize<'de> for AccountContext {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawAccountContext::deserialize(deserializer)?;
        Self::new(
            raw.account_id,
            raw.provider_connection,
            raw.provider_capabilities,
            raw.evidence_capabilities,
        )
        .map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::domain::{ProviderAccountId, ProviderConnectionId, ProviderNamespace};

    fn connection(account_id: ChordriftAccountId) -> ProviderConnectionIdentity {
        ProviderConnectionIdentity {
            connection_id: ProviderConnectionId::new(),
            account_id,
            provider_account_id: ProviderAccountId::new(
                ProviderNamespace::new("spotify").expect("namespace is valid"),
                "provider-account",
            )
            .expect("provider account is valid"),
        }
    }

    #[test]
    fn account_context_requires_one_owner_and_one_connection() {
        let account_id = ChordriftAccountId::new();
        let provider_connection = connection(account_id);
        let capabilities =
            ProviderCapabilities::new(provider_connection.connection_id, BTreeMap::new());
        let context = AccountContext::new(
            account_id,
            provider_connection,
            capabilities,
            EvidenceCapabilities::default(),
        )
        .expect("context identities agree");

        assert_eq!(context.account_id(), account_id);
    }

    #[test]
    fn account_context_rejects_mismatched_owner_or_capability_report() {
        let account_id = ChordriftAccountId::new();
        let provider_connection = connection(account_id);
        assert_eq!(
            AccountContext::new(
                ChordriftAccountId::new(),
                provider_connection.clone(),
                ProviderCapabilities::new(provider_connection.connection_id, BTreeMap::new()),
                EvidenceCapabilities::default(),
            ),
            Err(DomainValueError::OwnershipMismatch)
        );
        assert_eq!(
            AccountContext::new(
                account_id,
                provider_connection,
                ProviderCapabilities::new(ProviderConnectionId::new(), BTreeMap::new()),
                EvidenceCapabilities::default(),
            ),
            Err(DomainValueError::ProviderConnectionMismatch)
        );
    }
}
