use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::ProviderConnectionId;

/// Honest availability of one provider or evidence capability.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    /// The capability is fully available.
    Available,
    /// The capability is usable with visible limitations.
    Degraded,
    /// The capability is not available and must not be emulated silently.
    Unavailable,
}

/// Provider operation understood by the product domain.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCapability {
    /// Authorize or refresh a provider connection.
    Authorization,
    /// Observe complete current library inventory.
    LibraryInventoryRead,
    /// Observe bounded recent playback evidence.
    RecentPlaybackRead,
    /// Read playlists and their ordered membership.
    PlaylistRead,
    /// Create a playlist surface.
    PlaylistCreate,
    /// Replace or edit playlist membership.
    PlaylistMembershipWrite,
    /// Control exact playlist order.
    PlaylistOrderWrite,
    /// Upload playlist artwork.
    PlaylistArtworkWrite,
    /// Read the saved-track library surface.
    SavedTracksRead,
    /// Remove tracks from the saved-track library surface.
    SavedTracksWrite,
    /// Read the saved-album library surface.
    SavedAlbumsRead,
    /// Remove albums from the saved-album library surface.
    SavedAlbumsWrite,
    /// Construct provider deep links for client navigation.
    DeepLinks,
}

/// Evidence source understood by recipe and explanation policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceCapability {
    /// Current provider inventory is available.
    CurrentInventory,
    /// Provider save or Like timestamps are available.
    SavedAt,
    /// Bounded recent playback observations are available.
    RecentPlayback,
    /// Optional extended playback history is available.
    ExtendedPlaybackHistory,
    /// Trustworthy play-completion evidence is available.
    Completion,
    /// Trustworthy skip evidence is available.
    Skips,
    /// Lifetime rotation and deep-rediscovery evidence is available.
    LifetimeRotation,
}

/// Typed provider capability snapshot for one connection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProviderCapabilities {
    /// Connection described by this snapshot.
    pub provider_connection_id: ProviderConnectionId,
    states: BTreeMap<ProviderCapability, CapabilityStatus>,
}

impl ProviderCapabilities {
    /// Creates an honest provider capability snapshot.
    #[must_use]
    pub const fn new(
        provider_connection_id: ProviderConnectionId,
        states: BTreeMap<ProviderCapability, CapabilityStatus>,
    ) -> Self {
        Self {
            provider_connection_id,
            states,
        }
    }

    /// Returns the declared state, defaulting omitted capabilities to unavailable.
    #[must_use]
    pub fn status(&self, capability: ProviderCapability) -> CapabilityStatus {
        self.states
            .get(&capability)
            .copied()
            .unwrap_or(CapabilityStatus::Unavailable)
    }

    /// Returns all explicitly reported capabilities.
    #[must_use]
    pub const fn states(&self) -> &BTreeMap<ProviderCapability, CapabilityStatus> {
        &self.states
    }
}

/// Typed evidence capability snapshot used by recipes and Spins.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvidenceCapabilities {
    states: BTreeMap<EvidenceCapability, CapabilityStatus>,
}

impl EvidenceCapabilities {
    /// Creates an evidence capability snapshot.
    #[must_use]
    pub const fn new(states: BTreeMap<EvidenceCapability, CapabilityStatus>) -> Self {
        Self { states }
    }

    /// Returns the declared state, defaulting omitted evidence to unavailable.
    #[must_use]
    pub fn status(&self, capability: EvidenceCapability) -> CapabilityStatus {
        self.states
            .get(&capability)
            .copied()
            .unwrap_or(CapabilityStatus::Unavailable)
    }

    /// Returns all explicitly reported evidence capabilities.
    #[must_use]
    pub const fn states(&self) -> &BTreeMap<EvidenceCapability, CapabilityStatus> {
        &self.states
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_capabilities_are_unavailable_instead_of_assumed() {
        let capabilities = ProviderCapabilities::new(
            ProviderConnectionId::new(),
            BTreeMap::from([(
                ProviderCapability::PlaylistRead,
                CapabilityStatus::Available,
            )]),
        );

        assert_eq!(
            capabilities.status(ProviderCapability::PlaylistRead),
            CapabilityStatus::Available
        );
        assert_eq!(
            capabilities.status(ProviderCapability::PlaylistArtworkWrite),
            CapabilityStatus::Unavailable
        );
    }

    #[test]
    fn evidence_can_degrade_without_becoming_false_availability() {
        let evidence = EvidenceCapabilities::new(BTreeMap::from([(
            EvidenceCapability::RecentPlayback,
            CapabilityStatus::Degraded,
        )]));

        assert_eq!(
            evidence.status(EvidenceCapability::RecentPlayback),
            CapabilityStatus::Degraded
        );
        assert_eq!(
            evidence.status(EvidenceCapability::ExtendedPlaybackHistory),
            CapabilityStatus::Unavailable
        );
    }
}
