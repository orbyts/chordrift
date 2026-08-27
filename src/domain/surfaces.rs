use serde::{Deserialize, Serialize};

use super::{AccountOwnedId, SurfaceId};

/// Authority controlling a playlist surface.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceAuthority {
    /// The provider controls the surface.
    Provider,
    /// The user controls the surface.
    User,
    /// Chordrift controls the surface through approved policy.
    Chordrift,
    /// User directives and Chordrift generation share control.
    Collaborative,
}

/// Product purpose served by a playlist surface.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfacePurpose {
    /// Discovery, recommendation, or review intake.
    Intake,
    /// Ordered view into an otherwise unordered collection.
    CollectionView,
    /// Renewable listening experience generated from a recipe.
    RenewableExperience,
    /// Temporary or durable utility surface.
    Utility,
    /// Reference to an externally owned playlist.
    Bookmark,
}

/// Refresh behavior of a playlist surface.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceRefreshPolicy {
    /// Chordrift leaves the surface untouched.
    Untouched,
    /// Chordrift observes the surface without controlling it.
    Monitored,
    /// A user explicitly requests each Spin.
    ManualSpin,
    /// A later scheduler may request Spins under approved policy.
    Scheduled,
    /// The provider controls refresh timing and membership.
    ProviderControlled,
}

/// Provider-neutral identity and independent axes of one playlist surface.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlaylistSurface {
    /// Account-owned surface identity.
    pub surface_id: AccountOwnedId<SurfaceId>,
    /// Authority controlling the surface.
    pub authority: SurfaceAuthority,
    /// Product purpose of the surface.
    pub purpose: SurfacePurpose,
    /// Refresh behavior of the surface.
    pub refresh_policy: SurfaceRefreshPolicy,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ChordriftAccountId;

    #[test]
    fn surface_axes_combine_without_a_special_case_enum() {
        let surface = PlaylistSurface {
            surface_id: AccountOwnedId::new(ChordriftAccountId::new(), SurfaceId::new()),
            authority: SurfaceAuthority::Collaborative,
            purpose: SurfacePurpose::RenewableExperience,
            refresh_policy: SurfaceRefreshPolicy::ManualSpin,
        };
        let encoded = serde_json::to_string(&surface).expect("surface serializes");
        let decoded: PlaylistSurface =
            serde_json::from_str(&encoded).expect("surface deserializes");

        assert_eq!(decoded, surface);
        assert!(encoded.contains("collaborative"));
        assert!(encoded.contains("renewable_experience"));
        assert!(encoded.contains("manual_spin"));
    }
}
