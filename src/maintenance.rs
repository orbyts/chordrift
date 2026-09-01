//! Wrapper-neutral ordinary-maintenance workflow state.
//!
//! This module owns task-level maintenance transitions shared by CLI, web,
//! mobile, and future clients. It contains no terminal rendering, shell
//! sequencing, SQL, provider client, or HTTP behavior. Infrastructure adapters
//! assemble a [`MaintenanceProjection`] from the newest complete provider
//! snapshot and persist accepted decisions or authorized effects.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::contract::{
    ClientError, Command, ErrorCode, MaintenanceAllowedAction, MaintenanceChangeView,
    MaintenanceDecision, MaintenanceProviderEffectView, MaintenanceReviewId, MaintenanceSessionId,
    MaintenanceSessionState, MaintenanceSessionView, Query, ResourceId,
};

/// Complete application-owned input used to assemble one maintenance revision.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MaintenanceProjection {
    /// Newest complete provider snapshot accepted as the current baseline.
    pub provider_snapshot_id: ResourceId,
    /// Exact provider gestures folded cumulatively into the baseline.
    pub observed_changes: Vec<MaintenanceChangeView>,
    /// Chordrift-authored provider effects that still require authorization.
    pub provider_effects: Vec<MaintenanceProviderEffectView>,
    /// Immutable identity for the provider-effect review, when effects exist.
    pub review_id: Option<MaintenanceReviewId>,
}

/// Recomputed provider review after all ambiguity decisions are accepted.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MaintenanceDecisionProjection {
    /// Exact provider effects implied by the accepted decisions.
    pub provider_effects: Vec<MaintenanceProviderEffectView>,
    /// Immutable review identity when the effects are nonempty.
    pub review_id: Option<MaintenanceReviewId>,
}

/// Wrapper-neutral state machine for one ordinary-maintenance task.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaintenanceWorkflow {
    view: MaintenanceSessionView,
}

/// In-memory application-core index for wrapper-neutral maintenance sessions.
///
/// V021-04 will replace process memory with durable job/session persistence.
/// The transition and command-routing rules remain here and do not move into an
/// HTTP, CLI, browser, or mobile adapter.
#[derive(Clone, Debug, Default)]
pub struct MaintenanceSessions {
    sessions: BTreeMap<MaintenanceSessionId, MaintenanceWorkflow>,
}

