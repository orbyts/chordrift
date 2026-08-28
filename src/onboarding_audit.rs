//! Inventory-only new-account audit.
//!
//! This module reads only a V020-06 session and the immutable provider revisions
//! referenced by its inventory checkpoint. It does not read optional listening
//! history or existing Chordrift collection, recipe, Spin, or publication
//! intent, and it exposes no provider or database mutation.

use std::fmt;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Row, postgres::PgRow};
use storexa::Database;

use crate::{
    ChordriftError,
    application::ApplicationInvocation,
    contract::{CONTRACT_VERSION, ClientError, ErrorCode, Query, QueryRequest, ResourceId, View},
    domain::{
        AccountContext, CapabilityStatus, ChordriftAccountId, EvidenceCapabilities,
        EvidenceCapability, OnboardingSessionId, ProviderCapabilities, ProviderCapability,
    },
    onboarding::{ContentFingerprint, OnboardingEvidence},
};

/// Evidence boundary used to produce an onboarding audit.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEvidenceBasis {
    /// Only the immutable current provider inventory was consulted.
    CurrentInventoryOnly,
}

/// Evidence boundary used by the enriched onboarding audit.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnrichedAuditEvidenceBasis {
    /// Current inventory plus one explicitly captured extended-history import.
    CurrentInventoryAndExtendedHistory,
}

/// A conclusion that current inventory cannot establish by itself.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrengthenedConclusionKind {
    /// At least one selected-history play was observed.
    ListeningObserved,
    /// At least one track has more than one selected-history play.
    RepeatedListeningObserved,
    /// At least one track has observations at least 180 days apart.
    LongTermListeningObserved,
    /// Selected history includes a track outside the current inventory.
    HistoryOutsideCurrentInventory,
    /// Selected history contains explicit completion observations.
    CompletionEvidenceObserved,
    /// Selected history contains explicit skip observations.
    SkipEvidenceObserved,
}

/// Strength available for one audit conclusion.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditConclusionStrength {
    /// The inventory-only audit has no listening evidence for this conclusion.
    UnavailableFromCurrentInventory,
    /// The selected extended-history records directly support the conclusion.
    DirectlyObservedFromExtendedHistory,
}

/// Exact explanation of one conclusion strengthened by extended history.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EvidenceStrengthening {
    /// Stable conclusion category.
    pub conclusion: StrengthenedConclusionKind,
    /// Strength available from the unchanged inventory-only baseline.
    pub inventory_only_strength: AuditConclusionStrength,
    /// Strength after reading the selected history import.
    pub enriched_strength: AuditConclusionStrength,
    /// Selected-history records supporting this conclusion.
    pub supporting_records: u64,
    /// Distinct tracks supporting this conclusion.
    pub supporting_tracks: u64,
    /// Stable provider-neutral explanation suitable for a thin client.
    pub explanation: String,
}

/// Aggregate facts from the one captured extended-history import.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExtendedHistoryReport {
    /// Captured archive content fingerprint.
    pub content_fingerprint: ContentFingerprint,
    /// Record count declared by the V020-06 input manifest and import row.
    pub declared_records: u64,
    /// Normalized records linked to the selected import.
    pub readable_records: u64,
    /// Non-superseded records usable for conclusions.
    pub usable_records: u64,
    /// Superseded records retained but excluded from conclusions.
    pub superseded_records: u64,
    /// Earliest usable observation, when present.
    pub first_observed_at: Option<chrono::DateTime<Utc>>,
    /// Latest usable observation, when present.
    pub last_observed_at: Option<chrono::DateTime<Utc>>,
    /// Distinct provider-track identities in usable history.
    pub distinct_historical_tracks: u64,
    /// Current-inventory tracks with at least one usable history record.
    pub current_tracks_with_history: u64,
    /// Historical tracks absent from the captured current inventory.
    pub history_only_tracks: u64,
    /// Tracks with two or more usable observations.
    pub repeatedly_played_tracks: u64,
    /// Records belonging to tracks with two or more usable observations.
    pub repeated_track_records: u64,
    /// Tracks observed across at least 180 days.
    pub long_term_observed_tracks: u64,
    /// Records belonging to tracks observed across at least 180 days.
    pub long_term_observed_records: u64,
    /// Records belonging to historical tracks outside current inventory.
    pub history_only_records: u64,
    /// Highest usable observation count for one track.
    pub maximum_track_plays: u64,
    /// Records explicitly marked completed.
    pub completed_records: u64,
    /// Distinct tracks with an explicitly completed record.
    pub completed_tracks: u64,
    /// Records explicitly marked skipped.
    pub skipped_records: u64,
    /// Distinct tracks with an explicitly skipped record.
    pub skipped_tracks: u64,
}

/// Limits that remain even after selected extended history is available.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnrichedAuditLimitation {
    /// Listening observations do not approve or explain user intent.
    UserIntentNotInferred,
    /// Listening observations do not establish collection membership.
    CollectionMembershipNotInferred,
    /// Repetition is not silently labeled as preference or favorite status.
    PreferenceNotInferred,
    /// Conclusions cover the captured import rather than all possible listening.
    LimitedToSelectedHistory,
}

/// Enriched audit with an unchanged inventory-only baseline and traceable gains.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnrichedOnboardingAudit {
    /// Inventory-only analysis produced by the same acceptance path.
    pub inventory_baseline: OnboardingAudit,
    /// Evidence boundary used for the enriched result.
    pub evidence_basis: EnrichedAuditEvidenceBasis,
    /// Facts from the selected extended-history import.
    pub history: ExtendedHistoryReport,
    /// Only conclusions that became stronger, each with exact support counts.
    pub strengthened_conclusions: Vec<EvidenceStrengthening>,
    /// Inference limits that extended history does not remove.
    pub remaining_limitations: Vec<EnrichedAuditLimitation>,
    /// Fingerprint of the complete deterministic enriched value.
    pub audit_fingerprint: ContentFingerprint,
}

/// Capability snapshot relevant to an inventory-only audit.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InventoryCapabilityReport {
    /// Ability to read the provider library inventory.
    pub library_inventory_read: CapabilityStatus,
    /// Ability to read playlist surfaces and ordered membership.
    pub playlist_read: CapabilityStatus,
    /// Ability to read the saved-track surface.
    pub saved_tracks_read: CapabilityStatus,
    /// Ability to read the saved-album surface.
    pub saved_albums_read: CapabilityStatus,
    /// Availability of current-inventory evidence.
    pub current_inventory: CapabilityStatus,
    /// Availability of optional extended playback history.
    pub extended_playback_history: CapabilityStatus,
    /// Always false for V020-07.
    pub extended_history_used: bool,
}

/// Summary of one observed provider playlist in the selected checkpoint.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AuditedPlaylist {
    /// Provider-neutral resource identity for the provider playlist record.
    pub provider_playlist_id: ResourceId,
    /// Provider-observed name.
    pub name: String,
    /// Entries declared by the immutable revision.
    pub reported_entries: u64,
    /// Membership rows available to the audit.
    pub readable_entries: u64,
    /// Distinct provider tracks represented by those rows.
    pub unique_tracks: u64,
    /// Repeated occurrences of a track inside this playlist.
    pub duplicate_entries: u64,
    /// Declared entries without a readable provider-track identity.
    pub unreadable_entries: u64,
    /// Provider-reported public visibility, when available.
    pub public: Option<bool>,
    /// Provider-reported collaboration state.
    pub collaborative: bool,
}

