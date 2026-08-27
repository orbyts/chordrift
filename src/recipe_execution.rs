//! Provider-neutral Discovery + Rediscovery recipe execution.
//!
//! This boundary produces an unordered, deterministic selection draft. It
//! applies eligibility, hard boundaries, weighted source allocation, track and
//! artist budgets, familiar-anchor capacity, and narrative section sizes. It
//! deliberately does not assign playback order, persist a Spin, or expose a
//! provider mutation.

use std::{cmp::Reverse, collections::BTreeMap, fmt, future, num::NonZeroU16};

use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    application::ApplicationInvocation,
    contract::{ClientError, ErrorCode},
    domain::{
        AccountOwnedId, AllocationWeight, CanonicalArtistId, CanonicalTrackId, CapabilityStatus,
        CollectionId, EvidenceCapabilities, GuardrailKind, OrderingNarrative,
        RecipeRevisionIdentity, RecipeSection, RecipeSource, RecipeV1, SourceLane,
    },
};

/// Stable SHA-256 identity for recipe inputs or an unordered selection draft.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RecipeExecutionFingerprint(String);

impl RecipeExecutionFingerprint {
    fn from_serializable<T: Serialize>(value: &T) -> Result<Self, RecipeExecutionError> {
        let bytes = serde_json::to_vec(value).map_err(|_| RecipeExecutionError::Serialization)?;
        Ok(Self(format!("{:x}", Sha256::digest(bytes))))
    }

    /// Returns the lowercase SHA-256 value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for RecipeExecutionFingerprint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(D::Error::custom(
                "recipe fingerprint must be lowercase SHA-256",
            ));
        }
        Ok(Self(value))
    }
}

/// Eligibility facts resolved before provider-neutral recipe execution.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CandidateEligibility {
    /// Track belongs to the captured current inventory.
    pub in_current_inventory: bool,
    /// Track has a usable provider-neutral recording identity.
    pub playable: bool,
    /// Durable user intent explicitly excludes this track.
    pub explicitly_excluded: bool,
}

/// One candidate supplied by exactly one immutable lane/source dependency.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RecipeCandidate {
    track_id: AccountOwnedId<CanonicalTrackId>,
    artist_ids: Vec<CanonicalArtistId>,
    lane: SourceLane,
    source: RecipeSource,
    priority: u64,
    eligibility: CandidateEligibility,
    collection_memberships: Vec<AccountOwnedId<CollectionId>>,
}

impl RecipeCandidate {
    /// Creates one canonical candidate and normalizes identity sets.
    pub fn new(
        track_id: AccountOwnedId<CanonicalTrackId>,
        mut artist_ids: Vec<CanonicalArtistId>,
        lane: SourceLane,
        source: RecipeSource,
        priority: u64,
        eligibility: CandidateEligibility,
        mut collection_memberships: Vec<AccountOwnedId<CollectionId>>,
    ) -> Result<Self, RecipeExecutionError> {
        if collection_memberships
            .iter()
            .any(|collection| collection.account_id() != track_id.account_id())
            || matches!(
                &source,
                RecipeSource::Collection(collection)
                    if collection.account_id() != track_id.account_id()
            )
        {
            return Err(RecipeExecutionError::OwnershipMismatch);
        }
        artist_ids.sort_unstable();
        artist_ids.dedup();
        collection_memberships.sort_unstable();
        collection_memberships.dedup();
        Ok(Self {
            track_id,
            artist_ids,
            lane,
            source,
            priority,
            eligibility,
            collection_memberships,
        })
    }

    /// Returns the account-owned canonical track.
    #[must_use]
    pub const fn track_id(&self) -> AccountOwnedId<CanonicalTrackId> {
        self.track_id
    }
}

#[derive(Deserialize)]
struct RawRecipeCandidate {
    track_id: AccountOwnedId<CanonicalTrackId>,
    artist_ids: Vec<CanonicalArtistId>,
    lane: SourceLane,
    source: RecipeSource,
    priority: u64,
    eligibility: CandidateEligibility,
    collection_memberships: Vec<AccountOwnedId<CollectionId>>,
}

impl<'de> Deserialize<'de> for RecipeCandidate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawRecipeCandidate::deserialize(deserializer)?;
        Self::new(
            raw.track_id,
            raw.artist_ids,
            raw.lane,
            raw.source,
            raw.priority,
            raw.eligibility,
            raw.collection_memberships,
        )
        .map_err(D::Error::custom)
    }
}

/// Selection-stage repetition and artist limits.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SelectionBudgets {
    /// Maximum selected occurrences of one canonical track.
    pub max_occurrences_per_track: NonZeroU16,
    /// Maximum selected entries credited to one canonical artist.
    pub max_tracks_per_artist: NonZeroU16,
}

/// Immutable provider-neutral inputs to one recipe execution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RecipeExecutionRequest {
    recipe: RecipeV1,
    target_tracks: NonZeroU16,
    candidates: Vec<RecipeCandidate>,
    evidence_capabilities: EvidenceCapabilities,
    required_collections: Vec<AccountOwnedId<CollectionId>>,
    budgets: SelectionBudgets,
}