impl MaintenanceSessions {
    /// Creates an empty application-core maintenance index.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            sessions: BTreeMap::new(),
        }
    }

    /// Starts one session after an infrastructure adapter assembles its first projection.
    pub fn start(
        &mut self,
        command: &Command,
        projection: MaintenanceProjection,
    ) -> Result<MaintenanceSessionView, MaintenanceWorkflowError> {
        let Command::StartMaintenance { session_id, .. } = command else {
            return Err(MaintenanceWorkflowError::UnsupportedCommand);
        };
        if self.sessions.contains_key(session_id) {
            return Err(MaintenanceWorkflowError::DuplicateSession);
        }
        let workflow = MaintenanceWorkflow::new(*session_id, projection)?;
        let view = workflow.view();
        self.sessions.insert(*session_id, workflow);
        Ok(view)
    }

    /// Rebases one session after an adapter obtains a newer complete provider snapshot.
    pub fn refresh(
        &mut self,
        command: &Command,
        projection: MaintenanceProjection,
    ) -> Result<MaintenanceSessionView, MaintenanceWorkflowError> {
        let Command::RefreshMaintenance {
            session_id,
            expected_revision,
        } = command
        else {
            return Err(MaintenanceWorkflowError::UnsupportedCommand);
        };
        self.session_mut(*session_id)?
            .rebase(*expected_revision, projection)
    }

    /// Applies one task-level client command through Rust-owned workflow rules.
    pub fn execute(
        &mut self,
        command: &Command,
        decision_projection: Option<MaintenanceDecisionProjection>,
    ) -> Result<MaintenanceSessionView, MaintenanceWorkflowError> {
        match command {
            Command::ResolveMaintenance {
                session_id,
                expected_revision,
                decisions,
            } => {
                let projection = decision_projection
                    .ok_or(MaintenanceWorkflowError::MissingDecisionProjection)?;
                self.session_mut(*session_id)?.resolve(
                    *expected_revision,
                    decisions.clone(),
                    projection,
                )
            }
            Command::AuthorizeMaintenance {
                session_id,
                expected_revision,
                review_id,
            } => self
                .session_mut(*session_id)?
                .authorize(*expected_revision, *review_id),
            _ => Err(MaintenanceWorkflowError::UnsupportedCommand),
        }
    }

    /// Returns the immutable task view selected by a typed contract query.
    pub fn query(&self, query: &Query) -> Result<MaintenanceSessionView, MaintenanceWorkflowError> {
        let Query::MaintenanceSession { session_id } = query else {
            return Err(MaintenanceWorkflowError::UnsupportedQuery);
        };
        self.sessions
            .get(session_id)
            .map(MaintenanceWorkflow::view)
            .ok_or(MaintenanceWorkflowError::SessionNotFound)
    }

    /// Advances provider execution state through the same application-core index.
    pub fn mark_execution_state(
        &mut self,
        session_id: MaintenanceSessionId,
        state: MaintenanceSessionState,
    ) -> Result<MaintenanceSessionView, MaintenanceWorkflowError> {
        self.session_mut(session_id)?.mark_execution_state(state)
    }

    fn session_mut(
        &mut self,
        session_id: MaintenanceSessionId,
    ) -> Result<&mut MaintenanceWorkflow, MaintenanceWorkflowError> {
        self.sessions
            .get_mut(&session_id)
            .ok_or(MaintenanceWorkflowError::SessionNotFound)
    }
}

impl MaintenanceWorkflow {
    /// Creates a session from the newest complete provider projection.
    pub fn new(
        session_id: MaintenanceSessionId,
        projection: MaintenanceProjection,
    ) -> Result<Self, MaintenanceWorkflowError> {
        validate_projection(&projection)?;
        let mut workflow = Self {
            view: MaintenanceSessionView {
                session_id,
                revision: 1,
                provider_snapshot_id: projection.provider_snapshot_id,
                state: MaintenanceSessionState::Reconciling,
                observed_changes: projection.observed_changes,
                provider_effects: projection.provider_effects,
                review_id: projection.review_id,
                allowed_actions: Vec::new(),
            },
        };
        workflow.refresh_derived_state()?;
        Ok(workflow)
    }

    /// Rehydrates an already-validated durable session after a process restart.
    ///
    /// Infrastructure stores the typed view, while all subsequent transitions
    /// continue to run through this Rust-owned state machine.
    pub fn from_view(view: MaintenanceSessionView) -> Result<Self, MaintenanceWorkflowError> {
        validate_view(&view)?;
        Ok(Self { view })
    }

    /// Returns the immutable client-facing view for the current revision.
    #[must_use]
    pub fn view(&self) -> MaintenanceSessionView {
        self.view.clone()
    }

    /// Records all decisions required by the current immutable review.
    pub fn resolve(
        &mut self,
        expected_revision: u64,
        decisions: Vec<MaintenanceDecision>,
        decision_projection: MaintenanceDecisionProjection,
    ) -> Result<MaintenanceSessionView, MaintenanceWorkflowError> {
        self.require_revision(expected_revision)?;
        if self.view.state != MaintenanceSessionState::NeedsDecision {
            return Err(MaintenanceWorkflowError::DecisionsNotAccepted);
        }

        let unresolved = self
            .view
            .observed_changes
            .iter()
            .filter(|change| change.resolution.is_none())
            .map(|change| change.change_id)
            .collect::<BTreeSet<_>>();
        let submitted = decisions
            .iter()
            .map(|decision| decision.change_id)
            .collect::<BTreeSet<_>>();
        if submitted.len() != decisions.len() {
            return Err(MaintenanceWorkflowError::DuplicateDecision);
        }
        if submitted != unresolved {
            return Err(MaintenanceWorkflowError::IncompleteDecisionSet);
        }

        for decision in &decisions {
            let change = self
                .view
                .observed_changes
                .iter()
                .find(|change| change.change_id == decision.change_id)
                .ok_or(MaintenanceWorkflowError::UnknownChange)?;
            if !resolution_allowed(change, &decision.resolution) {
                return Err(MaintenanceWorkflowError::InvalidResolution);
            }
        }
        for decision in decisions {
            let change = self
                .view
                .observed_changes
                .iter_mut()
                .find(|change| change.change_id == decision.change_id)
                .ok_or(MaintenanceWorkflowError::UnknownChange)?;
            change.resolution = Some(decision.resolution);
        }
        self.view.revision += 1;
        self.view.provider_effects = decision_projection.provider_effects;
        self.view.review_id = decision_projection.review_id;
        self.refresh_derived_state()?;
        Ok(self.view())
    }

