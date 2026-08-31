//! Transport-neutral application contract shared by Chordrift clients.
//!
//! This module contains data only. It deliberately has no execution, storage,
//! provider, terminal, or platform dependencies. A CLI can pass these values in
//! process while a hosted client can serialize the same values over a transport.

use std::{collections::BTreeMap, fmt};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Current backward-compatible application-contract feature generation.
pub const CONTRACT_VERSION: ContractVersion = ContractVersion::new(1, 3);

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

uuid_id!(RequestId, "Identity of one command or query submission.");
uuid_id!(
    OperationId,
    "Identity of durable work created by a command."
);
uuid_id!(
    CancellationId,
    "Identity used to request cancellation cooperatively."
);
uuid_id!(
    IdempotencyKey,
    "Identity used to deduplicate a command submission."
);
uuid_id!(
    ResourceId,
    "Opaque contract identity for a domain resource."
);
uuid_id!(
    MaintenanceSessionId,
    "Identity of one wrapper-neutral ordinary-maintenance session."
);
uuid_id!(
    MaintenanceChangeId,
    "Identity of one observed provider change inside a maintenance session."
);
uuid_id!(
    MaintenanceReviewId,
    "Identity of one immutable human-readable maintenance review."
);
uuid_id!(
    ErrorId,
    "Safe correlation identity for a client-visible error."
);

/// A semantic contract version.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ContractVersion {
    /// Breaking-change generation.
    pub major: u16,
    /// Backward-compatible feature generation.
    pub minor: u16,
}

impl ContractVersion {
    /// Creates a semantic contract version.
    #[must_use]
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }
}

impl fmt::Display for ContractVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}

/// Inclusive contract-version range supported by one peer.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContractVersionRange {
    /// Oldest supported version.
    pub minimum: ContractVersion,
    /// Newest supported version.
    pub maximum: ContractVersion,
}

impl ContractVersionRange {
    /// Creates a supported range when its bounds are ordered and share a major version.
    pub fn new(
        minimum: ContractVersion,
        maximum: ContractVersion,
    ) -> Result<Self, CompatibilityError> {
        if minimum.major != maximum.major || minimum > maximum {
            return Err(CompatibilityError::InvalidContractRange);
        }
        Ok(Self { minimum, maximum })
    }

    /// Creates a range containing exactly one version.
    #[must_use]
    pub const fn exact(version: ContractVersion) -> Self {
        Self {
            minimum: version,
            maximum: version,
        }
    }
}

/// Inclusive database-schema versions understood by a client.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SchemaVersionRange {
    /// Oldest understood schema version.
    pub minimum: u32,
    /// Newest understood schema version.
    pub maximum: u32,
}

impl SchemaVersionRange {
    /// Creates an ordered schema range.
    pub fn new(minimum: u32, maximum: u32) -> Result<Self, CompatibilityError> {
        if minimum > maximum {
            return Err(CompatibilityError::InvalidSchemaRange);
        }
        Ok(Self { minimum, maximum })
    }

    /// Reports whether the range includes a schema version.
    #[must_use]
    pub const fn contains(self, version: u32) -> bool {
        self.minimum <= version && version <= self.maximum
    }
}

/// Stable machine-readable capability availability.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityAvailability {
    /// The capability is fully available.
    Available,
    /// The capability is available with visible limitations.
    Degraded,
    /// The capability is unavailable.
    Unavailable,
}

/// Capability report keyed by stable provider-neutral capability names.
pub type CapabilitySet = BTreeMap<String, CapabilityAvailability>;

/// Machine-readable schema used by `chordrift capabilities`.
pub const BINARY_CAPABILITY_SCHEMA_VERSION: u16 = 1;
/// Complete operator intake workflow with explicit review gates.
pub const CAPABILITY_MAINTENANCE_INTAKE_WORKFLOW: &str = "maintenance.intake-workflow.v1";
/// One low-friction workflow for ordinary provider-side library changes.
pub const CAPABILITY_UNIFIED_MAINTENANCE_WORKFLOW: &str = "maintenance.unified-workflow.v1";
/// Read-only audit of current intake against durable intent and history.
pub const CAPABILITY_MAINTENANCE_INTAKE_AUDIT: &str = "maintenance.intake-audit.v1";
/// Ordinary playlist additions execute only their enumerated track operations.
pub const CAPABILITY_ENUMERATED_PLAYLIST_ADDITIONS: &str =
    "maintenance.enumerated-playlist-additions.v1";
