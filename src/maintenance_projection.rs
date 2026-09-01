//! Retry-safe projection of provider-observed maintenance into canonical intent.

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};
use sqlx::Row as _;
use storexa::Database;
use uuid::Uuid;

use crate::{
    ChordriftError,
    contract::{
        ClientError, ErrorCode, MaintenanceChangeKind, MaintenanceChangeView,
        MaintenanceProviderEffectKind, MaintenanceProviderEffectView, MaintenanceResolution,
        MaintenanceReviewId, MaintenanceSessionView, ResourceId,
    },
    intake::{self, SavedTrackDisposition},
    maintenance::{MaintenanceDecisionProjection, MaintenanceProjection},
    proposals,
    service::AuthenticatedSubject,
    tracks,
};

/// Current placement policy for a newly assigned intake track.
///
/// This is deliberately owned by the Rust application layer rather than a
/// client or Spotify adapter. A future contract may replace it with an exact
/// top/bottom/specific-position choice; the beta default is the top.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntakePlacementPolicy {
    /// Insert before the current first playlist item.
    Top,
}

/// Returns the application-owned default for new intake placement.
#[must_use]
pub const fn intake_placement_policy() -> IntakePlacementPolicy {
    IntakePlacementPolicy::Top
}

/// Applies only record-only, user-authored provider intent to the canonical model.
pub struct CanonicalMaintenanceProjector<'a> {
    database: &'a Database,
}

impl<'a> CanonicalMaintenanceProjector<'a> {
    /// Creates a projector over the worker's canonical PostgreSQL connection.
    pub const fn new(database: &'a Database) -> Self {
        Self { database }
    }

    /// Converges every currently resolved gesture and approves only a complete,
    /// fully resolved generation. Retrying an already projected view is a no-op.
    pub async fn project(
        &self,
        subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
        view: &MaintenanceSessionView,
    ) -> Result<(), ClientError> {
        let account_label: String = sqlx::query_scalar(
            "SELECT account_label FROM provider_accounts
              WHERE id = $1 AND chordrift_account_id = $2 AND provider = 'spotify'",
        )
        .bind(provider_connection_id.as_uuid())
        .bind(subject.account_id.as_uuid())
        .fetch_optional(self.database.pool())
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(|| ClientError::new(ErrorCode::PermissionDenied, false))?;
        let actions = canonical_actions(&view.observed_changes);
        if actions.is_empty() || self.all_satisfied(provider_connection_id, &actions).await? {
            return Ok(());
        }
        let status = proposals::status(self.database, &account_label)
            .await
            .map_err(map_domain_error)?;
        if status.state == "approved" {
            proposals::fork_approved_for_maintenance(self.database, &account_label)
                .await
                .map_err(map_domain_error)?;
        } else if status.state != "proposed" {
            return Err(ClientError::new(ErrorCode::StateConflict, false));
        }
        let destinations = proposals::list(self.database, &account_label)
            .await
            .map_err(map_domain_error)?;
        let mut destination_keys = BTreeMap::new();
        for destination in destinations {
            let key = destination.name.trim().to_lowercase();
            if destination_keys
                .insert(key, destination.stable_key)
                .is_some()
            {
                return Err(ClientError::new(ErrorCode::StateConflict, false));
            }
        }
        let mut assignments: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut exclusions = BTreeSet::new();
        let mut reorders = BTreeSet::new();
        let mut saved_dispositions = BTreeMap::new();
        for action in &actions {
            match action {
                CanonicalAction::Place {
                    track_id,
                    destination,
                } => {
                    let spotify_id = self.spotify_id(*track_id).await?;
                    if self
                        .active_exclusion(provider_connection_id, *track_id)
                        .await?
                    {
                        tracks::restore(
                            self.database,
                            &account_label,
                            &spotify_id,
                            "Restored by observed provider placement",
                            &spotify_id,
                        )
                        .await
                        .map_err(map_domain_error)?;
                    }
                    let stable_key = destination_keys
                        .get(&destination.trim().to_lowercase())
                        .cloned()
                        .ok_or_else(|| ClientError::new(ErrorCode::StateConflict, false))?;
                    assignments.entry(stable_key).or_default().push(spotify_id);
                }
                CanonicalAction::Exclude { track_id } => {
                    exclusions.insert(self.spotify_id(*track_id).await?);
                }
                CanonicalAction::Reorder { destination } => {
                    reorders.insert(
                        destination_keys
                            .get(&destination.trim().to_lowercase())
                            .cloned()
                            .ok_or_else(|| ClientError::new(ErrorCode::StateConflict, false))?,
                    );
                }
                CanonicalAction::SavedDisposition {
                    track_id,
                    disposition,
                } => {
                    let spotify_id = self.spotify_id(*track_id).await?;
                    if saved_dispositions
                        .insert(spotify_id, *disposition)
                        .is_some_and(|existing| existing != *disposition)
                    {
                        return Err(ClientError::new(ErrorCode::StateConflict, false));
                    }
                }
            }
        }
        for (stable_key, spotify_ids) in assignments {
            proposals::assign_many(
                self.database,
                &account_label,
                &spotify_ids,
                &stable_key,
                "Accepted from current provider state",
            )
            .await
            .map_err(map_domain_error)?;
        }
        for spotify_id in exclusions {
            tracks::exclude(
                self.database,
                &account_label,
                &spotify_id,
                "Removed from a managed provider playlist",
                &spotify_id,
            )
            .await
            .map_err(map_domain_error)?;
        }
        for stable_key in reorders {
            proposals::align_provider_order(self.database, &account_label, &stable_key)
                .await
                .map_err(map_domain_error)?;
        }
        // A request to clear Liked Songs is intentionally projected last. This
        // ensures the durable canonical destination/exclusion intent exists
        // before a provider effect can remove the intake membership.
        for (spotify_id, disposition) in saved_dispositions {
            intake::set_saved_track_disposition(
                self.database,
                &account_label,
                &spotify_id,
                disposition,
                "Accepted from ordinary maintenance",
            )
            .await
            .map_err(map_domain_error)?;
        }
        if view
            .observed_changes
            .iter()
            .all(|change| change.resolution.is_some())
        {
            let status = proposals::status(self.database, &account_label)
                .await
                .map_err(map_domain_error)?;
            if status.state == "proposed" && status.coverage_complete {
                proposals::approve(self.database, &account_label, status.generation_id)
                    .await
                    .map_err(map_domain_error)?;
            }
        }
        Ok(())
    }