    /// Records authorization for one exact provider-effect review.
    pub fn authorize(
        &mut self,
        expected_revision: u64,
        review_id: MaintenanceReviewId,
    ) -> Result<MaintenanceSessionView, MaintenanceWorkflowError> {
        self.require_revision(expected_revision)?;
        if self.view.state != MaintenanceSessionState::ReadyForAuthorization {
            return Err(MaintenanceWorkflowError::AuthorizationNotAccepted);
        }
        if self.view.review_id != Some(review_id) {
            return Err(MaintenanceWorkflowError::ReviewMismatch);
        }
        self.view.revision += 1;
        self.view.state = MaintenanceSessionState::Authorized;
        self.view.allowed_actions = vec![
            MaintenanceAllowedAction::Refresh,
            MaintenanceAllowedAction::Cancel,
        ];
        Ok(self.view())
    }

    /// Rebases record-only intent onto a newer complete provider snapshot.
    ///
    /// Any authorization attached to the older snapshot disappears because the
    /// replacement projection must supply a newly assembled review identity.
    pub fn rebase(
        &mut self,
        expected_revision: u64,
        projection: MaintenanceProjection,
    ) -> Result<MaintenanceSessionView, MaintenanceWorkflowError> {
        self.require_revision(expected_revision)?;
        validate_projection(&projection)?;
        self.view.revision += 1;
        self.view.provider_snapshot_id = projection.provider_snapshot_id;
        self.view.observed_changes = projection.observed_changes;
        self.view.provider_effects = projection.provider_effects;
        self.view.review_id = projection.review_id;
        self.refresh_derived_state()?;
        Ok(self.view())
    }

    /// Advances server-owned execution state without changing reviewed effects.
    pub fn mark_execution_state(
        &mut self,
        state: MaintenanceSessionState,
    ) -> Result<MaintenanceSessionView, MaintenanceWorkflowError> {
        let allowed = matches!(
            (self.view.state, state),
            (
                MaintenanceSessionState::Authorized,
                MaintenanceSessionState::Applying
            ) | (
                MaintenanceSessionState::Applying,
                MaintenanceSessionState::Verifying
            ) | (
                MaintenanceSessionState::Verifying,
                MaintenanceSessionState::InSync
            ) | (_, MaintenanceSessionState::Recoverable)
        );
        if !allowed {
            return Err(MaintenanceWorkflowError::InvalidExecutionTransition);
        }
        self.view.revision += 1;
        self.view.state = state;
        self.view.allowed_actions = match state {
            MaintenanceSessionState::Applying | MaintenanceSessionState::Verifying => {
                vec![MaintenanceAllowedAction::Cancel]
            }
            MaintenanceSessionState::InSync => vec![MaintenanceAllowedAction::Refresh],
            MaintenanceSessionState::Recoverable => vec![
                MaintenanceAllowedAction::Refresh,
                MaintenanceAllowedAction::Resume,
            ],
            _ => Vec::new(),
        };
        Ok(self.view())
    }