/// Provider-library shape visible from the selected immutable checkpoint.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LibraryAuditSummary {
    /// Number of observed playlist surfaces.
    pub playlists: u64,
    /// Total declared playlist entries.
    pub reported_playlist_entries: u64,
    /// Total readable playlist membership rows.
    pub readable_playlist_entries: u64,
    /// Tracks declared by the saved-track revision.
    pub saved_tracks: u64,
    /// Albums declared by the saved-album revision.
    pub saved_albums: u64,
    /// Tracks represented by the saved-album revision.
    pub saved_album_tracks: u64,
    /// Distinct provider-track identities across every current surface.
    pub unique_tracks: u64,
}

/// Surface overlap visible without interpreting user intent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InventoryOverlapReport {
    /// Tracks present in at least two different playlists.
    pub tracks_in_multiple_playlists: u64,
    /// Highest number of distinct playlists containing one track.
    pub maximum_playlist_occurrences: u64,
    /// Saved-surface tracks also present in a playlist.
    pub saved_and_playlisted_tracks: u64,
    /// Saved-surface tracks absent from every playlist.
    pub saved_outside_playlists: u64,
    /// Playlist tracks absent from saved-track and saved-album surfaces.
    pub playlist_only_tracks: u64,
    /// Duplicate positions within individual playlists.
    pub duplicate_playlist_entries: u64,
}

/// Why the inventory-only audit cannot make a stronger conclusion.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditLimitation {
    /// Current inventory cannot reveal listening frequency or recency.
    ListeningBehaviorNotInferred,
    /// Provider placement is not treated as approved Chordrift intent.
    UserIntentNotInferred,
    /// Current surfaces cannot establish vibe or collection membership.
    CollectionMembershipNotInferred,
    /// Extended history was deliberately excluded from this path.
    ExtendedHistoryNotUsed,
    /// The checkpoint did not include immutable saved-surface revisions.
    SavedSurfacesMissing,
    /// Some provider-declared entries lacked readable identities.
    ProviderItemsUnreadable,
}

/// One relevant capability that was degraded or unavailable.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AuditCapabilityGap {
    /// Stable provider-neutral capability name.
    pub capability: String,
    /// Observed availability.
    pub status: CapabilityStatus,
}

/// Honest uncertainty attached to the audit.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InventoryUncertaintyReport {
    /// Provider-declared entries missing readable item identities.
    pub unreadable_item_references: u64,
    /// Whether the checkpoint lacked its saved-track/saved-album revision pair.
    pub saved_surfaces_missing: bool,
    /// Relevant degraded or unavailable capabilities.
    pub capability_gaps: Vec<AuditCapabilityGap>,
    /// Explicit limits on what current inventory can establish.
    pub limitations: Vec<AuditLimitation>,
}

/// Basis for one conservative starter collection proposal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StarterCollectionBasis {
    /// Root view over every observed provider track.
    AllObservedInventory,
    /// Existing provider playlist preserved as an overlapping view.
    ExistingProviderPlaylist,
    /// Saved-surface tracks that appear in no playlist.
    SavedOutsidePlaylists,
    /// Provider-declared items that need review because identity is unavailable.
    UnreadableProviderItems,
}

/// Confidence appropriate to a read-only starter proposal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StarterProposalConfidence {
    /// Directly observed provider structure.
    Observed,
    /// Safe preserve-first default rather than inferred intent.
    ConservativeDefault,
    /// Requires review before it can become durable intent.
    ReviewRequired,
}

/// One proposed overlapping collection; it is not approved or persisted intent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StarterCollectionProposal {
    /// Deterministic key local to this proposal.
    pub stable_key: String,
    /// Suggested display name.
    pub name: String,
    /// Inventory fact supporting this proposal.
    pub basis: StarterCollectionBasis,
    /// Estimated distinct tracks represented by the proposed view.
    pub estimated_tracks: u64,
    /// Strength of the inventory-only conclusion.
    pub confidence: StarterProposalConfidence,
    /// Existing provider playlist when this proposal preserves one.
    pub source_playlist_id: Option<ResourceId>,
}

/// Preserve-first starter organization produced without writing intent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StarterOrganizationProposal {
    /// Always true: existing provider playlists remain untouched.
    pub preserve_existing_playlists: bool,
    /// Always false: reviewing this proposal does not approve it.
    pub approved: bool,
    /// Ordered overlapping collection proposals.
    pub collections: Vec<StarterCollectionProposal>,
}

/// Deterministic inventory-only audit for one onboarding session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OnboardingAudit {
    /// Captured onboarding session being audited.
    pub session_id: OnboardingSessionId,
    /// Owning Chordrift account.
    pub account_id: ChordriftAccountId,
    /// Selected provider connection.
    pub provider_connection_id: ResourceId,
    /// Fingerprint of the immutable V020-06 inputs.
    pub input_fingerprint: ContentFingerprint,
    /// Fingerprint of this deterministic audit value.
    pub audit_fingerprint: ContentFingerprint,
    /// Evidence boundary used by the audit.
    pub evidence_basis: AuditEvidenceBasis,
    /// Capability availability and explicit history exclusion.
    pub capabilities: InventoryCapabilityReport,
    /// Current-library shape.
    pub library: LibraryAuditSummary,
    /// Provider playlists in deterministic order.
    pub playlists: Vec<AuditedPlaylist>,
    /// Surface-overlap facts.
    pub overlap: InventoryOverlapReport,
    /// Visible uncertainty and inference limits.
    pub uncertainty: InventoryUncertaintyReport,
    /// Read-only preserve-first organization proposal.
    pub starter_organization: StarterOrganizationProposal,
}

/// Fingerprints only the comparable inventory findings, excluding session identity.
///
/// Inventory-only and enriched sessions intentionally have different session and
/// input fingerprints. This projection lets clients prove that enrichment kept
/// the same inventory analysis without pretending the complete audits are equal.
pub fn inventory_findings_fingerprint(
    audit: &OnboardingAudit,
) -> Result<ContentFingerprint, OnboardingAuditError> {
    #[derive(Serialize)]
    struct ComparableInventoryFindings<'audit> {
        capabilities: &'audit InventoryCapabilityReport,
        library: &'audit LibraryAuditSummary,
        playlists: &'audit [AuditedPlaylist],
        overlap: &'audit InventoryOverlapReport,
        uncertainty: &'audit InventoryUncertaintyReport,
        starter_organization: &'audit StarterOrganizationProposal,
    }

    let payload = ComparableInventoryFindings {
        capabilities: &audit.capabilities,
        library: &audit.library,
        playlists: &audit.playlists,
        overlap: &audit.overlap,
        uncertainty: &audit.uncertainty,
        starter_organization: &audit.starter_organization,
    };
    ContentFingerprint::new(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&payload).map_err(ChordriftError::from)?)
    ))
    .map_err(|_| client_error(ErrorCode::Internal))
}

