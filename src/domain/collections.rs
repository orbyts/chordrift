use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use super::{AccountOwnedId, CanonicalTrackId, CollectionId, DomainValueError};

/// User-intent strength attached to a collection membership.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipStrength {
    /// Explicit user boundary that normal generation must not cross.
    HardBoundary,
    /// Strong user preference that controls eligibility or rank.
    StrongPreference,
    /// Supporting fact that informs rank without becoming a boundary.
    SupportingFact,
    /// Inert proposal awaiting review and approval.
    Proposed,
}

impl MembershipStrength {
    /// Reports whether this strength outranks another strength.
    #[must_use]
    pub const fn outranks(self, other: Self) -> bool {
        self.rank() > other.rank()
    }

    const fn rank(self) -> u8 {
        match self {
            Self::HardBoundary => 4,
            Self::StrongPreference => 3,
            Self::SupportingFact => 2,
            Self::Proposed => 1,
        }
    }
}

/// Provenance class explaining why a collection membership exists.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipProvenance {
    /// Direct user statement or correction.
    ExplicitUser,
    /// User-approved reusable rule.
    ApprovedRule,
    /// Reliable fact observed from a provider.
    ProviderFact,
    /// Reliable fact obtained from an independent external source.
    ExternalFact,
    /// Learned affinity proposed from repeated evidence.
    LearnedAffinity,
    /// Unresolved proposal retained for review.
    ReviewProposal,
}

/// Confidence in basis points from zero through ten thousand.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct MembershipConfidence(u16);

impl MembershipConfidence {
    /// Lowest representable confidence.
    pub const MIN: Self = Self(0);
    /// Highest representable confidence.
    pub const MAX: Self = Self(10_000);

    /// Creates a confidence value when it is within the basis-point range.
    #[must_use]
    pub const fn new(basis_points: u16) -> Option<Self> {
        if basis_points <= Self::MAX.0 {
            Some(Self(basis_points))
        } else {
            None
        }
    }

    /// Returns confidence in basis points.
    #[must_use]
    pub const fn basis_points(self) -> u16 {
        self.0
    }
}

impl<'de> Deserialize<'de> for MembershipConfidence {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u16::deserialize(deserializer)?;
        Self::new(value).ok_or_else(|| D::Error::custom("membership confidence exceeds 10000"))
    }
}

/// Account-scoped membership of a canonical track in an overlapping collection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CollectionMembership {
    /// Collection receiving the membership.
    collection_id: AccountOwnedId<CollectionId>,
    /// Canonical recording participating in the collection.
    track_id: AccountOwnedId<CanonicalTrackId>,
    /// User-intent strength of the membership.
    strength: MembershipStrength,
    /// Evidence class behind the membership.
    provenance: MembershipProvenance,
    /// Confidence in the underlying claim.
    confidence: MembershipConfidence,
}

impl CollectionMembership {
    /// Creates a membership only when both resources have the same owner.
    pub fn new(
        collection_id: AccountOwnedId<CollectionId>,
        track_id: AccountOwnedId<CanonicalTrackId>,
        strength: MembershipStrength,
        provenance: MembershipProvenance,
        confidence: MembershipConfidence,
    ) -> Result<Self, DomainValueError> {
        if collection_id.account_id() != track_id.account_id() {
            return Err(DomainValueError::OwnershipMismatch);
        }
        Ok(Self {
            collection_id,
            track_id,
            strength,
            provenance,
            confidence,
        })
    }

    /// Returns the account-owned collection identity.
    #[must_use]
    pub const fn collection_id(&self) -> &AccountOwnedId<CollectionId> {
        &self.collection_id
    }

    /// Returns the account-owned canonical track identity.
    #[must_use]
    pub const fn track_id(&self) -> &AccountOwnedId<CanonicalTrackId> {
        &self.track_id
    }

    /// Returns the user-intent strength.
    #[must_use]
    pub const fn strength(&self) -> MembershipStrength {
        self.strength
    }

    /// Returns the membership provenance.
    #[must_use]
    pub const fn provenance(&self) -> MembershipProvenance {
        self.provenance
    }

    /// Returns the membership confidence.
    #[must_use]
    pub const fn confidence(&self) -> MembershipConfidence {
        self.confidence
    }
}

#[derive(Deserialize)]
struct RawCollectionMembership {
    collection_id: AccountOwnedId<CollectionId>,
    track_id: AccountOwnedId<CanonicalTrackId>,
    strength: MembershipStrength,
    provenance: MembershipProvenance,
    confidence: MembershipConfidence,
}

impl<'de> Deserialize<'de> for CollectionMembership {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawCollectionMembership::deserialize(deserializer)?;
        Self::new(
            raw.collection_id,
            raw.track_id,
            raw.strength,
            raw.provenance,
            raw.confidence,
        )
        .map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ChordriftAccountId;

    #[test]
    fn hard_user_boundaries_outrank_learned_proposals() {
        assert!(MembershipStrength::HardBoundary.outranks(MembershipStrength::Proposed));
        assert!(MembershipStrength::StrongPreference.outranks(MembershipStrength::SupportingFact));
        assert!(!MembershipStrength::Proposed.outranks(MembershipStrength::HardBoundary));
    }

    #[test]
    fn membership_cannot_cross_account_ownership() {
        let result = CollectionMembership::new(
            AccountOwnedId::new(ChordriftAccountId::new(), CollectionId::new()),
            AccountOwnedId::new(ChordriftAccountId::new(), CanonicalTrackId::new()),
            MembershipStrength::Proposed,
            MembershipProvenance::ReviewProposal,
            MembershipConfidence::MIN,
        );

        assert_eq!(result, Err(DomainValueError::OwnershipMismatch));
    }

    #[test]
    fn confidence_is_bounded_and_precise() {
        assert_eq!(
            MembershipConfidence::new(8_750).map(MembershipConfidence::basis_points),
            Some(8_750)
        );
        assert_eq!(MembershipConfidence::new(10_001), None);
        assert!(serde_json::from_str::<MembershipConfidence>("10001").is_err());
    }
}