    async fn all_satisfied(
        &self,
        provider_connection_id: ResourceId,
        actions: &[CanonicalAction],
    ) -> Result<bool, ClientError> {
        for action in actions {
            let satisfied = match action {
                CanonicalAction::Place {
                    track_id,
                    destination,
                } => sqlx::query_scalar(
                    "WITH latest AS (
                           SELECT id FROM playlist_generations
                           WHERE provider_account_id = $1
                             AND status IN ('proposed', 'approved')
                           ORDER BY created_at DESC, id DESC LIMIT 1
                         )
                         SELECT EXISTS (
                           SELECT 1 FROM latest
                           JOIN playlist_generations generation ON generation.id = latest.id
                           JOIN playlists playlist ON playlist.generation_id = generation.id
                           JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
                           WHERE membership.track_id = $2
                             AND lower(playlist.name) = lower($3)
                             AND NOT EXISTS (
                               SELECT 1 FROM excluded_tracks exclusion
                               WHERE exclusion.provider_account_id = $1
                                 AND exclusion.track_id = $2
                                 AND exclusion.restored_at IS NULL))",
                )
                .bind(provider_connection_id.as_uuid())
                .bind(track_id)
                .bind(destination)
                .fetch_one(self.database.pool())
                .await
                .map_err(|_| unavailable())?,
                CanonicalAction::Exclude { track_id } => {
                    self.active_exclusion(provider_connection_id, *track_id)
                        .await?
                }
                CanonicalAction::Reorder { destination } => {
                    self.order_matches(provider_connection_id, destination)
                        .await?
                }
                CanonicalAction::SavedDisposition {
                    track_id,
                    disposition,
                } => sqlx::query_scalar(
                    "SELECT EXISTS (
                           SELECT 1
                           FROM provider_accounts account
                           JOIN playlist_surfaces surface
                             ON surface.chordrift_account_id = account.chordrift_account_id
                            AND surface.stable_key = 'provider-saved-tracks:' || account.id::text
                           JOIN playlist_track_directives directive
                             ON directive.chordrift_account_id = surface.chordrift_account_id
                            AND directive.surface_id = surface.id
                            AND directive.track_id = $2
                            AND directive.superseded_at IS NULL
                           WHERE account.id = $1 AND CASE directive.directive
                             WHEN 'include' THEN 'preserve'
                             WHEN 'exclude' THEN 'clear_after_verified_assignment'
                           END = $3)",
                )
                .bind(provider_connection_id.as_uuid())
                .bind(track_id)
                .bind(disposition.as_str())
                .fetch_one(self.database.pool())
                .await
                .map_err(|_| unavailable())?,
            };
            if !satisfied {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn spotify_id(&self, track_id: Uuid) -> Result<String, ClientError> {
        sqlx::query_scalar(
            "SELECT provider_track_id FROM provider_tracks
             WHERE provider = 'spotify' AND track_id = $1
             ORDER BY id LIMIT 1",
        )
        .bind(track_id)
        .fetch_optional(self.database.pool())
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(|| ClientError::new(ErrorCode::StateConflict, false))
    }

    async fn active_exclusion(
        &self,
        provider_connection_id: ResourceId,
        track_id: Uuid,
    ) -> Result<bool, ClientError> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM excluded_tracks
              WHERE provider_account_id = $1 AND track_id = $2
                AND restored_at IS NULL)",
        )
        .bind(provider_connection_id.as_uuid())
        .bind(track_id)
        .fetch_one(self.database.pool())
        .await
        .map_err(|_| unavailable())
    }

    async fn order_matches(
        &self,
        provider_connection_id: ResourceId,
        destination: &str,
    ) -> Result<bool, ClientError> {
        let row = sqlx::query(
            "WITH latest_generation AS (
               SELECT id FROM playlist_generations
               WHERE provider_account_id = $1 AND status IN ('proposed', 'approved')
               ORDER BY created_at DESC, id DESC LIMIT 1
             ), target AS (
               SELECT playlist.id, playlist.concept_id
               FROM latest_generation
               JOIN playlists playlist ON playlist.generation_id = latest_generation.id
               WHERE lower(playlist.name) = lower($2)
             )
             SELECT
               ARRAY(SELECT provider_track.track_id
                     FROM target
                     JOIN current_spotify_playlists current
                       ON current.provider_account_id = $1
                      AND current.signal_class = 'canonical'
                     JOIN provider_playlists provider_playlist
                       ON provider_playlist.id = current.provider_playlist_id
                      AND provider_playlist.concept_id = target.concept_id
                     JOIN provider_observed_playlist_tracks membership
                       ON membership.snapshot_id = current.snapshot_id
                      AND membership.provider_playlist_id = current.provider_playlist_id
                     JOIN provider_tracks provider_track
                       ON provider_track.id = membership.provider_track_id
                     ORDER BY membership.position) AS provider_order,
               ARRAY(SELECT membership.track_id
                     FROM target JOIN playlist_tracks membership
                       ON membership.playlist_id = target.id
                     ORDER BY membership.position) AS model_order",
        )
        .bind(provider_connection_id.as_uuid())
        .bind(destination)
        .fetch_one(self.database.pool())
        .await
        .map_err(|_| unavailable())?;
        let provider_order: Vec<Uuid> = row.try_get("provider_order").map_err(|_| unavailable())?;
        let model_order: Vec<Uuid> = row.try_get("model_order").map_err(|_| unavailable())?;
        Ok(!provider_order.is_empty() && provider_order == model_order)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CanonicalAction {
    Place {
        track_id: Uuid,
        destination: String,
    },
    Exclude {
        track_id: Uuid,
    },
    Reorder {
        destination: String,
    },
    SavedDisposition {
        track_id: Uuid,
        disposition: SavedTrackDisposition,
    },
}