/// Failure from the inventory-only audit boundary.
#[derive(Debug)]
pub enum OnboardingAuditError {
    /// Client-safe validation, ownership, or state failure.
    Client(ClientError),
    /// Infrastructure failed while reading immutable inputs.
    Infrastructure(ChordriftError),
}

impl OnboardingAuditError {
    /// Returns the stable client-safe representation of this failure.
    #[must_use]
    pub fn client_error(&self) -> ClientError {
        match self {
            Self::Client(error) => *error,
            Self::Infrastructure(_) => ClientError::new(ErrorCode::Internal, false),
        }
    }
}

impl fmt::Display for OnboardingAuditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client(error) => formatter.write_str(error.message()),
            Self::Infrastructure(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for OnboardingAuditError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Client(_) => None,
            Self::Infrastructure(error) => Some(error),
        }
    }
}

impl From<ChordriftError> for OnboardingAuditError {
    fn from(error: ChordriftError) -> Self {
        Self::Infrastructure(error)
    }
}

impl From<sqlx::Error> for OnboardingAuditError {
    fn from(error: sqlx::Error) -> Self {
        Self::Infrastructure(ChordriftError::from(error))
    }
}

/// PostgreSQL-backed, read-only inventory audit boundary.
pub struct InventoryOnlyAuditBoundary<'database> {
    database: &'database Database,
}

impl<'database> InventoryOnlyAuditBoundary<'database> {
    /// Creates an audit boundary over Chordrift's existing database connection.
    #[must_use]
    pub const fn new(database: &'database Database) -> Self {
        Self { database }
    }

    /// Reads one inventory-only onboarding audit without mutating any state.
    pub async fn read(
        &self,
        context: &AccountContext,
        request: &QueryRequest,
    ) -> Result<View<OnboardingAudit>, OnboardingAuditError> {
        let (audit, _) = self
            .read_baseline(context, request, AuditSessionSelection::InventoryOnly)
            .await?;
        Ok(View {
            contract_version: CONTRACT_VERSION,
            request_id: request.request_id,
            generated_at: Utc::now(),
            value: audit,
        })
    }

    async fn read_baseline(
        &self,
        context: &AccountContext,
        request: &QueryRequest,
        selection: AuditSessionSelection,
    ) -> Result<(OnboardingAudit, CapturedSession), OnboardingAuditError> {
        let session_id = validate_request(request)?;
        let captured = self.load_session(context, session_id, selection).await?;
        let playlists = self.load_playlists(captured.checkpoint_id).await?;
        let raw_summary = self.load_summary(captured.checkpoint_id).await?;
        let saved_surfaces_missing = !raw_summary.saved_surfaces_present;
        let capabilities = capability_report(&captured);
        let library = LibraryAuditSummary {
            playlists: count(&playlists)?,
            reported_playlist_entries: playlists
                .iter()
                .map(|playlist| playlist.reported_entries)
                .sum(),
            readable_playlist_entries: playlists
                .iter()
                .map(|playlist| playlist.readable_entries)
                .sum(),
            saved_tracks: raw_summary.saved_tracks,
            saved_albums: raw_summary.saved_albums,
            saved_album_tracks: raw_summary.saved_album_tracks,
            unique_tracks: raw_summary.unique_tracks,
        };
        let overlap = InventoryOverlapReport {
            tracks_in_multiple_playlists: raw_summary.tracks_in_multiple_playlists,
            maximum_playlist_occurrences: raw_summary.maximum_playlist_occurrences,
            saved_and_playlisted_tracks: raw_summary.saved_and_playlisted_tracks,
            saved_outside_playlists: raw_summary.saved_outside_playlists,
            playlist_only_tracks: raw_summary.playlist_only_tracks,
            duplicate_playlist_entries: playlists
                .iter()
                .map(|playlist| playlist.duplicate_entries)
                .sum(),
        };
        let unreadable_item_references = playlists
            .iter()
            .map(|playlist| playlist.unreadable_entries)
            .sum::<u64>()
            + raw_summary.unreadable_saved_tracks
            + raw_summary.unreadable_saved_album_tracks;
        let uncertainty = uncertainty_report(
            &capabilities,
            unreadable_item_references,
            saved_surfaces_missing,
        );
        let starter_organization = starter_organization(
            &playlists,
            library.unique_tracks,
            overlap.saved_outside_playlists,
            unreadable_item_references,
        );
        let audit_fingerprint = audit_fingerprint(
            session_id,
            captured.account_id,
            captured.provider_account_id,
            &captured.input_fingerprint,
            &capabilities,
            &library,
            &playlists,
            &overlap,
            &uncertainty,
            &starter_organization,
        )?;
        let audit = OnboardingAudit {
            session_id,
            account_id: captured.account_id,
            provider_connection_id: ResourceId::from_uuid(captured.provider_account_id),
            input_fingerprint: captured.input_fingerprint.clone(),
            audit_fingerprint,
            evidence_basis: AuditEvidenceBasis::CurrentInventoryOnly,
            capabilities,
            library,
            playlists,
            overlap,
            uncertainty,
            starter_organization,
        };
        Ok((audit, captured))
    }

