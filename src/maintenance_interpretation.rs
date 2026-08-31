//! Provider-first PostgreSQL interpretation for ordinary maintenance.
//!
//! This adapter reuses the Rust immutable planner and its set-based annotations,
//! then converts only record-only reconcile work into wrapper-neutral task
//! projections. It never invokes a shell or provider client and fails closed
//! when publication, retirement, or another provider-effect phase is present.

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};
use sqlx::Row as _;
use storexa::Database;
use uuid::Uuid;

use crate::{
    contract::{
        ClientError, ErrorCode, MaintenanceChangeId, MaintenanceChangeKind, MaintenanceChangeView,
        MaintenanceResolution, MaintenanceSessionView, MaintenanceSurfaceView,
        MaintenanceTrackView, ResourceId,
    },
    intake,
    maintenance::MaintenanceProjection,
    service::AuthenticatedSubject,
    sync_plan::{self, MaintenanceAnnotation, PlanOrigin, PlannedOperation},
};

/// Real PostgreSQL adapter from the newest provider observation to typed intent.
pub struct PostgresMaintenanceInterpreter<'a> {
    database: &'a Database,
}

impl<'a> PostgresMaintenanceInterpreter<'a> {
    /// Creates an interpreter over the worker's canonical database connection.
    pub const fn new(database: &'a Database) -> Self {
        Self { database }
    }

    /// Builds one cumulative provider-first projection without provider access.
    pub async fn project(
        &self,
        subject: AuthenticatedSubject,
        provider_connection_id: ResourceId,
        current: Option<&MaintenanceSessionView>,
    ) -> Result<MaintenanceProjection, ClientError> {
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
        let plan = sync_plan::create(self.database, &account_label, None)
            .await
            .map_err(map_domain_error)?;
        if plan.origin != PlanOrigin::Maintenance {
            return Err(ClientError::new(ErrorCode::StateConflict, false));
        }
        let snapshot_id = ResourceId::from_uuid(plan.source_snapshot_id);
        if let Some(view) = current.filter(|view| view.provider_snapshot_id == snapshot_id) {
            return Ok(MaintenanceProjection {
                provider_snapshot_id: snapshot_id,
                observed_changes: view.observed_changes.clone(),
                provider_effects: view.provider_effects.clone(),
                review_id: view.review_id,
            });
        }
        let (_, current_snapshot, operations) =
            sync_plan::show(self.database, &account_label, Some(plan.plan_id))
                .await
                .map_err(map_domain_error)?;
        if !current_snapshot {
            return Err(ClientError::new(ErrorCode::StateConflict, true));
        }
        if operations.iter().any(|operation| {
            !matches!(
                (operation.phase.as_str(), operation.operation_type.as_str()),
                ("reconcile", _)
                    | ("publish", "reorder_playlist")
                    | ("cleanup", "remove_saved_track")
            )
        }) {
            return Err(ClientError::new(ErrorCode::StateConflict, false));
        }
        let interpreted_operations: Vec<_> = operations
            .iter()
            .filter(|operation| operation.operation_type != "remove_saved_track")
            .cloned()
            .collect();
        let annotations = sync_plan::maintenance_annotations(
            self.database,
            &account_label,
            &interpreted_operations,
        )
        .await
        .map_err(map_domain_error)?;
        let tracks = track_views(self.database, &interpreted_operations).await?;
        let mut projection =
            projection_from_plan(snapshot_id, &interpreted_operations, &annotations, &tracks)?;
        append_saved_intake(
            self.database,
            &account_label,
            snapshot_id,
            &mut projection.observed_changes,
        )
        .await?;
        Ok(projection)
    }
}

async fn append_saved_intake(
    database: &Database,
    account_label: &str,
    snapshot_id: ResourceId,
    changes: &mut Vec<MaintenanceChangeView>,
) -> Result<(), ClientError> {
    let audit = intake::audit(database, account_label)
        .await
        .map_err(map_domain_error)?;
    let liked: Vec<_> = audit
        .items
        .iter()
        .filter(|item| item.sources.iter().any(|source| source == "Liked Songs"))
        .collect();
    let spotify_ids = liked
        .iter()
        .map(|item| item.spotify_id.clone())
        .collect::<BTreeSet<_>>();
    let tracks = track_views_for_ids(database, &spotify_ids).await?;
    let already_placed: BTreeSet<_> = changes
        .iter()
        .filter_map(|change| change.track.as_ref().map(|track| track.track_id))
        .collect();
    let liked_surface = surface("Liked Songs");
    for item in liked {
        let track = tracks.get(&item.spotify_id).cloned().ok_or_else(invalid)?;
        let represented = !item.current_destinations.is_empty()
            || !item.proposal_destinations.is_empty()
            || already_placed.contains(&track.track_id);
        if !represented {
            changes.push(MaintenanceChangeView {
                change_id: change_id(snapshot_id, &format!("liked-place:{}", item.spotify_id)),
                kind: MaintenanceChangeKind::DirectIntake,
                track: Some(track.clone()),
                previous_surface: Some(liked_surface.clone()),
                current_surface: None,
                summary: format!("Choose a destination for {}", track.title),
                resolution: None,
            });
        }
        let resolution = match item.saved_track_disposition.as_deref() {
            Some("preserve") => continue,
            Some("clear_after_verified_assignment") => Some(MaintenanceResolution::ConsumeIntake {
                source: liked_surface.clone(),
            }),
            _ => None,
        };
        changes.push(MaintenanceChangeView {
            change_id: change_id(snapshot_id, &format!("liked-state:{}", item.spotify_id)),
            kind: MaintenanceChangeKind::SavedState,
            track: Some(track.clone()),
            previous_surface: None,
            current_surface: Some(liked_surface.clone()),
            summary: format!("Choose whether {} remains in Liked Songs", track.title),
            resolution,
        });
    }
    Ok(())
}

