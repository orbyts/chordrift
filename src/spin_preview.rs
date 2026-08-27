//! Deterministic, provider-free Spin preview generation and persistence.
//!
//! This boundary consumes a verified V020-09 unordered draft, assigns exact
//! one-based playback positions, persists the preview in migration 0046's Spin
//! tables, and reads it back as a client-safe immutable view. It exposes no
//! provider port and cannot publish or approve a Spin.

use std::{cmp::Reverse, collections::BTreeSet, fmt};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use storexa::Database;

use crate::{
    ChordriftError,
    application::ApplicationInvocation,
    contract::{
        CONTRACT_VERSION, ClientError, Command, CommandRequest, ErrorCode, Query, QueryRequest,
        View,
    },
    domain::{
        AccountOwnedId, CanonicalArtistId, CanonicalTrackId, CapabilityStatus, ChordriftAccountId,
        EvidenceCapabilities, EvidenceCapability, GuardrailKind, OrderingNarrative,
        RecipeRevisionIdentity, RecipeSection, RecipeSource, SourceLane, SpinId, SpinIdentity,
    },
    onboarding::ContentFingerprint,
    recipe_execution::{
        DraftTrackSelection, RecipeExecutionDraft, RecipeExecutionFingerprint,
        SourceExecutionReport,
    },
};

/// Inputs required to turn an unordered recipe draft into one exact Spin.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpinPreviewInput {
    /// Verified unordered output from V020-09.
    pub draft: RecipeExecutionDraft,
    /// Exact evidence capability snapshot used by the recipe inputs.
    pub capability_snapshot: EvidenceCapabilities,
    /// Unsigned deterministic ordering seed.
    pub seed: u64,
}

/// One selection predicate already enforced by V020-09.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionGuarantee {
    /// Track was present in the captured current inventory.
    CurrentInventory,
    /// Track had a usable provider-neutral recording identity.
    Playable,
    /// No durable explicit exclusion applied.
    NotExplicitlyExcluded,
    /// Every required hard collection boundary was satisfied.
    RequiredCollections,
    /// Canonical-track repetition capacity remained.
    TrackRepetitionBudget,
    /// Every credited canonical artist remained within budget.
    ArtistBudget,
}

/// Why one track entered the selected set.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrackSelectionReason {
    /// Reason document schema.
    pub schema_version: u16,
    /// Primary lifecycle lane assigned before allocation.
    pub lane: SourceLane,
    /// Immutable collection or evidence source that supplied the candidate.
    pub source: RecipeSource,
    /// Provider-neutral candidate priority used during selection.
    pub priority: u64,
    /// Canonical artists retained with the persisted selection explanation.
    pub artist_ids: Vec<CanonicalArtistId>,
    /// Source availability observed by recipe execution.
    pub source_status: CapabilityStatus,
    /// Source seats assigned by deterministic allocation.
    pub allocated_source_seats: u16,
    /// Selection predicates V020-09 proved for this track.
    pub guarantees: Vec<SelectionGuarantee>,
    /// Stable concise explanation for thin clients.
    pub summary: String,
}

/// Result of the recipe's artist-spacing ordering guardrail at one position.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtistSpacingOutcome {
    /// The recipe did not request artist spacing.
    NotRequested,
    /// No adjacent credited artist was repeated.
    Satisfied,
    /// Every remaining eligible choice repeated an adjacent artist.
    RelaxedNoAlternative,
}

/// Why one selected track occupies its exact playback position.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrackOrderingReason {
    /// Reason document schema.
    pub schema_version: u16,
    /// Recipe ordering narrative used for this position.
    pub narrative: OrderingNarrative,
    /// Narrative section assigned to this position, when sectioned.
    pub section: Option<RecipeSection>,
    /// Whether this position was reserved by familiarity cadence.
    pub familiarity_anchor: bool,
    /// Artist-spacing disposition at this position.
    pub artist_spacing: ArtistSpacingOutcome,
    /// Stable SHA-256 tie-breaker derived from seed, draft, and track identity.
    pub seeded_rank: String,
    /// Stable concise explanation for thin clients.
    pub summary: String,
}

/// One exact one-based entry in the immutable Spin preview.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpinTrackPreview {
    /// One-based playback position.
    pub position: u16,
    /// Account-owned canonical recording.
    pub track_id: AccountOwnedId<CanonicalTrackId>,
    /// Canonical artists retained for ordering explanation and verification.
    pub artist_ids: Vec<CanonicalArtistId>,
    /// Exact structured selection explanation.
    pub selection_reason: TrackSelectionReason,
    /// Exact structured ordering explanation.
    pub ordering_reason: TrackOrderingReason,
}

/// Planned and actually occupied positions for one narrative section.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpinSectionSummary {
    /// Narrative section.
    pub section: RecipeSection,
    /// Seats planned by V020-09 against the target size.
    pub planned_seats: u16,
    /// Exact tracks assigned to the section in this preview.
    pub assigned_tracks: u16,
}

/// Honest limitation retained alongside an otherwise usable preview.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "details", rename_all = "snake_case")]
pub enum SpinPreviewWarning {
    /// V020-09 could not fill every requested seat without weakening policy.
    UnfilledSeats(u16),
    /// Not every requested familiar-anchor position could be populated.
    FamiliarityCadenceUnsatisfied(Vec<u16>),
    /// Artist spacing was relaxed only where no alternative remained.
    ArtistSpacingRelaxed(Vec<u16>),
    /// Recipe v1 names a guardrail category but has no executable numeric policy.
    GuardrailPolicyUnavailable(GuardrailKind),
}

/// Exact immutable Spin preview returned to every client.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpinPreview {
    /// Deterministic account-owned Spin and immutable recipe identity.
    pub identity: SpinIdentity,
    /// Canonical V020-09 input fingerprint persisted with the Spin.
    pub input_fingerprint: ContentFingerprint,
    /// Verified unordered-draft fingerprint consumed by the orderer.
    pub draft_fingerprint: ContentFingerprint,
    /// Fingerprint of the complete ordered preview payload.
    pub preview_fingerprint: ContentFingerprint,
    /// Unsigned deterministic ordering seed.
    pub seed: u64,
    /// Exact capability snapshot retained with the preview.
    pub capability_snapshot: EvidenceCapabilities,
    /// Ordering narrative from the immutable recipe revision.
    pub ordering_narrative: OrderingNarrative,
    /// Requested recipe target before honest unfilled seats.
    pub target_tracks: u16,
    /// Seats V020-09 could not fill without weakening policy.
    pub unfilled_seats: u16,
    /// Exact one-based playback order and per-track reasons.
    pub tracks: Vec<SpinTrackPreview>,
    /// Narrative section capacity and exact occupancy.
    pub sections: Vec<SpinSectionSummary>,
    /// Explicit limitations or locally relaxed ordering constraints.
    pub warnings: Vec<SpinPreviewWarning>,
    /// Always true for a V020-10 Spin preview.
    pub playback_order_assigned: bool,
}