    /// Completes verification against a newly observed provider snapshot.
    ///
    /// Reviewed effects and their authorization identity are consumed only
    /// after the provider result has been observed. The resolved gesture
    /// history remains available for explanation and audit.
    pub fn complete_verification(
        &mut self,
        projection: MaintenanceProjection,
    ) -> Result<MaintenanceSessionView, MaintenanceWorkflowError> {
        if self.view.state != MaintenanceSessionState::Verifying {
            return Err(MaintenanceWorkflowError::InvalidExecutionTransition);
        }
        validate_projection(&projection)?;
        self.view.revision += 1;
        self.view.provider_snapshot_id = projection.provider_snapshot_id;
        self.view.observed_changes = projection.observed_changes;
        self.view.provider_effects = projection.provider_effects;
        self.view.review_id = projection.review_id;
        self.refresh_derived_state()?;
        Ok(self.view())
    }

    fn require_revision(&self, expected: u64) -> Result<(), MaintenanceWorkflowError> {
        if expected != self.view.revision {
            return Err(MaintenanceWorkflowError::StaleRevision {
                expected,
                current: self.view.revision,
            });
        }
        Ok(())
    }

    fn refresh_derived_state(&mut self) -> Result<(), MaintenanceWorkflowError> {
        let unresolved = self
            .view
            .observed_changes
            .iter()
            .any(|change| change.resolution.is_none());
        if unresolved {
            if self.view.review_id.is_some() {
                return Err(MaintenanceWorkflowError::ReviewBeforeDecisions);
            }
            self.view.state = MaintenanceSessionState::NeedsDecision;
            self.view.allowed_actions = vec![
                MaintenanceAllowedAction::Refresh,
                MaintenanceAllowedAction::Resolve,
            ];
        } else if self.view.provider_effects.is_empty() {
            if self.view.review_id.is_some() {
                return Err(MaintenanceWorkflowError::ReviewWithoutEffects);
            }
            self.view.state = MaintenanceSessionState::InSync;
            self.view.allowed_actions = vec![MaintenanceAllowedAction::Refresh];
        } else {
            if self.view.review_id.is_none() {
                return Err(MaintenanceWorkflowError::EffectsWithoutReview);
            }
            self.view.state = MaintenanceSessionState::ReadyForAuthorization;
            self.view.allowed_actions = vec![
                MaintenanceAllowedAction::Refresh,
                MaintenanceAllowedAction::Authorize,
            ];
        }
        Ok(())
    }
}

fn validate_projection(projection: &MaintenanceProjection) -> Result<(), MaintenanceWorkflowError> {
    let change_ids = projection
        .observed_changes
        .iter()
        .map(|change| change.change_id)
        .collect::<BTreeSet<_>>();
    if change_ids.len() != projection.observed_changes.len() {
        return Err(MaintenanceWorkflowError::DuplicateChange);
    }
    let effect_ids = projection
        .provider_effects
        .iter()
        .map(|effect| effect.effect_id)
        .collect::<BTreeSet<_>>();
    if effect_ids.len() != projection.provider_effects.len() {
        return Err(MaintenanceWorkflowError::DuplicateEffect);
    }
    Ok(())
}

fn resolution_allowed(
    change: &MaintenanceChangeView,
    resolution: &crate::contract::MaintenanceResolution,
) -> bool {
    use crate::contract::{MaintenanceChangeKind as Kind, MaintenanceResolution as Resolution};
    match (change.kind, resolution) {
        (Kind::SavedState, Resolution::KeepObserved) => true,
        (Kind::SavedState, Resolution::ConsumeIntake { source }) => {
            change.current_surface.as_ref() == Some(source)
        }
        (Kind::DirectIntake | Kind::Reclassification, Resolution::Place { .. }) => true,
        (Kind::DirectIntake | Kind::Reclassification, Resolution::KeepObserved) => {
            change.current_surface.is_some()
        }
        (Kind::Removal, Resolution::Exclude | Resolution::Restore { .. }) => true,
        (
            Kind::Reorder | Kind::PlaylistMetadata | Kind::PlaylistCreated | Kind::PlaylistRemoved,
            Resolution::KeepObserved,
        ) => true,
        _ => false,
    }
}