    async fn load_session(
        &self,
        context: &AccountContext,
        session_id: OnboardingSessionId,
        selection: AuditSessionSelection,
    ) -> Result<CapturedSession, OnboardingAuditError> {
        let row = sqlx::query(
            "SELECT session.chordrift_account_id,
                    session.provider_account_id AS provider_connection_id,
                    session.provider_inventory_checkpoint_id,
                    session.include_extended_history, session.ignore_existing_intent,
                    session.input_fingerprint, session.input_manifest,
                    session.output_provenance, checkpoint.state_sha256,
                    capability.provider_capabilities,
                    capability.evidence_capabilities,
                    account.provider,
                    account.provider_account_id AS external_provider_account_id
               FROM onboarding_sessions session
               JOIN provider_accounts account ON account.id = session.provider_account_id
               JOIN provider_inventory_checkpoints checkpoint
                 ON checkpoint.id = session.provider_inventory_checkpoint_id
                AND checkpoint.provider_account_id = session.provider_account_id
               JOIN provider_capability_observations capability
                 ON capability.id = session.capability_observation_id
                AND capability.chordrift_account_id = session.chordrift_account_id
                AND capability.provider_account_id = session.provider_account_id
              WHERE session.id = $1",
        )
        .bind(session_id.as_uuid())
        .fetch_optional(self.database.pool())
        .await
        .map_err(ChordriftError::from)?;
        let Some(row) = row else {
            return Err(client_error(ErrorCode::ResourceNotFound));
        };
        let account_id: uuid::Uuid = row.try_get("chordrift_account_id")?;
        let provider_account_id: uuid::Uuid = row.try_get("provider_connection_id")?;
        if account_id != context.account_id().as_uuid()
            || provider_account_id != context.provider_connection().connection_id.as_uuid()
            || row.try_get::<String, _>("provider")?
                != context
                    .provider_connection()
                    .provider_account_id
                    .provider()
                    .as_str()
            || row.try_get::<String, _>("external_provider_account_id")?
                != context.provider_connection().provider_account_id.value()
        {
            return Err(client_error(ErrorCode::PermissionDenied));
        }
        let include_extended_history: bool = row.try_get("include_extended_history")?;
        let ignored_existing_intent: bool = row.try_get("ignore_existing_intent")?;
        let input_manifest: Value = row.try_get("input_manifest")?;
        let output_provenance: Value = row.try_get("output_provenance")?;
        let evidence: Vec<OnboardingEvidence> = serde_json::from_value(
            input_manifest
                .get("evidence")
                .cloned()
                .ok_or_else(|| client_error(ErrorCode::StateConflict))?,
        )
        .map_err(|_| client_error(ErrorCode::StateConflict))?;
        let selected_extended = evidence
            .iter()
            .filter(|item| item.capability == EvidenceCapability::ExtendedPlaybackHistory)
            .count();
        if include_extended_history != selection.include_extended_history()
            || evidence.len() != selected_extended
            || selected_extended != usize::from(include_extended_history)
            || !ignored_existing_intent
            || input_manifest.get("ignore_existing_intent") != Some(&Value::Bool(true))
            || output_provenance.get("chordrift_intent_read") != Some(&Value::Bool(false))
            || output_provenance.get("provider_write_requested") != Some(&Value::Bool(false))
        {
            return Err(client_error(ErrorCode::StateConflict));
        }
        let input_fingerprint =
            ContentFingerprint::new(row.try_get::<String, _>("input_fingerprint")?)
                .map_err(|_| client_error(ErrorCode::StateConflict))?;
        if hex_sha256(&serde_json::to_vec(&input_manifest).map_err(ChordriftError::from)?)
            != input_fingerprint.as_str()
            || input_manifest
                .pointer("/inventory/state_fingerprint")
                .and_then(Value::as_str)
                != Some(row.try_get::<String, _>("state_sha256")?.as_str())
        {
            return Err(client_error(ErrorCode::StateConflict));
        }
        let provider_capabilities: ProviderCapabilities =
            serde_json::from_value(row.try_get("provider_capabilities")?)
                .map_err(ChordriftError::from)?;
        let evidence_capabilities: EvidenceCapabilities =
            serde_json::from_value(row.try_get("evidence_capabilities")?)
                .map_err(ChordriftError::from)?;
        if provider_capabilities.provider_connection_id
            != context.provider_connection().connection_id
        {
            return Err(client_error(ErrorCode::StateConflict));
        }
        Ok(CapturedSession {
            account_id: ChordriftAccountId::from_uuid(account_id),
            provider_account_id,
            checkpoint_id: row.try_get("provider_inventory_checkpoint_id")?,
            input_fingerprint,
            provider_capabilities,
            evidence_capabilities,
            provider_namespace: row.try_get("provider")?,
            evidence,
        })
    }

    async fn load_playlists(
        &self,
        checkpoint_id: uuid::Uuid,
    ) -> Result<Vec<AuditedPlaylist>, OnboardingAuditError> {
        let rows = sqlx::query(
            "SELECT item.provider_playlist_id, item.name, item.public,
                    item.collaborative, revision.item_count::bigint AS reported_entries,
                    count(track.position)::bigint AS readable_entries,
                    count(DISTINCT track.provider_track_id)::bigint AS unique_tracks
               FROM provider_inventory_checkpoint_playlists item
               JOIN provider_playlist_revisions revision ON revision.id = item.revision_id
               LEFT JOIN provider_playlist_revision_tracks track
                 ON track.revision_id = item.revision_id
              WHERE item.checkpoint_id = $1
              GROUP BY item.provider_playlist_id, item.name, item.public,
                       item.collaborative, revision.item_count
              ORDER BY lower(item.name) COLLATE \"C\", item.name COLLATE \"C\",
                       item.provider_playlist_id",
        )
        .bind(checkpoint_id)
        .fetch_all(self.database.pool())
        .await
        .map_err(ChordriftError::from)?;
        rows.into_iter().map(playlist_from_row).collect()
    }

    async fn load_summary(
        &self,
        checkpoint_id: uuid::Uuid,
    ) -> Result<RawInventorySummary, OnboardingAuditError> {
        let row = sqlx::query(
            "WITH playlist_membership AS (
                 SELECT item.provider_playlist_id, track.provider_track_id
                   FROM provider_inventory_checkpoint_playlists item
                   JOIN provider_playlist_revision_tracks track
                     ON track.revision_id = item.revision_id
                  WHERE item.checkpoint_id = $1
             ), playlist_presence AS (
                 SELECT DISTINCT provider_playlist_id, provider_track_id
                   FROM playlist_membership
             ), playlist_occurrences AS (
                 SELECT provider_track_id, count(*)::bigint AS surfaces
                   FROM playlist_presence GROUP BY provider_track_id
             ), saved_track_revision AS (
                 SELECT surface.saved_track_revision_id AS id,
                        revision.item_count::bigint AS reported
                   FROM provider_inventory_checkpoint_saved_surfaces surface
                   JOIN provider_saved_track_revisions revision
                     ON revision.id = surface.saved_track_revision_id
                  WHERE surface.checkpoint_id = $1
             ), saved_album_revision AS (
                 SELECT surface.saved_album_revision_id AS id,
                        revision.album_count::bigint AS albums,
                        revision.track_count::bigint AS reported_tracks
                   FROM provider_inventory_checkpoint_saved_surfaces surface
                   JOIN provider_saved_album_revisions revision
                     ON revision.id = surface.saved_album_revision_id
                  WHERE surface.checkpoint_id = $1
             ), saved_tracks AS (
                 SELECT track.provider_track_id
                   FROM saved_track_revision revision
                   JOIN provider_saved_track_revision_tracks track
                     ON track.revision_id = revision.id
             ), saved_album_tracks AS (
                 SELECT DISTINCT track.provider_track_id
                   FROM saved_album_revision revision
                   JOIN provider_saved_album_revision_tracks track
                     ON track.revision_id = revision.id
             ), saved_inventory AS (
                 SELECT provider_track_id FROM saved_tracks
                 UNION SELECT provider_track_id FROM saved_album_tracks
             ), playlist_inventory AS (
                 SELECT DISTINCT provider_track_id FROM playlist_membership
             ), all_inventory AS (
                 SELECT provider_track_id FROM saved_inventory
                 UNION SELECT provider_track_id FROM playlist_inventory
             )
             SELECT
               EXISTS(SELECT 1 FROM provider_inventory_checkpoint_saved_surfaces
                       WHERE checkpoint_id = $1) AS saved_surfaces_present,
               COALESCE((SELECT reported FROM saved_track_revision), 0)::bigint
                   AS saved_tracks,
               COALESCE((SELECT albums FROM saved_album_revision), 0)::bigint
                   AS saved_albums,
               COALESCE((SELECT reported_tracks FROM saved_album_revision), 0)::bigint
                   AS saved_album_tracks,
               (SELECT count(*)::bigint FROM all_inventory) AS unique_tracks,
               (SELECT count(*)::bigint FROM playlist_occurrences WHERE surfaces > 1)
                   AS tracks_in_multiple_playlists,
               COALESCE((SELECT max(surfaces) FROM playlist_occurrences), 0)::bigint
                   AS maximum_playlist_occurrences,
               (SELECT count(*)::bigint FROM saved_inventory saved
                 WHERE EXISTS (SELECT 1 FROM playlist_inventory playlist
                                WHERE playlist.provider_track_id = saved.provider_track_id))
                   AS saved_and_playlisted_tracks,
               (SELECT count(*)::bigint FROM saved_inventory saved
                 WHERE NOT EXISTS (SELECT 1 FROM playlist_inventory playlist
                                    WHERE playlist.provider_track_id = saved.provider_track_id))
                   AS saved_outside_playlists,
               (SELECT count(*)::bigint FROM playlist_inventory playlist
                 WHERE NOT EXISTS (SELECT 1 FROM saved_inventory saved
                                    WHERE saved.provider_track_id = playlist.provider_track_id))
                   AS playlist_only_tracks,
               GREATEST(COALESCE((SELECT reported FROM saved_track_revision), 0)
                   - (SELECT count(*)::bigint FROM saved_tracks), 0)::bigint
                   AS unreadable_saved_tracks,
               GREATEST(COALESCE((SELECT reported_tracks FROM saved_album_revision), 0)
                   - (SELECT count(*)::bigint FROM saved_album_tracks), 0)::bigint
                   AS unreadable_saved_album_tracks",
        )
        .bind(checkpoint_id)
        .fetch_one(self.database.pool())
        .await
        .map_err(ChordriftError::from)?;
        RawInventorySummary::from_row(&row)
    }

    /// Wraps this query for execution through [`crate::application::ApplicationFacade`].
    #[must_use]
    pub const fn invocation<'request>(
        &'request self,
        context: &'request AccountContext,
        request: &'request QueryRequest,
    ) -> InventoryOnlyAuditInvocation<'request, 'database> {
        InventoryOnlyAuditInvocation {
            boundary: self,
            context,
            request,
        }
    }
}

