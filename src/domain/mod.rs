//! Provider-neutral product-domain values and invariants.
//!
//! These types contain no SQL records, provider payloads, terminal concepts, or
//! transport assumptions. They are the stable vocabulary used by later
//! application, provider, and storage adapters.

mod accounts;
mod capabilities;
mod collections;
mod ids;
mod recipes;
mod spins;
mod surfaces;

pub use accounts::AccountContext;
pub use capabilities::{
    CapabilityStatus, EvidenceCapabilities, EvidenceCapability, ProviderCapabilities,
    ProviderCapability,
};
pub use collections::{
    CollectionMembership, MembershipConfidence, MembershipProvenance, MembershipStrength,
};
pub use ids::{
    AccountOwnedId, CanonicalTrackId, ChordriftAccountId, CollectionId, OnboardingSessionId,
    ProviderAccountId, ProviderConnectionId, ProviderConnectionIdentity, ProviderNamespace,
    ProviderPlaylistId, ProviderTrackId, RecipeId, RecipeRevisionId, SpinId, SurfaceId,
};
pub use recipes::{
    AllocationWeight, FamiliarityCadence, GuardrailKind, OrderingNarrative, RecipeRevisionIdentity,
    RecipeSchemaVersion, RecipeSection, RecipeSource, RecipeV1, SourceAllocation, SourceLane,
};
pub use spins::SpinIdentity;
pub use surfaces::{PlaylistSurface, SurfaceAuthority, SurfacePurpose, SurfaceRefreshPolicy};

use std::fmt;

/// A domain value or relationship violated a provider-neutral invariant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DomainValueError {
    /// A required opaque value was empty.
    EmptyValue,
    /// A provider namespace was not a stable lowercase identifier.
    InvalidProviderNamespace,
    /// An opaque provider value contained a control character.
    ControlCharacter,
    /// Related values belonged to different Chordrift accounts.
    OwnershipMismatch,
    /// A capability report described a different provider connection.
    ProviderConnectionMismatch,
    /// A recipe had no allocation with a positive weight.
    NoActiveAllocations,
}

impl fmt::Display for DomainValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptyValue => "a required domain value was empty",
            Self::InvalidProviderNamespace => "the provider namespace was invalid",
            Self::ControlCharacter => "a provider identifier contained a control character",
            Self::OwnershipMismatch => "the resources belong to different Chordrift accounts",
            Self::ProviderConnectionMismatch => {
                "the capability report belongs to a different provider connection"
            }
            Self::NoActiveAllocations => "the recipe has no active source allocation",
        })
    }
}

impl std::error::Error for DomainValueError {}