/// Plan details include set-based labels and direct-move interpretation.
pub const CAPABILITY_BULK_MAINTENANCE_PREVIEW: &str = "maintenance.bulk-plan-preview.v1";
/// Direct additions to managed destinations are preserved and recorded as intake.
pub const CAPABILITY_DIRECT_MANAGED_INTAKE: &str = "maintenance.direct-managed-intake.v1";
/// Interrupted maintenance can resume with the complete approved artwork set.
pub const CAPABILITY_ARTWORK_CARRY_FORWARD: &str = "maintenance.artwork-carry-forward.v1";
/// Membership-equal provider order can become Neon intent without provider writes.
pub const CAPABILITY_PROVIDER_ORDER_INTENT: &str = "maintenance.provider-order-intent.v1";
/// Exact provider observations become durable record-only comparison baselines.
pub const CAPABILITY_PROVIDER_BASELINE: &str = "maintenance.provider-baseline.v1";
/// Wrapper-neutral maintenance task DTOs and Rust-owned transition rules.
pub const CAPABILITY_MAINTENANCE_TASK_SESSION: &str = "maintenance.task-session.v1";
/// Per-track saved/liked intake retention decisions are durable and explicit.
pub const CAPABILITY_SAVED_INTAKE_DISPOSITION: &str = "maintenance.saved-intake-disposition.v1";
/// Authenticated HTTP transport over typed application commands and queries.
pub const CAPABILITY_AUTHENTICATED_SERVICE_TRANSPORT: &str = "service.authenticated-transport.v1";
/// Persisted product identity, account ownership, and revocable sessions.
pub const CAPABILITY_PRODUCT_IDENTITY: &str = "service.product-identity.v1";
/// Server-side encrypted provider credential vault with rotation and revocation.
pub const CAPABILITY_PROVIDER_CREDENTIAL_VAULT: &str = "service.provider-credential-vault.v1";
/// Restart-safe job lifecycle, progress, cancellation, retry, and replay.
pub const CAPABILITY_DURABLE_OPERATIONS: &str = "service.durable-operations.v1";
/// Installed CLI consumes authenticated remote DTOs with explicit local development parity.
pub const CAPABILITY_REMOTE_CLI: &str = "service.remote-cli.v1";
/// Synchronization plans expose an origin that maintenance tools can reject.
pub const CAPABILITY_PLAN_ORIGIN: &str = "plan-origin.v1";
/// Approved Spins can become immutable, fake-provider-verified publication plans.
pub const CAPABILITY_SPIN_PUBLICATION_PLAN: &str = "spin-publication-plan.v1";

/// Installed-binary capabilities that scripts and future clients can negotiate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BinaryCapabilityManifest {
    /// Manifest schema understood by the caller.
    pub schema_version: u16,
    /// Installed crate version, informational rather than a feature proxy.
    pub binary_version: String,
    /// Application-contract versions exposed by this binary.
    pub contract_versions: ContractVersionRange,
    /// Stable features with explicit availability.
    pub capabilities: CapabilitySet,
}

impl BinaryCapabilityManifest {
    /// Reports whether one exact stable capability is available.
    #[must_use]
    pub fn supports(&self, capability: &str) -> bool {
        self.capabilities.get(capability) == Some(&CapabilityAvailability::Available)
    }
}

/// Compatibility offer sent by a client.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClientCompatibility {
    /// Contract versions the client can consume.
    pub contract_versions: ContractVersionRange,
    /// Database schema versions whose meaning the client understands.
    pub schema_versions: SchemaVersionRange,
    /// Optional features the client would like to expose.
    pub requested_features: Vec<String>,
}

/// Compatibility and capability declaration made by an application service.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ServiceCompatibility {
    /// Contract versions the service can produce.
    pub contract_versions: ContractVersionRange,
    /// Current database schema version behind the service.
    pub schema_version: u32,
    /// Service-level feature availability.
    pub features: CapabilitySet,
    /// Provider operations currently available for the selected connection.
    pub provider_capabilities: CapabilitySet,
    /// Evidence currently available for decisions and explanations.
    pub evidence_capabilities: CapabilitySet,
}

/// Successful result of compatibility negotiation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NegotiatedCompatibility {
    /// Highest mutually supported contract version.
    pub contract_version: ContractVersion,
    /// Service schema version accepted by the client.
    pub schema_version: u32,
    /// Requested features and their effective availability.
    pub features: CapabilitySet,
    /// Provider capability snapshot supplied by the service.
    pub provider_capabilities: CapabilitySet,
    /// Evidence capability snapshot supplied by the service.
    pub evidence_capabilities: CapabilitySet,
}

/// A deterministic compatibility-negotiation failure.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityError {
    /// A contract range was reversed or crossed major versions.
    InvalidContractRange,
    /// A schema range was reversed.
    InvalidSchemaRange,
    /// Client and service have no common contract version.
    ContractVersionMismatch,
    /// The service schema is outside the client's understood range.
    SchemaVersionMismatch,
}

/// Negotiates the highest common contract version and an honest capability view.
pub fn negotiate(
    client: &ClientCompatibility,
    service: &ServiceCompatibility,
) -> Result<NegotiatedCompatibility, CompatibilityError> {
    if client.contract_versions.minimum.major != client.contract_versions.maximum.major
        || client.contract_versions.minimum > client.contract_versions.maximum
        || service.contract_versions.minimum.major != service.contract_versions.maximum.major
        || service.contract_versions.minimum > service.contract_versions.maximum
    {
        return Err(CompatibilityError::InvalidContractRange);
    }
    if client.schema_versions.minimum > client.schema_versions.maximum {
        return Err(CompatibilityError::InvalidSchemaRange);
    }
    if client.contract_versions.maximum.major != service.contract_versions.maximum.major {
        return Err(CompatibilityError::ContractVersionMismatch);
    }

    let minimum = client
        .contract_versions
        .minimum
        .max(service.contract_versions.minimum);
    let maximum = client
        .contract_versions
        .maximum
        .min(service.contract_versions.maximum);
    if minimum > maximum {
        return Err(CompatibilityError::ContractVersionMismatch);
    }
    if !client.schema_versions.contains(service.schema_version) {
        return Err(CompatibilityError::SchemaVersionMismatch);
    }

    let features = client
        .requested_features
        .iter()
        .map(|feature| {
            (
                feature.clone(),
                service
                    .features
                    .get(feature)
                    .copied()
                    .unwrap_or(CapabilityAvailability::Unavailable),
            )
        })
        .collect();
    Ok(NegotiatedCompatibility {
        contract_version: maximum,
        schema_version: service.schema_version,
        features,
        provider_capabilities: service.provider_capabilities.clone(),
        evidence_capabilities: service.evidence_capabilities.clone(),
    })
}