impl RecipeExecutionRequest {
    /// Validates ownership and canonicalizes set-like inputs.
    pub fn new(
        recipe: RecipeV1,
        target_tracks: NonZeroU16,
        mut candidates: Vec<RecipeCandidate>,
        evidence_capabilities: EvidenceCapabilities,
        mut required_collections: Vec<AccountOwnedId<CollectionId>>,
        budgets: SelectionBudgets,
    ) -> Result<Self, RecipeExecutionError> {
        let account_id = recipe.identity().recipe_id.account_id();
        if candidates
            .iter()
            .any(|candidate| candidate.track_id.account_id() != account_id)
            || required_collections
                .iter()
                .any(|collection| collection.account_id() != account_id)
        {
            return Err(RecipeExecutionError::OwnershipMismatch);
        }
        let mut primary_assignments = BTreeMap::new();
        for candidate in &candidates {
            let assignment = (candidate.lane, candidate.source.clone());
            if primary_assignments
                .insert(candidate.track_id, assignment.clone())
                .is_some_and(|previous| previous != assignment)
            {
                return Err(RecipeExecutionError::AmbiguousCandidateAssignment);
            }
        }
        candidates.sort_by(candidate_order);
        required_collections.sort_unstable();
        required_collections.dedup();
        Ok(Self {
            recipe,
            target_tracks,
            candidates,
            evidence_capabilities,
            required_collections,
            budgets,
        })
    }
}

#[derive(Deserialize)]
struct RawRecipeExecutionRequest {
    recipe: RecipeV1,
    target_tracks: NonZeroU16,
    candidates: Vec<RecipeCandidate>,
    evidence_capabilities: EvidenceCapabilities,
    required_collections: Vec<AccountOwnedId<CollectionId>>,
    budgets: SelectionBudgets,
}

impl<'de> Deserialize<'de> for RecipeExecutionRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawRecipeExecutionRequest::deserialize(deserializer)?;
        Self::new(
            raw.recipe,
            raw.target_tracks,
            raw.candidates,
            raw.evidence_capabilities,
            raw.required_collections,
            raw.budgets,
        )
        .map_err(D::Error::custom)
    }
}

/// Why one otherwise supplied candidate was not eligible for selection.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateExclusionReason {
    /// Track was absent from the captured current inventory.
    OutsideCurrentInventory,
    /// Track lacked a playable canonical identity.
    Unplayable,
    /// Durable explicit intent excludes the track.
    ExplicitlyExcluded,
    /// Track did not satisfy the requested hard collection boundary.
    OutsideHardBoundary,
    /// Track occurrence budget was already full.
    TrackRepetitionBudget,
    /// At least one credited artist had reached its budget.
    ArtistBudget,
}

/// Availability of one immutable recipe source.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceExecutionReport {
    /// Allocated lifecycle lane.
    pub lane: SourceLane,
    /// Immutable collection or evidence dependency.
    pub source: RecipeSource,
    /// Immutable relative source weight from the recipe revision.
    pub weight: AllocationWeight,
    /// Observed availability; collection sources are available by construction.
    pub status: CapabilityStatus,
    /// Whether the source participated in seat allocation.
    pub enabled: bool,
    /// Seats assigned after weight allocation and cadence reservation.
    pub allocated_seats: u16,
    /// Entries selected from this source.
    pub selected_entries: u16,
}

/// One selected entry in the unordered draft.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DraftTrackSelection {
    /// Account-owned canonical track.
    pub track_id: AccountOwnedId<CanonicalTrackId>,
    /// Canonical artists used for budget enforcement.
    pub artist_ids: Vec<CanonicalArtistId>,
    /// Lane responsible for the selection.
    pub lane: SourceLane,
    /// Immutable source responsible for the selection.
    pub source: RecipeSource,
    /// Provider-neutral priority supplied by evidence preparation.
    pub priority: u64,
}

/// Selection-stage handling of one declared guardrail category.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardrailStage {
    /// Fully enforced while constructing the unordered selection draft.
    EnforcedDuringSelection,
    /// Requires exact ordering or prior-Spin state and remains for V020-10.
    DeferredToSpinPreview,
}

/// Visible guardrail disposition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GuardrailExecution {
    /// Recipe guardrail category.
    pub kind: GuardrailKind,
    /// Stage at which the category is enforced.
    pub stage: GuardrailStage,
}

/// Familiar-anchor capacity reserved before exact ordering.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FamiliarityCadencePlan {
    /// One-based positions requiring an anchor in the later ordered preview.
    pub anchor_positions: Vec<u16>,
    /// Familiar/high-rotation entries selected into the unordered draft.
    pub selected_anchor_entries: u16,
    /// Whether enough entries exist to satisfy every reserved position.
    pub satisfiable: bool,
}

/// Seat count reserved for one narrative section; no tracks are ordered here.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NarrativeSectionPlan {
    /// Narrative section from the immutable recipe.
    pub section: RecipeSection,
    /// Number of later ordered positions reserved for the section.
    pub seats: u16,
}

