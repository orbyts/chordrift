use std::num::NonZeroU16;

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use super::{
    AccountOwnedId, CollectionId, DomainValueError, EvidenceCapability, RecipeId, RecipeRevisionId,
};

/// Supported immutable recipe schema.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipeSchemaVersion {
    /// Initial Discovery + Rediscovery value vocabulary.
    V1,
}

/// Lifecycle lane from which a recipe allocates tracks.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceLane {
    /// Newly observed discoveries.
    Discovery,
    /// Material beginning to establish affinity.
    Emerging,
    /// Known anchor material.
    Familiar,
    /// Material currently receiving frequent playback.
    HighRotation,
    /// Previously known material outside current rotation.
    Dormant,
    /// Material intentionally returning after absence.
    Recovery,
}

/// Relative allocation weight; zero deliberately disables one source.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct AllocationWeight(u16);

impl AllocationWeight {
    /// Creates a relative allocation weight, including zero.
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Returns the relative weight.
    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }

    /// Reports whether this source is active.
    #[must_use]
    pub const fn is_active(self) -> bool {
        self.0 != 0
    }
}

/// Provider-neutral source used to populate a lifecycle lane.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum RecipeSource {
    /// Draw from an overlapping library collection.
    Collection(AccountOwnedId<CollectionId>),
    /// Draw from tracks supported by an evidence capability.
    Evidence(EvidenceCapability),
}

/// One weighted source assigned to a lifecycle lane.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SourceAllocation {
    /// Lifecycle role used during selection.
    pub lane: SourceLane,
    /// Collection or evidence source.
    pub source: RecipeSource,
    /// Relative number of seats allocated to the source.
    pub weight: AllocationWeight,
}

/// Periodic distribution of familiar anchors in the final order.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "mode", content = "positions", rename_all = "snake_case")]
pub enum FamiliarityCadence {
    /// Do not require periodic familiar anchors.
    Disabled,
    /// Place approximately one familiar anchor in each nonzero position span.
    Every(NonZeroU16),
}

/// High-level ordering philosophy applied after track selection.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderingNarrative {
    /// Deterministic seeded shuffle.
    Shuffle,
    /// Prefer smooth adjacent transitions.
    SmoothTransitions,
    /// Prefer deliberate changes in sound or energy.
    IntentionalContrast,
    /// Follow explicit warm-up, focus, and landing sections.
    SectionedJourney,
}

/// Narrative section used by a sectioned ordering policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipeSection {
    /// Opening section that establishes the experience.
    WarmUp,
    /// Main section carrying the recipe's central intent.
    Focus,
    /// Closing section that resolves the experience.
    Landing,
}

/// Hard policy category recognized by recipe v1.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardrailKind {
    /// Respect hard collection boundaries and exclusions.
    HardBoundaries,
    /// Bound repetition by the same artist.
    ArtistRepetition,
    /// Require spacing between tracks by the same artist.
    ArtistSpacing,
    /// Bound target listening duration.
    Duration,
    /// Bound reuse across related outputs.
    CrossOutputReuse,
}

/// Stable recipe identity plus one immutable revision.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecipeRevisionIdentity {
    /// Account-owned stable recipe identity.
    pub recipe_id: AccountOwnedId<RecipeId>,
    /// Immutable revision identity within the owning account.
    pub revision_id: RecipeRevisionId,
}

/// Immutable recipe-v1 value specification without execution behavior.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RecipeV1 {
    /// Stable and revision identities.
    identity: RecipeRevisionIdentity,
    /// Weighted collection and evidence sources.
    allocations: Vec<SourceAllocation>,
    /// Familiar-anchor distribution policy.
    familiarity_cadence: FamiliarityCadence,
    /// Ordering policy applied after selection.
    ordering: OrderingNarrative,
    /// Optional ordered narrative sections.
    sections: Vec<RecipeSection>,
    /// Hard policy categories required by this recipe.
    guardrails: Vec<GuardrailKind>,
}

impl RecipeV1 {
    /// Validates account ownership and requires at least one positive allocation.
    pub fn new(
        identity: RecipeRevisionIdentity,
        allocations: Vec<SourceAllocation>,
        familiarity_cadence: FamiliarityCadence,
        ordering: OrderingNarrative,
        sections: Vec<RecipeSection>,
        guardrails: Vec<GuardrailKind>,
    ) -> Result<Self, DomainValueError> {
        if !allocations
            .iter()
            .any(|allocation| allocation.weight.is_active())
        {
            return Err(DomainValueError::NoActiveAllocations);
        }
        let account_id = identity.recipe_id.account_id();
        if allocations.iter().any(|allocation| {
            matches!(
                &allocation.source,
                RecipeSource::Collection(collection) if collection.account_id() != account_id
            )
        }) {
            return Err(DomainValueError::OwnershipMismatch);
        }
        Ok(Self {
            identity,
            allocations,
            familiarity_cadence,
            ordering,
            sections,
            guardrails,
        })
    }