/// Provider-neutral command submitted to the application boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "parameters", rename_all = "snake_case")]
pub enum Command {
    /// Connect a provider account to a Chordrift account.
    ConnectProvider {
        /// Chordrift account receiving the connection.
        account_id: ResourceId,
        /// Stable provider kind such as `spotify` or `apple_music`.
        provider: String,
    },
    /// Observe current state without requesting provider mutation.
    ObserveProvider {
        /// Provider connection to observe.
        provider_connection_id: ResourceId,
    },
    /// Create an isolated onboarding rehearsal.
    CreateOnboardingSession {
        /// Owning Chordrift account.
        account_id: ResourceId,
        /// Whether optional extended listening evidence may be used.
        include_extended_history: bool,
    },
    /// Generate a provider-free Spin preview.
    PreviewSpin {
        /// Immutable recipe revision to evaluate.
        recipe_revision_id: ResourceId,
    },
    /// Approve an immutable publication candidate without applying it.
    ApprovePublication {
        /// Spin approved for later publication planning.
        spin_id: ResourceId,
    },
    /// Start or resume cumulative ordinary maintenance for one provider connection.
    StartMaintenance {
        /// Client-generated identity used for idempotent query and reconnect.
        session_id: MaintenanceSessionId,
        /// Provider connection whose newest complete state becomes the baseline.
        provider_connection_id: ResourceId,
    },
    /// Observe the provider again and rebase one maintenance session cumulatively.
    RefreshMaintenance {
        /// Existing maintenance session to refresh.
        session_id: MaintenanceSessionId,
        /// Exact session revision currently displayed to the client.
        expected_revision: u64,
    },
    /// Resolve genuinely ambiguous meanings in one exact maintenance revision.
    ResolveMaintenance {
        /// Maintenance session receiving the decisions.
        session_id: MaintenanceSessionId,
        /// Exact session revision displayed to the client.
        expected_revision: u64,
        /// Decisions for observed changes that require human meaning.
        decisions: Vec<MaintenanceDecision>,
    },
    /// Authorize one exact reviewed set of Chordrift-authored provider effects.
    AuthorizeMaintenance {
        /// Maintenance session containing the immutable review.
        session_id: MaintenanceSessionId,
        /// Exact session revision displayed to the client.
        expected_revision: u64,
        /// Immutable review being authorized.
        review_id: MaintenanceReviewId,
    },
    /// Request cooperative cancellation of an operation.
    CancelOperation(CancellationRequest),
}

/// Provider-neutral query submitted to the application boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "parameters", rename_all = "snake_case")]
pub enum Query {
    /// Read provider connections owned by the authenticated account.
    ProviderConnections,
    /// Read playlist surfaces from one explicit state plane.
    LibraryPlaylists {
        /// Account-owned provider connection.
        provider_connection_id: ResourceId,
        /// Whether to inspect the newest provider observation or Chordrift's model.
        source: LibraryStateSource,
    },
    /// Read ordered membership for one playlist surface.
    LibraryPlaylistTracks {
        /// Account-owned provider connection.
        provider_connection_id: ResourceId,
        /// Provider playlist identity or Chordrift stable collection key.
        playlist_id: String,
        /// State plane containing the playlist.
        source: LibraryStateSource,
    },
    /// Read one track's identity, placement, lifecycle, and listening evidence.
    LibraryTrack {
        /// Account-owned provider connection.
        provider_connection_id: ResourceId,
        /// Stable provider track identity.
        provider_track_id: String,
    },
    /// Read the active reversible exclusion archive.
    ExcludedTracks {
        /// Account-owned provider connection.
        provider_connection_id: ResourceId,
    },
    /// Read the audit produced by an onboarding session.
    OnboardingAudit {
        /// Session to inspect.
        session_id: ResourceId,
    },
    /// Read collections owned by an account.
    Collections {
        /// Account to inspect.
        account_id: ResourceId,
    },
    /// Read one immutable recipe revision.
    Recipe {
        /// Recipe revision to inspect.
        recipe_revision_id: ResourceId,
    },
    /// Read one immutable Spin preview.
    SpinPreview {
        /// Spin to inspect.
        spin_id: ResourceId,
    },
    /// Read one wrapper-neutral ordinary-maintenance session.
    MaintenanceSession {
        /// Session to inspect.
        session_id: MaintenanceSessionId,
    },
    /// Read current lifecycle state for an operation.
    Operation {
        /// Operation to inspect.
        operation_id: OperationId,
    },
    /// Read account-scoped operation history.
    OperationHistory {
        /// Account whose history is requested.
        account_id: ResourceId,
    },
    /// Read ordered lifecycle events after an optional operation-local cursor.
    OperationEvents {
        /// Operation whose events are requested.
        operation_id: OperationId,
        /// Return only events with a greater sequence number.
        after_sequence: Option<u64>,
    },
    /// Read client-safe diagnostics.
    Diagnostics {
        /// Optional operation that narrows the report.
        operation_id: Option<OperationId>,
    },
}