fn canonical_actions(changes: &[MaintenanceChangeView]) -> Vec<CanonicalAction> {
    changes.iter().filter_map(canonical_action).collect()
}

fn canonical_action(change: &MaintenanceChangeView) -> Option<CanonicalAction> {
    let resolution = change.resolution.as_ref()?;
    let track_id = change.track.as_ref().map(|track| track.track_id.as_uuid());
    match resolution {
        MaintenanceResolution::Place { destination }
        | MaintenanceResolution::Restore { destination } => Some(CanonicalAction::Place {
            track_id: track_id?,
            destination: destination.name.clone(),
        }),
        MaintenanceResolution::Exclude => Some(CanonicalAction::Exclude {
            track_id: track_id?,
        }),
        MaintenanceResolution::KeepObserved => match change.kind {
            MaintenanceChangeKind::DirectIntake | MaintenanceChangeKind::Reclassification => {
                Some(CanonicalAction::Place {
                    track_id: track_id?,
                    destination: change.current_surface.as_ref()?.name.clone(),
                })
            }
            MaintenanceChangeKind::Removal => Some(CanonicalAction::Exclude {
                track_id: track_id?,
            }),
            MaintenanceChangeKind::Reorder => Some(CanonicalAction::Reorder {
                destination: change.current_surface.as_ref()?.name.clone(),
            }),
            MaintenanceChangeKind::SavedState => Some(CanonicalAction::SavedDisposition {
                track_id: track_id?,
                disposition: SavedTrackDisposition::Preserve,
            }),
            _ => None,
        },
        MaintenanceResolution::ConsumeIntake { .. }
            if change.kind == MaintenanceChangeKind::SavedState =>
        {
            Some(CanonicalAction::SavedDisposition {
                track_id: track_id?,
                disposition: SavedTrackDisposition::ClearAfterVerifiedAssignment,
            })
        }
        MaintenanceResolution::ConsumeIntake { .. } => None,
    }
}

