//! Small, deterministic provider-behavior acceptance matrix.
//!
//! These scenarios model a completely fake account. They exercise the same
//! wrapper-neutral maintenance DTOs and Rust state machine used by web and CLI,
//! never open a network connection, and intentionally use only six tracks.

use std::collections::{BTreeMap, BTreeSet};

use chordrift::{
    contract::{
        MaintenanceChangeId, MaintenanceChangeKind, MaintenanceChangeView,
        MaintenanceProviderEffectKind, MaintenanceProviderEffectView, MaintenanceResolution,
        MaintenanceSessionId, MaintenanceSessionState, MaintenanceSurfaceView,
        MaintenanceTrackView, ResourceId,
    },
    maintenance::{MaintenanceProjection, MaintenanceWorkflow},
    maintenance_projection::maintenance_provider_effects,
};
use uuid::Uuid;

#[derive(Clone, Copy)]
enum Gesture {
    Add {
        track: u128,
        destination: &'static str,
    },
    Remove {
        track: u128,
        source: &'static str,
    },
    Move {
        track: u128,
        source: &'static str,
        destination: &'static str,
    },
    Reorder {
        playlist: &'static str,
    },
    Like {
        track: u128,
    },
}

struct FakeProviderAccount {
    next_snapshot: u128,
    delayed: Option<Vec<Gesture>>,
    playlists: BTreeMap<String, Vec<u128>>,
    liked: BTreeSet<u128>,
    writes: Vec<String>,
    fail_next_write: bool,
}

impl FakeProviderAccount {
    fn new() -> Self {
        Self {
            next_snapshot: 100,
            delayed: None,
            playlists: BTreeMap::new(),
            liked: BTreeSet::new(),
            writes: Vec::new(),
            fail_next_write: false,
        }
    }

    fn observe(&mut self, gestures: Vec<Gesture>) -> MaintenanceProjection {
        self.next_snapshot += 1;
        projection(self.next_snapshot, gestures)
    }

    fn delay_once(&mut self, gestures: Vec<Gesture>) -> MaintenanceProjection {
        self.delayed = Some(gestures);
        self.observe(Vec::new())
    }

    fn observe_delayed(&mut self) -> MaintenanceProjection {
        let gestures = self.delayed.take().expect("one delayed observation exists");
        self.observe(gestures)
    }

    fn apply(&mut self, effect: &MaintenanceProviderEffectView) -> Result<bool, &'static str> {
        if self.fail_next_write {
            self.fail_next_write = false;
            return Err("injected provider failure");
        }
        let track = effect
            .track
            .as_ref()
            .expect("fixture provider effect names a track")
            .track_id
            .as_uuid()
            .as_u128();
        match effect.kind {
            MaintenanceProviderEffectKind::AddTrack => {
                let destination = &effect
                    .surface
                    .as_ref()
                    .expect("addition names a destination")
                    .name;
                let membership = self.playlists.entry(destination.clone()).or_default();
                if !membership.contains(&track) {
                    membership.insert(0, track);
                    self.writes.push(format!("add:{track}:{destination}"));
                    return Ok(true);
                }
            }
            MaintenanceProviderEffectKind::UpdateSavedState => {
                if !self
                    .playlists
                    .values()
                    .any(|tracks| tracks.contains(&track))
                {
                    return Err("refused to consume the track before verified placement");
                }
                if self.liked.remove(&track) {
                    self.writes.push(format!("unlike:{track}"));
                    return Ok(true);
                }
            }
            _ => return Err("unsupported fixture provider effect"),
        }
        Ok(false)
    }
}

#[derive(Default)]
struct FakeDatabase {
    canonical_placements: BTreeMap<u128, String>,
    sessions: BTreeMap<MaintenanceSessionId, chordrift::contract::MaintenanceSessionView>,
    write_receipts: BTreeSet<String>,
}

impl FakeDatabase {
    fn record_placement(&mut self, track: u128, destination: &str) {
        self.canonical_placements
            .insert(track, destination.to_owned());
    }