/// Explicit library state plane selected by a client.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryStateSource {
    /// Newest complete provider observation retained in Neon.
    ProviderObservation,
    /// Newest approved or proposed Chordrift managed-playlist model.
    ChordriftModel,
}

/// One provider connection owned by the authenticated Chordrift account.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProviderConnectionView {
    /// Account-scoped provider connection identity.
    pub provider_connection_id: ResourceId,
    /// Provider namespace.
    pub provider: String,
    /// Human-facing account name when available.
    pub display_name: Option<String>,
    /// Time of the newest complete provider observation.
    pub observed_at: Option<DateTime<Utc>>,
    /// Whether an active encrypted provider credential is available.
    pub credential_ready: bool,
}

/// Provider connections visible to one authenticated account.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProviderConnectionsView {
    /// Account-owned provider connections.
    pub connections: Vec<ProviderConnectionView>,
}

/// One playlist surface in a selected state plane.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LibraryPlaylistView {
    /// Provider playlist ID or Chordrift stable collection key.
    pub playlist_id: String,
    /// Human-facing playlist name.
    pub name: String,
    /// Optional provider playlist identity when the model is published.
    pub provider_playlist_id: Option<String>,
    /// Number of tracks represented in this state plane.
    pub track_count: u64,
    /// Chordrift signal class when known.
    pub signal_class: Option<String>,
    /// Chordrift orchestration role when known.
    pub role: Option<String>,
}

/// Playlist catalog from one explicit state plane.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LibraryPlaylistsView {
    /// State plane represented by the result.
    pub source: LibraryStateSource,
    /// Observation or model creation time when available.
    pub state_at: Option<DateTime<Utc>>,
    /// Playlist surfaces in stable display order.
    pub playlists: Vec<LibraryPlaylistView>,
}

/// One ordered playlist membership row.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LibraryPlaylistTrackView {
    /// One-based custom-order position.
    pub position: u64,
    /// Stable provider track identity.
    pub provider_track_id: String,
    /// Canonical title.
    pub title: String,
    /// Ordered display artist string.
    pub artists: String,
    /// Album title when known.
    pub album: Option<String>,
}

/// Ordered contents of one playlist in one explicit state plane.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LibraryPlaylistTracksView {
    /// State plane represented by the result.
    pub source: LibraryStateSource,
    /// Selected playlist identity.
    pub playlist_id: String,
    /// Current human-facing name.
    pub name: String,
    /// Observation or model creation time when available.
    pub state_at: Option<DateTime<Utc>>,
    /// Ordered track membership.
    pub tracks: Vec<LibraryPlaylistTrackView>,
}

/// Current playlist placement for a track.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LibraryTrackPlacementView {
    /// Provider playlist ID or Chordrift stable collection key.
    pub playlist_id: String,
    /// Human-facing playlist name.
    pub name: String,
    /// One-based custom-order position.
    pub position: u64,
    /// State plane supplying this placement.
    pub source: LibraryStateSource,
}

/// Complete client-safe track detail and personal listening evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LibraryTrackView {
    /// Stable provider track identity.
    pub provider_track_id: String,
    /// Canonical title.
    pub title: String,
    /// Ordered display artist string.
    pub artists: String,
    /// Album title when known.
    pub album: Option<String>,
    /// Meaningful historical plays.
    pub play_count: u64,
    /// Raw retained listening events.
    pub event_count: u64,
    /// Total retained listening duration.
    pub total_ms_played: u64,
    /// Most recent retained playback.
    pub last_played_at: Option<DateTime<Utc>>,
    /// Whether the track is currently saved/liked.
    pub saved: bool,
    /// Active reversible exclusion reason.
    pub exclusion_reason: Option<String>,
    /// Current provider and Chordrift-model placements.
    pub placements: Vec<LibraryTrackPlacementView>,
}

/// One track in the reversible exclusion archive.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExcludedTrackView {
    /// Stable provider track identity.
    pub provider_track_id: String,
    /// Canonical title.
    pub title: String,
    /// Ordered display artist string.
    pub artists: String,
    /// Durable exclusion reason.
    pub reason: String,
    /// Time the active exclusion was created.
    pub excluded_at: DateTime<Utc>,
    /// Most recent playlist name before exclusion when known.
    pub previous_playlist: Option<String>,
}

/// Active reversible exclusions for one provider connection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExcludedTracksView {
    /// Active exclusions ordered newest first.
    pub tracks: Vec<ExcludedTrackView>,
}

/// Provider action observed during ordinary maintenance.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceChangeKind {
    /// A previously unknown track was added directly to one managed destination.
    DirectIntake,
    /// A track moved from one managed destination to another.
    Reclassification,
    /// A track disappeared from a managed destination without a clear replacement.
    Removal,
    /// Exact provider membership stayed equal while order changed.
    Reorder,
    /// Saved/liked state changed.
    SavedState,
    /// Playlist name, description, or artwork changed on the provider.
    PlaylistMetadata,
    /// A provider playlist appeared.
    PlaylistCreated,
    /// A provider playlist disappeared.
    PlaylistRemoved,
}

/// Client-safe track label used in a maintenance review.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MaintenanceTrackView {
    /// Opaque Chordrift track identity.
    pub track_id: ResourceId,
    /// Provider/catalog title suitable for display.
    pub title: String,
    /// Ordered display artist names.
    pub artists: Vec<String>,
}