impl SpinPreview {
    /// Verifies the deterministic fingerprint of the complete preview payload.
    pub fn verify_fingerprint(&self) -> Result<bool, SpinPreviewError> {
        Ok(preview_fingerprint(self)? == self.preview_fingerprint)
    }
}

/// Failure from exact Spin preview generation, persistence, or loading.
#[derive(Debug)]
pub enum SpinPreviewError {
    /// Stable validation, ownership, compatibility, or state failure.
    Client(ClientError),
    /// Database or serialization infrastructure failed.
    Infrastructure(ChordriftError),
}

impl SpinPreviewError {
    /// Returns the client-safe representation without infrastructure details.
    #[must_use]
    pub fn client_error(&self) -> ClientError {
        match self {
            Self::Client(error) => *error,
            Self::Infrastructure(_) => ClientError::new(ErrorCode::Internal, false),
        }
    }
}

impl fmt::Display for SpinPreviewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client(error) => formatter.write_str(error.message()),
            Self::Infrastructure(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for SpinPreviewError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Client(_) => None,
            Self::Infrastructure(error) => Some(error),
        }
    }
}

impl From<ChordriftError> for SpinPreviewError {
    fn from(error: ChordriftError) -> Self {
        Self::Infrastructure(error)
    }
}

impl From<sqlx::Error> for SpinPreviewError {
    fn from(error: sqlx::Error) -> Self {
        Self::Infrastructure(ChordriftError::from(error))
    }
}

impl From<serde_json::Error> for SpinPreviewError {
    fn from(error: serde_json::Error) -> Self {
        Self::Infrastructure(ChordriftError::from(error))
    }
}

/// PostgreSQL-backed deterministic Spin preview boundary.
pub struct SpinPreviewBoundary<'database> {
    database: &'database Database,
}

impl<'database> SpinPreviewBoundary<'database> {
    /// Creates a boundary over the existing Chordrift database connection.
    #[must_use]
    pub const fn new(database: &'database Database) -> Self {
        Self { database }
    }

    /// Generates and persists one exact provider-free preview.
    ///
    /// Replaying the same account, canonical input fingerprint, and seed returns
    /// the existing byte-equivalent preview. A conflicting stored value fails
    /// rather than silently replacing immutable history.
    pub async fn create(
        &self,
        account_id: ChordriftAccountId,
        request: &CommandRequest,
        input: &SpinPreviewInput,
    ) -> Result<SpinPreview, SpinPreviewError> {
        validate_create_request(account_id, request, input)?;
        let preview = build_preview(account_id, input)?;
        self.persist(account_id, &preview).await
    }

    /// Reads one existing immutable preview through the SpinPreview query.
    pub async fn read(
        &self,
        account_id: ChordriftAccountId,
        request: &QueryRequest,
    ) -> Result<View<SpinPreview>, SpinPreviewError> {
        let spin_id = validate_read_request(request)?;
        let preview = self.load(account_id, spin_id).await?;
        Ok(View {
            contract_version: CONTRACT_VERSION,
            request_id: request.request_id,
            generated_at: Utc::now(),
            value: preview,
        })
    }

    async fn persist(
        &self,
        account_id: ChordriftAccountId,
        preview: &SpinPreview,
    ) -> Result<SpinPreview, SpinPreviewError> {
        let mut transaction = self.database.pool().begin().await?;
        let recipe_id: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT recipe_id FROM playlist_recipe_revisions
              WHERE chordrift_account_id = $1 AND id = $2 FOR SHARE",
        )
        .bind(account_id.as_uuid())
        .bind(preview.identity.recipe_revision().revision_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(recipe_id) = recipe_id else {
            return Err(client_error(ErrorCode::ResourceNotFound));
        };
        if recipe_id
            != preview
                .identity
                .recipe_revision()
                .recipe_id
                .into_resource_id()
                .as_uuid()
        {
            return Err(client_error(ErrorCode::StateConflict));
        }

        let track_ids: Vec<_> = preview
            .tracks
            .iter()
            .map(|track| track.track_id.into_resource_id().as_uuid())
            .collect();
        let existing_tracks: i64 =
            sqlx::query_scalar("SELECT count(*) FROM tracks WHERE id = ANY($1)")
                .bind(&track_ids)
                .fetch_one(&mut *transaction)
                .await?;
        if existing_tracks != i64::try_from(track_ids.len()).unwrap_or(i64::MAX) {
            return Err(client_error(ErrorCode::ResourceNotFound));
        }

        let manifest = PersistedSpinManifest::from_preview(preview);
        let capability_snapshot = serde_json::to_value(&manifest)?;
        let inserted = sqlx::query(
            "INSERT INTO playlist_spins
                 (id, chordrift_account_id, recipe_revision_id, input_fingerprint,
                  seed, capability_snapshot, status)
             VALUES ($1, $2, $3, $4, $5::numeric, $6, 'preview')
             ON CONFLICT (chordrift_account_id, input_fingerprint, seed) DO NOTHING",
        )
        .bind(preview.identity.spin_id().into_resource_id().as_uuid())
        .bind(account_id.as_uuid())
        .bind(preview.identity.recipe_revision().revision_id.as_uuid())
        .bind(preview.input_fingerprint.as_str())
        .bind(preview.seed.to_string())
        .bind(capability_snapshot)
        .execute(&mut *transaction)
        .await?;

        if inserted.rows_affected() == 1 {
            for track in &preview.tracks {
                sqlx::query(
                    "INSERT INTO playlist_spin_tracks
                         (spin_id, track_id, position, lane,
                          selection_reason, ordering_reason)
                     VALUES ($1, $2, $3, $4, $5, $6)",
                )
                .bind(preview.identity.spin_id().into_resource_id().as_uuid())
                .bind(track.track_id.into_resource_id().as_uuid())
                .bind(i32::from(track.position) - 1)
                .bind(lane_name(track.selection_reason.lane))
                .bind(serde_json::to_value(&track.selection_reason)?)
                .bind(serde_json::to_value(&track.ordering_reason)?)
                .execute(&mut *transaction)
                .await?;
            }
        }
        transaction.commit().await?;

        let stored = self
            .load_by_input(account_id, &preview.input_fingerprint, preview.seed)
            .await?;
        if stored != *preview {
            return Err(client_error(ErrorCode::StateConflict));
        }
        Ok(stored)
    }