/// Adds the exact next provider review implied by resolved intake choices.
///
/// A new canonical placement is always published and verified before a saved
/// intake source may be consumed. Consequently one review contains additions
/// or saved-state removals, never both.
pub fn attach_maintenance_provider_effects(
    mut projection: MaintenanceProjection,
) -> MaintenanceProjection {
    let decision = maintenance_provider_effects(
        projection.provider_snapshot_id,
        &projection.observed_changes,
    );
    projection.provider_effects = decision.provider_effects;
    projection.review_id = decision.review_id;
    projection
}

/// Computes the next safe provider stage after one exact decision set.
pub fn maintenance_provider_effects(
    snapshot_id: ResourceId,
    changes: &[MaintenanceChangeView],
) -> MaintenanceDecisionProjection {
    if changes.iter().any(|change| change.resolution.is_none()) {
        return MaintenanceDecisionProjection {
            provider_effects: Vec::new(),
            review_id: None,
        };
    }
    let additions: Vec<_> = changes
        .iter()
        .filter_map(|change| {
            if change.kind != MaintenanceChangeKind::DirectIntake
                || change.current_surface.is_some()
            {
                return None;
            }
            let MaintenanceResolution::Place { destination } = change.resolution.as_ref()? else {
                return None;
            };
            let track = change.track.clone()?;
            Some(MaintenanceProviderEffectView {
                effect_id: ResourceId::from_uuid(stable_uuid(
                    "placement-effect",
                    &format!(
                        "{}:{}:{}",
                        snapshot_id, track.track_id, destination.surface_id
                    ),
                )),
                kind: MaintenanceProviderEffectKind::AddTrack,
                track: Some(track.clone()),
                surface: Some(destination.clone()),
                summary: format!("Add {} to the top of {}", track.title, destination.name),
            })
        })
        .collect();
    let effects: Vec<_> = if additions.is_empty() {
        changes
            .iter()
            .filter_map(|change| {
                if change.kind != MaintenanceChangeKind::SavedState {
                    return None;
                }
                let MaintenanceResolution::ConsumeIntake { source } = change.resolution.as_ref()?
                else {
                    return None;
                };
                if change.current_surface.as_ref() != Some(source) {
                    return None;
                }
                let track = change.track.clone()?;
                Some(MaintenanceProviderEffectView {
                    effect_id: ResourceId::from_uuid(stable_uuid(
                        "saved-effect",
                        &format!("{}:{}", snapshot_id, track.track_id),
                    )),
                    kind: MaintenanceProviderEffectKind::UpdateSavedState,
                    track: Some(track.clone()),
                    surface: Some(source.clone()),
                    summary: format!("Remove {} from Liked Songs", track.title),
                })
            })
            .collect()
    } else {
        additions
    };
    let review_id = (!effects.is_empty()).then(|| {
        MaintenanceReviewId::from_uuid(stable_uuid(
            "saved-review",
            &format!(
                "{}:{}",
                snapshot_id,
                effects
                    .iter()
                    .map(|effect| effect.effect_id.to_string())
                    .collect::<Vec<_>>()
                    .join(":")
            ),
        ))
    });
    MaintenanceDecisionProjection {
        provider_effects: effects,
        review_id,
    }
}