/// Client-safe playlist/surface label used in a maintenance review.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MaintenanceSurfaceView {
    /// Opaque Chordrift surface identity.
    pub surface_id: ResourceId,
    /// Current human-facing surface name.
    pub name: String,
}

/// Human resolution for broader meaning that cannot be inferred safely.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "parameters", rename_all = "snake_case")]
pub enum MaintenanceResolution {
    /// Keep the exact observed provider state without a broader directive.
    KeepObserved,
    /// Consume one temporary intake source after verified canonical placement.
    ConsumeIntake {
        /// Intake surface whose membership or saved state should be cleared.
        source: MaintenanceSurfaceView,
    },
    /// Treat one existing surface as canonical placement.
    Place {
        /// Selected canonical destination.
        destination: MaintenanceSurfaceView,
    },
    /// Record a surface-specific negative placement directive.
    Exclude,
    /// Supersede an active exclusion and retain the observed destination.
    Restore {
        /// Destination in which the track is explicitly restored.
        destination: MaintenanceSurfaceView,
    },
}

/// One decision submitted for an ambiguous observed change.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MaintenanceDecision {
    /// Observed change being resolved.
    pub change_id: MaintenanceChangeId,
    /// Human-selected broader meaning.
    pub resolution: MaintenanceResolution,
}

/// One observed provider gesture and its current interpretation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MaintenanceChangeView {
    /// Stable identity within the maintenance session.
    pub change_id: MaintenanceChangeId,
    /// Exact kind of provider gesture observed.
    pub kind: MaintenanceChangeKind,
    /// Track involved when the change is track-specific.
    pub track: Option<MaintenanceTrackView>,
    /// Previous surface when one is known.
    pub previous_surface: Option<MaintenanceSurfaceView>,
    /// Current observed surface when one is known.
    pub current_surface: Option<MaintenanceSurfaceView>,
    /// Concise client-safe explanation.
    pub summary: String,
    /// Current broader interpretation; absent only when a decision is required.
    pub resolution: Option<MaintenanceResolution>,
}

/// Chordrift-authored provider mutation shown for explicit authorization.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceProviderEffectKind {
    /// Add one enumerated track.
    AddTrack,
    /// Remove one enumerated track.
    RemoveTrack,
    /// Replace order only after proving identical membership.
    ReorderPlaylist,
    /// Create a new playlist container.
    CreatePlaylist,
    /// Rename or change the description of a playlist.
    UpdatePlaylistMetadata,
    /// Upload separately reviewed artwork.
    UploadArtwork,
    /// Remove an obsolete playlist relationship.
    RetirePlaylist,
    /// Change saved/liked state.
    UpdateSavedState,
}

/// Exact human-readable provider effect in one immutable review.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MaintenanceProviderEffectView {
    /// Opaque effect identity.
    pub effect_id: ResourceId,
    /// Provider mutation kind.
    pub kind: MaintenanceProviderEffectKind,
    /// Track affected when applicable.
    pub track: Option<MaintenanceTrackView>,
    /// Surface affected when applicable.
    pub surface: Option<MaintenanceSurfaceView>,
    /// Concise provider-visible outcome.
    pub summary: String,
}

/// Client action currently accepted by a maintenance session.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceAllowedAction {
    /// Observe the provider again and rebase cumulative intent.
    Refresh,
    /// Submit decisions for all currently ambiguous meanings.
    Resolve,
    /// Authorize the exact immutable provider-effect review.
    Authorize,
    /// Request cooperative cancellation of active work.
    Cancel,
    /// Resume recoverable work.
    Resume,
}

/// Wrapper-neutral lifecycle state for ordinary maintenance.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceSessionState {
    /// Current provider state is being observed or record-only intent is converging.
    Reconciling,
    /// One or more observed gestures need broader human meaning.
    NeedsDecision,
    /// Chordrift-authored provider effects await one exact authorization.
    ReadyForAuthorization,
    /// The exact immutable review has been authorized but not yet verified.
    Authorized,
    /// Provider execution is in progress.
    Applying,
    /// Provider results are being observed and verified.
    Verifying,
    /// Current provider and durable intent require no further action.
    InSync,
    /// Work stopped safely and may be resumed.
    Recoverable,
}

/// Immutable task-level view rendered by CLI, web, mobile, or another client.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MaintenanceSessionView {
    /// Maintenance session identity.
    pub session_id: MaintenanceSessionId,
    /// Monotonically increasing state revision used as a command precondition.
    pub revision: u64,
    /// Newest complete provider snapshot underlying this view.
    pub provider_snapshot_id: ResourceId,
    /// Current wrapper-neutral workflow state.
    pub state: MaintenanceSessionState,
    /// Exact provider gestures folded into the current baseline.
    pub observed_changes: Vec<MaintenanceChangeView>,
    /// Exact Chordrift-authored provider effects, if any.
    pub provider_effects: Vec<MaintenanceProviderEffectView>,
    /// Immutable review identity required for provider authorization.
    pub review_id: Option<MaintenanceReviewId>,
    /// Actions accepted at this revision.
    pub allowed_actions: Vec<MaintenanceAllowedAction>,
}

