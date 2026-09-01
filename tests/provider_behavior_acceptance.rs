//! Small, deterministic provider-behavior acceptance matrix.
//!
//! These scenarios model a completely fake account. They exercise the same
//! wrapper-neutral maintenance DTOs and Rust state machine used by web and CLI,
//! never open a network connection, and intentionally use only six tracks.

use chordrift::{
    contract::{
        MaintenanceChangeId, MaintenanceChangeKind, MaintenanceChangeView, MaintenanceResolution,
        MaintenanceSessionId, MaintenanceSessionState, MaintenanceSurfaceView,
        MaintenanceTrackView, ResourceId,
    },
    maintenance::{MaintenanceProjection, MaintenanceWorkflow},
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
}

impl FakeProviderAccount {
    fn new() -> Self {
        Self {
            next_snapshot: 100,
            delayed: None,
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
                },
                Gesture::Reorder { playlist } => MaintenanceChangeView {
                    change_id,
                    kind: MaintenanceChangeKind::Reorder,
                    track: None,
                    previous_surface: Some(surface(playlist)),
                    current_surface: Some(surface(playlist)),
                    summary: format!("Accepted provider order for {playlist}"),
                    resolution: Some(MaintenanceResolution::KeepObserved),
                },
                Gesture::Like { track: value } => MaintenanceChangeView {
                    change_id,
                    kind: MaintenanceChangeKind::SavedState,
                    track: Some(track(value)),
                    previous_surface: None,
                    current_surface: Some(surface("Liked Songs")),
                    summary: "Choose whether the track remains liked".to_owned(),
                    resolution: None,
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