/// Deterministic unordered output of recipe-v1 execution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RecipeExecutionDraft {
    /// Immutable recipe revision evaluated.
    pub recipe_revision: RecipeRevisionIdentity,
    /// Fingerprint of canonicalized execution inputs.
    pub input_fingerprint: RecipeExecutionFingerprint,
    /// Fingerprint of this complete deterministic draft.
    pub draft_fingerprint: RecipeExecutionFingerprint,
    /// Requested target size.
    pub target_tracks: u16,
    /// Selected entries, canonically sorted rather than playback ordered.
    pub selections: Vec<DraftTrackSelection>,
    /// Source availability and seat fulfillment.
    pub sources: Vec<SourceExecutionReport>,
    /// Candidate exclusions grouped by stable reason.
    pub exclusions: BTreeMap<CandidateExclusionReason, u64>,
    /// Familiar-anchor reservation for the future orderer.
    pub familiarity_cadence: FamiliarityCadencePlan,
    /// Narrative section seat counts for the future orderer.
    pub narrative_sections: Vec<NarrativeSectionPlan>,
    /// Exact enforcement/deferment of declared guardrails.
    pub guardrails: Vec<GuardrailExecution>,
    /// Target seats that could not be filled honestly.
    pub unfilled_seats: u16,
    /// Always false in V020-09; exact playback order belongs to V020-10.
    pub playback_order_assigned: bool,
}

/// Stable failure from provider-neutral recipe execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecipeExecutionError {
    /// Related inputs belonged to different Chordrift accounts.
    OwnershipMismatch,
    /// One canonical track was assigned to more than one primary lane/source.
    AmbiguousCandidateAssignment,
    /// Every positively weighted source was unavailable.
    NoUsableSources,
    /// Sectioned ordering lacked unique narrative sections.
    InvalidNarrativeSections,
    /// A deterministic fingerprint could not be encoded.
    Serialization,
}

impl RecipeExecutionError {
    /// Returns the client-safe contract representation.
    #[must_use]
    pub fn client_error(self) -> ClientError {
        let code = match self {
            Self::OwnershipMismatch => ErrorCode::PermissionDenied,
            Self::AmbiguousCandidateAssignment => ErrorCode::InvalidRequest,
            Self::NoUsableSources => ErrorCode::CapabilityUnavailable,
            Self::InvalidNarrativeSections | Self::Serialization => ErrorCode::InvalidRequest,
        };
        ClientError::new(code, false)
    }
}

impl fmt::Display for RecipeExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OwnershipMismatch => "recipe execution inputs have mismatched owners",
            Self::AmbiguousCandidateAssignment => {
                "one recipe candidate has multiple primary lane/source assignments"
            }
            Self::NoUsableSources => "recipe execution has no usable source",
            Self::InvalidNarrativeSections => "sectioned ordering requires unique sections",
            Self::Serialization => "recipe execution could not be fingerprinted",
        })
    }
}

impl std::error::Error for RecipeExecutionError {}

/// Stateless provider-neutral recipe-v1 executor.
#[derive(Clone, Copy, Debug, Default)]
pub struct RecipeExecutor;

impl RecipeExecutor {
    /// Creates a recipe executor.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Produces a deterministic unordered selection draft.
    pub fn execute(
        &self,
        request: &RecipeExecutionRequest,
    ) -> Result<RecipeExecutionDraft, RecipeExecutionError> {
        validate_sections(&request.recipe)?;
        let input_fingerprint = RecipeExecutionFingerprint::from_serializable(request)?;
        let target = request.target_tracks.get();
        let cadence_positions = cadence_positions(&request.recipe, target);
        let required_anchor_count = cadence_positions.len();
        let mut allocations = active_allocations(request);
        if allocations.is_empty() {
            return Err(RecipeExecutionError::NoUsableSources);
        }
        allocate_weighted_seats(&mut allocations, target);
        reserve_familiar_seats(&mut allocations, cadence_positions.len());

        let mut exclusions = BTreeMap::new();
        let mut track_counts = BTreeMap::new();
        let mut artist_counts = BTreeMap::new();
        let mut selections = Vec::new();
        for allocation in &mut allocations {
            let mut matching: Vec<_> = request
                .candidates
                .iter()
                .filter(|candidate| {
                    candidate.lane == allocation.lane && candidate.source == allocation.source
                })
                .collect();
            matching.sort_by(|left, right| candidate_order(left, right));
            for candidate in matching {
                if allocation.selected_entries >= allocation.allocated_seats {
                    break;
                }
                if let Some(reason) =
                    eligibility_exclusion(candidate, &request.required_collections)
                {
                    increment(&mut exclusions, reason);
                    continue;
                }
                let track_key = candidate.track_id.into_resource_id();
                if track_counts.get(&track_key).copied().unwrap_or(0)
                    >= request.budgets.max_occurrences_per_track.get()
                {
                    increment(
                        &mut exclusions,
                        CandidateExclusionReason::TrackRepetitionBudget,
                    );
                    continue;
                }
                if candidate.artist_ids.iter().any(|artist| {
                    artist_counts.get(artist).copied().unwrap_or(0)
                        >= request.budgets.max_tracks_per_artist.get()
                }) {
                    increment(&mut exclusions, CandidateExclusionReason::ArtistBudget);
                    continue;
                }
                *track_counts.entry(track_key).or_insert(0) += 1;
                for artist in &candidate.artist_ids {
                    *artist_counts.entry(*artist).or_insert(0) += 1;
                }
                allocation.selected_entries += 1;
                selections.push(DraftTrackSelection {
                    track_id: candidate.track_id,
                    artist_ids: candidate.artist_ids.clone(),
                    lane: candidate.lane,
                    source: candidate.source.clone(),
                    priority: candidate.priority,
                });
            }
        }
        selections.sort_by(selection_canonical_order);
        let selected_count = u16::try_from(selections.len()).unwrap_or(u16::MAX);
        let selected_anchor_entries = u16::try_from(
            selections
                .iter()
                .filter(|selection| is_familiar_lane(selection.lane))
                .count(),
        )
        .unwrap_or(u16::MAX);
        let source_reports = source_reports(request, &allocations);
        let guardrails = guardrail_report(&request.recipe);
        let narrative_sections = narrative_section_plan(&request.recipe, target);
        let mut draft = RecipeExecutionDraft {
            recipe_revision: request.recipe.identity(),
            input_fingerprint,
            draft_fingerprint: RecipeExecutionFingerprint("0".repeat(64)),
            target_tracks: target,
            selections,
            sources: source_reports,
            exclusions,
            familiarity_cadence: FamiliarityCadencePlan {
                anchor_positions: cadence_positions,
                selected_anchor_entries,
                satisfiable: usize::from(selected_anchor_entries) >= required_anchor_count,
            },
            narrative_sections,
            guardrails,
            unfilled_seats: target.saturating_sub(selected_count),
            playback_order_assigned: false,
        };
        draft.draft_fingerprint = draft_fingerprint(&draft)?;
        Ok(draft)
    }