    fn apply_stage(
        &mut self,
        provider: &mut FakeProviderAccount,
        effects: &[MaintenanceProviderEffectView],
    ) -> Result<(), &'static str> {
        for effect in effects {
            if provider.apply(effect)? {
                self.write_receipts.insert(effect.effect_id.to_string());
            }
        }
        Ok(())
    }

    fn persist(&mut self, view: chordrift::contract::MaintenanceSessionView) {
        self.sessions.insert(view.session_id, view);
    }

    fn restart(&self, session_id: MaintenanceSessionId) -> MaintenanceWorkflow {
        MaintenanceWorkflow::from_view(
            self.sessions
                .get(&session_id)
                .expect("fake database contains durable session")
                .clone(),
        )
        .expect("persisted production DTO rehydrates")
    }
}

fn id(value: u128) -> ResourceId {
    ResourceId::from_uuid(Uuid::from_u128(value))
}

fn surface(name: &str) -> MaintenanceSurfaceView {
    MaintenanceSurfaceView {
        surface_id: id(stable_number(name)),
        name: name.to_owned(),
    }
}

fn track(value: u128) -> MaintenanceTrackView {
    MaintenanceTrackView {
        track_id: id(value),
        title: format!("Fixture Track {value}"),
        artists: vec!["Fixture Artist".to_owned()],
    }
}

fn stable_number(value: &str) -> u128 {
    value.bytes().fold(17_u128, |state, byte| {
        state.wrapping_mul(257) + u128::from(byte)
    })
}

fn projection(snapshot: u128, gestures: Vec<Gesture>) -> MaintenanceProjection {
    let observed_changes = gestures
        .into_iter()
        .enumerate()
        .map(|(index, gesture)| {
            let change_id = MaintenanceChangeId::from_uuid(Uuid::from_u128(
                snapshot * 100 + u128::try_from(index).expect("fixture index fits u128"),
            ));
            match gesture {
                Gesture::Add {
                    track: value,
                    destination,
                } => MaintenanceChangeView {
                    change_id,
                    kind: MaintenanceChangeKind::DirectIntake,
                    track: Some(track(value)),
                    previous_surface: Some(surface("New intake")),
                    current_surface: Some(surface(destination)),
                    summary: format!("Accepted direct placement in {destination}"),
                    resolution: Some(MaintenanceResolution::Place {
                        destination: surface(destination),
                    }),
                    recommended_resolution: None,
                    recommendation_reason: None,
                },
                Gesture::Remove {
                    track: value,
                    source,
                } => MaintenanceChangeView {
                    change_id,
                    kind: MaintenanceChangeKind::Removal,
                    track: Some(track(value)),
                    previous_surface: Some(surface(source)),
                    current_surface: None,
                    summary: format!("Accepted removal from {source}"),
                    resolution: Some(MaintenanceResolution::Exclude),
                    recommended_resolution: None,
                    recommendation_reason: None,
                },
                Gesture::Move {
                    track: value,
                    source,
                    destination,
                } => MaintenanceChangeView {
                    change_id,
                    kind: MaintenanceChangeKind::Reclassification,
                    track: Some(track(value)),
                    previous_surface: Some(surface(source)),
                    current_surface: Some(surface(destination)),
                    summary: format!("Accepted move from {source} to {destination}"),
                    resolution: Some(MaintenanceResolution::Place {
                        destination: surface(destination),
                    }),
                    recommended_resolution: None,
                    recommendation_reason: None,
                },
                Gesture::Reorder { playlist } => MaintenanceChangeView {
                    change_id,
                    kind: MaintenanceChangeKind::Reorder,
                    track: None,
                    previous_surface: Some(surface(playlist)),
                    current_surface: Some(surface(playlist)),
                    summary: format!("Accepted provider order for {playlist}"),
                    resolution: Some(MaintenanceResolution::KeepObserved),
                    recommended_resolution: None,
                    recommendation_reason: None,
                },
                Gesture::Like { track: value } => MaintenanceChangeView {
                    change_id,
                    kind: MaintenanceChangeKind::SavedState,
                    track: Some(track(value)),
                    previous_surface: None,
                    current_surface: Some(surface("Liked Songs")),
                    summary: "Choose whether the track remains liked".to_owned(),
                    resolution: None,
                    recommended_resolution: None,
                    recommendation_reason: None,
                },
            }
        })
        .collect();
    MaintenanceProjection {
        provider_snapshot_id: id(snapshot),
        observed_changes,
        provider_effects: Vec::new(),
        review_id: None,
    }
}