    async fn load_by_input(
        &self,
        account_id: ChordriftAccountId,
        input_fingerprint: &ContentFingerprint,
        seed: u64,
    ) -> Result<SpinPreview, SpinPreviewError> {
        let spin_id: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT id FROM playlist_spins
              WHERE chordrift_account_id = $1 AND input_fingerprint = $2
                AND seed = $3::numeric",
        )
        .bind(account_id.as_uuid())
        .bind(input_fingerprint.as_str())
        .bind(seed.to_string())
        .fetch_optional(self.database.pool())
        .await?;
        let spin_id = spin_id.ok_or_else(|| client_error(ErrorCode::ResourceNotFound))?;
        self.load(account_id, SpinId::from_uuid(spin_id)).await
    }

    async fn load(
        &self,
        account_id: ChordriftAccountId,
        spin_id: SpinId,
    ) -> Result<SpinPreview, SpinPreviewError> {
        let row = sqlx::query(
            "SELECT spin.id, spin.input_fingerprint, spin.seed::text AS seed,
                    spin.capability_snapshot, spin.status,
                    revision.id AS recipe_revision_id, revision.recipe_id
               FROM playlist_spins spin
               JOIN playlist_recipe_revisions revision
                 ON revision.chordrift_account_id = spin.chordrift_account_id
                AND revision.id = spin.recipe_revision_id
              WHERE spin.chordrift_account_id = $1 AND spin.id = $2",
        )
        .bind(account_id.as_uuid())
        .bind(spin_id.as_uuid())
        .fetch_optional(self.database.pool())
        .await?;
        let Some(row) = row else {
            return Err(client_error(ErrorCode::ResourceNotFound));
        };
        if row.try_get::<String, _>("status")? != "preview" {
            return Err(client_error(ErrorCode::StateConflict));
        }
        let manifest: PersistedSpinManifest =
            serde_json::from_value(row.try_get::<Value, _>("capability_snapshot")?)?;
        if manifest.schema_version != 1 {
            return Err(client_error(ErrorCode::StateConflict));
        }
        let recipe_revision = RecipeRevisionIdentity {
            recipe_id: AccountOwnedId::new(
                account_id,
                crate::domain::RecipeId::from_uuid(row.try_get("recipe_id")?),
            ),
            revision_id: crate::domain::RecipeRevisionId::from_uuid(
                row.try_get("recipe_revision_id")?,
            ),
        };
        let identity = SpinIdentity::new(AccountOwnedId::new(account_id, spin_id), recipe_revision)
            .map_err(|_| client_error(ErrorCode::StateConflict))?;
        let input_fingerprint =
            ContentFingerprint::new(row.try_get::<String, _>("input_fingerprint")?)
                .map_err(|_| client_error(ErrorCode::StateConflict))?;
        let seed = row
            .try_get::<String, _>("seed")?
            .parse::<u64>()
            .map_err(|_| client_error(ErrorCode::StateConflict))?;

        let rows = sqlx::query(
            "SELECT track_id, position, lane, selection_reason, ordering_reason
               FROM playlist_spin_tracks WHERE spin_id = $1 ORDER BY position",
        )
        .bind(spin_id.as_uuid())
        .fetch_all(self.database.pool())
        .await?;
        let mut tracks = Vec::with_capacity(rows.len());
        for (expected, row) in rows.into_iter().enumerate() {
            let stored_position = row.try_get::<i32, _>("position")?;
            if stored_position != i32::try_from(expected).unwrap_or(i32::MAX) {
                return Err(client_error(ErrorCode::StateConflict));
            }
            let selection_reason: TrackSelectionReason =
                serde_json::from_value(row.try_get("selection_reason")?)?;
            let ordering_reason: TrackOrderingReason =
                serde_json::from_value(row.try_get("ordering_reason")?)?;
            if selection_reason.schema_version != 1
                || ordering_reason.schema_version != 1
                || ordering_reason.narrative != manifest.ordering_narrative
                || lane_name(selection_reason.lane) != row.try_get::<String, _>("lane")?
            {
                return Err(client_error(ErrorCode::StateConflict));
            }
            let artist_ids = selection_reason.artist_ids.clone();
            tracks.push(SpinTrackPreview {
                position: u16::try_from(expected + 1)
                    .map_err(|_| client_error(ErrorCode::StateConflict))?,
                track_id: AccountOwnedId::new(
                    account_id,
                    CanonicalTrackId::from_uuid(row.try_get("track_id")?),
                ),
                artist_ids,
                selection_reason,
                ordering_reason,
            });
        }

        let preview = SpinPreview {
            identity,
            input_fingerprint,
            draft_fingerprint: manifest.draft_fingerprint,
            preview_fingerprint: manifest.preview_fingerprint,
            seed,
            capability_snapshot: manifest.evidence_capabilities,
            ordering_narrative: manifest.ordering_narrative,
            target_tracks: manifest.target_tracks,
            unfilled_seats: manifest.unfilled_seats,
            tracks,
            sections: manifest.sections,
            warnings: manifest.warnings,
            playback_order_assigned: true,
        };
        if !preview.verify_fingerprint()? {
            return Err(client_error(ErrorCode::StateConflict));
        }
        Ok(preview)
    }

    /// Wraps preview creation for the shared application facade.
    #[must_use]
    pub const fn create_invocation<'request>(
        &'request self,
        account_id: ChordriftAccountId,
        request: &'request CommandRequest,
        input: &'request SpinPreviewInput,
    ) -> CreateSpinPreviewInvocation<'request, 'database> {
        CreateSpinPreviewInvocation {
            boundary: self,
            account_id,
            request,
            input,
        }
    }

    /// Wraps preview reading for the shared application facade.
    #[must_use]
    pub const fn read_invocation<'request>(
        &'request self,
        account_id: ChordriftAccountId,
        request: &'request QueryRequest,
    ) -> ReadSpinPreviewInvocation<'request, 'database> {
        ReadSpinPreviewInvocation {
            boundary: self,
            account_id,
            request,
        }
    }
}

/// One persisted preview command submitted through [`crate::application::ApplicationFacade`].
pub struct CreateSpinPreviewInvocation<'request, 'database> {
    boundary: &'request SpinPreviewBoundary<'database>,
    account_id: ChordriftAccountId,
    request: &'request CommandRequest,
    input: &'request SpinPreviewInput,
}

impl ApplicationInvocation for CreateSpinPreviewInvocation<'_, '_> {
    type Output = Result<SpinPreview, SpinPreviewError>;

    async fn execute(self) -> crate::Result<Self::Output> {
        Ok(self
            .boundary
            .create(self.account_id, self.request, self.input)
            .await)
    }
}