    /// Wraps execution for the shared application facade.
    #[must_use]
    pub const fn invocation<'request>(
        &'request self,
        request: &'request RecipeExecutionRequest,
    ) -> RecipeExecutionInvocation<'request> {
        RecipeExecutionInvocation {
            executor: self,
            request,
        }
    }
}

/// One provider-neutral recipe execution submitted through the application facade.
pub struct RecipeExecutionInvocation<'request> {
    executor: &'request RecipeExecutor,
    request: &'request RecipeExecutionRequest,
}

impl ApplicationInvocation for RecipeExecutionInvocation<'_> {
    type Output = Result<RecipeExecutionDraft, RecipeExecutionError>;

    fn execute(self) -> impl Future<Output = crate::Result<Self::Output>> {
        future::ready(Ok(self.executor.execute(self.request)))
    }
}

#[derive(Clone)]
struct ActiveAllocation {
    lane: SourceLane,
    source: RecipeSource,
    weight: u16,
    remainder: u64,
    allocated_seats: u16,
    selected_entries: u16,
}

fn active_allocations(request: &RecipeExecutionRequest) -> Vec<ActiveAllocation> {
    let mut allocations: Vec<_> = request
        .recipe
        .allocations()
        .iter()
        .filter(|allocation| allocation.weight.is_active())
        .filter(|allocation| match &allocation.source {
            RecipeSource::Collection(_) => true,
            RecipeSource::Evidence(capability) => {
                request.evidence_capabilities.status(*capability) != CapabilityStatus::Unavailable
            }
        })
        .map(|allocation| ActiveAllocation {
            lane: allocation.lane,
            source: allocation.source.clone(),
            weight: allocation.weight.value(),
            remainder: 0,
            allocated_seats: 0,
            selected_entries: 0,
        })
        .collect();
    allocations.sort_by(|left, right| {
        left.lane
            .cmp(&right.lane)
            .then_with(|| left.source.cmp(&right.source))
    });
    allocations
}

fn allocate_weighted_seats(allocations: &mut [ActiveAllocation], target: u16) {
    let total_weight: u64 = allocations.iter().map(|item| u64::from(item.weight)).sum();
    let mut assigned = 0_u16;
    for allocation in allocations.iter_mut() {
        let numerator = u64::from(target) * u64::from(allocation.weight);
        allocation.allocated_seats = u16::try_from(numerator / total_weight).unwrap_or(target);
        allocation.remainder = numerator % total_weight;
        assigned = assigned.saturating_add(allocation.allocated_seats);
    }
    let mut order: Vec<_> = (0..allocations.len()).collect();
    order.sort_by_key(|index| (Reverse(allocations[*index].remainder), *index));
    for index in order
        .into_iter()
        .take(usize::from(target.saturating_sub(assigned)))
    {
        allocations[index].allocated_seats += 1;
    }
}

fn reserve_familiar_seats(allocations: &mut [ActiveAllocation], required: usize) {
    let familiar: Vec<_> = allocations
        .iter()
        .enumerate()
        .filter_map(|(index, item)| is_familiar_lane(item.lane).then_some(index))
        .collect();
    if familiar.is_empty() {
        return;
    }
    let mut reserved = allocations
        .iter()
        .filter(|item| is_familiar_lane(item.lane))
        .map(|item| usize::from(item.allocated_seats))
        .sum::<usize>();
    let mut receiver = 0;
    while reserved < required {
        let donor = allocations
            .iter()
            .enumerate()
            .filter(|(_, item)| !is_familiar_lane(item.lane) && item.allocated_seats > 0)
            .max_by_key(|(index, item)| (item.allocated_seats, Reverse(*index)))
            .map(|(index, _)| index);
        let Some(donor) = donor else { break };
        let destination = familiar[receiver % familiar.len()];
        allocations[donor].allocated_seats -= 1;
        allocations[destination].allocated_seats += 1;
        receiver += 1;
        reserved += 1;
    }
}