/// Versioned command envelope with request and deduplication identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandRequest {
    /// Negotiated contract version.
    pub contract_version: ContractVersion,
    /// Identity of this submission.
    pub request_id: RequestId,
    /// Stable identity used to deduplicate retries.
    pub idempotency_key: IdempotencyKey,
    /// Command payload.
    pub command: Command,
}

/// Versioned query envelope.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QueryRequest {
    /// Negotiated contract version.
    pub contract_version: ContractVersion,
    /// Identity of this submission.
    pub request_id: RequestId,
    /// Query payload.
    pub query: Query,
}

/// Immutable acknowledgement returned after accepting a command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandReceipt {
    /// Contract version used for the result.
    pub contract_version: ContractVersion,
    /// Request being acknowledged.
    pub request_id: RequestId,
    /// Durable operation created or reused for this idempotency key.
    pub operation_id: OperationId,
    /// Cancellation identity for cooperative cancellation.
    pub cancellation_id: CancellationId,
}

/// Immutable query result envelope for a client-facing view type.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct View<V> {
    /// Contract version used for the result.
    pub contract_version: ContractVersion,
    /// Query request being answered.
    pub request_id: RequestId,
    /// Time at which the immutable view was assembled.
    pub generated_at: DateTime<Utc>,
    /// Client-facing result data.
    pub value: V,
}

/// Cooperative cancellation request; it does not imply cancellation succeeded.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CancellationRequest {
    /// Operation whose work should stop when safe.
    pub operation_id: OperationId,
    /// Cancellation identity issued with the command receipt.
    pub cancellation_id: CancellationId,
}

/// Outcome of a cooperative cancellation request.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationOutcome {
    /// The request was recorded and cancellation is pending.
    Requested,
    /// The operation reached a cancellation point and stopped.
    Cancelled,
    /// The operation had already reached a terminal state.
    TooLate,
}

/// Stable progress units that clients can render without parsing prose.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressUnit {
    /// An application-defined sequence of steps.
    Steps,
    /// Music tracks processed.
    Tracks,
    /// Playlist surfaces processed.
    Playlists,
    /// Provider items processed.
    Items,
    /// Bytes processed.
    Bytes,
}

/// Structured progress snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Progress {
    /// Stable machine-readable phase name.
    pub phase: String,
    /// Work completed in the declared unit.
    pub completed: u64,
    /// Total work when known.
    pub total: Option<u64>,
    /// Unit represented by the counts.
    pub unit: ProgressUnit,
}

impl Progress {
    /// Creates a valid progress snapshot.
    pub fn new(
        phase: impl Into<String>,
        completed: u64,
        total: Option<u64>,
        unit: ProgressUnit,
    ) -> Result<Self, ProgressError> {
        if total.is_some_and(|total| completed > total) {
            return Err(ProgressError::CompletedExceedsTotal);
        }
        Ok(Self {
            phase: phase.into(),
            completed,
            total,
            unit,
        })
    }
}

/// Invalid structured progress.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressError {
    /// Completed work exceeded the known total.
    CompletedExceedsTotal,
}

/// Why an operation is waiting for an external decision or condition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitingReason {
    /// Provider authorization is required.
    Authorization,
    /// User consent is required.
    Consent,
    /// Explicit approval of an immutable plan is required.
    Approval,
    /// A temporarily unavailable dependency must recover.
    Dependency,
}

/// Stable client-safe error classification.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// The submitted request is invalid.
    InvalidRequest,
    /// Authentication or authorization is required.
    Authorization,
    /// A requested resource was not found.
    NotFound,
    /// Current state conflicts with the request.
    Conflict,
    /// A required capability is unavailable.
    Unsupported,
    /// A dependency is temporarily unavailable.
    Unavailable,
    /// An unexpected internal failure occurred.
    Internal,
}

/// Stable client-safe error code.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Contract or payload validation failed.
    InvalidRequest,
    /// Contract compatibility negotiation failed.
    IncompatibleContract,
    /// Authentication is required or expired.
    AuthenticationRequired,
    /// The caller lacks authority for the resource.
    PermissionDenied,
    /// The requested resource does not exist.
    ResourceNotFound,
    /// Current state no longer matches the request.
    StateConflict,
    /// Provider or service capability is unavailable.
    CapabilityUnavailable,
    /// A dependency failed temporarily.
    DependencyUnavailable,
    /// The authenticated caller exceeded a transport request budget.
    RateLimited,
    /// The operation was cancelled.
    Cancelled,
    /// An unexpected internal failure occurred.
    Internal,
}

impl ErrorCode {
    /// Returns the stable category clients may use for presentation policy.
    #[must_use]
    pub const fn category(self) -> ErrorCategory {
        match self {
            Self::InvalidRequest | Self::IncompatibleContract => ErrorCategory::InvalidRequest,
            Self::AuthenticationRequired | Self::PermissionDenied => ErrorCategory::Authorization,
            Self::ResourceNotFound => ErrorCategory::NotFound,
            Self::StateConflict | Self::Cancelled => ErrorCategory::Conflict,
            Self::CapabilityUnavailable => ErrorCategory::Unsupported,
            Self::DependencyUnavailable | Self::RateLimited => ErrorCategory::Unavailable,
            Self::Internal => ErrorCategory::Internal,
        }
    }

