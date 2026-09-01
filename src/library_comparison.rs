//! Set-based provider/model library comparison shared by every client transport.

use std::collections::HashMap;

use sqlx::{PgPool, Row as _};
use uuid::Uuid;

use crate::contract::{
    ClientError, ErrorCode, LibraryComparisonStatus, LibraryComparisonView,
    LibraryPlaylistComparisonView, LibraryPlaylistView, ResourceId,
};

/// Compares the newest complete provider observation with the current model.
pub async fn query(
    pool: &PgPool,
    provider_connection_id: ResourceId,
) -> Result<LibraryComparisonView, ClientError> {
    let provider_state_at = sqlx::query_scalar(
        "SELECT captured_at FROM provider_current_inventories WHERE provider_account_id = $1",
    )
    .bind(provider_connection_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(|_| unavailable())?;
    let provider_playlists = sqlx::query(
        "SELECT spotify_playlist_id, name, total_items, signal_class, role
           FROM current_spotify_playlists
          WHERE provider_account_id = $1
          ORDER BY lower(name), spotify_playlist_id",
    )
    .bind(provider_connection_id.as_uuid())
    .fetch_all(pool)
    .await
    .map_err(|_| unavailable())?
    .into_iter()
    .map(|row| {
        let id: String = row
            .try_get("spotify_playlist_id")
            .map_err(|_| unavailable())?;
        let count: i32 = row.try_get("total_items").map_err(|_| unavailable())?;
        Ok(LibraryPlaylistView {
            playlist_id: id.clone(),
            provider_playlist_id: Some(id),
            name: row.try_get("name").map_err(|_| unavailable())?,
            track_count: u64::try_from(count).map_err(|_| unavailable())?,
            signal_class: row.try_get("signal_class").map_err(|_| unavailable())?,
            role: row.try_get("role").map_err(|_| unavailable())?,
        })
    })
    .collect::<Result<Vec<_>, ClientError>>()?;
    let generation = sqlx::query(
        "SELECT id, created_at FROM playlist_generations
          WHERE provider_account_id = $1
            AND status IN ('proposed', 'approved', 'published')
          ORDER BY CASE status WHEN 'proposed' THEN 0 WHEN 'approved' THEN 1 ELSE 2 END,
                   created_at DESC, id DESC LIMIT 1",
    )
    .bind(provider_connection_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(|_| unavailable())?;
    let (generation_id, chordrift_state_at, model_playlists) = if let Some(generation) = generation
    {
        let generation_id: Uuid = generation.try_get("id").map_err(|_| unavailable())?;
        let state_at = Some(
            generation
                .try_get("created_at")
                .map_err(|_| unavailable())?,
        );
        let playlists = sqlx::query(
            "SELECT concept.stable_key,
                    COALESCE(name_revision.name, playlist.name) AS name,
                    provider.provider_playlist_id,
                    count(membership.id)::bigint AS track_count
               FROM playlists playlist
               JOIN playlist_concepts concept ON concept.id = playlist.concept_id
               LEFT JOIN playlist_name_revisions name_revision
                 ON name_revision.playlist_id = playlist.id AND name_revision.selected
               LEFT JOIN provider_playlists provider
                 ON provider.concept_id = concept.id AND provider.provider = 'spotify'
               LEFT JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
              WHERE playlist.generation_id = $1 AND playlist.archived_at IS NULL
              GROUP BY concept.stable_key, COALESCE(name_revision.name, playlist.name),
                       provider.provider_playlist_id
              ORDER BY lower(COALESCE(name_revision.name, playlist.name)), concept.stable_key",
        )
        .bind(generation_id)
        .fetch_all(pool)
        .await
        .map_err(|_| unavailable())?
        .into_iter()
        .map(|row| {
            let count: i64 = row.try_get("track_count").map_err(|_| unavailable())?;
            Ok(LibraryPlaylistView {
                playlist_id: row.try_get("stable_key").map_err(|_| unavailable())?,
                name: row.try_get("name").map_err(|_| unavailable())?,
                provider_playlist_id: row
                    .try_get("provider_playlist_id")
                    .map_err(|_| unavailable())?,
                track_count: u64::try_from(count).map_err(|_| unavailable())?,
                signal_class: Some("canonical".to_owned()),
                role: Some("managed".to_owned()),
            })
        })
        .collect::<Result<Vec<_>, ClientError>>()?;
        (Some(generation_id), state_at, playlists)
    } else {
        (None, None, Vec::new())
    };
    let provider_rows = sqlx::query(
        "SELECT provider_playlist.provider_playlist_id, provider_track.provider_track_id
           FROM provider_current_playlists current_playlist
           JOIN provider_playlists provider_playlist
             ON provider_playlist.id = current_playlist.provider_playlist_id
           JOIN provider_playlist_revision_tracks membership
             ON membership.revision_id = current_playlist.revision_id
           JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
          WHERE current_playlist.provider_account_id = $1
          ORDER BY provider_playlist.provider_playlist_id, membership.position",
    )
    .bind(provider_connection_id.as_uuid())
    .fetch_all(pool)
    .await
    .map_err(|_| unavailable())?;
    let mut provider_membership: HashMap<String, Vec<String>> = HashMap::new();
    for row in provider_rows {
        provider_membership
            .entry(
                row.try_get("provider_playlist_id")
                    .map_err(|_| unavailable())?,
            )
            .or_default()
            .push(
                row.try_get("provider_track_id")
                    .map_err(|_| unavailable())?,
            );
    }
    let mut model_membership: HashMap<String, Vec<String>> = HashMap::new();
    if let Some(generation_id) = generation_id {
        for row in sqlx::query(
            "SELECT concept.stable_key, provider_track.provider_track_id
               FROM playlists playlist
               JOIN playlist_concepts concept ON concept.id = playlist.concept_id
               JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
               JOIN provider_tracks provider_track
                 ON provider_track.track_id = membership.track_id
                AND provider_track.provider = 'spotify'
              WHERE playlist.generation_id = $1 AND playlist.archived_at IS NULL
              ORDER BY concept.stable_key, membership.position",
        )
        .bind(generation_id)
        .fetch_all(pool)
        .await
        .map_err(|_| unavailable())?
        {
            model_membership
                .entry(row.try_get("stable_key").map_err(|_| unavailable())?)
                .or_default()
                .push(
                    row.try_get("provider_track_id")
                        .map_err(|_| unavailable())?,
                );
        }
    }
    Ok(compare_planes(
        provider_state_at,
        chordrift_state_at,
        provider_playlists,
        model_playlists,
        &provider_membership,
        &model_membership,
    ))
}

fn compare_planes(
    provider_state_at: Option<chrono::DateTime<chrono::Utc>>,
    chordrift_state_at: Option<chrono::DateTime<chrono::Utc>>,
    provider_playlists: Vec<LibraryPlaylistView>,
    model_playlists: Vec<LibraryPlaylistView>,
    provider_membership: &HashMap<String, Vec<String>>,
    model_membership: &HashMap<String, Vec<String>>,
) -> LibraryComparisonView {
    let mut model_by_provider = HashMap::new();
    for (index, playlist) in model_playlists.iter().enumerate() {
        if let Some(provider_id) = &playlist.provider_playlist_id {
            model_by_provider.insert(provider_id.clone(), index);
        }
    }
    let mut used_model = vec![false; model_playlists.len()];
    let mut comparisons = Vec::new();
    for provider in provider_playlists {
        let provider_id = provider
            .provider_playlist_id
            .as_deref()
            .unwrap_or(&provider.playlist_id);
        let provider_tracks = provider_membership
            .get(provider_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if let Some(index) = model_by_provider.get(provider_id).copied() {
            used_model[index] = true;
            comparisons.push(compare_linked(
                &provider,
                &model_playlists[index],
                provider_tracks,
                model_membership
                    .get(&model_playlists[index].playlist_id)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            ));
        } else {
            let resolved = u64::try_from(provider_tracks.len()).unwrap_or(u64::MAX);
            let unresolved = provider.track_count.saturating_sub(resolved);
            comparisons.push(LibraryPlaylistComparisonView {
                provider_playlist_id: Some(provider_id.to_owned()),
                chordrift_playlist_id: None,
                name: provider.name,
                provider_track_count: provider.track_count,
                chordrift_track_count: 0,
                provider_unresolved_track_count: unresolved,
                chordrift_unresolved_track_count: 0,
                provider_only_track_count: resolved,
                chordrift_only_track_count: 0,
                shared_track_count: 0,
                order_matches: None,
                status: LibraryComparisonStatus::ProviderOnly,
                explanation: if unresolved == 0 {
                    "Provider playlist is not linked to a Chordrift model surface.".to_owned()
                } else {
                    format!(
                        "Provider playlist is not linked; {unresolved} reported item(s) lack a comparable track identity."
                    )
                },
            });
        }
    }
    for (index, model) in model_playlists.into_iter().enumerate() {
        if used_model[index] {
            continue;
        }
        let model_tracks = model_membership
            .get(&model.playlist_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let unresolved = model
            .track_count
            .saturating_sub(u64::try_from(model_tracks.len()).unwrap_or(u64::MAX));
        comparisons.push(LibraryPlaylistComparisonView {
            provider_playlist_id: model.provider_playlist_id,
            chordrift_playlist_id: Some(model.playlist_id),
            name: model.name,
            provider_track_count: 0,
            chordrift_track_count: model.track_count,
            provider_unresolved_track_count: 0,
            chordrift_unresolved_track_count: unresolved,
            provider_only_track_count: 0,
            chordrift_only_track_count: u64::try_from(model_tracks.len()).unwrap_or(u64::MAX),
            shared_track_count: 0,
            order_matches: None,
            status: LibraryComparisonStatus::ChordriftOnly,
            explanation: if unresolved == 0 {
                "Chordrift model surface is not present in the provider observation.".to_owned()
            } else {
                format!(
                    "Chordrift model surface is not present; {unresolved} model item(s) lack a comparable provider track identity."
                )
            },
        });
    }
    comparisons.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.provider_playlist_id.cmp(&right.provider_playlist_id))
            .then_with(|| left.chordrift_playlist_id.cmp(&right.chordrift_playlist_id))
    });
    let aligned_playlists = u64::try_from(
        comparisons
            .iter()
            .filter(|playlist| playlist.status == LibraryComparisonStatus::Aligned)
            .count(),
    )
    .unwrap_or(u64::MAX);
    LibraryComparisonView {
        provider_state_at,
        chordrift_state_at,
        aligned_playlists,
        differing_playlists: u64::try_from(comparisons.len())
            .unwrap_or(u64::MAX)
            .saturating_sub(aligned_playlists),
        playlists: comparisons,
    }
}

fn compare_linked(
    provider: &LibraryPlaylistView,
    model: &LibraryPlaylistView,
    provider_tracks: &[String],
    model_tracks: &[String],
) -> LibraryPlaylistComparisonView {
    let (provider_only, model_only, shared) = occurrence_comparison(provider_tracks, model_tracks);
    let provider_unresolved = provider
        .track_count
        .saturating_sub(u64::try_from(provider_tracks.len()).unwrap_or(u64::MAX));
    let chordrift_unresolved = model
        .track_count
        .saturating_sub(u64::try_from(model_tracks.len()).unwrap_or(u64::MAX));
    let membership_matches = provider_only == 0
        && model_only == 0
        && provider_unresolved == 0
        && chordrift_unresolved == 0;
    let order_matches = membership_matches.then(|| provider_tracks == model_tracks);
    let (status, explanation) = if provider_unresolved > 0 || chordrift_unresolved > 0 {
        (
            LibraryComparisonStatus::MembershipDiffers,
            format!(
                "{provider_unresolved} provider and {chordrift_unresolved} Chordrift item(s) lack comparable identities; {provider_only} provider-only and {model_only} Chordrift-only resolved membership(s)."
            ),
        )
    } else if provider_only > 0 || model_only > 0 {
        (
            LibraryComparisonStatus::MembershipDiffers,
            format!("{provider_only} provider-only and {model_only} Chordrift-only membership(s)."),
        )
    } else if order_matches == Some(false) {
        (
            LibraryComparisonStatus::OrderDiffers,
            "Membership is identical; only custom order differs.".to_owned(),
        )
    } else {
        (
            LibraryComparisonStatus::Aligned,
            "Membership and custom order are aligned.".to_owned(),
        )
    };
    LibraryPlaylistComparisonView {
        provider_playlist_id: provider.provider_playlist_id.clone(),
        chordrift_playlist_id: Some(model.playlist_id.clone()),
        name: provider.name.clone(),
        provider_track_count: provider.track_count,
        chordrift_track_count: model.track_count,
        provider_unresolved_track_count: provider_unresolved,
        chordrift_unresolved_track_count: chordrift_unresolved,
        provider_only_track_count: provider_only,
        chordrift_only_track_count: model_only,
        shared_track_count: shared,
        order_matches,
        status,
        explanation,
    }
}

fn occurrence_comparison(left: &[String], right: &[String]) -> (u64, u64, u64) {
    let mut left_counts: HashMap<&str, u64> = HashMap::new();
    let mut right_counts: HashMap<&str, u64> = HashMap::new();
    for value in left {
        *left_counts.entry(value).or_default() += 1;
    }
    for value in right {
        *right_counts.entry(value).or_default() += 1;
    }
    let shared = left_counts
        .iter()
        .map(|(value, count)| count.min(right_counts.get(value).unwrap_or(&0)))
        .sum();
    (
        u64::try_from(left.len()).unwrap_or(u64::MAX) - shared,
        u64::try_from(right.len()).unwrap_or(u64::MAX) - shared,
        shared,
    )
}

fn unavailable() -> ClientError {
    ClientError::new(ErrorCode::DependencyUnavailable, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explains_membership_order_duplicates_and_unresolved_items() {
        let provider = playlist("provider-a", "Example", 4);
        let model = LibraryPlaylistView {
            playlist_id: "model-a".to_owned(),
            track_count: 3,
            ..playlist("provider-a", "Example", 0)
        };
        let provider_membership = HashMap::from([(
            "provider-a".to_owned(),
            vec!["a".to_owned(), "b".to_owned(), "b".to_owned()],
        )]);
        let model_membership = HashMap::from([(
            "model-a".to_owned(),
            vec!["b".to_owned(), "a".to_owned(), "c".to_owned()],
        )]);
        let comparison = compare_planes(
            None,
            None,
            vec![provider],
            vec![model],
            &provider_membership,
            &model_membership,
        );
        let result = &comparison.playlists[0];
        assert_eq!(result.status, LibraryComparisonStatus::MembershipDiffers);
        assert_eq!(result.provider_only_track_count, 1);
        assert_eq!(result.chordrift_only_track_count, 1);
        assert_eq!(result.shared_track_count, 2);
        assert_eq!(result.provider_unresolved_track_count, 1);
        assert_eq!(result.order_matches, None);

        let reordered = compare_linked(
            &playlist("provider-b", "Order", 2),
            &LibraryPlaylistView {
                playlist_id: "model-b".to_owned(),
                track_count: 2,
                ..playlist("provider-b", "Order", 0)
            },
            &["a".to_owned(), "b".to_owned()],
            &["b".to_owned(), "a".to_owned()],
        );
        assert_eq!(reordered.status, LibraryComparisonStatus::OrderDiffers);
        assert_eq!(reordered.order_matches, Some(false));
    }

    fn playlist(provider_id: &str, name: &str, track_count: u64) -> LibraryPlaylistView {
        LibraryPlaylistView {
            playlist_id: provider_id.to_owned(),
            name: name.to_owned(),
            provider_playlist_id: Some(provider_id.to_owned()),
            track_count,
            signal_class: None,
            role: None,
        }
    }
}