fn eligibility_exclusion(
    candidate: &RecipeCandidate,
    required_collections: &[AccountOwnedId<CollectionId>],
) -> Option<CandidateExclusionReason> {
    if !candidate.eligibility.in_current_inventory {
        return Some(CandidateExclusionReason::OutsideCurrentInventory);
    }
    if !candidate.eligibility.playable {
        return Some(CandidateExclusionReason::Unplayable);
    }
    if candidate.eligibility.explicitly_excluded {
        return Some(CandidateExclusionReason::ExplicitlyExcluded);
    }
    if !required_collections.iter().all(|required| {
        candidate
            .collection_memberships
            .binary_search(required)
            .is_ok()
    }) {
        return Some(CandidateExclusionReason::OutsideHardBoundary);
    }
    None
}

fn cadence_positions(recipe: &RecipeV1, target: u16) -> Vec<u16> {
    let crate::domain::FamiliarityCadence::Every(span) = recipe.familiarity_cadence() else {
        return Vec::new();
    };
    (span.get()..=target)
        .step_by(usize::from(span.get()))
        .collect()
}

fn narrative_section_plan(recipe: &RecipeV1, target: u16) -> Vec<NarrativeSectionPlan> {
    if recipe.ordering() != OrderingNarrative::SectionedJourney {
        return Vec::new();
    }
    let sections = recipe.sections();
    let section_count = u16::try_from(sections.len()).expect("validated section count fits u16");
    let base = target / section_count;
    let remainder = target % section_count;
    sections
        .iter()
        .enumerate()
        .map(|(index, section)| NarrativeSectionPlan {
            section: *section,
            seats: base
                + if u16::try_from(index).expect("section index fits") < remainder {
                    1
                } else {
                    0
                },
        })
        .collect()
}

fn validate_sections(recipe: &RecipeV1) -> Result<(), RecipeExecutionError> {
    if recipe.ordering() != OrderingNarrative::SectionedJourney {
        return Ok(());
    }
    let mut sections = recipe.sections().to_vec();
    sections.sort_by_key(|section| match section {
        RecipeSection::WarmUp => 0,
        RecipeSection::Focus => 1,
        RecipeSection::Landing => 2,
    });
    sections.dedup();
    if sections.is_empty() || sections.len() != recipe.sections().len() {
        return Err(RecipeExecutionError::InvalidNarrativeSections);
    }
    Ok(())
}

fn guardrail_report(recipe: &RecipeV1) -> Vec<GuardrailExecution> {
    recipe
        .guardrails()
        .iter()
        .map(|kind| GuardrailExecution {
            kind: *kind,
            stage: match kind {
                GuardrailKind::HardBoundaries | GuardrailKind::ArtistRepetition => {
                    GuardrailStage::EnforcedDuringSelection
                }
                GuardrailKind::ArtistSpacing
                | GuardrailKind::Duration
                | GuardrailKind::CrossOutputReuse => GuardrailStage::DeferredToSpinPreview,
            },
        })
        .collect()
}

fn source_reports(
    request: &RecipeExecutionRequest,
    active: &[ActiveAllocation],
) -> Vec<SourceExecutionReport> {
    let mut reports: Vec<_> = request
        .recipe
        .allocations()
        .iter()
        .filter(|allocation| allocation.weight.is_active())
        .map(|allocation| {
            let status = match &allocation.source {
                RecipeSource::Collection(_) => CapabilityStatus::Available,
                RecipeSource::Evidence(capability) => {
                    request.evidence_capabilities.status(*capability)
                }
            };
            let matched = active
                .iter()
                .find(|item| item.lane == allocation.lane && item.source == allocation.source);
            SourceExecutionReport {
                lane: allocation.lane,
                source: allocation.source.clone(),
                weight: allocation.weight,
                status,
                enabled: matched.is_some(),
                allocated_seats: matched.map_or(0, |item| item.allocated_seats),
                selected_entries: matched.map_or(0, |item| item.selected_entries),
            }
        })
        .collect();
    reports.sort_by(|left, right| {
        left.lane
            .cmp(&right.lane)
            .then_with(|| left.source.cmp(&right.source))
    });
    reports
}

#[derive(Serialize)]
struct DraftFingerprintPayload<'draft> {
    recipe_revision: RecipeRevisionIdentity,
    input_fingerprint: &'draft RecipeExecutionFingerprint,
    target_tracks: u16,
    selections: &'draft [DraftTrackSelection],
    sources: &'draft [SourceExecutionReport],
    exclusions: &'draft BTreeMap<CandidateExclusionReason, u64>,
    familiarity_cadence: &'draft FamiliarityCadencePlan,
    narrative_sections: &'draft [NarrativeSectionPlan],
    guardrails: &'draft [GuardrailExecution],
    unfilled_seats: u16,
    playback_order_assigned: bool,
}