    /// Returns a fixed secret-free message suitable for clients.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::InvalidRequest => "The request is invalid.",
            Self::IncompatibleContract => "The client and service are incompatible.",
            Self::AuthenticationRequired => "Authentication is required.",
            Self::PermissionDenied => "This action is not permitted.",
            Self::ResourceNotFound => "The requested resource was not found.",
            Self::StateConflict => "The request conflicts with current state.",
            Self::CapabilityUnavailable => "A required capability is unavailable.",
            Self::DependencyUnavailable => "A dependency is temporarily unavailable.",
            Self::RateLimited => "Too many requests were submitted.",
            Self::Cancelled => "The operation was cancelled.",
            Self::Internal => "Chordrift could not complete the operation.",
        }
    }
}

/// Client-visible structured error containing no source error or arbitrary details.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClientError {
    /// Safe correlation identity used to locate server-side diagnostics.
    pub error_id: ErrorId,
    /// Stable machine-readable error code.
    pub code: ErrorCode,
    /// Whether retrying the same request may succeed.
    pub retryable: bool,
    /// Suggested delay before retrying, when known.
    pub retry_after_seconds: Option<u32>,
}

impl ClientError {
    /// Creates a new client-safe error with no source text.
    #[must_use]
    pub fn new(code: ErrorCode, retryable: bool) -> Self {
        Self {
            error_id: ErrorId::new(),
            code,
            retryable,
            retry_after_seconds: None,
        }
    }

    /// Returns the stable category.
    #[must_use]
    pub const fn category(self) -> ErrorCategory {
        self.code.category()
    }

    /// Returns the fixed secret-free display message.
    #[must_use]
    pub const fn message(self) -> &'static str {
        self.code.message()
    }
}

/// Complete lifecycle state for asynchronous application work.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", content = "details", rename_all = "snake_case")]
pub enum OperationState {
    /// Work has been accepted but has not started.
    Queued,
    /// Work is actively running.
    Running {
        /// Latest structured progress snapshot, when available.
        progress: Option<Progress>,
    },
    /// Work is waiting for a decision or external condition.
    Waiting {
        /// Stable reason for waiting.
        reason: WaitingReason,
    },
    /// Work completed successfully.
    Completed {
        /// Optional immutable result resource.
        result_id: Option<ResourceId>,
    },
    /// Work failed terminally.
    Failed {
        /// Client-safe structured failure.
        error: ClientError,
    },
    /// Work stopped after cooperative cancellation.
    Cancelled,
    /// Work stopped safely and can be retried or resumed by policy.
    Recoverable {
        /// Client-safe structured reason.
        error: ClientError,
    },
}

impl OperationState {
    /// Reports whether no further lifecycle event is expected.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Failed { .. } | Self::Cancelled
        )
    }
}

/// Ordered lifecycle event emitted for an operation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationEvent {
    /// Contract version used for the event.
    pub contract_version: ContractVersion,
    /// Operation whose lifecycle changed.
    pub operation_id: OperationId,
    /// Monotonically increasing operation-local event sequence.
    pub sequence: u64,
    /// Time at which the lifecycle transition was recorded.
    pub occurred_at: DateTime<Utc>,
    /// New lifecycle state.
    pub state: OperationState,
}

/// Reconnectable client view of one application operation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationView {
    /// Operation identity returned by the accepting command.
    pub operation_id: OperationId,
    /// Cancellation identity required for cooperative cancellation.
    pub cancellation_id: CancellationId,
    /// Latest recorded lifecycle state.
    pub state: OperationState,
}

/// Account-scoped operation history visible to an authenticated caller.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationHistoryView {
    /// Operations in acceptance order.
    pub operations: Vec<OperationView>,
}

/// Cursor-filtered ordered lifecycle events for one operation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OperationEventsView {
    /// Operation whose events are returned.
    pub operation_id: OperationId,
    /// Strictly ordered events after the requested cursor.
    pub events: Vec<OperationEvent>,
}

/// Typed query result shared by in-process and serialized transports.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", content = "view", rename_all = "snake_case")]
pub enum QueryResponse {
    /// Provider connections owned by the authenticated account.
    ProviderConnections(View<ProviderConnectionsView>),
    /// Playlist catalog from one explicit state plane.
    LibraryPlaylists(View<LibraryPlaylistsView>),
    /// Ordered playlist contents from one explicit state plane.
    LibraryPlaylistTracks(View<LibraryPlaylistTracksView>),
    /// Track identity, placements, and personal evidence.
    LibraryTrack(View<LibraryTrackView>),
    /// Active reversible exclusion archive.
    ExcludedTracks(View<ExcludedTracksView>),
    /// One ordinary-maintenance session.
    MaintenanceSession(View<MaintenanceSessionView>),
    /// Latest state of one operation.
    Operation(View<OperationView>),
    /// Authenticated subject's operation history.
    OperationHistory(View<OperationHistoryView>),
    /// Ordered operation lifecycle events.
    OperationEvents(View<OperationEventsView>),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(minor_min: u16, minor_max: u16) -> ContractVersionRange {
        ContractVersionRange::new(
            ContractVersion::new(1, minor_min),
            ContractVersion::new(1, minor_max),
        )
        .expect("test range is valid")
    }