/// One persisted preview query submitted through [`crate::application::ApplicationFacade`].
pub struct ReadSpinPreviewInvocation<'request, 'database> {
    boundary: &'request SpinPreviewBoundary<'database>,
    account_id: ChordriftAccountId,
    request: &'request QueryRequest,
}

impl ApplicationInvocation for ReadSpinPreviewInvocation<'_, '_> {
    type Output = Result<View<SpinPreview>, SpinPreviewError>;

    async fn execute(self) -> crate::Result<Self::Output> {
        Ok(self.boundary.read(self.account_id, self.request).await)
    }
}

#[derive(Clone)]
struct RankedSelection {
    selection: DraftTrackSelection,
    seeded_rank: [u8; 32],
}

#[derive(Serialize)]
struct PreviewFingerprintPayload<'preview> {
    identity: SpinIdentity,
    input_fingerprint: &'preview ContentFingerprint,
    draft_fingerprint: &'preview ContentFingerprint,
    seed: u64,
    capability_snapshot: &'preview EvidenceCapabilities,
    ordering_narrative: OrderingNarrative,
    target_tracks: u16,
    unfilled_seats: u16,
    tracks: &'preview [SpinTrackPreview],
    sections: &'preview [SpinSectionSummary],
    warnings: &'preview [SpinPreviewWarning],
    playback_order_assigned: bool,
}

#[derive(Deserialize, Serialize)]
struct PersistedSpinManifest {
    schema_version: u16,
    evidence_capabilities: EvidenceCapabilities,
    draft_fingerprint: ContentFingerprint,
    preview_fingerprint: ContentFingerprint,
    ordering_narrative: OrderingNarrative,
    target_tracks: u16,
    unfilled_seats: u16,
    sections: Vec<SpinSectionSummary>,
    warnings: Vec<SpinPreviewWarning>,
}

impl PersistedSpinManifest {
    fn from_preview(preview: &SpinPreview) -> Self {
        Self {
            schema_version: 1,
            evidence_capabilities: preview.capability_snapshot.clone(),
            draft_fingerprint: preview.draft_fingerprint.clone(),
            preview_fingerprint: preview.preview_fingerprint.clone(),
            ordering_narrative: preview.ordering_narrative,
            target_tracks: preview.target_tracks,
            unfilled_seats: preview.unfilled_seats,
            sections: preview.sections.clone(),
            warnings: preview.warnings.clone(),
        }
    }
}

fn validate_create_request(
    account_id: ChordriftAccountId,
    request: &CommandRequest,
    input: &SpinPreviewInput,
) -> Result<(), SpinPreviewError> {
    if request.contract_version != CONTRACT_VERSION {
        return Err(client_error(ErrorCode::IncompatibleContract));
    }
    let Command::PreviewSpin { recipe_revision_id } = request.command else {
        return Err(client_error(ErrorCode::InvalidRequest));
    };
    if recipe_revision_id.as_uuid() != input.draft.recipe_revision.revision_id.as_uuid() {
        return Err(client_error(ErrorCode::StateConflict));
    }
    if input.draft.recipe_revision.recipe_id.account_id() != account_id
        || input
            .draft
            .selections
            .iter()
            .any(|selection| selection.track_id.account_id() != account_id)
    {
        return Err(client_error(ErrorCode::PermissionDenied));
    }
    if input.draft.playback_order_assigned
        || !input
            .draft
            .verify_fingerprint()
            .map_err(|_| client_error(ErrorCode::StateConflict))?
    {
        return Err(client_error(ErrorCode::StateConflict));
    }
    let mut track_ids = BTreeSet::new();
    if input
        .draft
        .selections
        .iter()
        .any(|selection| !track_ids.insert(selection.track_id.into_resource_id()))
    {
        return Err(client_error(ErrorCode::InvalidRequest));
    }
    for source in &input.draft.sources {
        if let RecipeSource::Evidence(capability) = source.source
            && source.status != input.capability_snapshot.status(capability)
        {
            return Err(client_error(ErrorCode::StateConflict));
        }
    }
    Ok(())
}

fn validate_read_request(request: &QueryRequest) -> Result<SpinId, SpinPreviewError> {
    if request.contract_version != CONTRACT_VERSION {
        return Err(client_error(ErrorCode::IncompatibleContract));
    }
    let Query::SpinPreview { spin_id } = request.query else {
        return Err(client_error(ErrorCode::InvalidRequest));
    };
    Ok(SpinId::from_uuid(spin_id.as_uuid()))
}