fn draft_fingerprint(
    draft: &RecipeExecutionDraft,
) -> Result<RecipeExecutionFingerprint, RecipeExecutionError> {
    RecipeExecutionFingerprint::from_serializable(&DraftFingerprintPayload {
        recipe_revision: draft.recipe_revision,
        input_fingerprint: &draft.input_fingerprint,
        target_tracks: draft.target_tracks,
        selections: &draft.selections,
        sources: &draft.sources,
        exclusions: &draft.exclusions,
        familiarity_cadence: &draft.familiarity_cadence,
        narrative_sections: &draft.narrative_sections,
        guardrails: &draft.guardrails,
        unfilled_seats: draft.unfilled_seats,
        playback_order_assigned: draft.playback_order_assigned,
    })
}

fn candidate_order(left: &RecipeCandidate, right: &RecipeCandidate) -> std::cmp::Ordering {
    Reverse(left.priority)
        .cmp(&Reverse(right.priority))
        .then_with(|| left.track_id.cmp(&right.track_id))
        .then_with(|| left.lane.cmp(&right.lane))
        .then_with(|| left.source.cmp(&right.source))
}

fn selection_canonical_order(
    left: &DraftTrackSelection,
    right: &DraftTrackSelection,
) -> std::cmp::Ordering {
    left.track_id
        .cmp(&right.track_id)
        .then_with(|| left.lane.cmp(&right.lane))
        .then_with(|| left.source.cmp(&right.source))
}

const fn is_familiar_lane(lane: SourceLane) -> bool {
    matches!(lane, SourceLane::Familiar | SourceLane::HighRotation)
}