fn liked_placement_changes(
    provider: &FakeProviderAccount,
    track_id: u128,
    destination: &str,
) -> Vec<MaintenanceChangeView> {
    let destination_surface = surface(destination);
    let liked_surface = surface("Liked Songs");
    let placed = provider
        .playlists
        .get(destination)
        .is_some_and(|tracks| tracks.contains(&track_id));
    let mut changes = vec![MaintenanceChangeView {
        change_id: MaintenanceChangeId::from_uuid(Uuid::from_u128(9001)),
        kind: MaintenanceChangeKind::DirectIntake,
        track: Some(track(track_id)),
        previous_surface: Some(liked_surface.clone()),
        current_surface: placed.then_some(destination_surface.clone()),
        summary: format!("Place Fixture Track {track_id} in {destination}"),
        resolution: Some(MaintenanceResolution::Place {
            destination: destination_surface,
        }),
        recommended_resolution: None,
        recommendation_reason: None,
    }];
    if provider.liked.contains(&track_id) {
        changes.push(MaintenanceChangeView {
            change_id: MaintenanceChangeId::from_uuid(Uuid::from_u128(9002)),
            kind: MaintenanceChangeKind::SavedState,
            track: Some(track(track_id)),
            previous_surface: None,
            current_surface: Some(liked_surface.clone()),
            summary: format!("Remove Fixture Track {track_id} from Liked Songs after placement"),
            resolution: Some(MaintenanceResolution::ConsumeIntake {
                source: liked_surface,
            }),
            recommended_resolution: None,
            recommendation_reason: None,
        });
    }
    changes
}

fn liked_placement_projection(
    snapshot: u128,
    provider: &FakeProviderAccount,
    track_id: u128,
    destination: &str,
) -> MaintenanceProjection {
    let observed_changes = liked_placement_changes(provider, track_id, destination);
    let decision = maintenance_provider_effects(id(snapshot), &observed_changes);
    MaintenanceProjection {
        provider_snapshot_id: id(snapshot),
        observed_changes,
        provider_effects: decision.provider_effects,
        review_id: decision.review_id,
    }
}

#[test]
fn single_provider_gestures_are_visible_without_provider_writes() {
    let cases = [
        Gesture::Add {
            track: 1,
            destination: "Cinema Monsoon",
        },
        Gesture::Remove {
            track: 2,
            source: "Neon Affection",
        },
        Gesture::Move {
            track: 3,
            source: "Rasa Archive",
            destination: "Cinema Monsoon",
        },
        Gesture::Reorder {
            playlist: "Celluloid Mehfil",
        },
    ];
    for gesture in cases {
        let mut provider = FakeProviderAccount::new();
        let view =
            MaintenanceWorkflow::new(MaintenanceSessionId::new(), provider.observe(vec![gesture]))
                .expect("single provider gesture is a valid maintenance projection")
                .view();
        assert_eq!(view.observed_changes.len(), 1);
        assert!(view.provider_effects.is_empty());
        assert_eq!(view.state, MaintenanceSessionState::InSync);
    }
}

#[test]
fn composite_provider_snapshot_keeps_every_distinct_gesture_once() {
    let mut provider = FakeProviderAccount::new();
    let gestures = vec![
        Gesture::Add {
            track: 1,
            destination: "Cinema Monsoon",
        },
        Gesture::Remove {
            track: 2,
            source: "Neon Affection",
        },
        Gesture::Move {
            track: 3,
            source: "Rasa Archive",
            destination: "Cinema Monsoon",
        },
        Gesture::Reorder {
            playlist: "Celluloid Mehfil",
        },
        Gesture::Like { track: 4 },
    ];
    let view = MaintenanceWorkflow::new(MaintenanceSessionId::new(), provider.observe(gestures))
        .expect("composite snapshot is valid")
        .view();

    assert_eq!(view.observed_changes.len(), 5);
    assert_eq!(view.state, MaintenanceSessionState::NeedsDecision);
    assert!(view.provider_effects.is_empty());
    assert_eq!(
        view.observed_changes
            .iter()
            .filter(|change| change.kind == MaintenanceChangeKind::DirectIntake)
            .count(),
        1
    );
}