fn stable_uuid(namespace: &str, value: &str) -> Uuid {
    let digest = Sha256::digest([namespace.as_bytes(), b"\0", value.as_bytes()].concat());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn map_domain_error(error: ChordriftError) -> ClientError {
    match error {
        ChordriftError::Configuration(_) => ClientError::new(ErrorCode::StateConflict, false),
        _ => unavailable(),
    }
}

fn unavailable() -> ClientError {
    ClientError::new(ErrorCode::DependencyUnavailable, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{MaintenanceChangeId, MaintenanceSurfaceView, MaintenanceTrackView};

    fn change(
        kind: MaintenanceChangeKind,
        resolution: MaintenanceResolution,
    ) -> MaintenanceChangeView {
        MaintenanceChangeView {
            change_id: MaintenanceChangeId::new(),
            kind,
            track: Some(MaintenanceTrackView {
                track_id: ResourceId::new(),
                title: "Song".to_owned(),
                artists: vec!["Artist".to_owned()],
            }),
            previous_surface: None,
            current_surface: Some(MaintenanceSurfaceView {
                surface_id: ResourceId::new(),
                name: "Destination".to_owned(),
            }),
            summary: "Observed".to_owned(),
            resolution: Some(resolution),
            recommended_resolution: None,
            recommendation_reason: None,
        }
    }

    #[test]
    fn keep_observed_maps_provider_gestures_to_canonical_intent() {
        assert!(matches!(
            canonical_action(&change(
                MaintenanceChangeKind::Reclassification,
                MaintenanceResolution::KeepObserved
            )),
            Some(CanonicalAction::Place { destination, .. }) if destination == "Destination"
        ));
        assert!(matches!(
            canonical_action(&change(
                MaintenanceChangeKind::Removal,
                MaintenanceResolution::KeepObserved
            )),
            Some(CanonicalAction::Exclude { .. })
        ));
    }

    #[test]
    fn saved_choice_is_intent_first_and_effect_review_is_all_decisions_bound() {
        let source = MaintenanceSurfaceView {
            surface_id: ResourceId::new(),
            name: "Liked Songs".to_owned(),
        };
        let mut consume = change(
            MaintenanceChangeKind::SavedState,
            MaintenanceResolution::ConsumeIntake {
                source: source.clone(),
            },
        );
        consume.current_surface = Some(source);
        assert!(matches!(
            canonical_action(&consume),
            Some(CanonicalAction::SavedDisposition {
                disposition: SavedTrackDisposition::ClearAfterVerifiedAssignment,
                ..
            })
        ));
        let mut unresolved = change(
            MaintenanceChangeKind::DirectIntake,
            MaintenanceResolution::KeepObserved,
        );
        unresolved.resolution = None;
        let snapshot = ResourceId::new();
        let withheld = maintenance_provider_effects(snapshot, &[consume.clone(), unresolved]);
        assert!(withheld.provider_effects.is_empty());
        assert!(withheld.review_id.is_none());

        let reviewed = maintenance_provider_effects(snapshot, &[consume]);
        assert_eq!(reviewed.provider_effects.len(), 1);
        assert_eq!(
            reviewed.provider_effects[0].kind,
            MaintenanceProviderEffectKind::UpdateSavedState
        );
        assert!(reviewed.review_id.is_some());
    }

    #[test]
    fn liked_only_placement_is_added_before_saved_intake_can_be_consumed() {
        let liked = MaintenanceSurfaceView {
            surface_id: ResourceId::new(),
            name: "Liked Songs".to_owned(),
        };
        let destination = MaintenanceSurfaceView {
            surface_id: ResourceId::new(),
            name: "Neon Affection".to_owned(),
        };
        let mut placement = change(
            MaintenanceChangeKind::DirectIntake,
            MaintenanceResolution::Place {
                destination: destination.clone(),
            },
        );
        placement.previous_surface = Some(liked.clone());
        placement.current_surface = None;
        let mut consume = change(
            MaintenanceChangeKind::SavedState,
            MaintenanceResolution::ConsumeIntake {
                source: liked.clone(),
            },
        );
        consume.current_surface = Some(liked);

        let first =
            maintenance_provider_effects(ResourceId::new(), &[placement.clone(), consume.clone()]);
        assert_eq!(first.provider_effects.len(), 1);
        assert_eq!(
            first.provider_effects[0].kind,
            MaintenanceProviderEffectKind::AddTrack
        );
        assert_eq!(first.provider_effects[0].surface, Some(destination.clone()));

        placement.current_surface = Some(destination);
        let second = maintenance_provider_effects(ResourceId::new(), &[placement, consume]);
        assert_eq!(second.provider_effects.len(), 1);
        assert_eq!(
            second.provider_effects[0].kind,
            MaintenanceProviderEffectKind::UpdateSavedState
        );
    }
}