fn validate_view(view: &MaintenanceSessionView) -> Result<(), MaintenanceWorkflowError> {
    validate_projection(&MaintenanceProjection {
        provider_snapshot_id: view.provider_snapshot_id,
        observed_changes: view.observed_changes.clone(),
        provider_effects: view.provider_effects.clone(),
        review_id: view.review_id,
    })?;
    if view.revision == 0 {
        return Err(MaintenanceWorkflowError::InvalidDurableView);
    }
    let unresolved = view
        .observed_changes
        .iter()
        .any(|change| change.resolution.is_none());
    let valid = match view.state {
        MaintenanceSessionState::NeedsDecision => {
            unresolved
                && view.review_id.is_none()
                && view.allowed_actions
                    == vec![
                        MaintenanceAllowedAction::Refresh,
                        MaintenanceAllowedAction::Resolve,
                    ]
        }
        MaintenanceSessionState::ReadyForAuthorization => {
            !unresolved
                && !view.provider_effects.is_empty()
                && view.review_id.is_some()
                && view.allowed_actions
                    == vec![
                        MaintenanceAllowedAction::Refresh,
                        MaintenanceAllowedAction::Authorize,
                    ]
        }
        MaintenanceSessionState::InSync => {
            !unresolved
                && view.provider_effects.is_empty()
                && view.review_id.is_none()
                && view.allowed_actions == vec![MaintenanceAllowedAction::Refresh]
        }
        MaintenanceSessionState::Authorized => {
            !unresolved
                && !view.provider_effects.is_empty()
                && view.review_id.is_some()
                && view.allowed_actions
                    == vec![
                        MaintenanceAllowedAction::Refresh,
                        MaintenanceAllowedAction::Cancel,
                    ]
        }
        MaintenanceSessionState::Applying | MaintenanceSessionState::Verifying => {
            !unresolved
                && !view.provider_effects.is_empty()
                && view.review_id.is_some()
                && view.allowed_actions == vec![MaintenanceAllowedAction::Cancel]
        }
        MaintenanceSessionState::Recoverable => {
            view.allowed_actions
                == vec![
                    MaintenanceAllowedAction::Refresh,
                    MaintenanceAllowedAction::Resume,
                ]
        }
        MaintenanceSessionState::Reconciling => view.allowed_actions.is_empty(),
    };
    if valid {
        Ok(())
    } else {
        Err(MaintenanceWorkflowError::InvalidDurableView)
    }
}

/// Invalid task-level maintenance transition or projection.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum MaintenanceWorkflowError {
    /// A client submitted a command against an older immutable revision.
    #[error("maintenance revision is stale: expected {expected}, current {current}")]
    StaleRevision {
        /// Revision supplied by the client.
        expected: u64,
        /// Current server-owned revision.
        current: u64,
    },
    /// More than one decision targeted the same observed change.
    #[error("maintenance decision set contains a duplicate change")]
    DuplicateDecision,
    /// The decision set did not cover exactly the unresolved changes.
    #[error("maintenance decision set must cover every unresolved change exactly once")]
    IncompleteDecisionSet,
    /// A decision referred to an unknown change.
    #[error("maintenance decision refers to an unknown change")]
    UnknownChange,
    /// A decision variant is not valid for the observed gesture it targets.
    #[error("maintenance resolution is not valid for this observed change")]
    InvalidResolution,
    /// The current state does not accept ambiguity decisions.
    #[error("maintenance session is not waiting for decisions")]
    DecisionsNotAccepted,
    /// The current state does not accept provider authorization.
    #[error("maintenance session is not waiting for authorization")]
    AuthorizationNotAccepted,
    /// Authorization did not name the immutable review currently displayed.
    #[error("maintenance review does not match the current revision")]
    ReviewMismatch,
    /// A projection repeated an observed-change identity.
    #[error("maintenance projection contains duplicate changes")]
    DuplicateChange,
    /// A projection repeated a provider-effect identity.
    #[error("maintenance projection contains duplicate provider effects")]
    DuplicateEffect,
    /// A persisted view violates the workflow's state invariants.
    #[error("persisted maintenance view violates workflow invariants")]
    InvalidDurableView,
    /// A provider-effect review was supplied before ambiguity was resolved.
    #[error("maintenance review cannot be finalized before decisions")]
    ReviewBeforeDecisions,
    /// A projection supplied a review even though it contains no provider effects.
    #[error("maintenance review exists without provider effects")]
    ReviewWithoutEffects,
    /// Provider effects were supplied without one immutable review identity.
    #[error("maintenance provider effects require an immutable review")]
    EffectsWithoutReview,
    /// Server-owned apply/verify state advanced in an invalid order.
    #[error("maintenance execution transition is invalid")]
    InvalidExecutionTransition,
    /// The adapter submitted a command owned by a different application workflow.
    #[error("command is not supported by ordinary maintenance")]
    UnsupportedCommand,
    /// The adapter submitted a query owned by a different application workflow.
    #[error("query is not supported by ordinary maintenance")]
    UnsupportedQuery,
    /// A resolve command omitted its recomputed provider-effect review.
    #[error("maintenance resolution requires a recomputed decision projection")]
    MissingDecisionProjection,
    /// No maintenance session exists for the supplied opaque identity.
    #[error("maintenance session was not found")]
    SessionNotFound,
    /// An adapter attempted to reuse a session identity for different work.
    #[error("maintenance session identity already exists")]
    DuplicateSession,
}