#[test]
fn delayed_observation_and_interrupted_retry_rebase_to_cumulative_truth() {
    let mut provider = FakeProviderAccount::new();
    let delayed = vec![
        Gesture::Add {
            track: 5,
            destination: "Cinema Monsoon",
        },
        Gesture::Reorder {
            playlist: "Cinema Monsoon",
        },
    ];
    let mut workflow =
        MaintenanceWorkflow::new(MaintenanceSessionId::new(), provider.delay_once(delayed))
            .expect("first provider read can legitimately lag");
    assert!(workflow.view().observed_changes.is_empty());

    let revision = workflow.view().revision;
    let refreshed = workflow
        .rebase(revision, provider.observe_delayed())
        .expect("next complete provider read becomes cumulative truth");
    assert_eq!(refreshed.observed_changes.len(), 2);
    assert!(refreshed.provider_effects.is_empty());

    let replay = workflow
        .rebase(
            refreshed.revision,
            projection(
                102,
                refreshed
                    .observed_changes
                    .iter()
                    .map(|change| match change.kind {
                        MaintenanceChangeKind::DirectIntake => Gesture::Add {
                            track: 5,
                            destination: "Cinema Monsoon",
                        },
                        MaintenanceChangeKind::Reorder => Gesture::Reorder {
                            playlist: "Cinema Monsoon",
                        },
                        _ => unreachable!("fixture contains only direct intake and reorder"),
                    })
                    .collect(),
            ),
        )
        .expect("retrying the same complete truth remains valid");
    assert_eq!(replay.observed_changes.len(), 2);
    assert!(replay.provider_effects.is_empty());
}

#[test]
fn fake_database_and_provider_never_consume_liked_before_verified_placement() {
    let track_id = 6;
    let destination = "Neon Affection";
    let mut provider = FakeProviderAccount::new();
    provider.liked.insert(track_id);
    let mut database = FakeDatabase::default();
    database.record_placement(track_id, destination);

    let first = liked_placement_projection(201, &provider, track_id, destination);
    assert_eq!(first.provider_effects.len(), 1);
    assert_eq!(
        first.provider_effects[0].kind,
        MaintenanceProviderEffectKind::AddTrack
    );

    let session_id = MaintenanceSessionId::new();
    let mut workflow = MaintenanceWorkflow::new(session_id, first.clone())
        .expect("exact addition is a valid durable review");
    let first_view = workflow.view();
    assert_eq!(
        first_view.state,
        MaintenanceSessionState::ReadyForAuthorization
    );
    let first_review = first_view.review_id.expect("addition has exact review");
    workflow
        .authorize(first_view.revision, first_review)
        .expect("user authorizes only the reviewed addition");
    let first_view = workflow
        .mark_execution_state(MaintenanceSessionState::Applying)
        .expect("durable executor starts applying");
    database.persist(first_view);

    provider.fail_next_write = true;
    assert!(
        database
            .apply_stage(&mut provider, &first.provider_effects)
            .is_err()
    );
    assert!(provider.liked.contains(&track_id));
    assert!(
        !provider
            .playlists
            .values()
            .any(|tracks| tracks.contains(&track_id))
    );

    // A worker restart reloads the same production session DTO and retries
    // the exact effect rather than recomputing arbitrary work.
    workflow = database.restart(session_id);
    database
        .apply_stage(&mut provider, &first.provider_effects)
        .expect("retry adds the exact reviewed track");
    let first_view = workflow
        .mark_execution_state(MaintenanceSessionState::Verifying)
        .expect("successful apply advances to verification");
    database.persist(first_view);

    // Simulate a crash after the provider accepted the write but before a new
    // receipt. Replaying the effect is idempotent and adds no duplicate.
    database
        .apply_stage(&mut provider, &first.provider_effects)
        .expect("replay does not duplicate membership");
    assert!(provider.liked.contains(&track_id));
    assert_eq!(provider.playlists[destination], vec![track_id]);

    let second = liked_placement_projection(202, &provider, track_id, destination);
    assert_eq!(second.provider_effects.len(), 1);
    assert_eq!(
        second.provider_effects[0].kind,
        MaintenanceProviderEffectKind::UpdateSavedState
    );
    let second_view = workflow
        .complete_verification(second.clone())
        .expect("fresh observation advances the same durable session");
    assert_eq!(
        second_view.state,
        MaintenanceSessionState::ReadyForAuthorization
    );
    let second_review = second_view
        .review_id
        .expect("cleanup has a separate review");
    workflow
        .authorize(second_view.revision, second_review)
        .expect("user separately authorizes intake cleanup");
    let second_view = workflow
        .mark_execution_state(MaintenanceSessionState::Applying)
        .expect("cleanup starts only after verified placement");
    database.persist(second_view);

    provider.fail_next_write = true;
    assert!(
        database
            .apply_stage(&mut provider, &second.provider_effects)
            .is_err()
    );
    assert!(provider.liked.contains(&track_id));
    assert_eq!(provider.playlists[destination], vec![track_id]);

    workflow = database.restart(session_id);
    database
        .apply_stage(&mut provider, &second.provider_effects)
        .expect("verified placement permits saved intake cleanup");
    let final_view = workflow
        .mark_execution_state(MaintenanceSessionState::Verifying)
        .expect("cleanup becomes verifiable");
    database.persist(final_view);

    assert!(!provider.liked.contains(&track_id));
    assert_eq!(provider.playlists[destination], vec![track_id]);
    assert_eq!(
        provider.writes,
        vec!["add:6:Neon Affection".to_owned(), "unlike:6".to_owned()]
    );
    assert_eq!(database.canonical_placements[&track_id], destination);
    assert_eq!(database.write_receipts.len(), 2);
}