fn projection_from_plan(
    snapshot_id: ResourceId,
    operations: &[PlannedOperation],
    annotations: &BTreeMap<i32, MaintenanceAnnotation>,
    tracks: &BTreeMap<String, MaintenanceTrackView>,
) -> Result<MaintenanceProjection, ClientError> {
    let mut changes = Vec::new();
    let mut interpreted_tracks = BTreeSet::new();
    let mut interpreted_reorders = BTreeSet::new();
    for operation in operations {
        let annotation = annotations.get(&operation.sequence).ok_or_else(invalid)?;
        if matches!(
            annotation.interpretation.as_str(),
            "direct_move" | "ambiguous_move"
        ) {
            let spotify_id = operation.spotify_track_id.as_ref().ok_or_else(invalid)?;
            if !interpreted_tracks.insert(spotify_id.clone()) {
                continue;
            }
            let track = tracks.get(spotify_id).cloned().ok_or_else(invalid)?;
            let old = annotation.old_destination.as_deref().map(surface);
            let destination = annotation.destination.as_deref().map(surface);
            let direct_intake = annotation.old_destination.as_deref() == Some("New intake");
            let resolution = match (&destination, annotation.interpretation.as_str()) {
                (Some(destination), "direct_move") => Some(MaintenanceResolution::Place {
                    destination: destination.clone(),
                }),
                _ => None,
            };
            changes.push(MaintenanceChangeView {
                change_id: change_id(snapshot_id, &format!("move:{spotify_id}")),
                kind: if direct_intake {
                    MaintenanceChangeKind::DirectIntake
                } else {
                    MaintenanceChangeKind::Reclassification
                },
                track: Some(track.clone()),
                previous_surface: old,
                current_surface: destination,
                summary: if direct_intake {
                    format!("Accepted direct placement of {}", track.title)
                } else {
                    format!("Accepted provider reclassification of {}", track.title)
                },
                resolution,
            });
            continue;
        }
        match operation.operation_type.as_str() {
            "exclude_track" => {
                let spotify_id = operation.spotify_track_id.as_ref().ok_or_else(invalid)?;
                if !interpreted_tracks.insert(spotify_id.clone()) {
                    continue;
                }
                let track = tracks.get(spotify_id).cloned().ok_or_else(invalid)?;
                changes.push(MaintenanceChangeView {
                    change_id: change_id(snapshot_id, &format!("remove:{spotify_id}")),
                    kind: MaintenanceChangeKind::Removal,
                    track: Some(track.clone()),
                    previous_surface: Some(surface(&operation.playlist_name)),
                    current_surface: None,
                    summary: format!("Accepted provider removal of {}", track.title),
                    resolution: Some(MaintenanceResolution::Exclude),
                });
            }
            "reorder_playlist" => {
                if !interpreted_reorders.insert(operation.playlist_name.clone()) {
                    continue;
                }
                changes.push(MaintenanceChangeView {
                    change_id: change_id(
                        snapshot_id,
                        &format!("reorder:{}", operation.playlist_name),
                    ),
                    kind: MaintenanceChangeKind::Reorder,
                    track: None,
                    previous_surface: Some(surface(&operation.playlist_name)),
                    current_surface: Some(surface(&operation.playlist_name)),
                    summary: format!(
                        "Accepted current provider order for {}",
                        operation.playlist_name
                    ),
                    resolution: Some(MaintenanceResolution::KeepObserved),
                });
            }
            _ => return Err(ClientError::new(ErrorCode::StateConflict, false)),
        }
    }
    Ok(MaintenanceProjection {
        provider_snapshot_id: snapshot_id,
        observed_changes: changes,
        provider_effects: Vec::new(),
        review_id: None,
    })
}

async fn track_views(
    database: &Database,
    operations: &[PlannedOperation],
) -> Result<BTreeMap<String, MaintenanceTrackView>, ClientError> {
    let spotify_ids = operations
        .iter()
        .filter_map(|operation| operation.spotify_track_id.clone())
        .collect::<BTreeSet<_>>();
    track_views_for_ids(database, &spotify_ids).await
}