fn build_preview(
    account_id: ChordriftAccountId,
    input: &SpinPreviewInput,
) -> Result<SpinPreview, SpinPreviewError> {
    let input_fingerprint = content_fingerprint(&input.draft.input_fingerprint)?;
    let draft_fingerprint = content_fingerprint(&input.draft.draft_fingerprint)?;
    let identity = SpinIdentity::new(
        AccountOwnedId::new(
            account_id,
            deterministic_spin_id(account_id, &input_fingerprint, input.seed),
        ),
        input.draft.recipe_revision,
    )
    .map_err(|_| client_error(ErrorCode::PermissionDenied))?;
    let artist_spacing_requested = input
        .draft
        .guardrails
        .iter()
        .any(|guardrail| guardrail.kind == GuardrailKind::ArtistSpacing);
    let mut remaining: Vec<_> = input
        .draft
        .selections
        .iter()
        .cloned()
        .map(|selection| RankedSelection {
            seeded_rank: seeded_rank(input.seed, &draft_fingerprint, selection.track_id),
            selection,
        })
        .collect();
    let mut ordered: Vec<SpinTrackPreview> = Vec::with_capacity(remaining.len());
    let mut relaxed_artist_positions = Vec::new();
    let mut unmet_anchor_positions = Vec::new();
    let total_positions =
        u16::try_from(remaining.len()).map_err(|_| client_error(ErrorCode::InvalidRequest))?;

    for position in 1..=total_positions {
        let anchor_required = input
            .draft
            .familiarity_cadence
            .anchor_positions
            .binary_search(&position)
            .is_ok();
        let future_anchors = input
            .draft
            .familiarity_cadence
            .anchor_positions
            .iter()
            .filter(|anchor| **anchor > position && **anchor <= total_positions)
            .count();
        let familiar_remaining = remaining
            .iter()
            .filter(|item| is_familiar_lane(item.selection.lane))
            .count();
        let section = section_for_position(&input.draft, position);
        let previous = ordered.last();

        let mut eligible: Vec<usize> = (0..remaining.len()).collect();
        if anchor_required {
            let familiar: Vec<_> = eligible
                .iter()
                .copied()
                .filter(|index| is_familiar_lane(remaining[*index].selection.lane))
                .collect();
            if familiar.is_empty() {
                unmet_anchor_positions.push(position);
            } else {
                eligible = familiar;
            }
        } else if familiar_remaining <= future_anchors {
            let nonfamiliar: Vec<_> = eligible
                .iter()
                .copied()
                .filter(|index| !is_familiar_lane(remaining[*index].selection.lane))
                .collect();
            if !nonfamiliar.is_empty() {
                eligible = nonfamiliar;
            }
        }

        let mut spacing_outcome = if artist_spacing_requested {
            ArtistSpacingOutcome::Satisfied
        } else {
            ArtistSpacingOutcome::NotRequested
        };
        if artist_spacing_requested && let Some(previous) = previous {
            let spaced: Vec<_> = eligible
                .iter()
                .copied()
                .filter(|index| {
                    !shares_artist(
                        &previous.artist_ids,
                        &remaining[*index].selection.artist_ids,
                    )
                })
                .collect();
            if spaced.is_empty() {
                spacing_outcome = ArtistSpacingOutcome::RelaxedNoAlternative;
                relaxed_artist_positions.push(position);
            } else {
                eligible = spaced;
            }
        }

        let selected_index = eligible
            .into_iter()
            .min_by(|left, right| {
                ordering_key(
                    &remaining[*left],
                    previous,
                    input.draft.ordering_narrative,
                    section,
                )
                .cmp(&ordering_key(
                    &remaining[*right],
                    previous,
                    input.draft.ordering_narrative,
                    section,
                ))
            })
            .ok_or_else(|| client_error(ErrorCode::StateConflict))?;
        let selected = remaining.remove(selected_index);
        let source = source_report(&input.draft, &selected.selection)?;
        ordered.push(SpinTrackPreview {
            position,
            track_id: selected.selection.track_id,
            artist_ids: selected.selection.artist_ids.clone(),
            selection_reason: TrackSelectionReason {
                schema_version: 1,
                lane: selected.selection.lane,
                source: selected.selection.source.clone(),
                priority: selected.selection.priority,
                artist_ids: selected.selection.artist_ids.clone(),
                source_status: source.status,
                allocated_source_seats: source.allocated_seats,
                guarantees: vec![
                    SelectionGuarantee::CurrentInventory,
                    SelectionGuarantee::Playable,
                    SelectionGuarantee::NotExplicitlyExcluded,
                    SelectionGuarantee::RequiredCollections,
                    SelectionGuarantee::TrackRepetitionBudget,
                    SelectionGuarantee::ArtistBudget,
                ],
                summary: selection_summary(&selected.selection),
            },
            ordering_reason: TrackOrderingReason {
                schema_version: 1,
                narrative: input.draft.ordering_narrative,
                section,
                familiarity_anchor: anchor_required && is_familiar_lane(selected.selection.lane),
                artist_spacing: spacing_outcome,
                seeded_rank: hex_bytes(&selected.seeded_rank),
                summary: ordering_summary(
                    input.draft.ordering_narrative,
                    section,
                    anchor_required && is_familiar_lane(selected.selection.lane),
                    spacing_outcome,
                ),
            },
        });
    }
    for anchor in &input.draft.familiarity_cadence.anchor_positions {
        if *anchor > total_positions {
            unmet_anchor_positions.push(*anchor);
        }
    }
    unmet_anchor_positions.sort_unstable();
    unmet_anchor_positions.dedup();

    let sections = section_summaries(&input.draft, &ordered);
    let warnings = warnings(
        &input.draft,
        unmet_anchor_positions,
        relaxed_artist_positions,
    );
    let mut preview = SpinPreview {
        identity,
        input_fingerprint,
        draft_fingerprint,
        preview_fingerprint: ContentFingerprint::new("0".repeat(64))
            .expect("placeholder fingerprint is valid"),
        seed: input.seed,
        capability_snapshot: input.capability_snapshot.clone(),
        ordering_narrative: input.draft.ordering_narrative,
        target_tracks: input.draft.target_tracks,
        unfilled_seats: input.draft.unfilled_seats,
        tracks: ordered,
        sections,
        warnings,
        playback_order_assigned: true,
    };
    preview.preview_fingerprint = preview_fingerprint(&preview)?;
    Ok(preview)
}

fn ordering_key(
    candidate: &RankedSelection,
    previous: Option<&SpinTrackPreview>,
    narrative: OrderingNarrative,
    section: Option<RecipeSection>,
) -> (u8, Reverse<u8>, [u8; 32], AccountOwnedId<CanonicalTrackId>) {
    let previous_lane = previous.map(|track| track.selection_reason.lane);
    let distance = previous_lane.map_or(0, |lane| lane_distance(lane, candidate.selection.lane));
    let (primary, contrast) = match narrative {
        OrderingNarrative::Shuffle => (0, Reverse(0)),
        OrderingNarrative::SmoothTransitions => (distance, Reverse(0)),
        OrderingNarrative::IntentionalContrast => (0, Reverse(distance)),
        OrderingNarrative::SectionedJourney => (
            section.map_or(0, |section| {
                section_lane_rank(section, candidate.selection.lane)
            }),
            Reverse(0),
        ),
    };
    (
        primary,
        contrast,
        candidate.seeded_rank,
        candidate.selection.track_id,
    )
}

fn source_report<'draft>(
    draft: &'draft RecipeExecutionDraft,
    selection: &DraftTrackSelection,
) -> Result<&'draft SourceExecutionReport, SpinPreviewError> {
    draft
        .sources
        .iter()
        .find(|source| source.lane == selection.lane && source.source == selection.source)
        .ok_or_else(|| client_error(ErrorCode::StateConflict))
}

fn section_for_position(draft: &RecipeExecutionDraft, position: u16) -> Option<RecipeSection> {
    let mut end = 0_u16;
    for section in &draft.narrative_sections {
        end = end.saturating_add(section.seats);
        if position <= end {
            return Some(section.section);
        }
    }
    None
}

fn section_summaries(
    draft: &RecipeExecutionDraft,
    tracks: &[SpinTrackPreview],
) -> Vec<SpinSectionSummary> {
    draft
        .narrative_sections
        .iter()
        .map(|planned| SpinSectionSummary {
            section: planned.section,
            planned_seats: planned.seats,
            assigned_tracks: u16::try_from(
                tracks
                    .iter()
                    .filter(|track| track.ordering_reason.section == Some(planned.section))
                    .count(),
            )
            .unwrap_or(u16::MAX),
        })
        .collect()
}