impl MaintenanceWorkflowError {
    /// Converts workflow failure into a fixed, secret-free client error.
    #[must_use]
    pub fn client_error(&self) -> ClientError {
        let code = match self {
            Self::StaleRevision { .. }
            | Self::DecisionsNotAccepted
            | Self::AuthorizationNotAccepted
            | Self::ReviewMismatch
            | Self::InvalidExecutionTransition => ErrorCode::StateConflict,
            Self::DuplicateDecision
            | Self::IncompleteDecisionSet
            | Self::UnknownChange
            | Self::InvalidResolution
            | Self::DuplicateChange
            | Self::DuplicateEffect
            | Self::InvalidDurableView
            | Self::ReviewBeforeDecisions
            | Self::ReviewWithoutEffects
            | Self::EffectsWithoutReview
            | Self::UnsupportedCommand
            | Self::UnsupportedQuery
            | Self::MissingDecisionProjection => ErrorCode::InvalidRequest,
            Self::SessionNotFound => ErrorCode::ResourceNotFound,
            Self::DuplicateSession => ErrorCode::StateConflict,
        };
        ClientError::new(code, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        MaintenanceChangeId, MaintenanceChangeKind, MaintenanceProviderEffectKind,
        MaintenanceResolution, MaintenanceSurfaceView, MaintenanceTrackView,
    };

    fn track() -> MaintenanceTrackView {
        MaintenanceTrackView {
            track_id: ResourceId::new(),
            title: "Fixture Song".to_owned(),
            artists: vec!["Fixture Artist".to_owned()],
        }
    }

    fn surface(name: &str) -> MaintenanceSurfaceView {
        MaintenanceSurfaceView {
            surface_id: ResourceId::new(),
            name: name.to_owned(),
        }
    }

    fn ambiguous_change() -> MaintenanceChangeView {
        MaintenanceChangeView {
            change_id: MaintenanceChangeId::new(),
            kind: MaintenanceChangeKind::Removal,
            track: Some(track()),
            previous_surface: Some(surface("Old Vibe")),
            current_surface: None,
            summary: "Removed from Old Vibe".to_owned(),
            resolution: None,
            recommended_resolution: None,
            recommendation_reason: None,
        }
    }

    fn effect() -> MaintenanceProviderEffectView {
        MaintenanceProviderEffectView {
            effect_id: ResourceId::new(),
            kind: MaintenanceProviderEffectKind::RemoveTrack,
            track: Some(track()),
            surface: Some(surface("Old Vibe")),
            summary: "Remove Fixture Song from Old Vibe".to_owned(),
        }
    }

    #[test]
    fn ambiguity_then_authorization_is_revision_bound() {
        let change = ambiguous_change();
        let review_id = MaintenanceReviewId::new();
        let mut workflow = MaintenanceWorkflow::new(
            MaintenanceSessionId::new(),
            MaintenanceProjection {
                provider_snapshot_id: ResourceId::new(),
                observed_changes: vec![change.clone()],
                provider_effects: vec![effect()],
                review_id: None,
            },
        )
        .expect("valid session");
        assert_eq!(
            workflow.view().state,
            MaintenanceSessionState::NeedsDecision
        );

        let resolved = workflow
            .resolve(
                1,
                vec![MaintenanceDecision {
                    change_id: change.change_id,
                    resolution: MaintenanceResolution::Exclude,
                }],
                MaintenanceDecisionProjection {
                    provider_effects: vec![effect()],
                    review_id: Some(review_id),
                },
            )
            .expect("exact decision accepted");
        assert_eq!(resolved.revision, 2);
        assert_eq!(
            resolved.state,
            MaintenanceSessionState::ReadyForAuthorization
        );
        assert_eq!(resolved.review_id, Some(review_id));

        let stale = workflow
            .authorize(1, review_id)
            .expect_err("old revision fails");
        assert!(matches!(
            stale,
            MaintenanceWorkflowError::StaleRevision { .. }
        ));
        let authorized = workflow.authorize(2, review_id).expect("review accepted");
        assert_eq!(authorized.state, MaintenanceSessionState::Authorized);
    }

    #[test]
    fn accepted_decision_replaces_predecision_provider_effects() {
        let change = ambiguous_change();
        let review_id = MaintenanceReviewId::new();
        let replacement = MaintenanceProviderEffectView {
            effect_id: ResourceId::new(),
            kind: MaintenanceProviderEffectKind::AddTrack,
            track: Some(track()),
            surface: Some(surface("Restored Vibe")),
            summary: "Restore Fixture Song to Restored Vibe".to_owned(),
        };
        let mut workflow = MaintenanceWorkflow::new(
            MaintenanceSessionId::new(),
            MaintenanceProjection {
                provider_snapshot_id: ResourceId::new(),
                observed_changes: vec![change.clone()],
                provider_effects: vec![effect()],
                review_id: None,
            },
        )
        .unwrap();
        let resolved = workflow
            .resolve(
                1,
                vec![MaintenanceDecision {
                    change_id: change.change_id,
                    resolution: MaintenanceResolution::Restore {
                        destination: surface("Restored Vibe"),
                    },
                }],
                MaintenanceDecisionProjection {
                    provider_effects: vec![replacement.clone()],
                    review_id: Some(review_id),
                },
            )
            .unwrap();
        assert_eq!(resolved.provider_effects, vec![replacement]);
        assert_eq!(resolved.review_id, Some(review_id));
    }

    #[test]
    fn decision_variant_must_match_the_server_observed_gesture() {
        let change = ambiguous_change();
        let mut workflow = MaintenanceWorkflow::new(
            MaintenanceSessionId::new(),
            MaintenanceProjection {
                provider_snapshot_id: ResourceId::new(),
                observed_changes: vec![change.clone()],
                provider_effects: Vec::new(),
                review_id: None,
            },
        )
        .unwrap();
        let error = workflow
            .resolve(
                1,
                vec![MaintenanceDecision {
                    change_id: change.change_id,
                    resolution: MaintenanceResolution::ConsumeIntake {
                        source: surface("Liked Songs"),
                    },
                }],
                MaintenanceDecisionProjection {
                    provider_effects: Vec::new(),
                    review_id: None,
                },
            )
            .expect_err("a removal cannot be rewritten as saved-track cleanup");
        assert_eq!(error, MaintenanceWorkflowError::InvalidResolution);
    }

    #[test]
    fn rebase_clears_old_authorization_and_uses_new_snapshot() {
        let old_review = MaintenanceReviewId::new();
        let mut workflow = MaintenanceWorkflow::new(
            MaintenanceSessionId::new(),
            MaintenanceProjection {
                provider_snapshot_id: ResourceId::new(),
                observed_changes: Vec::new(),
                provider_effects: vec![effect()],
                review_id: Some(old_review),
            },
        )
        .expect("valid session");
        workflow.authorize(1, old_review).expect("review accepted");

        let new_snapshot = ResourceId::new();
        let rebased = workflow
            .rebase(
                2,
                MaintenanceProjection {
                    provider_snapshot_id: new_snapshot,
                    observed_changes: Vec::new(),
                    provider_effects: Vec::new(),
                    review_id: None,
                },
            )
            .expect("new provider state wins");
        assert_eq!(rebased.provider_snapshot_id, new_snapshot);
        assert_eq!(rebased.state, MaintenanceSessionState::InSync);
        assert_eq!(rebased.review_id, None);
    }

    #[test]
    fn record_only_provider_order_converges_without_authorization() {
        let change = MaintenanceChangeView {
            change_id: MaintenanceChangeId::new(),
            kind: MaintenanceChangeKind::Reorder,
            track: None,
            previous_surface: None,
            current_surface: Some(surface("Celluloid Mehfil")),
            summary: "Accepted current provider order".to_owned(),
            resolution: Some(MaintenanceResolution::KeepObserved),
            recommended_resolution: None,
            recommendation_reason: None,
        };
        let workflow = MaintenanceWorkflow::new(
            MaintenanceSessionId::new(),
            MaintenanceProjection {
                provider_snapshot_id: ResourceId::new(),
                observed_changes: vec![change],
                provider_effects: Vec::new(),
                review_id: None,
            },
        )
        .expect("record-only state");
        assert_eq!(workflow.view().state, MaintenanceSessionState::InSync);
        assert_eq!(
            workflow.view().allowed_actions,
            vec![MaintenanceAllowedAction::Refresh]
        );
    }

    #[test]
    fn durable_rehydration_rejects_tampered_state_or_actions() {
        let workflow = MaintenanceWorkflow::new(
            MaintenanceSessionId::new(),
            MaintenanceProjection {
                provider_snapshot_id: ResourceId::new(),
                observed_changes: Vec::new(),
                provider_effects: Vec::new(),
                review_id: None,
            },
        )
        .unwrap();
        assert!(MaintenanceWorkflow::from_view(workflow.view()).is_ok());

        let mut tampered = workflow.view();
        tampered.allowed_actions = vec![MaintenanceAllowedAction::Authorize];
        assert_eq!(
            MaintenanceWorkflow::from_view(tampered).unwrap_err(),
            MaintenanceWorkflowError::InvalidDurableView
        );
    }

    #[test]
    fn verification_consumes_the_exact_review_after_a_fresh_snapshot() {
        let review_id = MaintenanceReviewId::new();
        let mut workflow = MaintenanceWorkflow::new(
            MaintenanceSessionId::new(),
            MaintenanceProjection {
                provider_snapshot_id: ResourceId::new(),
                observed_changes: Vec::new(),
                provider_effects: vec![effect()],
                review_id: Some(review_id),
            },
        )
        .unwrap();
        workflow.authorize(1, review_id).unwrap();
        workflow
            .mark_execution_state(MaintenanceSessionState::Applying)
            .unwrap();
        workflow
            .mark_execution_state(MaintenanceSessionState::Verifying)
            .unwrap();
        let verified_snapshot = ResourceId::new();
        let completed = workflow
            .complete_verification(MaintenanceProjection {
                provider_snapshot_id: verified_snapshot,
                observed_changes: Vec::new(),
                provider_effects: Vec::new(),
                review_id: None,
            })
            .unwrap();
        assert_eq!(completed.provider_snapshot_id, verified_snapshot);
        assert_eq!(completed.state, MaintenanceSessionState::InSync);
        assert!(completed.provider_effects.is_empty());
        assert_eq!(completed.review_id, None);
        MaintenanceWorkflow::from_view(completed).expect("verified view remains durable");
    }
}