    /// Returns the recipe schema implemented by this value.
    #[must_use]
    pub const fn schema_version(&self) -> RecipeSchemaVersion {
        RecipeSchemaVersion::V1
    }

    /// Returns the immutable recipe revision identity.
    #[must_use]
    pub const fn identity(&self) -> RecipeRevisionIdentity {
        self.identity
    }

    /// Returns weighted source allocations.
    #[must_use]
    pub fn allocations(&self) -> &[SourceAllocation] {
        &self.allocations
    }

    /// Returns the familiar-anchor cadence.
    #[must_use]
    pub const fn familiarity_cadence(&self) -> FamiliarityCadence {
        self.familiarity_cadence
    }

    /// Returns the post-selection ordering narrative.
    #[must_use]
    pub const fn ordering(&self) -> OrderingNarrative {
        self.ordering
    }

    /// Returns the ordered narrative sections.
    #[must_use]
    pub fn sections(&self) -> &[RecipeSection] {
        &self.sections
    }

    /// Returns the required hard guardrail categories.
    #[must_use]
    pub fn guardrails(&self) -> &[GuardrailKind] {
        &self.guardrails
    }
}

#[derive(Deserialize)]
struct RawRecipeV1 {
    identity: RecipeRevisionIdentity,
    allocations: Vec<SourceAllocation>,
    familiarity_cadence: FamiliarityCadence,
    ordering: OrderingNarrative,
    sections: Vec<RecipeSection>,
    guardrails: Vec<GuardrailKind>,
}

impl<'de> Deserialize<'de> for RecipeV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawRecipeV1::deserialize(deserializer)?;
        Self::new(
            raw.identity,
            raw.allocations,
            raw.familiarity_cadence,
            raw.ordering,
            raw.sections,
            raw.guardrails,
        )
        .map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ChordriftAccountId;

    fn identity(account_id: ChordriftAccountId) -> RecipeRevisionIdentity {
        RecipeRevisionIdentity {
            recipe_id: AccountOwnedId::new(account_id, RecipeId::new()),
            revision_id: RecipeRevisionId::new(),
        }
    }

    #[test]
    fn zero_weight_is_valid_when_another_lane_is_active() {
        let account_id = ChordriftAccountId::new();
        let recipe = RecipeV1::new(
            identity(account_id),
            vec![
                SourceAllocation {
                    lane: SourceLane::Familiar,
                    source: RecipeSource::Collection(AccountOwnedId::new(
                        account_id,
                        CollectionId::new(),
                    )),
                    weight: AllocationWeight::new(0),
                },
                SourceAllocation {
                    lane: SourceLane::Discovery,
                    source: RecipeSource::Evidence(EvidenceCapability::SavedAt),
                    weight: AllocationWeight::new(4),
                },
            ],
            FamiliarityCadence::Disabled,
            OrderingNarrative::Shuffle,
            Vec::new(),
            vec![GuardrailKind::HardBoundaries],
        )
        .expect("discovery-only recipe is valid");

        assert_eq!(recipe.schema_version(), RecipeSchemaVersion::V1);
        assert_eq!(recipe.allocations()[0].weight.value(), 0);
    }

    #[test]
    fn all_zero_allocations_are_not_an_executable_recipe() {
        let result = RecipeV1::new(
            identity(ChordriftAccountId::new()),
            vec![SourceAllocation {
                lane: SourceLane::Familiar,
                source: RecipeSource::Evidence(EvidenceCapability::LifetimeRotation),
                weight: AllocationWeight::new(0),
            }],
            FamiliarityCadence::Disabled,
            OrderingNarrative::Shuffle,
            Vec::new(),
            Vec::new(),
        );

        assert_eq!(result, Err(DomainValueError::NoActiveAllocations));
    }

    #[test]
    fn recipe_collection_sources_cannot_cross_accounts() {
        let result = RecipeV1::new(
            identity(ChordriftAccountId::new()),
            vec![SourceAllocation {
                lane: SourceLane::Recovery,
                source: RecipeSource::Collection(AccountOwnedId::new(
                    ChordriftAccountId::new(),
                    CollectionId::new(),
                )),
                weight: AllocationWeight::new(1),
            }],
            FamiliarityCadence::Every(NonZeroU16::new(4).expect("nonzero cadence")),
            OrderingNarrative::SectionedJourney,
            vec![
                RecipeSection::WarmUp,
                RecipeSection::Focus,
                RecipeSection::Landing,
            ],
            Vec::new(),
        );

        assert_eq!(result, Err(DomainValueError::OwnershipMismatch));
    }

    #[test]
    fn deserialization_cannot_bypass_recipe_validation() {
        let account_id = ChordriftAccountId::new();
        let invalid = serde_json::json!({
            "identity": identity(account_id),
            "allocations": [{
                "lane": "familiar",
                "source": {"type": "evidence", "value": "lifetime_rotation"},
                "weight": 0
            }],
            "familiarity_cadence": {"mode": "disabled"},
            "ordering": "shuffle",
            "sections": [],
            "guardrails": []
        });

        assert!(serde_json::from_value::<RecipeV1>(invalid).is_err());
    }
}