#[test]
fn composite_placement_retry_preserves_each_track_and_never_duplicates() {
    let destination = "Cinema Monsoon";
    let mut provider = FakeProviderAccount::new();
    provider.liked.extend([7, 8]);
    let mut database = FakeDatabase::default();
    database.record_placement(7, destination);
    database.record_placement(8, destination);

    let mut changes = liked_placement_changes(&provider, 7, destination);
    let mut second = liked_placement_changes(&provider, 8, destination);
    second[0].change_id = MaintenanceChangeId::from_uuid(Uuid::from_u128(9011));
    second[1].change_id = MaintenanceChangeId::from_uuid(Uuid::from_u128(9012));
    changes.extend(second);
    let additions = maintenance_provider_effects(id(301), &changes);
    assert_eq!(additions.provider_effects.len(), 2);
    assert!(
        additions
            .provider_effects
            .iter()
            .all(|effect| effect.kind == MaintenanceProviderEffectKind::AddTrack)
    );

    database
        .apply_stage(&mut provider, &additions.provider_effects[..1])
        .expect("first enumerated addition succeeds");
    provider.fail_next_write = true;
    assert!(
        database
            .apply_stage(&mut provider, &additions.provider_effects[1..])
            .is_err()
    );
    assert_eq!(provider.playlists[destination], vec![7]);
    assert!(provider.liked.contains(&7));
    assert!(provider.liked.contains(&8));

    database
        .apply_stage(&mut provider, &additions.provider_effects)
        .expect("whole exact review safely resumes");
    assert_eq!(provider.playlists[destination], vec![8, 7]);
    assert_eq!(
        provider.playlists[destination]
            .iter()
            .copied()
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([7, 8])
    );

    let mut cleanup_changes = liked_placement_changes(&provider, 7, destination);
    let mut second_cleanup = liked_placement_changes(&provider, 8, destination);
    second_cleanup[0].change_id = MaintenanceChangeId::from_uuid(Uuid::from_u128(9021));
    second_cleanup[1].change_id = MaintenanceChangeId::from_uuid(Uuid::from_u128(9022));
    cleanup_changes.extend(second_cleanup);
    let cleanup = maintenance_provider_effects(id(302), &cleanup_changes);
    assert_eq!(cleanup.provider_effects.len(), 2);
    assert!(
        cleanup
            .provider_effects
            .iter()
            .all(|effect| effect.kind == MaintenanceProviderEffectKind::UpdateSavedState)
    );
    database
        .apply_stage(&mut provider, &cleanup.provider_effects)
        .expect("cleanup follows verified composite placement");

    assert!(provider.liked.is_empty());
    assert_eq!(provider.playlists[destination], vec![8, 7]);
    assert_eq!(
        provider.writes,
        vec![
            "add:7:Cinema Monsoon".to_owned(),
            "add:8:Cinema Monsoon".to_owned(),
            "unlike:7".to_owned(),
            "unlike:8".to_owned(),
        ]
    );
}