/// One inventory-only audit query submitted through the shared application facade.
pub struct InventoryOnlyAuditInvocation<'request, 'database> {
    boundary: &'request InventoryOnlyAuditBoundary<'database>,
    context: &'request AccountContext,
    request: &'request QueryRequest,
}

impl ApplicationInvocation for InventoryOnlyAuditInvocation<'_, '_> {
    type Output = Result<View<OnboardingAudit>, OnboardingAuditError>;

    async fn execute(self) -> crate::Result<Self::Output> {
        Ok(self.boundary.read(self.context, self.request).await)
    }
}

/// PostgreSQL-backed enriched audit over one explicitly captured history import.
pub struct EnrichedAuditBoundary<'database> {
    database: &'database Database,
}

impl<'database> EnrichedAuditBoundary<'database> {
    /// Creates an enriched audit boundary over Chordrift's database connection.
    #[must_use]
    pub const fn new(database: &'database Database) -> Self {
        Self { database }
    }

    /// Reads the inventory baseline and its selected extended-history evidence.
    pub async fn read(
        &self,
        context: &AccountContext,
        request: &QueryRequest,
    ) -> Result<View<EnrichedOnboardingAudit>, OnboardingAuditError> {
        let inventory_boundary = InventoryOnlyAuditBoundary::new(self.database);
        let (inventory_baseline, captured) = inventory_boundary
            .read_baseline(context, request, AuditSessionSelection::ExtendedHistory)
            .await?;
        let evidence = captured
            .evidence
            .first()
            .ok_or_else(|| client_error(ErrorCode::StateConflict))?;
        let history = self.load_history(&captured, evidence).await?;
        let strengthened_conclusions = strengthened_conclusions(&history);
        let remaining_limitations = vec![
            EnrichedAuditLimitation::UserIntentNotInferred,
            EnrichedAuditLimitation::CollectionMembershipNotInferred,
            EnrichedAuditLimitation::PreferenceNotInferred,
            EnrichedAuditLimitation::LimitedToSelectedHistory,
        ];
        let audit_fingerprint = enriched_audit_fingerprint(
            &inventory_baseline,
            &history,
            &strengthened_conclusions,
            &remaining_limitations,
        )?;
        Ok(View {
            contract_version: CONTRACT_VERSION,
            request_id: request.request_id,
            generated_at: Utc::now(),
            value: EnrichedOnboardingAudit {
                inventory_baseline,
                evidence_basis: EnrichedAuditEvidenceBasis::CurrentInventoryAndExtendedHistory,
                history,
                strengthened_conclusions,
                remaining_limitations,
                audit_fingerprint,
            },
        })
    }