    #[test]
    fn negotiates_highest_common_minor_and_capabilities() {
        let client = ClientCompatibility {
            contract_versions: range(1, 3),
            schema_versions: SchemaVersionRange::new(40, 50).expect("range is valid"),
            requested_features: vec!["spin_preview".to_owned(), "artwork".to_owned()],
        };
        let service = ServiceCompatibility {
            contract_versions: range(0, 2),
            schema_version: 45,
            features: BTreeMap::from([(
                "spin_preview".to_owned(),
                CapabilityAvailability::Available,
            )]),
            provider_capabilities: BTreeMap::from([(
                "playlist_read".to_owned(),
                CapabilityAvailability::Available,
            )]),
            evidence_capabilities: BTreeMap::from([(
                "extended_history".to_owned(),
                CapabilityAvailability::Degraded,
            )]),
        };

        let negotiated = negotiate(&client, &service).expect("peers overlap");
        assert_eq!(negotiated.contract_version, ContractVersion::new(1, 2));
        assert_eq!(
            negotiated.features["artwork"],
            CapabilityAvailability::Unavailable
        );
        assert_eq!(
            negotiated.evidence_capabilities["extended_history"],
            CapabilityAvailability::Degraded
        );
    }

    #[test]
    fn binary_manifest_uses_exact_capability_names_instead_of_version_guessing() {
        let manifest = BinaryCapabilityManifest {
            schema_version: BINARY_CAPABILITY_SCHEMA_VERSION,
            binary_version: "0.1.4+development".to_owned(),
            contract_versions: ContractVersionRange::exact(CONTRACT_VERSION),
            capabilities: BTreeMap::from([(
                CAPABILITY_MAINTENANCE_INTAKE_WORKFLOW.to_owned(),
                CapabilityAvailability::Available,
            )]),
        };

        assert!(manifest.supports(CAPABILITY_MAINTENANCE_INTAKE_WORKFLOW));
        assert!(!manifest.supports(CAPABILITY_PLAN_ORIGIN));
        let encoded = serde_json::to_string(&manifest).expect("manifest serializes");
        let decoded: BinaryCapabilityManifest =
            serde_json::from_str(&encoded).expect("manifest deserializes");
        assert_eq!(decoded, manifest);
    }

    #[test]
    fn rejects_breaking_version_and_unknown_schema() {
        let client = ClientCompatibility {
            contract_versions: range(0, 2),
            schema_versions: SchemaVersionRange::new(40, 44).expect("range is valid"),
            requested_features: Vec::new(),
        };
        let mut service = ServiceCompatibility {
            contract_versions: ContractVersionRange::exact(ContractVersion::new(2, 0)),
            schema_version: 45,
            features: BTreeMap::new(),
            provider_capabilities: BTreeMap::new(),
            evidence_capabilities: BTreeMap::new(),
        };
        assert_eq!(
            negotiate(&client, &service),
            Err(CompatibilityError::ContractVersionMismatch)
        );

        service.contract_versions = range(0, 0);
        assert_eq!(
            negotiate(&client, &service),
            Err(CompatibilityError::SchemaVersionMismatch)
        );
    }

    #[test]
    fn command_and_event_round_trip_without_transport_assumptions() {
        let request = CommandRequest {
            contract_version: CONTRACT_VERSION,
            request_id: RequestId::new(),
            idempotency_key: IdempotencyKey::new(),
            command: Command::CreateOnboardingSession {
                account_id: ResourceId::new(),
                include_extended_history: false,
            },
        };
        let encoded = serde_json::to_string(&request).expect("contract serializes");
        let decoded: CommandRequest =
            serde_json::from_str(&encoded).expect("contract deserializes");
        assert_eq!(decoded, request);

        let event = OperationEvent {
            contract_version: CONTRACT_VERSION,
            operation_id: OperationId::new(),
            sequence: 3,
            occurred_at: Utc::now(),
            state: OperationState::Running {
                progress: Some(
                    Progress::new("observe_inventory", 4, Some(10), ProgressUnit::Playlists)
                        .expect("progress is valid"),
                ),
            },
        };
        let encoded = serde_json::to_string(&event).expect("event serializes");
        let decoded: OperationEvent = serde_json::from_str(&encoded).expect("event deserializes");
        assert_eq!(decoded, event);
    }

    #[test]
    fn lifecycle_covers_active_terminal_and_recoverable_work() {
        assert!(!OperationState::Queued.is_terminal());
        assert!(
            !OperationState::Waiting {
                reason: WaitingReason::Consent
            }
            .is_terminal()
        );
        assert!(
            !OperationState::Recoverable {
                error: ClientError::new(ErrorCode::DependencyUnavailable, true)
            }
            .is_terminal()
        );
        assert!(OperationState::Completed { result_id: None }.is_terminal());
        assert!(
            OperationState::Failed {
                error: ClientError::new(ErrorCode::Internal, false)
            }
            .is_terminal()
        );
        assert!(OperationState::Cancelled.is_terminal());
    }

    #[test]
    fn client_errors_serialize_without_source_text_or_secrets() {
        let error = ClientError::new(ErrorCode::DependencyUnavailable, true);
        let encoded = serde_json::to_string(&error).expect("error serializes");

        assert_eq!(error.category(), ErrorCategory::Unavailable);
        assert_eq!(error.message(), "A dependency is temporarily unavailable.");
        assert!(!encoded.contains("message"));
        assert!(!encoded.contains("source"));
        assert!(!encoded.contains("secret"));
    }

    #[test]
    fn progress_rejects_impossible_counts() {
        assert_eq!(
            Progress::new("observe", 2, Some(1), ProgressUnit::Items),
            Err(ProgressError::CompletedExceedsTotal)
        );
    }
}
