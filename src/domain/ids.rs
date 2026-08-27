use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::DomainValueError;

macro_rules! uuid_id {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Creates a new random identity.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Wraps an existing UUID.
            #[must_use]
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            /// Returns the underlying UUID.
            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

uuid_id!(
    ChordriftAccountId,
    "Identity of one Chordrift ownership boundary."
);
uuid_id!(
    ProviderConnectionId,
    "Identity of one provider connection owned by a Chordrift account."
);
uuid_id!(
    CanonicalTrackId,
    "Provider-neutral identity of one recording."
);
uuid_id!(
    CollectionId,
    "Identity of one overlapping library collection."
);
uuid_id!(SurfaceId, "Identity of one playlist surface.");
uuid_id!(RecipeId, "Stable identity of one playlist recipe.");
uuid_id!(
    RecipeRevisionId,
    "Identity of one immutable playlist-recipe revision."
);
uuid_id!(SpinId, "Identity of one immutable generated Spin.");
uuid_id!(
    OnboardingSessionId,
    "Identity of one provider-read-only onboarding input capture."
);

/// A typed resource identity paired with its Chordrift owner.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AccountOwnedId<I> {
    account_id: ChordriftAccountId,
    resource_id: I,
}

impl<I> AccountOwnedId<I> {
    /// Associates a resource identity with exactly one account.
    #[must_use]
    pub const fn new(account_id: ChordriftAccountId, resource_id: I) -> Self {
        Self {
            account_id,
            resource_id,
        }
    }

    /// Returns the owning account.
    #[must_use]
    pub const fn account_id(&self) -> ChordriftAccountId {
        self.account_id
    }

    /// Returns the contained resource identity by reference.
    #[must_use]
    pub const fn resource_id(&self) -> &I {
        &self.resource_id
    }

    /// Consumes the association and returns the resource identity.
    #[must_use]
    pub fn into_resource_id(self) -> I {
        self.resource_id
    }
}

/// Stable lowercase namespace identifying a provider adapter.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ProviderNamespace(String);

impl ProviderNamespace {
    /// Validates a provider namespace such as `spotify` or `apple_music`.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainValueError> {
        let value = value.into();
        if value.is_empty()
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err(DomainValueError::InvalidProviderNamespace);
        }
        Ok(Self(value))
    }

    /// Returns the stable namespace string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderNamespace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ProviderNamespace {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

#[derive(Deserialize)]
struct RawProviderId {
    provider: ProviderNamespace,
    value: String,
}

macro_rules! provider_id {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        pub struct $name {
            provider: ProviderNamespace,
            value: String,
        }

        impl $name {
            /// Creates a provider-qualified identity.
            pub fn new(
                provider: ProviderNamespace,
                value: impl Into<String>,
            ) -> Result<Self, DomainValueError> {
                let value = value.into();
                if value.is_empty() {
                    return Err(DomainValueError::EmptyValue);
                }
                if value.chars().any(char::is_control) {
                    return Err(DomainValueError::ControlCharacter);
                }
                Ok(Self { provider, value })
            }

            /// Returns the provider namespace.
            #[must_use]
            pub const fn provider(&self) -> &ProviderNamespace {
                &self.provider
            }

            /// Returns the opaque provider-owned value.
            #[must_use]
            pub fn value(&self) -> &str {
                &self.value
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}:{}", self.provider, self.value)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let raw = RawProviderId::deserialize(deserializer)?;
                Self::new(raw.provider, raw.value).map_err(D::Error::custom)
            }
        }
    };
}

provider_id!(
    ProviderAccountId,
    "Provider-qualified identity of an account at a music provider."
);
provider_id!(
    ProviderTrackId,
    "Provider-qualified identity of a track at a music provider."
);
provider_id!(
    ProviderPlaylistId,
    "Provider-qualified identity of a playlist at a music provider."
);

/// Account ownership and provider identity for one connected provider account.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProviderConnectionIdentity {
    /// Chordrift-owned connection identity.
    pub connection_id: ProviderConnectionId,
    /// Product account that exclusively owns this connection.
    pub account_id: ChordriftAccountId,
    /// Provider-qualified account identity.
    pub provider_account_id: ProviderAccountId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_ids_are_qualified_by_namespace() {
        let spotify = ProviderTrackId::new(
            ProviderNamespace::new("spotify").expect("namespace is valid"),
            "same-id",
        )
        .expect("provider ID is valid");
        let apple = ProviderTrackId::new(
            ProviderNamespace::new("apple_music").expect("namespace is valid"),
            "same-id",
        )
        .expect("provider ID is valid");

        assert_ne!(spotify, apple);
        assert_eq!(spotify.to_string(), "spotify:same-id");
        assert_eq!(apple.to_string(), "apple_music:same-id");
    }

    #[test]
    fn rejects_ambiguous_provider_namespaces_and_opaque_control_characters() {
        assert_eq!(
            ProviderNamespace::new("Apple Music"),
            Err(DomainValueError::InvalidProviderNamespace)
        );
        assert_eq!(
            ProviderTrackId::new(
                ProviderNamespace::new("spotify").expect("namespace is valid"),
                "bad\nvalue",
            ),
            Err(DomainValueError::ControlCharacter)
        );
        assert!(serde_json::from_str::<ProviderNamespace>("\"Apple Music\"").is_err());
        assert!(
            serde_json::from_str::<ProviderTrackId>(
                r#"{"provider":"spotify","value":"bad\nvalue"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn owned_ids_round_trip_without_losing_the_account_boundary() {
        let owned = AccountOwnedId::new(ChordriftAccountId::new(), CollectionId::new());
        let encoded = serde_json::to_string(&owned).expect("owned ID serializes");
        let decoded: AccountOwnedId<CollectionId> =
            serde_json::from_str(&encoded).expect("owned ID deserializes");

        assert_eq!(decoded, owned);
        assert_eq!(decoded.account_id(), owned.account_id());
    }
}