    async fn load_history(
        &self,
        captured: &CapturedSession,
        evidence: &OnboardingEvidence,
    ) -> Result<ExtendedHistoryReport, OnboardingAuditError> {
        let import = sqlx::query(
            "SELECT id, event_count::bigint AS declared_records
               FROM listening_evidence_imports
              WHERE provider_account_id = $1
                AND provider = $2
                AND archive_kind = 'extended_streaming_history'
                AND archive_sha256 = $3",
        )
        .bind(captured.provider_account_id)
        .bind(&captured.provider_namespace)
        .bind(evidence.content_fingerprint.as_str())
        .fetch_optional(self.database.pool())
        .await
        .map_err(ChordriftError::from)?;
        let Some(import) = import else {
            return Err(client_error(ErrorCode::StateConflict));
        };
        let declared_records = nonnegative(&import, "declared_records")?;
        if declared_records != evidence.record_count {
            return Err(client_error(ErrorCode::StateConflict));
        }
        let import_id: uuid::Uuid = import.try_get("id")?;
        let row = sqlx::query(
            "WITH current_track_ids AS (
                 SELECT track.provider, track.provider_track_id
                   FROM provider_inventory_checkpoint_playlists playlist
                   JOIN provider_playlist_revision_tracks member
                     ON member.revision_id = playlist.revision_id
                   JOIN provider_tracks track ON track.id = member.provider_track_id
                  WHERE playlist.checkpoint_id = $1
                 UNION
                 SELECT track.provider, track.provider_track_id
                   FROM provider_inventory_checkpoint_saved_surfaces surface
                   JOIN provider_saved_track_revision_tracks member
                     ON member.revision_id = surface.saved_track_revision_id
                   JOIN provider_tracks track ON track.id = member.provider_track_id
                  WHERE surface.checkpoint_id = $1
                 UNION
                 SELECT track.provider, track.provider_track_id
                   FROM provider_inventory_checkpoint_saved_surfaces surface
                   JOIN provider_saved_album_revision_tracks member
                     ON member.revision_id = surface.saved_album_revision_id
                   JOIN provider_tracks track ON track.id = member.provider_track_id
                  WHERE surface.checkpoint_id = $1
             ), selected_events AS (
                 SELECT event.played_at, event.completed, event.skipped,
                        event.superseded_at, event.source_kind,
                        identity.provider, identity.provider_track_id
                   FROM normalized_listening_events event
                   JOIN historical_provider_track_identities identity
                     ON identity.id = event.historical_identity_id
                  WHERE event.source_import_id = $2
             ), usable_events AS (
                 SELECT * FROM selected_events WHERE superseded_at IS NULL
             ), track_history AS (
                 SELECT provider, provider_track_id, count(*)::bigint AS plays,
                        min(played_at) AS first_played_at,
                        max(played_at) AS last_played_at
                   FROM usable_events
                  GROUP BY provider, provider_track_id
             )
             SELECT
               (SELECT count(*)::bigint FROM selected_events) AS readable_records,
               (SELECT count(*)::bigint FROM usable_events) AS usable_records,
               (SELECT count(*)::bigint FROM selected_events
                 WHERE superseded_at IS NOT NULL) AS superseded_records,
               (SELECT count(*)::bigint FROM selected_events
                 WHERE source_kind <> 'archive') AS invalid_source_records,
               (SELECT min(played_at) FROM usable_events) AS first_observed_at,
               (SELECT max(played_at) FROM usable_events) AS last_observed_at,
               (SELECT count(*)::bigint FROM track_history)
                 AS distinct_historical_tracks,
               (SELECT count(*)::bigint FROM track_history history
                 WHERE EXISTS (SELECT 1 FROM current_track_ids current
                                WHERE current.provider = history.provider
                                  AND current.provider_track_id = history.provider_track_id))
                 AS current_tracks_with_history,
               (SELECT count(*)::bigint FROM track_history history
                 WHERE NOT EXISTS (SELECT 1 FROM current_track_ids current
                                    WHERE current.provider = history.provider
                                      AND current.provider_track_id = history.provider_track_id))
                 AS history_only_tracks,
               (SELECT count(*)::bigint FROM track_history WHERE plays >= 2)
                 AS repeatedly_played_tracks,
               COALESCE((SELECT sum(plays) FROM track_history WHERE plays >= 2), 0)::bigint
                 AS repeated_track_records,
               (SELECT count(*)::bigint FROM track_history
                 WHERE last_played_at - first_played_at >= interval '180 days')
                 AS long_term_observed_tracks,
               COALESCE((SELECT sum(plays) FROM track_history
                 WHERE last_played_at - first_played_at >= interval '180 days'), 0)::bigint
                 AS long_term_observed_records,
               COALESCE((SELECT sum(plays) FROM track_history history
                 WHERE NOT EXISTS (SELECT 1 FROM current_track_ids current
                                    WHERE current.provider = history.provider
                                      AND current.provider_track_id = history.provider_track_id)), 0)::bigint
                 AS history_only_records,
               COALESCE((SELECT max(plays) FROM track_history), 0)::bigint
                 AS maximum_track_plays,
               (SELECT count(*)::bigint FROM usable_events WHERE completed IS TRUE)
                 AS completed_records,
               (SELECT count(DISTINCT (provider, provider_track_id))::bigint
                  FROM usable_events WHERE completed IS TRUE) AS completed_tracks,
               (SELECT count(*)::bigint FROM usable_events WHERE skipped IS TRUE)
                 AS skipped_records,
               (SELECT count(DISTINCT (provider, provider_track_id))::bigint
                  FROM usable_events WHERE skipped IS TRUE) AS skipped_tracks",
        )
        .bind(captured.checkpoint_id)
        .bind(import_id)
        .fetch_one(self.database.pool())
        .await
        .map_err(ChordriftError::from)?;
        let readable_records = nonnegative(&row, "readable_records")?;
        if readable_records != declared_records || nonnegative(&row, "invalid_source_records")? != 0
        {
            return Err(client_error(ErrorCode::StateConflict));
        }
        Ok(ExtendedHistoryReport {
            content_fingerprint: evidence.content_fingerprint.clone(),
            declared_records,
            readable_records,
            usable_records: nonnegative(&row, "usable_records")?,
            superseded_records: nonnegative(&row, "superseded_records")?,
            first_observed_at: row.try_get("first_observed_at")?,
            last_observed_at: row.try_get("last_observed_at")?,
            distinct_historical_tracks: nonnegative(&row, "distinct_historical_tracks")?,
            current_tracks_with_history: nonnegative(&row, "current_tracks_with_history")?,
            history_only_tracks: nonnegative(&row, "history_only_tracks")?,
            repeatedly_played_tracks: nonnegative(&row, "repeatedly_played_tracks")?,
            repeated_track_records: nonnegative(&row, "repeated_track_records")?,
            long_term_observed_tracks: nonnegative(&row, "long_term_observed_tracks")?,
            long_term_observed_records: nonnegative(&row, "long_term_observed_records")?,
            history_only_records: nonnegative(&row, "history_only_records")?,
            maximum_track_plays: nonnegative(&row, "maximum_track_plays")?,
            completed_records: nonnegative(&row, "completed_records")?,
            completed_tracks: nonnegative(&row, "completed_tracks")?,
            skipped_records: nonnegative(&row, "skipped_records")?,
            skipped_tracks: nonnegative(&row, "skipped_tracks")?,
        })
    }

    /// Wraps this query for execution through [`crate::application::ApplicationFacade`].
    #[must_use]
    pub const fn invocation<'request>(
        &'request self,
        context: &'request AccountContext,
        request: &'request QueryRequest,
    ) -> EnrichedAuditInvocation<'request, 'database> {
        EnrichedAuditInvocation {
            boundary: self,
            context,
            request,
        }
    }
}

/// One enriched onboarding query submitted through the shared application facade.
pub struct EnrichedAuditInvocation<'request, 'database> {
    boundary: &'request EnrichedAuditBoundary<'database>,
    context: &'request AccountContext,
    request: &'request QueryRequest,
}

impl ApplicationInvocation for EnrichedAuditInvocation<'_, '_> {
    type Output = Result<View<EnrichedOnboardingAudit>, OnboardingAuditError>;

    async fn execute(self) -> crate::Result<Self::Output> {
        Ok(self.boundary.read(self.context, self.request).await)
    }
}

#[derive(Clone, Copy)]
enum AuditSessionSelection {
    InventoryOnly,
    ExtendedHistory,
}

impl AuditSessionSelection {
    const fn include_extended_history(self) -> bool {
        matches!(self, Self::ExtendedHistory)
    }
}

struct CapturedSession {
    account_id: ChordriftAccountId,
    provider_account_id: uuid::Uuid,
    checkpoint_id: uuid::Uuid,
    input_fingerprint: ContentFingerprint,
    provider_capabilities: ProviderCapabilities,
    evidence_capabilities: EvidenceCapabilities,
    provider_namespace: String,
    evidence: Vec<OnboardingEvidence>,
}

struct RawInventorySummary {
    saved_surfaces_present: bool,
    saved_tracks: u64,
    saved_albums: u64,
    saved_album_tracks: u64,
    unique_tracks: u64,
    tracks_in_multiple_playlists: u64,
    maximum_playlist_occurrences: u64,
    saved_and_playlisted_tracks: u64,
    saved_outside_playlists: u64,
    playlist_only_tracks: u64,
    unreadable_saved_tracks: u64,
    unreadable_saved_album_tracks: u64,
}

