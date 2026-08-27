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
    onboarding::ContentFingerprint,
};

/// Evidence boundary used to produce an onboarding audit.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEvidenceBasis {
    /// Only the immutable current provider inventory was consulted.
    CurrentInventoryOnly,
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
        let session_id = validate_request(request)?;
        let captured = self.load_session(context, session_id).await?;
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
            input_fingerprint: captured.input_fingerprint,
            audit_fingerprint,
            evidence_basis: AuditEvidenceBasis::CurrentInventoryOnly,
            capabilities,
            library,
            playlists,
            overlap,
            uncertainty,
            starter_organization,
        };
        Ok(View {
            contract_version: CONTRACT_VERSION,
            request_id: request.request_id,
            generated_at: Utc::now(),
            value: audit,
        })
    }

    async fn load_session(
        &self,
        context: &AccountContext,
        session_id: OnboardingSessionId,
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
        if include_extended_history
            || !ignored_existing_intent
            || input_manifest
                .get("evidence")
                .and_then(Value::as_array)
                .is_none_or(|evidence| !evidence.is_empty())
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

struct CapturedSession {
    account_id: ChordriftAccountId,
    provider_account_id: uuid::Uuid,
    checkpoint_id: uuid::Uuid,
    input_fingerprint: ContentFingerprint,
    provider_capabilities: ProviderCapabilities,
    evidence_capabilities: EvidenceCapabilities,
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
}