async fn track_views_for_ids(
    database: &Database,
    spotify_ids: &BTreeSet<String>,
) -> Result<BTreeMap<String, MaintenanceTrackView>, ClientError> {
    if spotify_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let rows = sqlx::query(
        "SELECT provider.provider_track_id, track.id, track.title,
                ARRAY(
                    SELECT artist.name FROM track_artists link
                    JOIN artists artist ON artist.id = link.artist_id
                    WHERE link.track_id = track.id ORDER BY link.position
                ) AS artists
           FROM provider_tracks provider
           JOIN tracks track ON track.id = provider.track_id
          WHERE provider.provider = 'spotify'
            AND provider.provider_track_id = ANY($1)
          ORDER BY provider.provider_track_id",
    )
    .bind(spotify_ids.iter().cloned().collect::<Vec<_>>())
    .fetch_all(database.pool())
    .await
    .map_err(|_| unavailable())?;
    rows.into_iter()
        .map(|row| {
            Ok((
                row.try_get("provider_track_id")
                    .map_err(|_| unavailable())?,
                MaintenanceTrackView {
                    track_id: ResourceId::from_uuid(row.try_get("id").map_err(|_| unavailable())?),
                    title: row.try_get("title").map_err(|_| unavailable())?,
                    artists: row.try_get("artists").map_err(|_| unavailable())?,
                },
            ))
        })
        .collect()
}

fn surface(name: &str) -> MaintenanceSurfaceView {
    MaintenanceSurfaceView {
        surface_id: ResourceId::from_uuid(stable_uuid("surface", name)),
        name: name.to_owned(),
    }
}

fn change_id(snapshot_id: ResourceId, key: &str) -> MaintenanceChangeId {
    MaintenanceChangeId::from_uuid(stable_uuid(&snapshot_id.to_string(), key))
}

fn stable_uuid(namespace: &str, value: &str) -> Uuid {
    let digest = Sha256::digest([namespace.as_bytes(), b"\0", value.as_bytes()].concat());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn map_domain_error(error: crate::ChordriftError) -> ClientError {
    match error {
        crate::ChordriftError::Configuration(_) => {
            ClientError::new(ErrorCode::StateConflict, false)
        }
        _ => ClientError::new(ErrorCode::DependencyUnavailable, true),
    }
}

fn invalid() -> ClientError {
    ClientError::new(ErrorCode::InvalidRequest, false)
}

fn unavailable() -> ClientError {
    ClientError::new(ErrorCode::DependencyUnavailable, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn operation(sequence: i32, kind: &str, spotify_id: Option<&str>) -> PlannedOperation {
        PlannedOperation {
            sequence,
            phase: "reconcile".to_owned(),
            operation_type: kind.to_owned(),
            operation_key: format!("operation:{sequence}"),
            playlist_name: "Cinema Monsoon".to_owned(),
            spotify_playlist_id: Some("playlist".to_owned()),
            spotify_track_id: spotify_id.map(str::to_owned),
            payload: json!({}),
            safety: json!({}),
        }
    }

    #[test]
    fn collapses_two_plan_halves_into_one_provider_move() {
        let snapshot = ResourceId::new();
        let operations = vec![
            operation(0, "exclude_track", Some("track-1")),
            operation(1, "remove_track", Some("track-1")),
        ];
        let annotations = BTreeMap::from([
            (
                0,
                MaintenanceAnnotation {
                    title: Some("Song".to_owned()),
                    artists: Some("Artist".to_owned()),
                    interpretation: "direct_move".to_owned(),
                    old_destination: Some("Rasa Archive".to_owned()),
                    destination: Some("Cinema Monsoon".to_owned()),
                },
            ),
            (
                1,
                MaintenanceAnnotation {
                    title: Some("Song".to_owned()),
                    artists: Some("Artist".to_owned()),
                    interpretation: "direct_move".to_owned(),
                    old_destination: Some("Rasa Archive".to_owned()),
                    destination: Some("Cinema Monsoon".to_owned()),
                },
            ),
        ]);
        let tracks = BTreeMap::from([(
            "track-1".to_owned(),
            MaintenanceTrackView {
                track_id: ResourceId::new(),
                title: "Song".to_owned(),
                artists: vec!["Artist".to_owned()],
            },
        )]);
        let projection =
            projection_from_plan(snapshot, &operations, &annotations, &tracks).unwrap();
        assert_eq!(projection.observed_changes.len(), 1);
        assert_eq!(
            projection.observed_changes[0].kind,
            MaintenanceChangeKind::Reclassification
        );
        assert!(projection.provider_effects.is_empty());
    }

    #[test]
    fn same_identity_inputs_produce_stable_change_ids() {
        let snapshot = ResourceId::new();
        assert_eq!(change_id(snapshot, "move:a"), change_id(snapshot, "move:a"));
        assert_ne!(change_id(snapshot, "move:a"), change_id(snapshot, "move:b"));
    }
}