impl RawInventorySummary {
    fn from_row(row: &PgRow) -> Result<Self, OnboardingAuditError> {
        Ok(Self {
            saved_surfaces_present: row.try_get("saved_surfaces_present")?,
            saved_tracks: nonnegative(row, "saved_tracks")?,
            saved_albums: nonnegative(row, "saved_albums")?,
            saved_album_tracks: nonnegative(row, "saved_album_tracks")?,
            unique_tracks: nonnegative(row, "unique_tracks")?,
            tracks_in_multiple_playlists: nonnegative(row, "tracks_in_multiple_playlists")?,
            maximum_playlist_occurrences: nonnegative(row, "maximum_playlist_occurrences")?,
            saved_and_playlisted_tracks: nonnegative(row, "saved_and_playlisted_tracks")?,
            saved_outside_playlists: nonnegative(row, "saved_outside_playlists")?,
            playlist_only_tracks: nonnegative(row, "playlist_only_tracks")?,
            unreadable_saved_tracks: nonnegative(row, "unreadable_saved_tracks")?,
            unreadable_saved_album_tracks: nonnegative(row, "unreadable_saved_album_tracks")?,
        })
    }
}

fn validate_request(request: &QueryRequest) -> Result<OnboardingSessionId, OnboardingAuditError> {
    if request.contract_version != CONTRACT_VERSION {
        return Err(client_error(ErrorCode::IncompatibleContract));
    }
    let Query::OnboardingAudit { session_id } = request.query else {
        return Err(client_error(ErrorCode::InvalidRequest));
    };
    Ok(OnboardingSessionId::from_uuid(session_id.as_uuid()))
}

fn playlist_from_row(row: PgRow) -> Result<AuditedPlaylist, OnboardingAuditError> {
    let reported_entries = nonnegative(&row, "reported_entries")?;
    let readable_entries = nonnegative(&row, "readable_entries")?;
    let unique_tracks = nonnegative(&row, "unique_tracks")?;
    Ok(AuditedPlaylist {
        provider_playlist_id: ResourceId::from_uuid(row.try_get("provider_playlist_id")?),
        name: row.try_get("name")?,
        reported_entries,
        readable_entries,
        unique_tracks,
        duplicate_entries: readable_entries.saturating_sub(unique_tracks),
        unreadable_entries: reported_entries.saturating_sub(readable_entries),
        public: row.try_get("public")?,
        collaborative: row.try_get("collaborative")?,
    })
}

fn capability_report(captured: &CapturedSession) -> InventoryCapabilityReport {
    InventoryCapabilityReport {
        library_inventory_read: captured
            .provider_capabilities
            .status(ProviderCapability::LibraryInventoryRead),
        playlist_read: captured
            .provider_capabilities
            .status(ProviderCapability::PlaylistRead),
        saved_tracks_read: captured
            .provider_capabilities
            .status(ProviderCapability::SavedTracksRead),
        saved_albums_read: captured
            .provider_capabilities
            .status(ProviderCapability::SavedAlbumsRead),
        current_inventory: captured
            .evidence_capabilities
            .status(EvidenceCapability::CurrentInventory),
        extended_playback_history: captured
            .evidence_capabilities
            .status(EvidenceCapability::ExtendedPlaybackHistory),
        extended_history_used: false,
    }
}

fn uncertainty_report(
    capabilities: &InventoryCapabilityReport,
    unreadable_item_references: u64,
    saved_surfaces_missing: bool,
) -> InventoryUncertaintyReport {
    let mut capability_gaps = Vec::new();
    for (capability, status) in [
        (
            "library_inventory_read",
            capabilities.library_inventory_read,
        ),
        ("playlist_read", capabilities.playlist_read),
        ("saved_tracks_read", capabilities.saved_tracks_read),
        ("saved_albums_read", capabilities.saved_albums_read),
        ("current_inventory", capabilities.current_inventory),
    ] {
        if status != CapabilityStatus::Available {
            capability_gaps.push(AuditCapabilityGap {
                capability: capability.to_owned(),
                status,
            });
        }
    }
    let mut limitations = vec![
        AuditLimitation::ListeningBehaviorNotInferred,
        AuditLimitation::UserIntentNotInferred,
        AuditLimitation::CollectionMembershipNotInferred,
        AuditLimitation::ExtendedHistoryNotUsed,
    ];
    if saved_surfaces_missing {
        limitations.push(AuditLimitation::SavedSurfacesMissing);
    }
    if unreadable_item_references > 0 {
        limitations.push(AuditLimitation::ProviderItemsUnreadable);
    }
    InventoryUncertaintyReport {
        unreadable_item_references,
        saved_surfaces_missing,
        capability_gaps,
        limitations,
    }
}

fn starter_organization(
    playlists: &[AuditedPlaylist],
    unique_tracks: u64,
    saved_outside_playlists: u64,
    unreadable_item_references: u64,
) -> StarterOrganizationProposal {
    let mut collections = vec![StarterCollectionProposal {
        stable_key: "preserved-library".to_owned(),
        name: "Preserved Library".to_owned(),
        basis: StarterCollectionBasis::AllObservedInventory,
        estimated_tracks: unique_tracks,
        confidence: StarterProposalConfidence::ConservativeDefault,
        source_playlist_id: None,
    }];
    collections.extend(playlists.iter().map(|playlist| StarterCollectionProposal {
        stable_key: format!("provider-playlist-{}", playlist.provider_playlist_id),
        name: playlist.name.clone(),
        basis: StarterCollectionBasis::ExistingProviderPlaylist,
        estimated_tracks: playlist.unique_tracks,
        confidence: StarterProposalConfidence::Observed,
        source_playlist_id: Some(playlist.provider_playlist_id),
    }));
    if saved_outside_playlists > 0 {
        collections.push(StarterCollectionProposal {
            stable_key: "saved-outside-playlists".to_owned(),
            name: "Saved Outside Playlists".to_owned(),
            basis: StarterCollectionBasis::SavedOutsidePlaylists,
            estimated_tracks: saved_outside_playlists,
            confidence: StarterProposalConfidence::Observed,
            source_playlist_id: None,
        });
    }
    if unreadable_item_references > 0 {
        collections.push(StarterCollectionProposal {
            stable_key: "needs-review".to_owned(),
            name: "Needs Review".to_owned(),
            basis: StarterCollectionBasis::UnreadableProviderItems,
            estimated_tracks: unreadable_item_references,
            confidence: StarterProposalConfidence::ReviewRequired,
            source_playlist_id: None,
        });
    }
    StarterOrganizationProposal {
        preserve_existing_playlists: true,
        approved: false,
        collections,
    }
}

#[allow(clippy::too_many_arguments)]
fn audit_fingerprint(
    session_id: OnboardingSessionId,
    account_id: ChordriftAccountId,
    provider_account_id: uuid::Uuid,
    input_fingerprint: &ContentFingerprint,
    capabilities: &InventoryCapabilityReport,
    library: &LibraryAuditSummary,
    playlists: &[AuditedPlaylist],
    overlap: &InventoryOverlapReport,
    uncertainty: &InventoryUncertaintyReport,
    starter_organization: &StarterOrganizationProposal,
) -> Result<ContentFingerprint, OnboardingAuditError> {
    let value = serde_json::json!({
        "schema_version": 1,
        "session_id": session_id,
        "account_id": account_id,
        "provider_connection_id": provider_account_id,
        "input_fingerprint": input_fingerprint,
        "evidence_basis": AuditEvidenceBasis::CurrentInventoryOnly,
        "capabilities": capabilities,
        "library": library,
        "playlists": playlists,
        "overlap": overlap,
        "uncertainty": uncertainty,
        "starter_organization": starter_organization,
    });
    ContentFingerprint::new(hex_sha256(
        &serde_json::to_vec(&value).map_err(ChordriftError::from)?,
    ))
    .map_err(|_| client_error(ErrorCode::Internal))
}