fn warnings(
    draft: &RecipeExecutionDraft,
    unmet_anchors: Vec<u16>,
    relaxed_artist_positions: Vec<u16>,
) -> Vec<SpinPreviewWarning> {
    let mut warnings = Vec::new();
    if draft.unfilled_seats > 0 {
        warnings.push(SpinPreviewWarning::UnfilledSeats(draft.unfilled_seats));
    }
    if !unmet_anchors.is_empty() {
        warnings.push(SpinPreviewWarning::FamiliarityCadenceUnsatisfied(
            unmet_anchors,
        ));
    }
    if !relaxed_artist_positions.is_empty() {
        warnings.push(SpinPreviewWarning::ArtistSpacingRelaxed(
            relaxed_artist_positions,
        ));
    }
    for guardrail in &draft.guardrails {
        if matches!(
            guardrail.kind,
            GuardrailKind::Duration | GuardrailKind::CrossOutputReuse
        ) {
            warnings.push(SpinPreviewWarning::GuardrailPolicyUnavailable(
                guardrail.kind,
            ));
        }
    }
    warnings
}

fn preview_fingerprint(preview: &SpinPreview) -> Result<ContentFingerprint, SpinPreviewError> {
    let payload = PreviewFingerprintPayload {
        identity: preview.identity,
        input_fingerprint: &preview.input_fingerprint,
        draft_fingerprint: &preview.draft_fingerprint,
        seed: preview.seed,
        capability_snapshot: &preview.capability_snapshot,
        ordering_narrative: preview.ordering_narrative,
        target_tracks: preview.target_tracks,
        unfilled_seats: preview.unfilled_seats,
        tracks: &preview.tracks,
        sections: &preview.sections,
        warnings: &preview.warnings,
        playback_order_assigned: preview.playback_order_assigned,
    };
    let bytes = serde_json::to_vec(&payload)?;
    ContentFingerprint::new(hex_bytes(&Sha256::digest(bytes)))
        .map_err(|_| client_error(ErrorCode::StateConflict))
}

fn deterministic_spin_id(
    account_id: ChordriftAccountId,
    input_fingerprint: &ContentFingerprint,
    seed: u64,
) -> SpinId {
    let mut hasher = Sha256::new();
    hasher.update(b"chordrift-spin-v1\0");
    hasher.update(account_id.as_uuid().as_bytes());
    hasher.update(input_fingerprint.as_str().as_bytes());
    hasher.update(seed.to_be_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    SpinId::from_uuid(uuid::Uuid::from_bytes(bytes))
}

fn seeded_rank(
    seed: u64,
    draft_fingerprint: &ContentFingerprint,
    track_id: AccountOwnedId<CanonicalTrackId>,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"chordrift-spin-order-v1\0");
    hasher.update(seed.to_be_bytes());
    hasher.update(draft_fingerprint.as_str().as_bytes());
    hasher.update(track_id.into_resource_id().as_uuid().as_bytes());
    hasher.finalize().into()
}

fn content_fingerprint(
    fingerprint: &RecipeExecutionFingerprint,
) -> Result<ContentFingerprint, SpinPreviewError> {
    ContentFingerprint::new(fingerprint.as_str())
        .map_err(|_| client_error(ErrorCode::StateConflict))
}

fn selection_summary(selection: &DraftTrackSelection) -> String {
    format!(
        "Selected from the {} lane through {}.",
        lane_name(selection.lane).replace('_', " "),
        source_name(&selection.source)
    )
}

fn ordering_summary(
    narrative: OrderingNarrative,
    section: Option<RecipeSection>,
    anchor: bool,
    spacing: ArtistSpacingOutcome,
) -> String {
    if anchor {
        return "Placed here to satisfy the familiar-anchor cadence.".to_owned();
    }
    if spacing == ArtistSpacingOutcome::RelaxedNoAlternative {
        return "Placed deterministically; artist spacing had no remaining alternative.".to_owned();
    }
    match (narrative, section) {
        (OrderingNarrative::SectionedJourney, Some(section)) => format!(
            "Placed in the {} section by deterministic narrative ranking.",
            section_name(section)
        ),
        (OrderingNarrative::Shuffle, _) => "Placed by the deterministic seeded shuffle.".to_owned(),
        (OrderingNarrative::SmoothTransitions, _) => {
            "Placed to keep adjacent lifecycle lanes close when possible.".to_owned()
        }
        (OrderingNarrative::IntentionalContrast, _) => {
            "Placed to contrast the preceding lifecycle lane when possible.".to_owned()
        }
        (OrderingNarrative::SectionedJourney, None) => {
            "Placed by deterministic section overflow ordering.".to_owned()
        }
    }
}

fn source_name(source: &RecipeSource) -> &'static str {
    match source {
        RecipeSource::Collection(_) => "an explicit collection source",
        RecipeSource::Evidence(EvidenceCapability::CurrentInventory) => {
            "current-inventory evidence"
        }
        RecipeSource::Evidence(EvidenceCapability::SavedAt) => "saved-at evidence",
        RecipeSource::Evidence(EvidenceCapability::RecentPlayback) => "recent-playback evidence",
        RecipeSource::Evidence(EvidenceCapability::ExtendedPlaybackHistory) => {
            "extended-playback-history evidence"
        }
        RecipeSource::Evidence(EvidenceCapability::Completion) => "completion evidence",
        RecipeSource::Evidence(EvidenceCapability::Skips) => "skip evidence",
        RecipeSource::Evidence(EvidenceCapability::LifetimeRotation) => {
            "lifetime-rotation evidence"
        }
    }
}

const fn lane_name(lane: SourceLane) -> &'static str {
    match lane {
        SourceLane::Discovery => "discovery",
        SourceLane::Emerging => "emerging",
        SourceLane::Familiar => "familiar",
        SourceLane::HighRotation => "high_rotation",
        SourceLane::Dormant => "dormant",
        SourceLane::Recovery => "recovery",
    }
}

const fn section_name(section: RecipeSection) -> &'static str {
    match section {
        RecipeSection::WarmUp => "warm-up",
        RecipeSection::Focus => "focus",
        RecipeSection::Landing => "landing",
    }
}

const fn lane_rank(lane: SourceLane) -> u8 {
    match lane {
        SourceLane::Discovery => 0,
        SourceLane::Emerging => 1,
        SourceLane::Familiar => 2,
        SourceLane::HighRotation => 3,
        SourceLane::Dormant => 4,
        SourceLane::Recovery => 5,
    }
}

fn lane_distance(left: SourceLane, right: SourceLane) -> u8 {
    lane_rank(left).abs_diff(lane_rank(right))
}

