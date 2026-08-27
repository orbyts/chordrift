use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use super::{AccountOwnedId, DomainValueError, RecipeRevisionIdentity, SpinId};

/// Account-bound identity of one immutable Spin and its recipe revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SpinIdentity {
    /// Account-owned Spin identity.
    spin_id: AccountOwnedId<SpinId>,
    /// Immutable recipe revision used to generate the Spin.
    recipe_revision: RecipeRevisionIdentity,
}

impl SpinIdentity {
    /// Associates a Spin with a recipe revision under the same account owner.
    pub fn new(
        spin_id: AccountOwnedId<SpinId>,
        recipe_revision: RecipeRevisionIdentity,
    ) -> Result<Self, DomainValueError> {
        if spin_id.account_id() != recipe_revision.recipe_id.account_id() {
            return Err(DomainValueError::OwnershipMismatch);
        }
        Ok(Self {
            spin_id,
            recipe_revision,
        })
    }

    /// Returns the account-owned Spin identity.
    #[must_use]
    pub const fn spin_id(&self) -> AccountOwnedId<SpinId> {
        self.spin_id
    }

    /// Returns the immutable recipe revision identity.
    #[must_use]
    pub const fn recipe_revision(&self) -> RecipeRevisionIdentity {
        self.recipe_revision
    }
}

#[derive(Deserialize)]
struct RawSpinIdentity {
    spin_id: AccountOwnedId<SpinId>,
    recipe_revision: RecipeRevisionIdentity,
}

impl<'de> Deserialize<'de> for SpinIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawSpinIdentity::deserialize(deserializer)?;
        Self::new(raw.spin_id, raw.recipe_revision).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ChordriftAccountId, RecipeId, RecipeRevisionId};

    #[test]
    fn spin_identity_is_bound_to_its_recipe_owner() {
        let account_id = ChordriftAccountId::new();
        let identity = SpinIdentity::new(
            AccountOwnedId::new(account_id, SpinId::new()),
            RecipeRevisionIdentity {
                recipe_id: AccountOwnedId::new(account_id, RecipeId::new()),
                revision_id: RecipeRevisionId::new(),
            },
        )
        .expect("owners match");

        let encoded = serde_json::to_string(&identity).expect("Spin identity serializes");
        let decoded: SpinIdentity =
            serde_json::from_str(&encoded).expect("Spin identity deserializes");
        assert_eq!(decoded, identity);
    }

    #[test]
    fn spin_identity_rejects_a_recipe_from_another_account() {
        let result = SpinIdentity::new(
            AccountOwnedId::new(ChordriftAccountId::new(), SpinId::new()),
            RecipeRevisionIdentity {
                recipe_id: AccountOwnedId::new(ChordriftAccountId::new(), RecipeId::new()),
                revision_id: RecipeRevisionId::new(),
            },
        );

        assert_eq!(result, Err(DomainValueError::OwnershipMismatch));
    }
}