fn strengthened_conclusions(history: &ExtendedHistoryReport) -> Vec<EvidenceStrengthening> {
    let mut conclusions = Vec::new();
    let mut add = |conclusion, records, tracks, explanation: &str| {
        if records > 0 || tracks > 0 {
            conclusions.push(EvidenceStrengthening {
                conclusion,
                inventory_only_strength: AuditConclusionStrength::UnavailableFromCurrentInventory,
                enriched_strength: AuditConclusionStrength::DirectlyObservedFromExtendedHistory,
                supporting_records: records,
                supporting_tracks: tracks,
                explanation: explanation.to_owned(),
            });
        }
    };
    add(
        StrengthenedConclusionKind::ListeningObserved,
        history.usable_records,
        history.distinct_historical_tracks,
        "The selected extended-history import directly records listening events.",
    );
    add(
        StrengthenedConclusionKind::RepeatedListeningObserved,
        history.repeated_track_records,
        history.repeatedly_played_tracks,
        "At least two selected-history observations exist for each supporting track.",
    );
    add(
        StrengthenedConclusionKind::LongTermListeningObserved,
        history.long_term_observed_records,
        history.long_term_observed_tracks,
        "Supporting tracks have selected-history observations at least 180 days apart.",
    );
    add(
        StrengthenedConclusionKind::HistoryOutsideCurrentInventory,
        history.history_only_records,
        history.history_only_tracks,
        "Selected history includes provider-track identities absent from the captured inventory.",
    );
    add(
        StrengthenedConclusionKind::CompletionEvidenceObserved,
        history.completed_records,
        history.completed_tracks,
        "Selected history explicitly marks these records completed.",
    );
    add(
        StrengthenedConclusionKind::SkipEvidenceObserved,
        history.skipped_records,
        history.skipped_tracks,
        "Selected history explicitly marks these records skipped.",
    );
    conclusions
}

fn enriched_audit_fingerprint(
    inventory_baseline: &OnboardingAudit,
    history: &ExtendedHistoryReport,
    strengthened_conclusions: &[EvidenceStrengthening],
    remaining_limitations: &[EnrichedAuditLimitation],
) -> Result<ContentFingerprint, OnboardingAuditError> {
    let value = serde_json::json!({
        "schema_version": 1,
        "inventory_baseline": inventory_baseline,
        "evidence_basis": EnrichedAuditEvidenceBasis::CurrentInventoryAndExtendedHistory,
        "history": history,
        "strengthened_conclusions": strengthened_conclusions,
        "remaining_limitations": remaining_limitations,
    });
    ContentFingerprint::new(hex_sha256(
        &serde_json::to_vec(&value).map_err(ChordriftError::from)?,
    ))
    .map_err(|_| client_error(ErrorCode::Internal))
}

fn nonnegative(row: &PgRow, column: &str) -> Result<u64, OnboardingAuditError> {
    u64::try_from(row.try_get::<i64, _>(column)?)
        .map_err(|_| client_error(ErrorCode::StateConflict))
}

fn count<T>(values: &[T]) -> Result<u64, OnboardingAuditError> {
    u64::try_from(values.len()).map_err(|_| client_error(ErrorCode::Internal))
}

fn client_error(code: ErrorCode) -> OnboardingAuditError {
    OnboardingAuditError::Client(ClientError::new(code, false))
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history_report() -> ExtendedHistoryReport {
        ExtendedHistoryReport {
            content_fingerprint: ContentFingerprint::new("e".repeat(64))
                .expect("fixture fingerprint is valid"),
            declared_records: 0,
            readable_records: 0,
            usable_records: 0,
            superseded_records: 0,
            first_observed_at: None,
            last_observed_at: None,
            distinct_historical_tracks: 0,
            current_tracks_with_history: 0,
            history_only_tracks: 0,
            repeatedly_played_tracks: 0,
            repeated_track_records: 0,
            long_term_observed_tracks: 0,
            long_term_observed_records: 0,
            history_only_records: 0,
            maximum_track_plays: 0,
            completed_records: 0,
            completed_tracks: 0,
            skipped_records: 0,
            skipped_tracks: 0,
        }
    }

    fn playlist(id: u128, name: &str, tracks: u64) -> AuditedPlaylist {
        AuditedPlaylist {
            provider_playlist_id: ResourceId::from_uuid(uuid::Uuid::from_u128(id)),
            name: name.to_owned(),
            reported_entries: tracks,
            readable_entries: tracks,
            unique_tracks: tracks,
            duplicate_entries: 0,
            unreadable_entries: 0,
            public: None,
            collaborative: false,
        }
    }

    #[test]
    fn starter_organization_is_preserve_first_and_never_approved() {
        let proposal = starter_organization(&[playlist(1, "Existing", 3)], 5, 2, 1);

        assert!(proposal.preserve_existing_playlists);
        assert!(!proposal.approved);
        assert_eq!(proposal.collections.len(), 4);
        assert_eq!(
            proposal.collections[1].basis,
            StarterCollectionBasis::ExistingProviderPlaylist
        );
        assert_eq!(
            proposal.collections[3].confidence,
            StarterProposalConfidence::ReviewRequired
        );
    }

    #[test]
    fn inventory_only_uncertainty_never_claims_listening_or_intent() {
        let capabilities = InventoryCapabilityReport {
            library_inventory_read: CapabilityStatus::Available,
            playlist_read: CapabilityStatus::Degraded,
            saved_tracks_read: CapabilityStatus::Available,
            saved_albums_read: CapabilityStatus::Unavailable,
            current_inventory: CapabilityStatus::Available,
            extended_playback_history: CapabilityStatus::Available,
            extended_history_used: false,
        };
        let uncertainty = uncertainty_report(&capabilities, 1, false);

        assert_eq!(uncertainty.capability_gaps.len(), 2);
        assert!(
            uncertainty
                .limitations
                .contains(&AuditLimitation::ListeningBehaviorNotInferred)
        );
        assert!(
            uncertainty
                .limitations
                .contains(&AuditLimitation::UserIntentNotInferred)
        );
        assert!(
            uncertainty
                .limitations
                .contains(&AuditLimitation::ExtendedHistoryNotUsed)
        );
    }

    #[test]
    fn enriched_conclusions_include_only_directly_supported_gains() {
        assert!(strengthened_conclusions(&history_report()).is_empty());

        let mut history = history_report();
        history.usable_records = 3;
        history.distinct_historical_tracks = 2;
        history.repeatedly_played_tracks = 1;
        history.repeated_track_records = 2;
        let conclusions = strengthened_conclusions(&history);

        assert_eq!(conclusions.len(), 2);
        assert_eq!(
            conclusions[0].inventory_only_strength,
            AuditConclusionStrength::UnavailableFromCurrentInventory
        );
        assert_eq!(
            conclusions[0].enriched_strength,
            AuditConclusionStrength::DirectlyObservedFromExtendedHistory
        );
        assert_eq!(conclusions[1].supporting_records, 2);
        assert_eq!(conclusions[1].supporting_tracks, 1);
    }
}