const fn section_lane_rank(section: RecipeSection, lane: SourceLane) -> u8 {
    match section {
        RecipeSection::WarmUp => match lane {
            SourceLane::Familiar => 0,
            SourceLane::Emerging => 1,
            SourceLane::Dormant => 2,
            SourceLane::Discovery => 3,
            SourceLane::Recovery => 4,
            SourceLane::HighRotation => 5,
        },
        RecipeSection::Focus => match lane {
            SourceLane::Discovery => 0,
            SourceLane::Emerging => 1,
            SourceLane::HighRotation => 2,
            SourceLane::Familiar => 3,
            SourceLane::Recovery => 4,
            SourceLane::Dormant => 5,
        },
        RecipeSection::Landing => match lane {
            SourceLane::Familiar => 0,
            SourceLane::Dormant => 1,
            SourceLane::Recovery => 2,
            SourceLane::Emerging => 3,
            SourceLane::Discovery => 4,
            SourceLane::HighRotation => 5,
        },
    }
}

fn shares_artist(left: &[CanonicalArtistId], right: &[CanonicalArtistId]) -> bool {
    left.iter()
        .any(|artist| right.binary_search(artist).is_ok())
}

const fn is_familiar_lane(lane: SourceLane) -> bool {
    matches!(lane, SourceLane::Familiar | SourceLane::HighRotation)
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn client_error(code: ErrorCode) -> SpinPreviewError {
    SpinPreviewError::Client(ClientError::new(code, false))
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, num::NonZeroU16};

    use super::*;
    use crate::{
        contract::{IdempotencyKey, RequestId, ResourceId},
        domain::{AllocationWeight, RecipeId, RecipeRevisionId, SourceAllocation},
        recipe_execution::{
            CandidateEligibility, RecipeCandidate, RecipeExecutionRequest, RecipeExecutor,
            SelectionBudgets,
        },
    };

    fn recipe(account: ChordriftAccountId, ordering: OrderingNarrative) -> crate::domain::RecipeV1 {
        crate::domain::RecipeV1::new(
            RecipeRevisionIdentity {
                recipe_id: AccountOwnedId::new(
                    account,
                    RecipeId::from_uuid(uuid::Uuid::from_u128(100)),
                ),
                revision_id: RecipeRevisionId::from_uuid(uuid::Uuid::from_u128(101)),
            },
            vec![
                SourceAllocation {
                    lane: SourceLane::Discovery,
                    source: RecipeSource::Evidence(EvidenceCapability::SavedAt),
                    weight: AllocationWeight::new(2),
                },
                SourceAllocation {
                    lane: SourceLane::Familiar,
                    source: RecipeSource::Evidence(EvidenceCapability::RecentPlayback),
                    weight: AllocationWeight::new(1),
                },
                SourceAllocation {
                    lane: SourceLane::Dormant,
                    source: RecipeSource::Evidence(EvidenceCapability::LifetimeRotation),
                    weight: AllocationWeight::new(1),
                },
            ],
            crate::domain::FamiliarityCadence::Every(NonZeroU16::new(3).expect("nonzero")),
            ordering,
            if ordering == OrderingNarrative::SectionedJourney {
                vec![
                    RecipeSection::WarmUp,
                    RecipeSection::Focus,
                    RecipeSection::Landing,
                ]
            } else {
                Vec::new()
            },
            vec![
                GuardrailKind::HardBoundaries,
                GuardrailKind::ArtistRepetition,
                GuardrailKind::ArtistSpacing,
                GuardrailKind::Duration,
                GuardrailKind::CrossOutputReuse,
            ],
        )
        .expect("fixture recipe is valid")
    }

    fn candidate(
        account: ChordriftAccountId,
        track: u128,
        artist: u128,
        lane: SourceLane,
        capability: EvidenceCapability,
        priority: u64,
    ) -> RecipeCandidate {
        RecipeCandidate::new(
            AccountOwnedId::new(
                account,
                CanonicalTrackId::from_uuid(uuid::Uuid::from_u128(track)),
            ),
            vec![CanonicalArtistId::from_uuid(uuid::Uuid::from_u128(artist))],
            lane,
            RecipeSource::Evidence(capability),
            priority,
            CandidateEligibility {
                in_current_inventory: true,
                playable: true,
                explicitly_excluded: false,
            },
            Vec::new(),
        )
        .expect("fixture candidate is valid")
    }

    fn capabilities() -> EvidenceCapabilities {
        EvidenceCapabilities::new(BTreeMap::from([
            (EvidenceCapability::SavedAt, CapabilityStatus::Available),
            (
                EvidenceCapability::RecentPlayback,
                CapabilityStatus::Degraded,
            ),
            (
                EvidenceCapability::LifetimeRotation,
                CapabilityStatus::Available,
            ),
        ]))
    }

    fn input(account: ChordriftAccountId, ordering: OrderingNarrative) -> SpinPreviewInput {
        let candidates = vec![
            candidate(
                account,
                1,
                1,
                SourceLane::Discovery,
                EvidenceCapability::SavedAt,
                80,
            ),
            candidate(
                account,
                2,
                1,
                SourceLane::Discovery,
                EvidenceCapability::SavedAt,
                70,
            ),
            candidate(
                account,
                3,
                2,
                SourceLane::Discovery,
                EvidenceCapability::SavedAt,
                60,
            ),
            candidate(
                account,
                4,
                3,
                SourceLane::Discovery,
                EvidenceCapability::SavedAt,
                50,
            ),
            candidate(
                account,
                5,
                1,
                SourceLane::Familiar,
                EvidenceCapability::RecentPlayback,
                40,
            ),
            candidate(
                account,
                6,
                4,
                SourceLane::Familiar,
                EvidenceCapability::RecentPlayback,
                30,
            ),
            candidate(
                account,
                7,
                2,
                SourceLane::Dormant,
                EvidenceCapability::LifetimeRotation,
                20,
            ),
            candidate(
                account,
                8,
                5,
                SourceLane::Dormant,
                EvidenceCapability::LifetimeRotation,
                10,
            ),
        ];
        let capability_snapshot = capabilities();
        let request = RecipeExecutionRequest::new(
            recipe(account, ordering),
            NonZeroU16::new(8).expect("nonzero"),
            candidates,
            capability_snapshot.clone(),
            Vec::new(),
            SelectionBudgets {
                max_occurrences_per_track: NonZeroU16::new(1).expect("nonzero"),
                max_tracks_per_artist: NonZeroU16::new(3).expect("nonzero"),
            },
        )
        .expect("fixture request is valid");
        SpinPreviewInput {
            draft: RecipeExecutor::new()
                .execute(&request)
                .expect("fixture recipe executes"),
            capability_snapshot,
            seed: 42,
        }
    }

    fn command(input: &SpinPreviewInput) -> CommandRequest {
        CommandRequest {
            contract_version: CONTRACT_VERSION,
            request_id: RequestId::new(),
            idempotency_key: IdempotencyKey::new(),
            command: Command::PreviewSpin {
                recipe_revision_id: ResourceId::from_uuid(
                    input.draft.recipe_revision.revision_id.as_uuid(),
                ),
            },
        }
    }

    #[test]
    fn identical_inputs_produce_the_same_exact_order_identity_and_reasons() {
        let account = ChordriftAccountId::from_uuid(uuid::Uuid::from_u128(10));
        let input = input(account, OrderingNarrative::SectionedJourney);
        validate_create_request(account, &command(&input), &input).expect("input is accepted");

        let first = build_preview(account, &input).expect("preview builds");
        let second = build_preview(account, &input).expect("preview replays");

        assert_eq!(first, second);
        assert!(first.verify_fingerprint().expect("fingerprint verifies"));
        assert!(first.playback_order_assigned);
        assert_eq!(first.tracks.len(), 8);
        assert_eq!(
            first
                .tracks
                .iter()
                .map(|track| track.position)
                .collect::<Vec<_>>(),
            (1_u16..=8).collect::<Vec<_>>()
        );
        for anchor in [3_u16, 6] {
            let track = &first.tracks[usize::from(anchor - 1)];
            assert!(is_familiar_lane(track.selection_reason.lane));
            assert!(track.ordering_reason.familiarity_anchor);
        }
        assert_eq!(
            first
                .sections
                .iter()
                .map(|section| section.assigned_tracks)
                .sum::<u16>(),
            8
        );
        assert!(first.tracks.iter().all(|track| {
            !track.selection_reason.summary.is_empty()
                && !track.ordering_reason.summary.is_empty()
                && track.ordering_reason.seeded_rank.len() == 64
        }));
        for pair in first.tracks.windows(2) {
            if pair[1].ordering_reason.artist_spacing == ArtistSpacingOutcome::Satisfied {
                assert!(!shares_artist(&pair[0].artist_ids, &pair[1].artist_ids));
            }
        }
    }

    #[test]
    fn seed_changes_identity_and_tie_breakers_without_changing_selection() {
        let account = ChordriftAccountId::from_uuid(uuid::Uuid::from_u128(11));
        let first_input = input(account, OrderingNarrative::Shuffle);
        let mut second_input = first_input.clone();
        second_input.seed += 1;

        let first = build_preview(account, &first_input).expect("preview builds");
        let second = build_preview(account, &second_input).expect("preview builds");

        assert_ne!(first.identity.spin_id(), second.identity.spin_id());
        assert_ne!(first.preview_fingerprint, second.preview_fingerprint);
        assert_eq!(
            first
                .tracks
                .iter()
                .map(|track| track.track_id)
                .collect::<BTreeSet<_>>(),
            second
                .tracks
                .iter()
                .map(|track| track.track_id)
                .collect::<BTreeSet<_>>()
        );
    }

    #[test]
    fn mutated_draft_capability_or_owner_fails_before_persistence() {
        let account = ChordriftAccountId::from_uuid(uuid::Uuid::from_u128(12));
        let input = input(account, OrderingNarrative::SmoothTransitions);
        let mut mutated = input.clone();
        mutated.draft.target_tracks += 1;
        assert_eq!(
            validate_create_request(account, &command(&mutated), &mutated)
                .expect_err("mutated fingerprint fails")
                .client_error()
                .code,
            ErrorCode::StateConflict
        );

        let mut capability_mismatch = input.clone();
        capability_mismatch.capability_snapshot = EvidenceCapabilities::default();
        assert_eq!(
            validate_create_request(
                account,
                &command(&capability_mismatch),
                &capability_mismatch,
            )
            .expect_err("capability mismatch fails")
            .client_error()
            .code,
            ErrorCode::StateConflict
        );

        assert_eq!(
            validate_create_request(ChordriftAccountId::new(), &command(&input), &input)
                .expect_err("cross-account input fails")
                .client_error()
                .code,
            ErrorCode::PermissionDenied
        );
    }

    #[test]
    fn unfilled_seats_and_unreachable_cadence_positions_remain_visible() {
        let account = ChordriftAccountId::from_uuid(uuid::Uuid::from_u128(14));
        let capability_snapshot = capabilities();
        let request = RecipeExecutionRequest::new(
            recipe(account, OrderingNarrative::SectionedJourney),
            NonZeroU16::new(6).expect("nonzero"),
            vec![
                candidate(
                    account,
                    1,
                    1,
                    SourceLane::Discovery,
                    EvidenceCapability::SavedAt,
                    5,
                ),
                candidate(
                    account,
                    2,
                    2,
                    SourceLane::Discovery,
                    EvidenceCapability::SavedAt,
                    4,
                ),
                candidate(
                    account,
                    3,
                    3,
                    SourceLane::Discovery,
                    EvidenceCapability::SavedAt,
                    3,
                ),
                candidate(
                    account,
                    4,
                    4,
                    SourceLane::Familiar,
                    EvidenceCapability::RecentPlayback,
                    2,
                ),
                candidate(
                    account,
                    5,
                    5,
                    SourceLane::Dormant,
                    EvidenceCapability::LifetimeRotation,
                    1,
                ),
            ],
            capability_snapshot.clone(),
            Vec::new(),
            SelectionBudgets {
                max_occurrences_per_track: NonZeroU16::new(1).expect("nonzero"),
                max_tracks_per_artist: NonZeroU16::new(2).expect("nonzero"),
            },
        )
        .expect("fixture request is valid");
        let input = SpinPreviewInput {
            draft: RecipeExecutor::new()
                .execute(&request)
                .expect("fixture recipe executes"),
            capability_snapshot,
            seed: 7,
        };

        let preview = build_preview(account, &input).expect("preview builds honestly");

        assert_eq!(preview.tracks.len(), 5);
        assert_eq!(preview.unfilled_seats, 1);
        assert!(
            preview
                .warnings
                .contains(&SpinPreviewWarning::UnfilledSeats(1))
        );
        assert!(
            preview
                .warnings
                .contains(&SpinPreviewWarning::FamiliarityCadenceUnsatisfied(vec![6]))
        );
    }

    #[test]
    fn unavailable_numeric_guardrails_remain_visible() {
        let account = ChordriftAccountId::from_uuid(uuid::Uuid::from_u128(13));
        let input = input(account, OrderingNarrative::IntentionalContrast);

        let preview = build_preview(account, &input).expect("preview builds");

        assert!(
            preview
                .warnings
                .contains(&SpinPreviewWarning::GuardrailPolicyUnavailable(
                    GuardrailKind::Duration
                ))
        );
        assert!(
            preview
                .warnings
                .contains(&SpinPreviewWarning::GuardrailPolicyUnavailable(
                    GuardrailKind::CrossOutputReuse
                ))
        );
    }
}