fn increment(map: &mut BTreeMap<CandidateExclusionReason, u64>, reason: CandidateExclusionReason) {
    *map.entry(reason).or_insert(0) += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::ApplicationFacade,
        domain::{
            ChordriftAccountId, CollectionId, EvidenceCapability, FamiliarityCadence, RecipeId,
            RecipeRevisionId, SourceAllocation,
        },
    };
    use std::collections::BTreeMap;

    fn recipe(account: ChordriftAccountId) -> RecipeV1 {
        RecipeV1::new(
            RecipeRevisionIdentity {
                recipe_id: AccountOwnedId::new(
                    account,
                    RecipeId::from_uuid(uuid::Uuid::from_u128(1)),
                ),
                revision_id: RecipeRevisionId::from_uuid(uuid::Uuid::from_u128(2)),
            },
            vec![
                SourceAllocation {
                    lane: SourceLane::Discovery,
                    source: RecipeSource::Evidence(EvidenceCapability::SavedAt),
                    weight: AllocationWeight::new(3),
                },
                SourceAllocation {
                    lane: SourceLane::Dormant,
                    source: RecipeSource::Evidence(EvidenceCapability::LifetimeRotation),
                    weight: AllocationWeight::new(2),
                },
                SourceAllocation {
                    lane: SourceLane::Familiar,
                    source: RecipeSource::Evidence(EvidenceCapability::ExtendedPlaybackHistory),
                    weight: AllocationWeight::new(1),
                },
            ],
            crate::domain::FamiliarityCadence::Every(NonZeroU16::new(3).expect("nonzero")),
            OrderingNarrative::SectionedJourney,
            vec![
                RecipeSection::WarmUp,
                RecipeSection::Focus,
                RecipeSection::Landing,
            ],
            vec![
                GuardrailKind::HardBoundaries,
                GuardrailKind::ArtistRepetition,
                GuardrailKind::ArtistSpacing,
            ],
        )
        .expect("recipe is valid")
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
        .expect("candidate is valid")
    }

    fn request(candidates: Vec<RecipeCandidate>) -> RecipeExecutionRequest {
        let account = candidates[0].track_id.account_id();
        RecipeExecutionRequest::new(
            recipe(account),
            NonZeroU16::new(6).expect("nonzero"),
            candidates,
            EvidenceCapabilities::new(BTreeMap::from([
                (EvidenceCapability::SavedAt, CapabilityStatus::Available),
                (
                    EvidenceCapability::LifetimeRotation,
                    CapabilityStatus::Degraded,
                ),
                (
                    EvidenceCapability::ExtendedPlaybackHistory,
                    CapabilityStatus::Available,
                ),
            ])),
            Vec::new(),
            SelectionBudgets {
                max_occurrences_per_track: NonZeroU16::new(1).expect("nonzero"),
                max_tracks_per_artist: NonZeroU16::new(1).expect("nonzero"),
            },
        )
        .expect("request is valid")
    }

    #[tokio::test]
    async fn deterministic_draft_enforces_all_selection_boundaries() {
        let account = ChordriftAccountId::from_uuid(uuid::Uuid::from_u128(10));
        let candidates = vec![
            candidate(
                account,
                1,
                1,
                SourceLane::Discovery,
                EvidenceCapability::SavedAt,
                90,
            ),
            candidate(
                account,
                2,
                1,
                SourceLane::Discovery,
                EvidenceCapability::SavedAt,
                80,
            ),
            candidate(
                account,
                3,
                2,
                SourceLane::Discovery,
                EvidenceCapability::SavedAt,
                70,
            ),
            candidate(
                account,
                4,
                6,
                SourceLane::Dormant,
                EvidenceCapability::LifetimeRotation,
                60,
            ),
            candidate(
                account,
                5,
                3,
                SourceLane::Dormant,
                EvidenceCapability::LifetimeRotation,
                50,
            ),
            candidate(
                account,
                6,
                4,
                SourceLane::Familiar,
                EvidenceCapability::ExtendedPlaybackHistory,
                40,
            ),
            candidate(
                account,
                7,
                5,
                SourceLane::Familiar,
                EvidenceCapability::ExtendedPlaybackHistory,
                30,
            ),
        ];
        let mut reversed = candidates.clone();
        reversed.reverse();
        let reversed_request = request(reversed);
        let request = request(candidates);
        let executor = RecipeExecutor::new();
        let first = ApplicationFacade::new()
            .invoke(executor.invocation(&request))
            .await
            .expect("facade succeeds")
            .expect("recipe succeeds");
        let second = executor.execute(&request).expect("replay succeeds");
        let reordered = executor
            .execute(&reversed_request)
            .expect("reordered input succeeds");

        assert_eq!(first, second);
        assert_eq!(first, reordered);
        assert_eq!(first.selections.len(), 6);
        assert!(!first.playback_order_assigned);
        assert_eq!(first.familiarity_cadence.anchor_positions, vec![3, 6]);
        assert!(first.familiarity_cadence.satisfiable);
        assert_eq!(
            first
                .narrative_sections
                .iter()
                .map(|item| item.seats)
                .sum::<u16>(),
            6
        );
        assert_eq!(
            first
                .exclusions
                .get(&CandidateExclusionReason::ArtistBudget),
            Some(&1)
        );
        assert!(
            first
                .sources
                .iter()
                .any(|source| source.status == CapabilityStatus::Degraded && source.enabled)
        );
        assert!(first.guardrails.iter().any(|guardrail| {
            guardrail.kind == GuardrailKind::ArtistSpacing
                && guardrail.stage == GuardrailStage::DeferredToSpinPreview
        }));
    }

    #[test]
    fn unavailable_evidence_is_disabled_without_emulation() {
        let account = ChordriftAccountId::new();
        let mut request = request(vec![candidate(
            account,
            1,
            1,
            SourceLane::Discovery,
            EvidenceCapability::SavedAt,
            1,
        )]);
        request.evidence_capabilities = EvidenceCapabilities::new(BTreeMap::from([
            (EvidenceCapability::SavedAt, CapabilityStatus::Available),
            (
                EvidenceCapability::ExtendedPlaybackHistory,
                CapabilityStatus::Unavailable,
            ),
        ]));
        let draft = RecipeExecutor::new()
            .execute(&request)
            .expect("one source remains");

        let disabled = draft
            .sources
            .iter()
            .find(|source| {
                source.source == RecipeSource::Evidence(EvidenceCapability::ExtendedPlaybackHistory)
            })
            .expect("history source is reported");
        assert!(!disabled.enabled);
        assert_eq!(disabled.allocated_seats, 0);
        assert_eq!(draft.selections.len(), 1);
        assert_eq!(draft.unfilled_seats, 5);
    }

    #[test]
    fn cross_account_and_invalid_sections_fail_visibly() {
        let account = ChordriftAccountId::new();
        let other = ChordriftAccountId::new();
        let cross_account = RecipeExecutionRequest::new(
            recipe(account),
            NonZeroU16::new(1).expect("nonzero"),
            vec![candidate(
                other,
                1,
                1,
                SourceLane::Discovery,
                EvidenceCapability::SavedAt,
                1,
            )],
            EvidenceCapabilities::new(BTreeMap::new()),
            Vec::new(),
            SelectionBudgets {
                max_occurrences_per_track: NonZeroU16::new(1).expect("nonzero"),
                max_tracks_per_artist: NonZeroU16::new(1).expect("nonzero"),
            },
        );
        assert_eq!(cross_account, Err(RecipeExecutionError::OwnershipMismatch));

        let ambiguous = RecipeExecutionRequest::new(
            recipe(account),
            NonZeroU16::new(1).expect("nonzero"),
            vec![
                candidate(
                    account,
                    1,
                    1,
                    SourceLane::Discovery,
                    EvidenceCapability::SavedAt,
                    2,
                ),
                candidate(
                    account,
                    1,
                    1,
                    SourceLane::Dormant,
                    EvidenceCapability::LifetimeRotation,
                    1,
                ),
            ],
            EvidenceCapabilities::new(BTreeMap::new()),
            Vec::new(),
            SelectionBudgets {
                max_occurrences_per_track: NonZeroU16::new(1).expect("nonzero"),
                max_tracks_per_artist: NonZeroU16::new(1).expect("nonzero"),
            },
        );
        assert_eq!(
            ambiguous,
            Err(RecipeExecutionError::AmbiguousCandidateAssignment)
        );

        let identity = recipe(account).identity();
        let invalid = RecipeV1::new(
            identity,
            vec![SourceAllocation {
                lane: SourceLane::Discovery,
                source: RecipeSource::Evidence(EvidenceCapability::SavedAt),
                weight: AllocationWeight::new(1),
            }],
            FamiliarityCadence::Disabled,
            OrderingNarrative::SectionedJourney,
            vec![RecipeSection::Focus, RecipeSection::Focus],
            Vec::new(),
        )
        .expect("domain vocabulary allows later execution validation");
        let request = RecipeExecutionRequest::new(
            invalid,
            NonZeroU16::new(1).expect("nonzero"),
            vec![candidate(
                account,
                1,
                1,
                SourceLane::Discovery,
                EvidenceCapability::SavedAt,
                1,
            )],
            EvidenceCapabilities::new(BTreeMap::from([(
                EvidenceCapability::SavedAt,
                CapabilityStatus::Available,
            )])),
            Vec::new(),
            SelectionBudgets {
                max_occurrences_per_track: NonZeroU16::new(1).expect("nonzero"),
                max_tracks_per_artist: NonZeroU16::new(1).expect("nonzero"),
            },
        )
        .expect("request ownership is valid");
        assert_eq!(
            RecipeExecutor::new().execute(&request),
            Err(RecipeExecutionError::InvalidNarrativeSections)
        );
    }

    #[test]
    fn hard_collection_boundary_excludes_nonmembers() {
        let account = ChordriftAccountId::new();
        let required = AccountOwnedId::new(account, CollectionId::new());
        let mut inside = candidate(
            account,
            1,
            1,
            SourceLane::Discovery,
            EvidenceCapability::SavedAt,
            2,
        );
        inside.collection_memberships = vec![required];
        let outside = candidate(
            account,
            2,
            2,
            SourceLane::Discovery,
            EvidenceCapability::SavedAt,
            1,
        );
        let mut request = request(vec![inside, outside]);
        request.required_collections = vec![required];
        let draft = RecipeExecutor::new()
            .execute(&request)
            .expect("recipe succeeds");

        assert_eq!(draft.selections.len(), 1);
        assert_eq!(
            draft
                .exclusions
                .get(&CandidateExclusionReason::OutsideHardBoundary),
            Some(&1)
        );
    }

    #[test]
    fn eligibility_and_track_repetition_exclusions_are_explicit() {
        let account = ChordriftAccountId::new();
        let allocation = SourceAllocation {
            lane: SourceLane::Discovery,
            source: RecipeSource::Evidence(EvidenceCapability::SavedAt),
            weight: AllocationWeight::new(1),
        };
        let recipe = RecipeV1::new(
            RecipeRevisionIdentity {
                recipe_id: AccountOwnedId::new(account, RecipeId::new()),
                revision_id: RecipeRevisionId::new(),
            },
            vec![allocation],
            FamiliarityCadence::Disabled,
            OrderingNarrative::Shuffle,
            Vec::new(),
            vec![GuardrailKind::HardBoundaries],
        )
        .expect("recipe is valid");
        let mut outside_inventory = candidate(
            account,
            1,
            1,
            SourceLane::Discovery,
            EvidenceCapability::SavedAt,
            6,
        );
        outside_inventory.eligibility.in_current_inventory = false;
        let mut unplayable = candidate(
            account,
            2,
            2,
            SourceLane::Discovery,
            EvidenceCapability::SavedAt,
            5,
        );
        unplayable.eligibility.playable = false;
        let mut excluded = candidate(
            account,
            3,
            3,
            SourceLane::Discovery,
            EvidenceCapability::SavedAt,
            4,
        );
        excluded.eligibility.explicitly_excluded = true;
        let first = candidate(
            account,
            4,
            4,
            SourceLane::Discovery,
            EvidenceCapability::SavedAt,
            3,
        );
        let repeated_track = candidate(
            account,
            4,
            4,
            SourceLane::Discovery,
            EvidenceCapability::SavedAt,
            2,
        );
        let second = candidate(
            account,
            5,
            5,
            SourceLane::Discovery,
            EvidenceCapability::SavedAt,
            1,
        );
        let request = RecipeExecutionRequest::new(
            recipe,
            NonZeroU16::new(2).expect("nonzero"),
            vec![
                second,
                repeated_track,
                excluded,
                unplayable,
                outside_inventory,
                first,
            ],
            EvidenceCapabilities::new(BTreeMap::from([(
                EvidenceCapability::SavedAt,
                CapabilityStatus::Available,
            )])),
            Vec::new(),
            SelectionBudgets {
                max_occurrences_per_track: NonZeroU16::new(1).expect("nonzero"),
                max_tracks_per_artist: NonZeroU16::new(2).expect("nonzero"),
            },
        )
        .expect("request is valid");

        let draft = RecipeExecutor::new()
            .execute(&request)
            .expect("recipe succeeds");

        assert_eq!(draft.selections.len(), 2);
        for reason in [
            CandidateExclusionReason::OutsideCurrentInventory,
            CandidateExclusionReason::Unplayable,
            CandidateExclusionReason::ExplicitlyExcluded,
            CandidateExclusionReason::TrackRepetitionBudget,
        ] {
            assert_eq!(draft.exclusions.get(&reason), Some(&1));
        }
    }

    #[test]
    fn all_unavailable_sources_fail_with_capability_error() {
        let account = ChordriftAccountId::new();
        let mut request = request(vec![candidate(
            account,
            1,
            1,
            SourceLane::Discovery,
            EvidenceCapability::SavedAt,
            1,
        )]);
        request.evidence_capabilities = EvidenceCapabilities::new(BTreeMap::new());

        let error = RecipeExecutor::new()
            .execute(&request)
            .expect_err("every source is unavailable");

        assert_eq!(error, RecipeExecutionError::NoUsableSources);
        assert_eq!(error.client_error().code, ErrorCode::CapabilityUnavailable);
    }
}
