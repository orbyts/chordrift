use keyring::{Entry, Error as KeyringError};

use crate::{ChordriftError, Result};

const SERVICE: &str = "io.github.orbyts.chordrift.credentials";

/// Provider-, account-, and purpose-scoped credential identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecretId {
    provider: String,
    account: String,
    kind: String,
}

impl SecretId {
    /// Creates a validated credential identity.
    pub fn new(provider: &str, account: &str, kind: &str) -> Result<Self> {
        for (label, value) in [
            ("provider", provider),
            ("account", account),
            ("credential kind", kind),
        ] {
            if value.trim().is_empty() || value.contains(':') {
                return Err(ChordriftError::Configuration(format!(
                    "credential {label} must be non-empty and cannot contain ':'"
                )));
            }
        }
        Ok(Self {
            provider: provider.to_owned(),
            account: account.to_owned(),
            kind: kind.to_owned(),
        })
    }

    fn username(&self) -> String {
        format!("{}:{}:{}", self.provider, self.account, self.kind)
    }
}

/// Secret-store boundary used by provider authentication.
pub trait CredentialStore {
    /// Saves or replaces one credential.
    fn save(&self, id: &SecretId, secret: &[u8]) -> Result<()>;
    /// Loads one credential, returning `None` when it does not exist.
    fn load(&self, id: &SecretId) -> Result<Option<Vec<u8>>>;
    /// Deletes one credential and reports whether it existed.
    fn delete(&self, id: &SecretId) -> Result<bool>;
}

/// macOS Passwords/Keychain-backed credential store.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemCredentialStore;

impl SystemCredentialStore {
    fn entry(id: &SecretId) -> Result<Entry> {
        Entry::new(SERVICE, &id.username()).map_err(ChordriftError::Credential)
    }
}

impl CredentialStore for SystemCredentialStore {
    fn save(&self, id: &SecretId, secret: &[u8]) -> Result<()> {
        Self::entry(id)?
            .set_secret(secret)
            .map_err(ChordriftError::Credential)
    }

    fn load(&self, id: &SecretId) -> Result<Option<Vec<u8>>> {
        match Self::entry(id)?.get_secret() {
            Ok(secret) => Ok(Some(secret)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(ChordriftError::Credential(error)),
        }
    }

    fn delete(&self, id: &SecretId) -> Result<bool> {
        match Self::entry(id)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(KeyringError::NoEntry) => Ok(false),
            Err(error) => Err(ChordriftError::Credential(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SecretId;

    #[test]
    fn credential_identity_is_provider_and_account_scoped() {
        let id = SecretId::new("spotify", "personal", "oauth").expect("valid identity");
        assert_eq!(id.username(), "spotify:personal:oauth");
        assert!(SecretId::new("spot:ify", "personal", "oauth").is_err());
    }
}
