//! Non-destructive, account-scoped proposed playlist libraries.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result, clusters};

const PROPOSAL_MODEL: &str = "cluster-to-playlist";
const PROPOSAL_VERSION: &str = "1";
const LINEAGE_MIN_OVERLAP: f64 = 0.5;

/// Result of creating or reusing a proposed playlist generation.
#[derive(Clone, Debug, PartialEq)]
pub struct GenerationReport {
    /// Immutable proposal generation identity.
    pub generation_id: Uuid,
    /// Exact cluster generation consumed.
    pub cluster_generation_id: Uuid,
    /// Whether the identical proposal already existed.
    pub reused: bool,
    /// Proposed canonical playlists.
    pub playlist_count: usize,
    /// Tracks assigned to a proposed playlist.
    pub assigned_track_count: usize,
    /// Legacy and intake tracks that require representation.
    pub required_track_count: usize,
    /// Required tracks represented by a proposed playlist.
    pub represented_track_count: usize,
    /// Whether retirement coverage is complete.
    pub coverage_complete: bool,
    /// Reproducibility hash.
    pub input_hash: String,
}

/// Current proposal state.
#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    /// Immutable proposal generation identity.
    pub generation_id: Uuid,
    /// Exact cluster generation consumed.
    pub cluster_generation_id: Uuid,
    /// Lifecycle state.
    pub state: String,
    /// Proposed playlists.
    pub playlist_count: usize,
    /// Required legacy and intake tracks.
    pub required_track_count: usize,
    /// Required tracks represented.
    pub represented_track_count: usize,
    /// Whether all required tracks are represented.
    pub coverage_complete: bool,
    /// Playlists with selected generated naming.
    pub named_playlist_count: usize,
    /// Reproducibility hash.
    pub input_hash: String,
    /// Naming-context hash, if exported.
    pub naming_context_hash: Option<String>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

/// Inspectable proposed playlist summary.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistSummary {
    /// Stable identity used by naming and later correction workflows.
    pub stable_key: String,
    /// Current selected display name or machine label.
    pub name: String,
    /// Whether an external naming revision has been selected.
    pub named: bool,
    /// Assigned track count.
    pub track_count: usize,
    /// Source cluster label.
    pub machine_label: String,
    /// Description when named.
    pub description: Option<String>,
    /// Semantic tags when named.
    pub tags: Vec<String>,
}

/// One proposed playlist track.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistTrack {
    /// Proposed order.
    pub position: usize,
    /// Track title.
    pub title: String,
    /// Display artists.
    pub artists: String,
    /// Spotify track identity.
    pub spotify_id: String,
}

/// One unresolved track within an analytical cluster.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupTrack {
    /// Representative rank within the analytical cluster.
    pub position: usize,
    /// Cluster membership score.
    pub score: f64,
    /// Track title.
    pub title: String,
    /// Display artists.
    pub artists: String,
    /// Spotify track identity.
    pub spotify_id: String,
}

/// Coverage for one retireable source playlist.
#[derive(Clone, Debug, PartialEq)]
pub struct CoverageRow {
    /// Current provider playlist name.
    pub source_name: String,
    /// Stable Spotify playlist identity.
    pub spotify_id: String,
    /// Configured signal class.
    pub signal_class: String,
    /// Unique tracks requiring representation.
    pub required_tracks: usize,
    /// Tracks represented in the proposal.
    pub represented_tracks: usize,
    /// Tracks still missing.
    pub missing_tracks: usize,
}

/// One retirement-source track not represented by the proposal.
#[derive(Clone, Debug, PartialEq)]
pub struct MissingTrack {
    /// Track title.
    pub title: String,
    /// Display artists.
    pub artists: String,
    /// Spotify track identity.
    pub spotify_id: String,
    /// Current semantic-legacy or intake playlists containing the track.
    pub source_playlists: String,
}

/// Coverage across the complete preserved-library inventory.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoricalCoverageRow {
    /// Source class, or `complete_inventory` for the deduplicated total.
    pub signal_class: String,
    /// Distinct source playlists in this row; zero for saved tracks and exclusions.
    pub playlist_count: usize,
    /// Distinct source tracks.
    pub unique_tracks: usize,
    /// Tracks placed in the approved canonical proposal.
    pub represented_tracks: usize,
    /// Tracks intentionally excluded from canonical output.
    pub excluded_tracks: usize,
    /// Tracks with neither a canonical placement nor an active exclusion.
    pub missing_tracks: usize,
    /// Tracks placed in multiple destinations or both placed and excluded.
    pub conflicting_tracks: usize,
}

/// One unresolved track from the complete preserved-library inventory.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoricalMissingTrack {
    /// Track title.
    pub title: String,
    /// Display artists.
    pub artists: String,
    /// Stable Spotify track ID.
    pub spotify_id: String,
    /// Preserving source names.
    pub source_playlists: String,
    /// Preserving source classes.
    pub signal_classes: String,
}

/// Read-only fit of unresolved inventory against approved playlist centroids.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacementAudit {
    /// Approved proposal used as stable destinations.
    pub proposal_generation_id: Uuid,
    /// Complete-inventory embedding generation used for scoring.
    pub embedding_generation_id: Uuid,
    /// Distinct tracks in the complete inventory.
    pub inventory_tracks: usize,
    /// Tracks already placed in the approved proposal.
    pub already_placed_tracks: usize,
    /// Unresolved tracks with an embedding.
    pub embedded_unresolved_tracks: usize,
    /// Unresolved tracks without an embedding.
    pub unembedded_unresolved_tracks: usize,
    /// Strong existing-destination fits (cosine similarity at least 0.20).
    pub strong_fit_tracks: usize,
    /// Usable existing-destination fits (similarity 0.05 through 0.20).
    pub usable_fit_tracks: usize,
    /// Weak fits that should be tested as possible new playlist groups.
    pub weak_fit_tracks: usize,
    /// Proposed additions by stable existing destination.
    pub destinations: Vec<PlacementDestinationAudit>,
    /// Weak-fit tracks grouped by the latest analytical cluster.
    pub new_group_candidates: Vec<PlacementGroupAudit>,
}

/// One existing playlist's projected additions.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacementDestinationAudit {
    /// Stable concept key.
    pub stable_key: String,
    /// Approved display name.
    pub name: String,
    /// Strong-fit additions.
    pub strong_fit_tracks: usize,
    /// Usable-fit additions.
    pub usable_fit_tracks: usize,
}

/// One analytical group that may warrant a new managed playlist.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacementGroupAudit {
    /// Content-derived analytical cluster label.
    pub machine_label: String,
    /// Representative track and artist display.
    pub representative: String,
    /// All tracks in the analytical cluster.
    pub cluster_tracks: usize,
    /// Weak existing-destination fits in this group.
    pub weak_fit_tracks: usize,
    /// Existing members already represented in the current proposal.
    pub placed_tracks: usize,
    /// Existing destination with the largest membership overlap.
    pub dominant_destination: Option<String>,
    /// Members already assigned to the dominant destination.
    pub dominant_tracks: usize,
}

/// Result of assigning unresolved embedded tracks by analytical-group consensus.
#[derive(Clone, Debug, PartialEq)]
pub struct ConsensusAssignmentReport {
    /// Proposal generation modified in Neon.
    pub generation_id: Uuid,
    /// Tracks assigned by this operation.
    pub assigned_tracks: usize,
    /// Complete preserved inventory.
    pub required_tracks: usize,
    /// Tracks represented after the operation.
    pub represented_tracks: usize,
    /// Tracks still unresolved.
    pub unresolved_tracks: usize,
}

/// Result of creating a stable manual playlist category.
#[derive(Clone, Debug, PartialEq)]
pub struct ManualCategory {
    /// Stable destination key.
    pub stable_key: String,
    /// User-facing category name.
    pub name: String,
    /// Current proposal generation containing it.
    pub generation_id: Uuid,
}

/// One empty destination removed from an editable proposal while its concept survives.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetiredEmptyPlaylist {
    /// Proposal generation edited.
    pub generation_id: Uuid,
    /// Stable concept key retained for history and provider retirement.
    pub stable_key: String,
    /// Former display name.
    pub name: String,
}

/// Result of a reversible manual assignment decision.
#[derive(Clone, Debug, PartialEq)]
pub struct AssignmentReport {
    /// Track title.
    pub title: String,
    /// Spotify track identity.
    pub spotify_id: String,
    /// Stable destination key, or none when returned to review.
    pub destination: Option<String>,
    /// Required tracks represented after the decision.
    pub represented_track_count: usize,
    /// Required tracks still missing after the decision.
    pub missing_track_count: usize,
}

/// JSON document given to a naming model.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct NamingContext {
    /// Artifact schema version.
    pub schema_version: u32,
    /// Account label, never provider profile PII.
    pub account: String,
    /// Proposal being named.
    pub generation_id: Uuid,
    /// Hash of all context fields other than this value.
    pub context_sha256: String,
    /// Naming constraints.
    pub instructions: Vec<String>,
    /// Proposed playlist evidence.
    pub playlists: Vec<NamingPlaylistContext>,
}

/// Evidence for naming one proposed playlist.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct NamingPlaylistContext {
    /// Stable playlist identity.
    pub stable_key: String,
    /// Source machine label.
    pub machine_label: String,
    /// Assigned track count.
    pub track_count: usize,
    /// Representative tracks.
    pub sample_tracks: Vec<NamingTrack>,
}

/// Minimal naming-safe track evidence.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct NamingTrack {
    /// Track title.
    pub title: String,
    /// Display artists.
    pub artists: String,
}

/// Strict versioned naming-result artifact.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NamingArtifact {
    /// Artifact schema version; currently 1.
    pub schema_version: u32,
    /// Exact proposal generation named.
    pub generation_id: Uuid,
    /// Exact naming-context hash consumed.
    pub context_sha256: String,
    /// Generator provenance.
    pub generator: NamingGenerator,
    /// One result for every proposed playlist.
    pub playlists: Vec<NamingResult>,
}

/// Naming generator provenance.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NamingGenerator {
    /// Product or runtime that generated the names.
    pub provider: String,
    /// Model identity.
    pub model: String,
    /// Model or prompt revision.
    pub model_version: String,
}

/// Selected name, description, and tags for one stable playlist.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NamingResult {
    /// Stable key copied from the context artifact.
    pub stable_key: String,
    /// User-facing playlist name.
    pub name: String,
    /// User-facing playlist description.
    pub description: String,
    /// Short semantic tags.
    pub tags: Vec<String>,
}

/// Creates an immutable proposed playlist library from the latest clusters.
pub async fn generate(database: &Database, account_label: &str) -> Result<GenerationReport> {
    let account_id = account_id(database, account_label).await?;
    let cluster_status = clusters::status(database, account_label).await?;
    let input_hash = hash_parts(&[
        PROPOSAL_MODEL,
        PROPOSAL_VERSION,
        &cluster_status.input_hash,
        &cluster_status.generation_id.to_string(),
    ]);
    if let Some(report) = reused_report(database, account_id, &input_hash).await? {
        return Ok(report);
    }

    let cluster_rows = sqlx::query(
        "SELECT cluster.id, cluster.machine_label
         FROM clusters cluster WHERE cluster.generation_id = $1
         ORDER BY cluster.machine_label",
    )
    .bind(cluster_status.generation_id)
    .fetch_all(database.pool())
    .await?;
    let required_track_count = required_track_count(database, account_id).await?;
    let previous_generation: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM playlist_generations
         WHERE provider_account_id = $1
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?;

    let mut transaction = database.pool().begin().await?;
    let generation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlist_generations
         (model, model_version, status, parameters, provider_account_id,
          cluster_generation_id, input_hash)
         VALUES ($1, $2, 'proposed', $3, $4, $5, $6) RETURNING id",
    )
    .bind(PROPOSAL_MODEL)
    .bind(PROPOSAL_VERSION)
    .bind(json!({
        "lineage_min_overlap": LINEAGE_MIN_OVERLAP,
        "spotify_writes": false,
        "unassigned_policy": "inspectable-not-publishable"
    }))
    .bind(account_id)
    .bind(cluster_status.generation_id)
    .bind(&input_hash)
    .fetch_one(&mut *transaction)
    .await?;

    let mut used_concepts = HashSet::new();
    for row in cluster_rows {
        let cluster_id: Uuid = row.try_get("id")?;
        let machine_label: String = row.try_get("machine_label")?;
        let concept_id = find_lineage_concept(
            &mut transaction,
            cluster_id,
            previous_generation,
            &used_concepts,
        )
        .await?
        .unwrap_or_else(Uuid::new_v4);
        if !used_concepts.contains(&concept_id) {
            sqlx::query(
                "INSERT INTO playlist_concepts (id, provider_account_id, stable_key)
                 VALUES ($1, $2, $3) ON CONFLICT (id) DO NOTHING",
            )
            .bind(concept_id)
            .bind(account_id)
            .bind(format!(
                "playlist-{}",
                &concept_id.simple().to_string()[..12]
            ))
            .execute(&mut *transaction)
            .await?;
        }
        used_concepts.insert(concept_id);
        let playlist_id: Uuid = sqlx::query_scalar(
            "INSERT INTO playlists
             (generation_id, concept_id, name, kind, machine_label)
             VALUES ($1, $2, $3, 'generated', $3) RETURNING id",
        )
        .bind(generation_id)
        .bind(concept_id)
        .bind(&machine_label)
        .fetch_one(&mut *transaction)
        .await?;
        let track_rows = sqlx::query(
            "SELECT track_id, representative_rank, membership_score
             FROM cluster_tracks WHERE cluster_id = $1
             ORDER BY representative_rank, track_id",
        )
        .bind(cluster_id)
        .fetch_all(&mut *transaction)
        .await?;
        for track in track_rows {
            let track_id: Uuid = track.try_get("track_id")?;
            sqlx::query(
                "INSERT INTO playlist_tracks
                 (playlist_id, track_id, position, source, provenance)
                 VALUES ($1, $2, $3, 'generated', $4)",
            )
            .bind(playlist_id)
            .bind(track_id)
            .bind(track.try_get::<i32, _>("representative_rank")? - 1)
            .bind(json!({
                "cluster_generation_id": cluster_status.generation_id,
                "cluster_id": cluster_id,
                "membership_score": track.try_get::<f64, _>("membership_score")?
            }))
            .execute(&mut *transaction)
            .await?;
        }
    }

    replay_assignment_overrides(&mut transaction, account_id, generation_id).await?;

    let assigned_track_count: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT membership.track_id)::bigint
         FROM playlists playlist
         JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
         WHERE playlist.generation_id = $1",
    )
    .bind(generation_id)
    .fetch_one(&mut *transaction)
    .await?;
    let playlist_count: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM playlists WHERE generation_id = $1")
            .bind(generation_id)
            .fetch_one(&mut *transaction)
            .await?;

    let represented_track_count =
        represented_required_track_count_tx(&mut transaction, account_id, generation_id).await?;
    let coverage_complete = represented_track_count == required_track_count;
    sqlx::query(
        "UPDATE playlist_generations SET coverage_complete = $2,
         required_track_count = $3, represented_track_count = $4 WHERE id = $1",
    )
    .bind(generation_id)
    .bind(coverage_complete)
    .bind(as_i32(required_track_count)?)
    .bind(as_i32(represented_track_count)?)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok(GenerationReport {
        generation_id,
        cluster_generation_id: cluster_status.generation_id,
        reused: false,
        playlist_count: as_usize_i64(playlist_count)?,
        assigned_track_count: as_usize_i64(assigned_track_count)?,
        required_track_count,
        represented_track_count,
        coverage_complete,
        input_hash,
    })
}

/// Returns the latest proposed playlist generation.
pub async fn status(database: &Database, account_label: &str) -> Result<Status> {
    let account_id = account_id(database, account_label).await?;
    let row = sqlx::query(
        "SELECT generation.id, generation.cluster_generation_id, generation.status,
                generation.required_track_count, generation.represented_track_count,
                generation.coverage_complete, generation.input_hash,
                generation.naming_context_hash, generation.created_at,
                count(DISTINCT playlist.id)::bigint AS playlist_count,
                count(DISTINCT revision.playlist_id)::bigint AS named_playlist_count
         FROM playlist_generations generation
         LEFT JOIN playlists playlist ON playlist.generation_id = generation.id
         LEFT JOIN playlist_name_revisions revision
           ON revision.playlist_id = playlist.id AND revision.selected
         WHERE generation.id = (
             SELECT id FROM playlist_generations WHERE provider_account_id = $1
             ORDER BY created_at DESC, id DESC LIMIT 1)
         GROUP BY generation.id",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(
            "no proposal exists; run `chordrift proposals generate`".to_owned(),
        )
    })?;
    Ok(Status {
        generation_id: row.try_get("id")?,
        cluster_generation_id: row.try_get("cluster_generation_id")?,
        state: row.try_get("status")?,
        playlist_count: as_usize_i64(row.try_get("playlist_count")?)?,
        required_track_count: as_usize(row.try_get("required_track_count")?)?,
        represented_track_count: as_usize(row.try_get("represented_track_count")?)?,
        coverage_complete: row.try_get("coverage_complete")?,
        named_playlist_count: as_usize_i64(row.try_get("named_playlist_count")?)?,
        input_hash: row.try_get("input_hash")?,
        naming_context_hash: row.try_get("naming_context_hash")?,
        created_at: row.try_get("created_at")?,
    })
}

/// Lists the latest proposed playlists.
pub async fn list(database: &Database, account_label: &str) -> Result<Vec<PlaylistSummary>> {
    let generation = status(database, account_label).await?;
    let rows = sqlx::query(
        "SELECT concept.stable_key, playlist.name, playlist.description,
                playlist.machine_label, playlist.machine_tags,
                (revision.id IS NOT NULL) AS named,
                count(membership.id)::bigint AS track_count
         FROM playlists playlist
         JOIN playlist_concepts concept ON concept.id = playlist.concept_id
         LEFT JOIN playlist_name_revisions revision
           ON revision.playlist_id = playlist.id AND revision.selected
         LEFT JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
         WHERE playlist.generation_id = $1
         GROUP BY playlist.id, concept.stable_key, revision.id
         ORDER BY playlist.name, concept.stable_key",
    )
    .bind(generation.generation_id)
    .fetch_all(database.pool())
    .await?;
    rows.into_iter().map(summary_from_row).collect()
}

/// Lists tracks for one stable proposed playlist key.
pub async fn tracks(
    database: &Database,
    account_label: &str,
    stable_key: &str,
    limit: u32,
) -> Result<Vec<PlaylistTrack>> {
    if limit == 0 || limit > 1_000 {
        return Err(ChordriftError::Configuration(
            "proposal track limit must be between 1 and 1000".to_owned(),
        ));
    }
    let generation = status(database, account_label).await?;
    let rows = sqlx::query(
        "SELECT membership.position, track.title,
                COALESCE(string_agg(DISTINCT artist.name, ', '), '') AS artists,
                min(provider.provider_track_id) AS spotify_id
         FROM playlists playlist
         JOIN playlist_concepts concept ON concept.id = playlist.concept_id
         JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
         JOIN tracks track ON track.id = membership.track_id
         JOIN provider_tracks provider
           ON provider.track_id = track.id AND provider.provider = 'spotify'
         LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
         LEFT JOIN artists artist ON artist.id = track_artist.artist_id
         WHERE playlist.generation_id = $1 AND concept.stable_key = $2
         GROUP BY membership.id, membership.position, track.title
         ORDER BY membership.position LIMIT $3",
    )
    .bind(generation.generation_id)
    .bind(stable_key)
    .bind(i64::from(limit))
    .fetch_all(database.pool())
    .await?;
    if rows.is_empty() {
        return Err(ChordriftError::Configuration(
            "stable playlist key was not found in the latest proposal".to_owned(),
        ));
    }
    rows.into_iter()
        .map(|row| {
            Ok(PlaylistTrack {
                position: as_usize(row.try_get::<i32, _>("position")?)? + 1,
                title: row.try_get("title")?,
                artists: row.try_get("artists")?,
                spotify_id: row.try_get("spotify_id")?,
            })
        })
        .collect()
}

/// Reports retirement coverage per semantic-legacy or intake playlist.
pub async fn coverage(database: &Database, account_label: &str) -> Result<Vec<CoverageRow>> {
    let account_id = account_id(database, account_label).await?;
    let generation = status(database, account_label).await?;
    let rows = sqlx::query(
        "WITH latest AS (
             SELECT id FROM provider_inventory_observations
             WHERE provider_account_id = $1 ORDER BY captured_at DESC, id DESC LIMIT 1
         ), proposed AS (
             SELECT DISTINCT membership.track_id
             FROM playlists playlist JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
             WHERE playlist.generation_id = $2
         )
         SELECT snapshot.name AS source_name, provider_playlist.provider_playlist_id AS spotify_id,
                account_playlist.signal_class,
                count(DISTINCT provider_track.track_id)::bigint AS required_tracks,
                count(DISTINCT provider_track.track_id) FILTER
                    (WHERE proposed.track_id IS NOT NULL)::bigint AS represented_tracks
         FROM provider_account_playlists account_playlist
         JOIN provider_playlists provider_playlist ON provider_playlist.id = account_playlist.provider_playlist_id
         JOIN latest ON true
         JOIN provider_observed_playlists snapshot
           ON snapshot.provider_playlist_id = provider_playlist.id AND snapshot.snapshot_id = latest.id
         JOIN provider_observed_playlist_tracks membership
           ON membership.provider_playlist_id = provider_playlist.id AND membership.snapshot_id = latest.id
         JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
         LEFT JOIN proposed ON proposed.track_id = provider_track.track_id
         WHERE account_playlist.provider_account_id = $1
           AND account_playlist.signal_class IN ('semantic_legacy', 'intake')
         GROUP BY snapshot.name, provider_playlist.provider_playlist_id, account_playlist.signal_class
         ORDER BY snapshot.name, provider_playlist.provider_playlist_id",
    )
    .bind(account_id)
    .bind(generation.generation_id)
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            let required = as_usize_i64(row.try_get("required_tracks")?)?;
            let represented = as_usize_i64(row.try_get("represented_tracks")?)?;
            Ok(CoverageRow {
                source_name: row.try_get("source_name")?,
                spotify_id: row.try_get("spotify_id")?,
                signal_class: row.try_get("signal_class")?,
                required_tracks: required,
                represented_tracks: represented,
                missing_tracks: required.saturating_sub(represented),
            })
        })
        .collect()
}

/// Lists retirement-source tracks not represented in the latest proposal.
pub async fn missing(
    database: &Database,
    account_label: &str,
    limit: u32,
) -> Result<Vec<MissingTrack>> {
    if limit == 0 || limit > 1_000 {
        return Err(ChordriftError::Configuration(
            "missing-track limit must be between 1 and 1000".to_owned(),
        ));
    }
    let account_id = account_id(database, account_label).await?;
    let generation = status(database, account_label).await?;
    let rows = sqlx::query(
        "WITH latest AS (
             SELECT id FROM provider_inventory_observations
             WHERE provider_account_id = $1 ORDER BY captured_at DESC, id DESC LIMIT 1
         ), required AS (
             SELECT provider_track.track_id,
                    string_agg(DISTINCT snapshot.name, ', ' ORDER BY snapshot.name)
                        AS source_playlists
             FROM provider_account_playlists account_playlist
             JOIN provider_playlists provider_playlist
               ON provider_playlist.id = account_playlist.provider_playlist_id
             JOIN latest ON true
             JOIN provider_observed_playlists snapshot
               ON snapshot.provider_playlist_id = provider_playlist.id
              AND snapshot.snapshot_id = latest.id
             JOIN provider_observed_playlist_tracks membership
               ON membership.provider_playlist_id = provider_playlist.id
              AND membership.snapshot_id = latest.id
             JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
             WHERE account_playlist.provider_account_id = $1
               AND account_playlist.signal_class IN ('semantic_legacy', 'intake')
             GROUP BY provider_track.track_id
         ), proposed AS (
             SELECT membership.track_id,
                    count(DISTINCT membership.playlist_id)::bigint AS destinations
             FROM playlists playlist
             JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
             WHERE playlist.generation_id = $2
             GROUP BY membership.track_id
         )
         SELECT track.title,
                COALESCE(string_agg(DISTINCT artist.name, ', '), '') AS artists,
                min(provider_track.provider_track_id) AS spotify_id,
                required.source_playlists
         FROM required
         JOIN tracks track ON track.id = required.track_id
         JOIN provider_tracks provider_track
           ON provider_track.track_id = track.id AND provider_track.provider = 'spotify'
         LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
         LEFT JOIN artists artist ON artist.id = track_artist.artist_id
         LEFT JOIN proposed ON proposed.track_id = required.track_id
         WHERE proposed.track_id IS NULL
         GROUP BY track.id, track.title, required.source_playlists
         ORDER BY lower(track.title), spotify_id
         LIMIT $3",
    )
    .bind(account_id)
    .bind(generation.generation_id)
    .bind(i64::from(limit))
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(MissingTrack {
                title: row.try_get("title")?,
                artists: row.try_get("artists")?,
                spotify_id: row.try_get("spotify_id")?,
                source_playlists: row.try_get("source_playlists")?,
            })
        })
        .collect()
}

/// Audits the complete preserved-library universe.
pub async fn historical_coverage(
    database: &Database,
    account_label: &str,
) -> Result<Vec<HistoricalCoverageRow>> {
    let account_id = account_id(database, account_label).await?;
    let generation = status(database, account_label).await?;
    let rows = sqlx::query(
        "WITH latest AS (
             SELECT id FROM provider_inventory_observations
             WHERE provider_account_id = $1
             ORDER BY captured_at DESC, id DESC LIMIT 1
         ), sources AS (
             SELECT DISTINCT 'saved'::text AS signal_class, NULL::uuid AS playlist_id,
                    provider_track.track_id
             FROM latest
             JOIN provider_observed_saved_tracks saved ON saved.snapshot_id = latest.id
             JOIN provider_tracks provider_track ON provider_track.id = saved.provider_track_id
             UNION ALL
             SELECT DISTINCT policy.signal_class, policy.provider_playlist_id,
                    provider_track.track_id
             FROM provider_account_playlists policy
             JOIN provider_inventory_observations library
               ON library.provider_account_id = policy.provider_account_id
             JOIN provider_observed_playlist_tracks membership
               ON membership.snapshot_id = library.id
              AND membership.provider_playlist_id = policy.provider_playlist_id
             JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
             WHERE policy.provider_account_id = $1
               AND policy.signal_class IN ('semantic_legacy', 'transport', 'intake', 'canonical')
             UNION ALL
             SELECT 'exclusion'::text, NULL::uuid, exclusion.track_id
             FROM excluded_tracks exclusion
             WHERE exclusion.provider_account_id = $1 AND exclusion.restored_at IS NULL
         ), proposed AS (
             SELECT membership.track_id,
                    count(DISTINCT membership.playlist_id)::bigint AS destinations
             FROM playlists playlist
             JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
             WHERE playlist.generation_id = $2
             GROUP BY membership.track_id
         ), active_exclusion AS (
             SELECT DISTINCT track_id FROM excluded_tracks
             WHERE provider_account_id = $1 AND restored_at IS NULL
         ), grouped AS (
             SELECT signal_class, count(DISTINCT playlist_id)::bigint AS playlist_count,
                    count(DISTINCT track_id)::bigint AS unique_tracks,
                    count(DISTINCT track_id) FILTER (WHERE proposed.track_id IS NOT NULL)::bigint
                        AS represented_tracks,
                    count(DISTINCT track_id) FILTER (WHERE active_exclusion.track_id IS NOT NULL)::bigint
                        AS excluded_tracks,
                    count(DISTINCT track_id) FILTER (
                        WHERE proposed.track_id IS NULL AND active_exclusion.track_id IS NULL
                    )::bigint AS missing_tracks,
                    count(DISTINCT track_id) FILTER (
                        WHERE proposed.destinations > 1 OR (
                            proposed.track_id IS NOT NULL AND active_exclusion.track_id IS NOT NULL
                        )
                    )::bigint AS conflicting_tracks
             FROM sources
             LEFT JOIN proposed USING (track_id)
             LEFT JOIN active_exclusion USING (track_id)
             GROUP BY signal_class
         ), total AS (
             SELECT 'complete_inventory'::text AS signal_class,
                    count(DISTINCT playlist_id)::bigint AS playlist_count,
                    count(DISTINCT track_id)::bigint AS unique_tracks,
                    count(DISTINCT track_id) FILTER (WHERE proposed.track_id IS NOT NULL)::bigint
                        AS represented_tracks,
                    count(DISTINCT track_id) FILTER (WHERE active_exclusion.track_id IS NOT NULL)::bigint
                        AS excluded_tracks,
                    count(DISTINCT track_id) FILTER (
                        WHERE proposed.track_id IS NULL AND active_exclusion.track_id IS NULL
                    )::bigint AS missing_tracks,
                    count(DISTINCT track_id) FILTER (
                        WHERE proposed.destinations > 1 OR (
                            proposed.track_id IS NOT NULL AND active_exclusion.track_id IS NOT NULL
                        )
                    )::bigint AS conflicting_tracks
             FROM sources
             LEFT JOIN proposed USING (track_id)
             LEFT JOIN active_exclusion USING (track_id)
         )
         SELECT * FROM (SELECT * FROM total UNION ALL SELECT * FROM grouped) inventory_rows
         ORDER BY CASE WHEN signal_class = 'complete_inventory' THEN 0 ELSE 1 END,
                  signal_class",
    )
    .bind(account_id)
    .bind(generation.generation_id)
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(HistoricalCoverageRow {
                signal_class: row.try_get("signal_class")?,
                playlist_count: as_usize_i64(row.try_get("playlist_count")?)?,
                unique_tracks: as_usize_i64(row.try_get("unique_tracks")?)?,
                represented_tracks: as_usize_i64(row.try_get("represented_tracks")?)?,
                excluded_tracks: as_usize_i64(row.try_get("excluded_tracks")?)?,
                missing_tracks: as_usize_i64(row.try_get("missing_tracks")?)?,
                conflicting_tracks: as_usize_i64(row.try_get("conflicting_tracks")?)?,
            })
        })
        .collect()
}

/// Lists unresolved tracks across the complete preserved-library universe.
pub async fn historical_missing(
    database: &Database,
    account_label: &str,
    limit: u32,
) -> Result<Vec<HistoricalMissingTrack>> {
    if limit == 0 || limit > 10_000 {
        return Err(ChordriftError::Configuration(
            "inventory unresolved-track limit must be between 1 and 10000".to_owned(),
        ));
    }
    let account_id = account_id(database, account_label).await?;
    let generation = status(database, account_label).await?;
    let rows = sqlx::query(
        "WITH latest AS (
             SELECT id FROM provider_inventory_observations
             WHERE provider_account_id = $1
             ORDER BY captured_at DESC, id DESC LIMIT 1
         ), sources AS (
             SELECT DISTINCT provider_track.track_id, 'Saved tracks'::text AS name,
                    'saved'::text AS signal_class
             FROM latest
             JOIN provider_observed_saved_tracks saved ON saved.snapshot_id = latest.id
             JOIN provider_tracks provider_track ON provider_track.id = saved.provider_track_id
             UNION ALL
             SELECT DISTINCT provider_track.track_id, snapshot.name, policy.signal_class
             FROM provider_account_playlists policy
             JOIN provider_inventory_observations library
               ON library.provider_account_id = policy.provider_account_id
             JOIN provider_observed_playlists snapshot
               ON snapshot.snapshot_id = library.id
              AND snapshot.provider_playlist_id = policy.provider_playlist_id
             JOIN provider_observed_playlist_tracks membership
               ON membership.snapshot_id = library.id
              AND membership.provider_playlist_id = policy.provider_playlist_id
             JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
             WHERE policy.provider_account_id = $1
               AND policy.signal_class IN ('semantic_legacy', 'transport', 'intake', 'canonical')
             UNION ALL
             SELECT exclusion.track_id, 'Explicit exclusion'::text, 'exclusion'::text
             FROM excluded_tracks exclusion
             WHERE exclusion.provider_account_id = $1 AND exclusion.restored_at IS NULL
         ), proposed AS (
             SELECT DISTINCT membership.track_id
             FROM playlists playlist
             JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
             WHERE playlist.generation_id = $2
         )
         SELECT track.title,
                COALESCE(string_agg(DISTINCT artist.name, ', '), '') AS artists,
                min(provider_track.provider_track_id) AS spotify_id,
                string_agg(DISTINCT sources.name, ' / ' ORDER BY sources.name)
                    AS source_playlists,
                string_agg(DISTINCT sources.signal_class, ', ' ORDER BY sources.signal_class)
                    AS signal_classes
         FROM sources
         JOIN tracks track ON track.id = sources.track_id
         JOIN provider_tracks provider_track
           ON provider_track.track_id = track.id AND provider_track.provider = 'spotify'
         LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
         LEFT JOIN artists artist ON artist.id = track_artist.artist_id
         LEFT JOIN proposed ON proposed.track_id = sources.track_id
         LEFT JOIN excluded_tracks exclusion
           ON exclusion.provider_account_id = $1 AND exclusion.track_id = sources.track_id
          AND exclusion.restored_at IS NULL
         WHERE proposed.track_id IS NULL AND exclusion.id IS NULL
         GROUP BY track.id, track.title
         ORDER BY lower(track.title), spotify_id LIMIT $3",
    )
    .bind(account_id)
    .bind(generation.generation_id)
    .bind(i64::from(limit))
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(HistoricalMissingTrack {
                title: row.try_get("title")?,
                artists: row.try_get("artists")?,
                spotify_id: row.try_get("spotify_id")?,
                source_playlists: row.try_get("source_playlists")?,
                signal_classes: row.try_get("signal_classes")?,
            })
        })
        .collect()
}

/// Scores unresolved preserved tracks against the stable approved destinations.
pub async fn placement_audit(database: &Database, account_label: &str) -> Result<PlacementAudit> {
    let account_id = account_id(database, account_label).await?;
    let proposal_generation_id = status(database, account_label).await?.generation_id;
    let embedding_generation_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM embedding_generations WHERE provider_account_id = $1
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| ChordriftError::Configuration("no embedding generation exists".to_owned()))?;

    struct Destination {
        stable_key: String,
        name: String,
        sum: Vec<f64>,
        count: usize,
        strong: usize,
        usable: usize,
    }
    let rows = sqlx::query(
        "SELECT playlist.id, concept.stable_key, playlist.name, embedding.embedding
         FROM playlists playlist
         JOIN playlist_concepts concept ON concept.id = playlist.concept_id
         JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
         JOIN account_track_embeddings embedding
           ON embedding.track_id = membership.track_id AND embedding.generation_id = $2
         WHERE playlist.generation_id = $1
         ORDER BY playlist.id, membership.position",
    )
    .bind(proposal_generation_id)
    .bind(embedding_generation_id)
    .fetch_all(database.pool())
    .await?;
    let mut destinations: HashMap<Uuid, Destination> = HashMap::new();
    for row in rows {
        let playlist_id: Uuid = row.try_get("id")?;
        let vector: Vec<f64> = row.try_get("embedding")?;
        let destination = destinations
            .entry(playlist_id)
            .or_insert_with(|| Destination {
                stable_key: row
                    .try_get("stable_key")
                    .expect("selected stable key has correct type"),
                name: row
                    .try_get("name")
                    .expect("selected playlist name has correct type"),
                sum: vec![0.0; vector.len()],
                count: 0,
                strong: 0,
                usable: 0,
            });
        if destination.sum.len() != vector.len() {
            return Err(ChordriftError::Configuration(
                "approved playlist embeddings have inconsistent dimensions".to_owned(),
            ));
        }
        for (total, value) in destination.sum.iter_mut().zip(vector) {
            *total += value;
        }
        destination.count += 1;
    }
    if destinations.is_empty() {
        return Err(ChordriftError::Configuration(
            "approved proposal has no embedded destination tracks".to_owned(),
        ));
    }
    for destination in destinations.values_mut() {
        normalize(&mut destination.sum)?;
    }

    let rows = sqlx::query(
        "WITH placement AS (
             SELECT DISTINCT membership.track_id
             FROM playlists playlist
             JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
             WHERE playlist.generation_id = $2
         )
         SELECT track.id, embedding.embedding
         FROM tracks track
         LEFT JOIN placement ON placement.track_id = track.id
         LEFT JOIN account_track_embeddings embedding
           ON embedding.track_id = track.id AND embedding.generation_id = $3
         LEFT JOIN excluded_tracks exclusion
           ON exclusion.provider_account_id = $1 AND exclusion.track_id = track.id
          AND exclusion.restored_at IS NULL
         WHERE account_track_is_library_candidate($1, track.id)
           AND placement.track_id IS NULL AND exclusion.id IS NULL
         ORDER BY track.id",
    )
    .bind(account_id)
    .bind(proposal_generation_id)
    .bind(embedding_generation_id)
    .fetch_all(database.pool())
    .await?;
    let unresolved_tracks = rows.len();
    let mut embedded_unresolved_tracks = 0;
    let mut strong_fit_tracks = 0;
    let mut usable_fit_tracks = 0;
    let mut weak_fit_tracks = 0;
    let mut weak_track_ids = Vec::new();
    for row in rows {
        let track_id: Uuid = row.try_get("id")?;
        let Some(vector) = row.try_get::<Option<Vec<f64>>, _>("embedding")? else {
            continue;
        };
        embedded_unresolved_tracks += 1;
        let (destination_id, score) = destinations
            .iter()
            .map(|(id, destination)| (*id, dot(&vector, &destination.sum)))
            .max_by(|left, right| {
                left.1
                    .total_cmp(&right.1)
                    .then_with(|| left.0.cmp(&right.0))
            })
            .ok_or_else(|| {
                ChordriftError::Configuration("no destination centroid exists".to_owned())
            })?;
        if score >= 0.20 {
            strong_fit_tracks += 1;
            destinations
                .get_mut(&destination_id)
                .expect("selected destination exists")
                .strong += 1;
        } else if score >= 0.05 {
            usable_fit_tracks += 1;
            destinations
                .get_mut(&destination_id)
                .expect("selected destination exists")
                .usable += 1;
        } else {
            weak_fit_tracks += 1;
            weak_track_ids.push(track_id);
        }
    }
    let inventory_tracks: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM tracks track
         WHERE account_track_is_library_candidate($1, track.id)",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let already_placed_tracks: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT membership.track_id)::bigint
         FROM playlists playlist
         JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
         WHERE playlist.generation_id = $1",
    )
    .bind(proposal_generation_id)
    .fetch_one(database.pool())
    .await?;
    let mut destination_rows: Vec<_> = destinations
        .into_values()
        .map(|destination| PlacementDestinationAudit {
            stable_key: destination.stable_key,
            name: destination.name,
            strong_fit_tracks: destination.strong,
            usable_fit_tracks: destination.usable,
        })
        .collect();
    destination_rows.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.stable_key.cmp(&right.stable_key))
    });
    let cluster_generation = clusters::status(database, account_label).await?;
    let current_proposal = status(database, account_label).await?;
    let group_rows = sqlx::query(
        "SELECT cluster.machine_label,
                representative_track.title || ' — ' ||
                    COALESCE(string_agg(DISTINCT artist.name, ', '), '') AS representative,
                count(DISTINCT all_member.track_id)::bigint AS cluster_tracks,
                count(DISTINCT weak_member.track_id)::bigint AS weak_fit_tracks,
                COALESCE(placed.placed_tracks, 0)::bigint AS placed_tracks,
                dominant.name AS dominant_destination,
                COALESCE(dominant.dominant_tracks, 0)::bigint AS dominant_tracks
         FROM clusters cluster
         JOIN cluster_tracks all_member ON all_member.cluster_id = cluster.id
         JOIN cluster_tracks representative
           ON representative.cluster_id = cluster.id AND representative.representative_rank = 1
         JOIN tracks representative_track ON representative_track.id = representative.track_id
         LEFT JOIN track_artists track_artist ON track_artist.track_id = representative.track_id
         LEFT JOIN artists artist ON artist.id = track_artist.artist_id
         LEFT JOIN cluster_tracks weak_member
           ON weak_member.cluster_id = cluster.id AND weak_member.track_id = ANY($2)
         LEFT JOIN LATERAL (
             SELECT count(DISTINCT current_member.track_id)::bigint AS placed_tracks
             FROM cluster_tracks current_member
             JOIN playlist_tracks proposed ON proposed.track_id = current_member.track_id
             JOIN playlists playlist
               ON playlist.id = proposed.playlist_id AND playlist.generation_id = $3
             WHERE current_member.cluster_id = cluster.id
         ) placed ON TRUE
         LEFT JOIN LATERAL (
             SELECT playlist.name,
                    count(DISTINCT current_member.track_id)::bigint AS dominant_tracks
             FROM cluster_tracks current_member
             JOIN playlist_tracks proposed ON proposed.track_id = current_member.track_id
             JOIN playlists playlist
               ON playlist.id = proposed.playlist_id AND playlist.generation_id = $3
             WHERE current_member.cluster_id = cluster.id
             GROUP BY playlist.id, playlist.name
             ORDER BY count(DISTINCT current_member.track_id) DESC, playlist.name
             LIMIT 1
         ) dominant ON TRUE
         WHERE cluster.generation_id = $1
         GROUP BY cluster.id, cluster.machine_label, representative_track.title,
                  placed.placed_tracks, dominant.name, dominant.dominant_tracks
         HAVING count(DISTINCT weak_member.track_id) > 0
         ORDER BY count(DISTINCT weak_member.track_id) DESC, cluster.machine_label",
    )
    .bind(cluster_generation.generation_id)
    .bind(&weak_track_ids)
    .bind(current_proposal.generation_id)
    .fetch_all(database.pool())
    .await?;
    let new_group_candidates = group_rows
        .into_iter()
        .map(|row| {
            Ok(PlacementGroupAudit {
                machine_label: row.try_get("machine_label")?,
                representative: row.try_get("representative")?,
                cluster_tracks: as_usize_i64(row.try_get("cluster_tracks")?)?,
                weak_fit_tracks: as_usize_i64(row.try_get("weak_fit_tracks")?)?,
                placed_tracks: as_usize_i64(row.try_get("placed_tracks")?)?,
                dominant_destination: row.try_get("dominant_destination")?,
                dominant_tracks: as_usize_i64(row.try_get("dominant_tracks")?)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(PlacementAudit {
        proposal_generation_id,
        embedding_generation_id,
        inventory_tracks: as_usize_i64(inventory_tracks)?,
        already_placed_tracks: as_usize_i64(already_placed_tracks)?,
        embedded_unresolved_tracks,
        unembedded_unresolved_tracks: unresolved_tracks.saturating_sub(embedded_unresolved_tracks),
        strong_fit_tracks,
        usable_fit_tracks,
        weak_fit_tracks,
        destinations: destination_rows,
        new_group_candidates,
    })
}

fn normalize(vector: &mut [f64]) -> Result<()> {
    let norm = vector.iter().map(|value| value * value).sum::<f64>().sqrt();
    if norm <= f64::EPSILON {
        return Err(ChordriftError::Configuration(
            "playlist centroid has zero norm".to_owned(),
        ));
    }
    for value in vector {
        *value /= norm;
    }
    Ok(())
}

fn dot(left: &[f64], right: &[f64]) -> f64 {
    left.iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}

/// Lists unresolved tracks from one current analytical cluster.
pub async fn unresolved_group_tracks(
    database: &Database,
    account_label: &str,
    machine_label: &str,
    limit: u32,
) -> Result<Vec<GroupTrack>> {
    if limit == 0 || limit > 1_000 {
        return Err(ChordriftError::Configuration(
            "group-track limit must be between 1 and 1000".to_owned(),
        ));
    }
    let account_id = account_id(database, account_label).await?;
    let proposal = status(database, account_label).await?;
    let cluster_generation = clusters::status(database, account_label).await?;
    let rows = sqlx::query(
        "WITH placed AS (
             SELECT DISTINCT membership.track_id
             FROM playlists playlist
             JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
             WHERE playlist.generation_id = $3
         )
         SELECT membership.representative_rank, membership.membership_score,
                track.title,
                COALESCE(string_agg(DISTINCT artist.name, ', '), '') AS artists,
                min(provider_track.provider_track_id) AS spotify_id
         FROM clusters cluster
         JOIN cluster_tracks membership ON membership.cluster_id = cluster.id
         JOIN tracks track ON track.id = membership.track_id
         JOIN provider_tracks provider_track
           ON provider_track.track_id = track.id AND provider_track.provider = 'spotify'
         LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
         LEFT JOIN artists artist ON artist.id = track_artist.artist_id
         LEFT JOIN placed ON placed.track_id = track.id
         LEFT JOIN excluded_tracks exclusion
           ON exclusion.provider_account_id = $4 AND exclusion.track_id = track.id
          AND exclusion.restored_at IS NULL
         WHERE cluster.generation_id = $1 AND cluster.machine_label = $2
           AND placed.track_id IS NULL AND exclusion.id IS NULL
         GROUP BY membership.representative_rank, membership.membership_score,
                  track.id, track.title
         ORDER BY membership.representative_rank, track.id LIMIT $5",
    )
    .bind(cluster_generation.generation_id)
    .bind(machine_label)
    .bind(proposal.generation_id)
    .bind(account_id)
    .bind(i64::from(limit))
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(GroupTrack {
                position: as_usize(row.try_get("representative_rank")?)?,
                score: row.try_get("membership_score")?,
                title: row.try_get("title")?,
                artists: row.try_get("artists")?,
                spotify_id: row.try_get("spotify_id")?,
            })
        })
        .collect()
}

/// Assigns unresolved embedded tracks to a cluster's dominant existing destination.
pub async fn assign_by_group_consensus(
    database: &Database,
    account_label: &str,
    min_dominance: f64,
    min_evidence: u32,
) -> Result<ConsensusAssignmentReport> {
    if !(0.5..=1.0).contains(&min_dominance) || !min_dominance.is_finite() {
        return Err(ChordriftError::Configuration(
            "minimum group dominance must be finite and between 0.5 and 1".to_owned(),
        ));
    }
    if !(2..=10_000).contains(&min_evidence) {
        return Err(ChordriftError::Configuration(
            "minimum group evidence must be between 2 and 10000".to_owned(),
        ));
    }
    let account_id = account_id(database, account_label).await?;
    let proposal = status(database, account_label).await?;
    require_editable(&proposal)?;
    let cluster_generation = clusters::status(database, account_label).await?;
    let rows = sqlx::query(
        "WITH placed AS (
             SELECT membership.track_id, playlist.id AS playlist_id
             FROM playlists playlist
             JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
             WHERE playlist.generation_id = $2
         ), destination_count AS (
             SELECT cluster.id AS cluster_id, placed.playlist_id,
                    count(DISTINCT membership.track_id)::bigint AS destination_tracks
             FROM clusters cluster
             JOIN cluster_tracks membership ON membership.cluster_id = cluster.id
             JOIN placed ON placed.track_id = membership.track_id
             WHERE cluster.generation_id = $1
             GROUP BY cluster.id, placed.playlist_id
         ), ranked AS (
             SELECT destination_count.*,
                    sum(destination_tracks) OVER (PARTITION BY cluster_id)::bigint AS placed_tracks,
                    row_number() OVER (
                        PARTITION BY cluster_id
                        ORDER BY destination_tracks DESC, playlist_id
                    ) AS destination_rank
             FROM destination_count
         ), dominant AS (
             SELECT cluster_id, playlist_id, destination_tracks, placed_tracks
             FROM ranked
             WHERE destination_rank = 1 AND placed_tracks >= $3
               AND destination_tracks::double precision / placed_tracks >= $4
         )
         SELECT membership.track_id, dominant.playlist_id,
                dominant.destination_tracks, dominant.placed_tracks,
                membership.membership_score, cluster.machine_label
         FROM clusters cluster
         JOIN dominant ON dominant.cluster_id = cluster.id
         JOIN cluster_tracks membership ON membership.cluster_id = cluster.id
         JOIN account_track_embeddings embedding
           ON embedding.track_id = membership.track_id
          AND embedding.generation_id = (
              SELECT id FROM embedding_generations
              WHERE provider_account_id = $5 ORDER BY created_at DESC, id DESC LIMIT 1
          )
         LEFT JOIN placed ON placed.track_id = membership.track_id
         LEFT JOIN excluded_tracks exclusion
           ON exclusion.provider_account_id = $5
          AND exclusion.track_id = membership.track_id AND exclusion.restored_at IS NULL
         WHERE cluster.generation_id = $1
           AND placed.track_id IS NULL AND exclusion.id IS NULL
         ORDER BY dominant.playlist_id, membership.representative_rank, membership.track_id",
    )
    .bind(cluster_generation.generation_id)
    .bind(proposal.generation_id)
    .bind(i64::from(min_evidence))
    .bind(min_dominance)
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    let assigned_tracks = rows.len();
    let mut transaction = database.pool().begin().await?;
    let mut next_positions: HashMap<Uuid, i32> = HashMap::new();
    for row in rows {
        let playlist_id: Uuid = row.try_get("playlist_id")?;
        let position = if let Some(position) = next_positions.get_mut(&playlist_id) {
            position
        } else {
            let next: i32 = sqlx::query_scalar(
                "SELECT COALESCE(max(position) + 1, 0) FROM playlist_tracks WHERE playlist_id = $1",
            )
            .bind(playlist_id)
            .fetch_one(&mut *transaction)
            .await?;
            next_positions.entry(playlist_id).or_insert(next)
        };
        let destination_tracks: i64 = row.try_get("destination_tracks")?;
        let placed_tracks: i64 = row.try_get("placed_tracks")?;
        sqlx::query(
            "INSERT INTO playlist_tracks
             (playlist_id, track_id, position, source, provenance)
             VALUES ($1, $2, $3, 'generated', $4)",
        )
        .bind(playlist_id)
        .bind(row.try_get::<Uuid, _>("track_id")?)
        .bind(*position)
        .bind(json!({
            "method": "analytical-cluster-dominant-destination",
            "cluster_generation_id": cluster_generation.generation_id,
            "cluster": row.try_get::<String, _>("machine_label")?,
            "cluster_membership_score": row.try_get::<f64, _>("membership_score")?,
            "dominant_known_tracks": destination_tracks,
            "known_placed_tracks": placed_tracks,
            "dominance": destination_tracks as f64 / placed_tracks as f64,
            "minimum_dominance": min_dominance,
            "minimum_evidence": min_evidence
        }))
        .execute(&mut *transaction)
        .await?;
        *position += 1;
    }
    let represented_tracks = refresh_coverage_tx(
        &mut transaction,
        account_id,
        proposal.generation_id,
        proposal.required_track_count,
    )
    .await?;
    transaction.commit().await?;
    Ok(ConsensusAssignmentReport {
        generation_id: proposal.generation_id,
        assigned_tracks,
        required_tracks: proposal.required_track_count,
        represented_tracks,
        unresolved_tracks: proposal
            .required_track_count
            .saturating_sub(represented_tracks),
    })
}

/// Assigns unresolved embedded tracks directly to sufficiently similar destinations.
pub async fn assign_by_existing_centroid(
    database: &Database,
    account_label: &str,
    min_similarity: f64,
) -> Result<ConsensusAssignmentReport> {
    if !(-1.0..=1.0).contains(&min_similarity) || !min_similarity.is_finite() {
        return Err(ChordriftError::Configuration(
            "centroid minimum similarity must be finite and between -1 and 1".to_owned(),
        ));
    }
    let account_id = account_id(database, account_label).await?;
    let proposal = status(database, account_label).await?;
    require_editable(&proposal)?;
    let embedding_generation_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM embedding_generations WHERE provider_account_id = $1
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let rows = sqlx::query(
        "SELECT playlist.id, embedding.embedding
         FROM playlists playlist
         JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
         JOIN account_track_embeddings embedding
           ON embedding.track_id = membership.track_id AND embedding.generation_id = $2
         WHERE playlist.generation_id = $1 ORDER BY playlist.id, membership.position",
    )
    .bind(proposal.generation_id)
    .bind(embedding_generation_id)
    .fetch_all(database.pool())
    .await?;
    let mut centroids: HashMap<Uuid, Vec<f64>> = HashMap::new();
    for row in rows {
        let playlist_id: Uuid = row.try_get("id")?;
        let vector: Vec<f64> = row.try_get("embedding")?;
        let centroid = centroids
            .entry(playlist_id)
            .or_insert_with(|| vec![0.0; vector.len()]);
        for (total, value) in centroid.iter_mut().zip(vector) {
            *total += value;
        }
    }
    for centroid in centroids.values_mut() {
        normalize(centroid)?;
    }
    let rows = sqlx::query(
        "WITH placed AS (
             SELECT DISTINCT membership.track_id
             FROM playlists playlist
             JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
             WHERE playlist.generation_id = $2
         )
         SELECT track.id, embedding.embedding
         FROM tracks track
         JOIN account_track_embeddings embedding
           ON embedding.track_id = track.id AND embedding.generation_id = $3
         LEFT JOIN placed ON placed.track_id = track.id
         LEFT JOIN excluded_tracks exclusion
           ON exclusion.provider_account_id = $1 AND exclusion.track_id = track.id
          AND exclusion.restored_at IS NULL
         WHERE account_track_is_library_candidate($1, track.id)
           AND placed.track_id IS NULL AND exclusion.id IS NULL
         ORDER BY track.id",
    )
    .bind(account_id)
    .bind(proposal.generation_id)
    .bind(embedding_generation_id)
    .fetch_all(database.pool())
    .await?;
    let mut assignments = Vec::new();
    for row in rows {
        let track_id: Uuid = row.try_get("id")?;
        let vector: Vec<f64> = row.try_get("embedding")?;
        if let Some((playlist_id, score)) = centroids
            .iter()
            .map(|(id, centroid)| (*id, dot(&vector, centroid)))
            .max_by(|left, right| {
                left.1
                    .total_cmp(&right.1)
                    .then_with(|| left.0.cmp(&right.0))
            })
            .filter(|(_, score)| *score >= min_similarity)
        {
            assignments.push((playlist_id, track_id, score));
        }
    }
    assignments.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| right.2.total_cmp(&left.2))
            .then_with(|| left.1.cmp(&right.1))
    });
    let assigned_tracks = assignments.len();
    let mut transaction = database.pool().begin().await?;
    let mut next_positions: HashMap<Uuid, i32> = HashMap::new();
    for (playlist_id, track_id, score) in assignments {
        let position = if let Some(position) = next_positions.get_mut(&playlist_id) {
            position
        } else {
            let next: i32 = sqlx::query_scalar(
                "SELECT COALESCE(max(position) + 1, 0) FROM playlist_tracks WHERE playlist_id = $1",
            )
            .bind(playlist_id)
            .fetch_one(&mut *transaction)
            .await?;
            next_positions.entry(playlist_id).or_insert(next)
        };
        sqlx::query(
            "INSERT INTO playlist_tracks
             (playlist_id, track_id, position, source, provenance)
             VALUES ($1, $2, $3, 'generated', $4)",
        )
        .bind(playlist_id)
        .bind(track_id)
        .bind(*position)
        .bind(json!({
            "method": "current-playlist-centroid",
            "similarity": score,
            "minimum_similarity": min_similarity,
            "embedding_generation_id": embedding_generation_id
        }))
        .execute(&mut *transaction)
        .await?;
        *position += 1;
    }
    let represented_tracks = refresh_coverage_tx(
        &mut transaction,
        account_id,
        proposal.generation_id,
        proposal.required_track_count,
    )
    .await?;
    transaction.commit().await?;
    Ok(ConsensusAssignmentReport {
        generation_id: proposal.generation_id,
        assigned_tracks,
        required_tracks: proposal.required_track_count,
        represented_tracks,
        unresolved_tracks: proposal
            .required_track_count
            .saturating_sub(represented_tracks),
    })
}

/// Preserves an approved playlist structure and appends credible centroid fits.
pub async fn extend_approved(
    database: &Database,
    account_label: &str,
    min_similarity: f64,
) -> Result<GenerationReport> {
    if !(-1.0..=1.0).contains(&min_similarity) || !min_similarity.is_finite() {
        return Err(ChordriftError::Configuration(
            "extension minimum similarity must be finite and between -1 and 1".to_owned(),
        ));
    }
    let account_id = account_id(database, account_label).await?;
    let base = sqlx::query(
        "SELECT id, cluster_generation_id, input_hash
         FROM playlist_generations
         WHERE provider_account_id = $1 AND status = 'approved'
         ORDER BY approved_at DESC, created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| ChordriftError::Configuration("no approved proposal exists".to_owned()))?;
    let base_generation_id: Uuid = base.try_get("id")?;
    let cluster_generation_id: Uuid = base.try_get("cluster_generation_id")?;
    let base_input_hash: String = base.try_get("input_hash")?;
    let embedding = sqlx::query(
        "SELECT id, input_hash FROM embedding_generations
         WHERE provider_account_id = $1 ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let embedding_generation_id: Uuid = embedding.try_get("id")?;
    let embedding_input_hash: String = embedding.try_get("input_hash")?;
    let input_hash = hash_parts(&[
        "stable-playlist-extension",
        "2",
        &base_generation_id.to_string(),
        &base_input_hash,
        &embedding_generation_id.to_string(),
        &embedding_input_hash,
        &min_similarity.to_bits().to_string(),
    ]);
    if let Some(report) = reused_report(database, account_id, &input_hash).await? {
        return Ok(report);
    }

    struct Centroid {
        sum: Vec<f64>,
    }
    let rows = sqlx::query(
        "SELECT playlist.id, embedding.embedding
         FROM playlists playlist
         JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
         JOIN account_track_embeddings embedding
           ON embedding.track_id = membership.track_id AND embedding.generation_id = $2
         WHERE playlist.generation_id = $1
         ORDER BY playlist.id, membership.position",
    )
    .bind(base_generation_id)
    .bind(embedding_generation_id)
    .fetch_all(database.pool())
    .await?;
    let mut centroids: HashMap<Uuid, Centroid> = HashMap::new();
    for row in rows {
        let playlist_id: Uuid = row.try_get("id")?;
        let vector: Vec<f64> = row.try_get("embedding")?;
        let centroid = centroids.entry(playlist_id).or_insert_with(|| Centroid {
            sum: vec![0.0; vector.len()],
        });
        for (total, value) in centroid.sum.iter_mut().zip(vector) {
            *total += value;
        }
    }
    for centroid in centroids.values_mut() {
        normalize(&mut centroid.sum)?;
    }
    let candidate_rows = sqlx::query(
        "WITH placement AS (
             SELECT DISTINCT membership.track_id
             FROM playlists playlist
             JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
             WHERE playlist.generation_id = $2
         )
         SELECT track.id, embedding.embedding
         FROM tracks track
         JOIN account_track_embeddings embedding
           ON embedding.track_id = track.id AND embedding.generation_id = $3
         LEFT JOIN placement ON placement.track_id = track.id
         LEFT JOIN excluded_tracks exclusion
           ON exclusion.provider_account_id = $1 AND exclusion.track_id = track.id
          AND exclusion.restored_at IS NULL
         WHERE account_track_is_library_candidate($1, track.id)
           AND placement.track_id IS NULL AND exclusion.id IS NULL
         ORDER BY track.id",
    )
    .bind(account_id)
    .bind(base_generation_id)
    .bind(embedding_generation_id)
    .fetch_all(database.pool())
    .await?;
    let mut assignments = Vec::new();
    for row in candidate_rows {
        let track_id: Uuid = row.try_get("id")?;
        let vector: Vec<f64> = row.try_get("embedding")?;
        let Some((playlist_id, score)) = centroids
            .iter()
            .map(|(id, centroid)| (*id, dot(&vector, &centroid.sum)))
            .max_by(|left, right| {
                left.1
                    .total_cmp(&right.1)
                    .then_with(|| left.0.cmp(&right.0))
            })
        else {
            continue;
        };
        if score >= min_similarity {
            assignments.push((playlist_id, track_id, score));
        }
    }
    assignments.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| right.2.total_cmp(&left.2))
            .then_with(|| left.1.cmp(&right.1))
    });

    let required_track_count = required_track_count(database, account_id).await?;
    let mut transaction = database.pool().begin().await?;
    let generation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlist_generations
         (model, model_version, status, parameters, provider_account_id,
          cluster_generation_id, input_hash)
         VALUES ('stable-playlist-extension', '2', 'proposed', $1, $2, $3, $4)
         RETURNING id",
    )
    .bind(json!({
        "base_generation_id": base_generation_id,
        "embedding_generation_id": embedding_generation_id,
        "min_similarity": min_similarity,
        "preserves_existing_membership": true,
        "omits_active_exclusions": true,
        "spotify_writes": false
    }))
    .bind(account_id)
    .bind(cluster_generation_id)
    .bind(&input_hash)
    .fetch_one(&mut *transaction)
    .await?;
    let playlist_rows = sqlx::query(
        "SELECT id, concept_id, name, description, kind, machine_label, machine_tags
         FROM playlists WHERE generation_id = $1 ORDER BY id",
    )
    .bind(base_generation_id)
    .fetch_all(&mut *transaction)
    .await?;
    let mut playlist_map = HashMap::new();
    let mut next_positions = HashMap::new();
    for row in playlist_rows {
        let old_id: Uuid = row.try_get("id")?;
        let new_id: Uuid = sqlx::query_scalar(
            "INSERT INTO playlists
             (generation_id, concept_id, name, description, kind, machine_label, machine_tags)
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
        )
        .bind(generation_id)
        .bind(row.try_get::<Uuid, _>("concept_id")?)
        .bind(row.try_get::<String, _>("name")?)
        .bind(row.try_get::<Option<String>, _>("description")?)
        .bind(row.try_get::<String, _>("kind")?)
        .bind(row.try_get::<Option<String>, _>("machine_label")?)
        .bind(row.try_get::<Value, _>("machine_tags")?)
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position, source, provenance)
             SELECT $2, membership.track_id, membership.position, membership.source,
                    membership.provenance ||
                    jsonb_build_object('extended_from_generation_id', $3::uuid)
             FROM playlist_tracks membership
             LEFT JOIN excluded_tracks exclusion
               ON exclusion.provider_account_id = $4
              AND exclusion.track_id = membership.track_id
              AND exclusion.restored_at IS NULL
             WHERE membership.playlist_id = $1 AND exclusion.id IS NULL
             ORDER BY membership.position",
        )
        .bind(old_id)
        .bind(new_id)
        .bind(base_generation_id)
        .bind(account_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO playlist_name_revisions
             (playlist_id, name, description, machine_tags, generator_provider,
              generator_model, generator_model_version, artifact_sha256, selected)
             SELECT $2, name, description, machine_tags, generator_provider,
                    generator_model, generator_model_version, artifact_sha256, selected
             FROM playlist_name_revisions WHERE playlist_id = $1 AND selected",
        )
        .bind(old_id)
        .bind(new_id)
        .execute(&mut *transaction)
        .await?;
        let next: i32 = sqlx::query_scalar(
            "SELECT COALESCE(max(position) + 1, 0) FROM playlist_tracks WHERE playlist_id = $1",
        )
        .bind(new_id)
        .fetch_one(&mut *transaction)
        .await?;
        playlist_map.insert(old_id, new_id);
        next_positions.insert(old_id, next);
    }
    for (old_playlist_id, track_id, score) in assignments {
        let new_playlist_id = playlist_map[&old_playlist_id];
        let position = next_positions
            .get_mut(&old_playlist_id)
            .expect("copied destination has next position");
        sqlx::query(
            "INSERT INTO playlist_tracks
             (playlist_id, track_id, position, source, provenance)
             VALUES ($1, $2, $3, 'generated', $4)",
        )
        .bind(new_playlist_id)
        .bind(track_id)
        .bind(*position)
        .bind(json!({
            "method": "approved-playlist-centroid",
            "similarity": score,
            "embedding_generation_id": embedding_generation_id,
            "base_generation_id": base_generation_id
        }))
        .execute(&mut *transaction)
        .await?;
        *position += 1;
    }
    replay_assignment_overrides(&mut transaction, account_id, generation_id).await?;
    let assigned_track_count: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT membership.track_id)::bigint
         FROM playlists playlist JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
         WHERE playlist.generation_id = $1",
    )
    .bind(generation_id)
    .fetch_one(&mut *transaction)
    .await?;
    let represented_track_count =
        represented_required_track_count_tx(&mut transaction, account_id, generation_id).await?;
    let playlist_count: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM playlists WHERE generation_id = $1")
            .bind(generation_id)
            .fetch_one(&mut *transaction)
            .await?;
    let coverage_complete = represented_track_count == required_track_count;
    sqlx::query(
        "UPDATE playlist_generations SET required_track_count = $2,
         represented_track_count = $3, coverage_complete = $4 WHERE id = $1",
    )
    .bind(generation_id)
    .bind(as_i32(required_track_count)?)
    .bind(as_i32(represented_track_count)?)
    .bind(coverage_complete)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(GenerationReport {
        generation_id,
        cluster_generation_id,
        reused: false,
        playlist_count: as_usize_i64(playlist_count)?,
        assigned_track_count: as_usize_i64(assigned_track_count)?,
        required_track_count,
        represented_track_count,
        coverage_complete,
        input_hash,
    })
}

/// Creates a stable manual category in the latest proposal.
pub async fn create_category(
    database: &Database,
    account_label: &str,
    name: &str,
    description: &str,
    tags: &[String],
) -> Result<ManualCategory> {
    let generation = status(database, account_label).await?;
    require_editable(&generation)?;
    validate_manual_category(name, description, tags)?;
    if list(database, account_label)
        .await?
        .iter()
        .any(|playlist| playlist.name.eq_ignore_ascii_case(name.trim()))
    {
        return Err(ChordriftError::Configuration(
            "a proposed playlist already uses that name".to_owned(),
        ));
    }
    let account_id = account_id(database, account_label).await?;
    let concept_id = Uuid::new_v4();
    let stable_key = format!("playlist-{}", &concept_id.simple().to_string()[..12]);
    let machine_label = format!("manual-{}", &concept_id.simple().to_string()[..12]);
    let mut transaction = database.pool().begin().await?;
    sqlx::query(
        "INSERT INTO playlist_concepts
         (id, provider_account_id, stable_key, origin, manual_name,
          manual_description, manual_tags)
         VALUES ($1, $2, $3, 'manual', $4, $5, $6)",
    )
    .bind(concept_id)
    .bind(account_id)
    .bind(&stable_key)
    .bind(name.trim())
    .bind(description.trim())
    .bind(json!(tags))
    .execute(&mut *transaction)
    .await?;
    let playlist_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlists
         (generation_id, concept_id, name, description, kind, machine_label, machine_tags)
         VALUES ($1, $2, $3, $4, 'manual', $5, $6) RETURNING id",
    )
    .bind(generation.generation_id)
    .bind(concept_id)
    .bind(name.trim())
    .bind(description.trim())
    .bind(machine_label)
    .bind(json!(tags))
    .fetch_one(&mut *transaction)
    .await?;
    let artifact_sha256 = hash_parts(&[
        "manual-category",
        &generation.generation_id.to_string(),
        &stable_key,
        name.trim(),
        description.trim(),
        &tags.join("\0"),
    ]);
    sqlx::query(
        "INSERT INTO playlist_name_revisions
         (playlist_id, name, description, machine_tags, generator_provider,
          generator_model, generator_model_version, artifact_sha256)
         VALUES ($1, $2, $3, $4, 'account-owner', 'manual-category', '1', $5)",
    )
    .bind(playlist_id)
    .bind(name.trim())
    .bind(description.trim())
    .bind(json!(tags))
    .bind(artifact_sha256)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(ManualCategory {
        stable_key,
        name: name.trim().to_owned(),
        generation_id: generation.generation_id,
    })
}

/// Assigns a track to a stable proposed playlist and supersedes any prior decision.
pub async fn assign(
    database: &Database,
    account_label: &str,
    spotify_id: &str,
    stable_key: &str,
    reason: &str,
) -> Result<AssignmentReport> {
    let spotify_ids = vec![spotify_id.to_owned()];
    assign_many(database, account_label, &spotify_ids, stable_key, reason)
        .await?
        .pop()
        .ok_or_else(|| ChordriftError::Configuration("no track was assigned".to_owned()))
}

/// Assigns several tracks to one stable proposed playlist in one transaction.
pub async fn assign_many(
    database: &Database,
    account_label: &str,
    spotify_ids: &[String],
    stable_key: &str,
    reason: &str,
) -> Result<Vec<AssignmentReport>> {
    if spotify_ids.is_empty() {
        return Err(ChordriftError::Configuration(
            "at least one Spotify track ID is required".to_owned(),
        ));
    }
    if reason.trim().is_empty() || reason.chars().count() > 300 {
        return Err(ChordriftError::Configuration(
            "assignment reason must contain 1-300 characters".to_owned(),
        ));
    }
    let unique_ids: HashSet<_> = spotify_ids.iter().collect();
    if unique_ids.len() != spotify_ids.len() {
        return Err(ChordriftError::Configuration(
            "Spotify track IDs must not be repeated".to_owned(),
        ));
    }
    let generation = status(database, account_label).await?;
    require_editable(&generation)?;
    let account_id = account_id(database, account_label).await?;
    let destination_concept: Uuid = sqlx::query_scalar(
        "SELECT concept.id
         FROM playlist_concepts concept
         JOIN playlists playlist ON playlist.concept_id = concept.id
         WHERE concept.provider_account_id = $1 AND concept.stable_key = $2
           AND playlist.generation_id = $3",
    )
    .bind(account_id)
    .bind(stable_key)
    .bind(generation.generation_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(
            "destination stable key is not in the latest proposal".to_owned(),
        )
    })?;
    let rows = sqlx::query(
        "SELECT track.id, track.title, provider_track.provider_track_id AS spotify_id
         FROM provider_tracks provider_track
         JOIN tracks track ON track.id = provider_track.track_id
         WHERE provider_track.provider = 'spotify'
           AND provider_track.provider_track_id = ANY($2)
           AND account_track_is_library_candidate($1, track.id)",
    )
    .bind(account_id)
    .bind(spotify_ids)
    .fetch_all(database.pool())
    .await?;
    let tracks: HashMap<String, (Uuid, String)> = rows
        .into_iter()
        .map(|row| {
            Ok((
                row.try_get("spotify_id")?,
                (row.try_get("id")?, row.try_get("title")?),
            ))
        })
        .collect::<Result<_>>()?;
    if tracks.len() != spotify_ids.len() {
        return Err(ChordriftError::Configuration(
            "one or more Spotify track IDs are not in this account's preserved library inventory"
                .to_owned(),
        ));
    }
    let track_ids: Vec<Uuid> = spotify_ids.iter().map(|id| tracks[id].0).collect();

    let mut transaction = database.pool().begin().await?;
    sqlx::query(
        "UPDATE track_playlist_assignment_revisions SET superseded_at = now()
         WHERE provider_account_id = $1 AND track_id = ANY($2)
           AND superseded_at IS NULL",
    )
    .bind(account_id)
    .bind(&track_ids)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO track_playlist_assignment_revisions
         (provider_account_id, track_id, destination_concept_id, decision,
          source_generation_id, reason)
         SELECT $1, input.track_id, $3, 'assign', $4, $5
         FROM unnest($2::uuid[]) WITH ORDINALITY AS input(track_id, position)
         ORDER BY input.position",
    )
    .bind(account_id)
    .bind(&track_ids)
    .bind(destination_concept)
    .bind(generation.generation_id)
    .bind(reason.trim())
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "DELETE FROM playlist_tracks membership USING playlists playlist
         WHERE membership.playlist_id = playlist.id
           AND playlist.generation_id = $1 AND membership.track_id = ANY($2)",
    )
    .bind(generation.generation_id)
    .bind(&track_ids)
    .execute(&mut *transaction)
    .await?;
    let playlist_id = ensure_concept_playlist(
        &mut transaction,
        generation.generation_id,
        destination_concept,
    )
    .await?;
    let next_position: i32 = sqlx::query_scalar(
        "SELECT COALESCE(max(position) + 1, 0) FROM playlist_tracks WHERE playlist_id = $1",
    )
    .bind(playlist_id)
    .fetch_one(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO playlist_tracks
         (playlist_id, track_id, position, source, provenance)
         SELECT $1, input.track_id, $3 + input.position::integer - 1, 'manual',
                jsonb_build_object('assignment_revision_id', revision.id)
         FROM unnest($2::uuid[]) WITH ORDINALITY AS input(track_id, position)
         JOIN track_playlist_assignment_revisions revision
           ON revision.provider_account_id = $4
          AND revision.track_id = input.track_id AND revision.superseded_at IS NULL
         ORDER BY input.position",
    )
    .bind(playlist_id)
    .bind(&track_ids)
    .bind(next_position)
    .bind(account_id)
    .execute(&mut *transaction)
    .await?;
    let represented = refresh_coverage_tx(
        &mut transaction,
        account_id,
        generation.generation_id,
        generation.required_track_count,
    )
    .await?;
    transaction.commit().await?;
    Ok(spotify_ids
        .iter()
        .map(|spotify_id| AssignmentReport {
            title: tracks[spotify_id].1.clone(),
            spotify_id: spotify_id.clone(),
            destination: Some(stable_key.to_owned()),
            represented_track_count: represented,
            missing_track_count: generation.required_track_count.saturating_sub(represented),
        })
        .collect())
}

/// Result of accepting the exact current provider order for one proposal playlist.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderOrderReport {
    /// Editable proposal whose order was updated.
    pub generation_id: Uuid,
    /// Stable destination key.
    pub stable_key: String,
    /// Human-facing destination name.
    pub name: String,
    /// Membership count proven equal on both sides.
    pub track_count: usize,
}

/// Copies current provider ordering into an editable proposal only when exact
/// membership equality proves that no track can be added or removed.
pub async fn align_provider_order(
    database: &Database,
    account_label: &str,
    stable_key: &str,
) -> Result<ProviderOrderReport> {
    let generation = status(database, account_label).await?;
    require_editable(&generation)?;
    let account_id = account_id(database, account_label).await?;
    let row = sqlx::query(
        "SELECT playlist.id, playlist.name, playlist.concept_id
         FROM playlists playlist
         JOIN playlist_concepts concept ON concept.id = playlist.concept_id
         WHERE playlist.generation_id = $1
           AND concept.provider_account_id = $2
           AND concept.stable_key = $3",
    )
    .bind(generation.generation_id)
    .bind(account_id)
    .bind(stable_key)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(
            "destination stable key is not in the latest proposal".to_owned(),
        )
    })?;
    let playlist_id: Uuid = row.try_get("id")?;
    let name: String = row.try_get("name")?;
    let concept_id: Uuid = row.try_get("concept_id")?;
    let provider_order = sqlx::query_scalar::<_, Uuid>(
        "SELECT provider_track.track_id
         FROM current_spotify_playlists current
         JOIN provider_playlists provider_playlist
           ON provider_playlist.id = current.provider_playlist_id
         JOIN provider_observed_playlist_tracks membership
           ON membership.snapshot_id = current.snapshot_id
          AND membership.provider_playlist_id = current.provider_playlist_id
         JOIN provider_tracks provider_track
           ON provider_track.id = membership.provider_track_id
         WHERE current.provider_account_id = $1
           AND current.signal_class = 'canonical'
           AND provider_playlist.concept_id = $2
         ORDER BY membership.position",
    )
    .bind(account_id)
    .bind(concept_id)
    .fetch_all(database.pool())
    .await?;
    let proposal_order = sqlx::query_scalar::<_, Uuid>(
        "SELECT track_id FROM playlist_tracks
         WHERE playlist_id = $1 ORDER BY position",
    )
    .bind(playlist_id)
    .fetch_all(database.pool())
    .await?;
    if !orders_have_equal_unique_membership(&provider_order, &proposal_order) {
        return Err(ChordriftError::Configuration(format!(
            "cannot accept provider order for `{stable_key}` unless provider and proposal memberships are exactly equal"
        )));
    }
    let track_count = provider_order.len();
    if provider_order != proposal_order {
        let mut transaction = database.pool().begin().await?;
        let offset = i32::try_from(track_count)
            .ok()
            .and_then(|count| count.checked_add(1_000_000))
            .ok_or_else(|| {
                ChordriftError::Configuration("playlist is too large to align safely".to_owned())
            })?;
        sqlx::query("UPDATE playlist_tracks SET position = position + $2 WHERE playlist_id = $1")
            .bind(playlist_id)
            .bind(offset)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "UPDATE playlist_tracks membership
             SET position = ordered.position::integer - 1
             FROM unnest($2::uuid[]) WITH ORDINALITY AS ordered(track_id, position)
             WHERE membership.playlist_id = $1
               AND membership.track_id = ordered.track_id",
        )
        .bind(playlist_id)
        .bind(&provider_order)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
    }
    Ok(ProviderOrderReport {
        generation_id: generation.generation_id,
        stable_key: stable_key.to_owned(),
        name,
        track_count,
    })
}

fn orders_have_equal_unique_membership(provider: &[Uuid], proposal: &[Uuid]) -> bool {
    let provider_set = provider.iter().copied().collect::<HashSet<_>>();
    let proposal_set = proposal.iter().copied().collect::<HashSet<_>>();
    provider.len() == provider_set.len()
        && proposal.len() == proposal_set.len()
        && provider_set == proposal_set
}

/// Returns a track to the internal needs-review queue.
pub async fn needs_review(
    database: &Database,
    account_label: &str,
    spotify_id: &str,
    reason: &str,
) -> Result<AssignmentReport> {
    change_assignment(database, account_label, spotify_id, None, reason).await
}

/// Removes an empty playlist from the latest editable proposal.
pub async fn retire_empty(
    database: &Database,
    account_label: &str,
    stable_key: &str,
    confirm: &str,
) -> Result<RetiredEmptyPlaylist> {
    let stable_key = stable_key.trim();
    if stable_key.is_empty() || confirm.trim() != stable_key {
        return Err(ChordriftError::Configuration(
            "--confirm must exactly repeat the stable playlist key".to_owned(),
        ));
    }
    let generation = status(database, account_label).await?;
    require_editable(&generation)?;
    let row = sqlx::query(
        "SELECT playlist.id, playlist.name,
                count(membership.id)::bigint AS tracks
         FROM playlists playlist
         JOIN playlist_concepts concept ON concept.id = playlist.concept_id
         LEFT JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
         WHERE playlist.generation_id = $1 AND concept.stable_key = $2
         GROUP BY playlist.id, playlist.name",
    )
    .bind(generation.generation_id)
    .bind(stable_key)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(format!(
            "latest proposal has no playlist with stable key `{stable_key}`"
        ))
    })?;
    let tracks: i64 = row.try_get("tracks")?;
    if tracks != 0 {
        return Err(ChordriftError::Configuration(format!(
            "playlist `{stable_key}` still contains {tracks} track(s); reassign or exclude every track first"
        )));
    }
    let playlist_id: Uuid = row.try_get("id")?;
    let name: String = row.try_get("name")?;
    sqlx::query("DELETE FROM playlists WHERE id = $1")
        .bind(playlist_id)
        .execute(database.pool())
        .await?;
    Ok(RetiredEmptyPlaylist {
        generation_id: generation.generation_id,
        stable_key: stable_key.to_owned(),
        name,
    })
}

/// Builds and records a deterministic naming context for the latest proposal.
pub async fn naming_context(database: &Database, account_label: &str) -> Result<NamingContext> {
    let generation = status(database, account_label).await?;
    if generation.state != "proposed" {
        return Err(ChordriftError::Configuration(
            "only a proposed generation can receive naming artifacts".to_owned(),
        ));
    }
    let playlists = list(database, account_label).await?;
    let mut contexts = Vec::with_capacity(playlists.len());
    for playlist in playlists {
        let samples = tracks(database, account_label, &playlist.stable_key, 12).await?;
        contexts.push(NamingPlaylistContext {
            stable_key: playlist.stable_key,
            machine_label: playlist.machine_label,
            track_count: playlist.track_count,
            sample_tracks: samples
                .into_iter()
                .map(|track| NamingTrack {
                    title: track.title,
                    artists: track.artists,
                })
                .collect(),
        });
    }
    let instructions = vec![
        "Create a concise, distinctive vibe name; do not use provider or intake playlist names."
            .to_owned(),
        "Describe the shared sound without claiming facts absent from the samples.".to_owned(),
        "Return 2-6 short lowercase semantic tags per playlist.".to_owned(),
        "Preserve every stable_key exactly and return one result per playlist.".to_owned(),
    ];
    let unhashed = json!({
        "schema_version": 1,
        "account": account_label,
        "generation_id": generation.generation_id,
        "instructions": instructions,
        "playlists": contexts,
    });
    let context_sha256 = sha256(&serde_json::to_vec(&unhashed)?);
    sqlx::query(
        "UPDATE playlist_generations SET naming_context_hash = $2
         WHERE id = $1 AND status = 'proposed'",
    )
    .bind(generation.generation_id)
    .bind(&context_sha256)
    .execute(database.pool())
    .await?;
    Ok(NamingContext {
        schema_version: 1,
        account: account_label.to_owned(),
        generation_id: generation.generation_id,
        context_sha256,
        instructions,
        playlists: contexts,
    })
}

/// Imports a strict naming artifact and selects it for the latest proposal.
pub async fn import_names(
    database: &Database,
    account_label: &str,
    artifact: NamingArtifact,
    artifact_bytes: &[u8],
) -> Result<usize> {
    let generation = status(database, account_label).await?;
    validate_artifact(&generation, &artifact)?;
    let current = list(database, account_label).await?;
    validate_results(&current, &artifact.playlists)?;
    let by_key: HashMap<_, _> = artifact
        .playlists
        .iter()
        .map(|result| (result.stable_key.as_str(), result))
        .collect();
    let artifact_sha256 = sha256(artifact_bytes);
    let mut transaction = database.pool().begin().await?;
    for playlist in &current {
        let result = by_key[playlist.stable_key.as_str()];
        let playlist_id: Uuid = sqlx::query_scalar(
            "SELECT playlist.id FROM playlists playlist
             JOIN playlist_concepts concept ON concept.id = playlist.concept_id
             WHERE playlist.generation_id = $1 AND concept.stable_key = $2",
        )
        .bind(generation.generation_id)
        .bind(&playlist.stable_key)
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE playlist_name_revisions SET selected = FALSE
             WHERE playlist_id = $1 AND selected",
        )
        .bind(playlist_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO playlist_name_revisions
             (playlist_id, name, description, machine_tags, generator_provider,
              generator_model, generator_model_version, artifact_sha256)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(playlist_id)
        .bind(result.name.trim())
        .bind(result.description.trim())
        .bind(json!(result.tags))
        .bind(artifact.generator.provider.trim())
        .bind(artifact.generator.model.trim())
        .bind(artifact.generator.model_version.trim())
        .bind(&artifact_sha256)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE playlists SET name = $2, description = $3, machine_tags = $4,
             updated_at = now() WHERE id = $1",
        )
        .bind(playlist_id)
        .bind(result.name.trim())
        .bind(result.description.trim())
        .bind(json!(result.tags))
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(current.len())
}

/// Explicitly approves a fully named, fully covered proposal.
pub async fn approve(
    database: &Database,
    account_label: &str,
    generation_id: Uuid,
) -> Result<Status> {
    let current = status(database, account_label).await?;
    if current.generation_id != generation_id {
        return Err(ChordriftError::Configuration(
            "confirmation must equal the latest proposal generation ID".to_owned(),
        ));
    }
    if current.state != "proposed" {
        return Err(ChordriftError::Configuration(
            "the latest proposal is not awaiting approval".to_owned(),
        ));
    }
    if !current.coverage_complete {
        return Err(ChordriftError::Configuration(format!(
            "proposal cannot be approved: {} required tracks are not represented",
            current
                .required_track_count
                .saturating_sub(current.represented_track_count)
        )));
    }
    if current.named_playlist_count != current.playlist_count {
        return Err(ChordriftError::Configuration(format!(
            "proposal cannot be approved: {} playlists do not have selected names",
            current
                .playlist_count
                .saturating_sub(current.named_playlist_count)
        )));
    }
    sqlx::query(
        "UPDATE playlist_generations SET status = 'approved', approved_at = now(),
         approved_by = 'account-owner' WHERE id = $1 AND status = 'proposed'",
    )
    .bind(generation_id)
    .execute(database.pool())
    .await?;
    status(database, account_label).await
}

fn validate_artifact(status: &Status, artifact: &NamingArtifact) -> Result<()> {
    if artifact.schema_version != 1 {
        return Err(ChordriftError::Configuration(
            "naming artifact schema_version must be 1".to_owned(),
        ));
    }
    if artifact.generation_id != status.generation_id {
        return Err(ChordriftError::Configuration(
            "naming artifact targets a different proposal generation".to_owned(),
        ));
    }
    if status.naming_context_hash.as_deref() != Some(artifact.context_sha256.as_str()) {
        return Err(ChordriftError::Configuration(
            "naming artifact context hash does not match the exported context".to_owned(),
        ));
    }
    for value in [
        &artifact.generator.provider,
        &artifact.generator.model,
        &artifact.generator.model_version,
    ] {
        if value.trim().is_empty() || value.len() > 120 {
            return Err(ChordriftError::Configuration(
                "naming generator provenance must be non-empty and at most 120 characters"
                    .to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_results(current: &[PlaylistSummary], results: &[NamingResult]) -> Result<()> {
    let expected: HashSet<_> = current
        .iter()
        .map(|item| item.stable_key.as_str())
        .collect();
    let actual: HashSet<_> = results
        .iter()
        .map(|item| item.stable_key.as_str())
        .collect();
    if results.len() != expected.len() || actual != expected {
        return Err(ChordriftError::Configuration(
            "naming artifact must contain exactly one result for every stable playlist key"
                .to_owned(),
        ));
    }
    let reserved = ["inbox", "from friends", "liked from radio", "spatial audio"];
    let mut names = HashSet::new();
    for result in results {
        let name = result.name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err(ChordriftError::Configuration(
                "playlist names must contain 1-80 characters".to_owned(),
            ));
        }
        let normalized = name.to_lowercase();
        if reserved.contains(&normalized.as_str()) || !names.insert(normalized) {
            return Err(ChordriftError::Configuration(
                "playlist names must be unique and must not use reserved intake/spatial names"
                    .to_owned(),
            ));
        }
        if result.description.trim().is_empty() || result.description.chars().count() > 300 {
            return Err(ChordriftError::Configuration(
                "playlist descriptions must contain 1-300 characters".to_owned(),
            ));
        }
        if !(2..=6).contains(&result.tags.len())
            || result
                .tags
                .iter()
                .any(|tag| tag.trim().is_empty() || tag.chars().count() > 40)
        {
            return Err(ChordriftError::Configuration(
                "each playlist must have 2-6 non-empty tags of at most 40 characters".to_owned(),
            ));
        }
    }
    Ok(())
}

fn require_editable(status: &Status) -> Result<()> {
    if status.state != "proposed" {
        return Err(ChordriftError::Configuration(
            "manual categories and assignments require a proposal awaiting approval".to_owned(),
        ));
    }
    Ok(())
}

fn validate_manual_category(name: &str, description: &str, tags: &[String]) -> Result<()> {
    if name.trim().is_empty() || name.chars().count() > 80 {
        return Err(ChordriftError::Configuration(
            "manual category names must contain 1-80 characters".to_owned(),
        ));
    }
    if description.trim().is_empty() || description.chars().count() > 300 {
        return Err(ChordriftError::Configuration(
            "manual category descriptions must contain 1-300 characters".to_owned(),
        ));
    }
    if !(2..=6).contains(&tags.len())
        || tags
            .iter()
            .any(|tag| tag.trim().is_empty() || tag.chars().count() > 40)
    {
        return Err(ChordriftError::Configuration(
            "manual categories require 2-6 non-empty tags of at most 40 characters".to_owned(),
        ));
    }
    Ok(())
}

async fn change_assignment(
    database: &Database,
    account_label: &str,
    spotify_id: &str,
    destination: Option<&str>,
    reason: &str,
) -> Result<AssignmentReport> {
    if reason.trim().is_empty() || reason.chars().count() > 300 {
        return Err(ChordriftError::Configuration(
            "assignment reason must contain 1-300 characters".to_owned(),
        ));
    }
    let generation = status(database, account_label).await?;
    require_editable(&generation)?;
    let account_id = account_id(database, account_label).await?;
    let row = sqlx::query(
        "SELECT track.id, track.title
         FROM provider_tracks provider_track
         JOIN tracks track ON track.id = provider_track.track_id
         WHERE provider_track.provider = 'spotify' AND provider_track.provider_track_id = $2
           AND account_track_is_library_candidate($1, track.id)",
    )
    .bind(account_id)
    .bind(spotify_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(
            "Spotify track ID is not in this account's preserved library inventory".to_owned(),
        )
    })?;
    let track_id: Uuid = row.try_get("id")?;
    let title: String = row.try_get("title")?;
    let destination_concept: Option<Uuid> = if let Some(stable_key) = destination {
        Some(
            sqlx::query_scalar(
                "SELECT concept.id
                 FROM playlist_concepts concept
                 JOIN playlists playlist ON playlist.concept_id = concept.id
                 WHERE concept.provider_account_id = $1 AND concept.stable_key = $2
                   AND playlist.generation_id = $3",
            )
            .bind(account_id)
            .bind(stable_key)
            .bind(generation.generation_id)
            .fetch_optional(database.pool())
            .await?
            .ok_or_else(|| {
                ChordriftError::Configuration(
                    "destination stable key is not in the latest proposal".to_owned(),
                )
            })?,
        )
    } else {
        None
    };

    let mut transaction = database.pool().begin().await?;
    sqlx::query(
        "UPDATE track_playlist_assignment_revisions SET superseded_at = now()
         WHERE provider_account_id = $1 AND track_id = $2 AND superseded_at IS NULL",
    )
    .bind(account_id)
    .bind(track_id)
    .execute(&mut *transaction)
    .await?;
    let decision_id: Uuid = sqlx::query_scalar(
        "INSERT INTO track_playlist_assignment_revisions
         (provider_account_id, track_id, destination_concept_id, decision,
          source_generation_id, reason)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(account_id)
    .bind(track_id)
    .bind(destination_concept)
    .bind(if destination.is_some() {
        "assign"
    } else {
        "needs_review"
    })
    .bind(generation.generation_id)
    .bind(reason.trim())
    .fetch_one(&mut *transaction)
    .await?;
    sqlx::query(
        "DELETE FROM playlist_tracks membership USING playlists playlist
         WHERE membership.playlist_id = playlist.id
           AND playlist.generation_id = $1 AND membership.track_id = $2",
    )
    .bind(generation.generation_id)
    .bind(track_id)
    .execute(&mut *transaction)
    .await?;
    if let Some(concept_id) = destination_concept {
        let playlist_id =
            ensure_concept_playlist(&mut transaction, generation.generation_id, concept_id).await?;
        let position: i32 = sqlx::query_scalar(
            "SELECT COALESCE(max(position) + 1, 0) FROM playlist_tracks WHERE playlist_id = $1",
        )
        .bind(playlist_id)
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO playlist_tracks
             (playlist_id, track_id, position, source, provenance)
             VALUES ($1, $2, $3, 'manual', $4)",
        )
        .bind(playlist_id)
        .bind(track_id)
        .bind(position)
        .bind(json!({"assignment_revision_id": decision_id}))
        .execute(&mut *transaction)
        .await?;
    }
    let represented = refresh_coverage_tx(
        &mut transaction,
        account_id,
        generation.generation_id,
        generation.required_track_count,
    )
    .await?;
    transaction.commit().await?;
    Ok(AssignmentReport {
        title,
        spotify_id: spotify_id.to_owned(),
        destination: destination.map(str::to_owned),
        represented_track_count: represented,
        missing_track_count: generation.required_track_count.saturating_sub(represented),
    })
}

async fn replay_assignment_overrides(
    transaction: &mut Transaction<'_, Postgres>,
    account_id: Uuid,
    generation_id: Uuid,
) -> Result<()> {
    let destination_concepts: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT destination_concept_id
         FROM track_playlist_assignment_revisions
         WHERE provider_account_id = $1 AND superseded_at IS NULL
           AND decision = 'assign'
         ORDER BY destination_concept_id",
    )
    .bind(account_id)
    .fetch_all(&mut **transaction)
    .await?;
    for concept_id in destination_concepts {
        ensure_concept_playlist(transaction, generation_id, concept_id).await?;
    }
    sqlx::query(
        "DELETE FROM playlist_tracks membership USING playlists playlist
         WHERE membership.playlist_id = playlist.id
           AND playlist.generation_id = $1
           AND EXISTS (
               SELECT 1 FROM track_playlist_assignment_revisions revision
               WHERE revision.provider_account_id = $2
                 AND revision.track_id = membership.track_id
                 AND revision.superseded_at IS NULL
           )",
    )
    .bind(generation_id)
    .bind(account_id)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "WITH ranked AS (
             SELECT revision.id AS decision_id, revision.track_id, playlist.id AS playlist_id,
                    row_number() OVER (
                        PARTITION BY playlist.id ORDER BY revision.created_at, revision.id
                    )::integer AS destination_position
             FROM track_playlist_assignment_revisions revision
             JOIN playlists playlist
               ON playlist.generation_id = $1
              AND playlist.concept_id = revision.destination_concept_id
             WHERE revision.provider_account_id = $2
               AND revision.superseded_at IS NULL AND revision.decision = 'assign'
         ), base_position AS (
             SELECT playlist.id AS playlist_id,
                    COALESCE(max(membership.position), -1)::integer AS value
             FROM playlists playlist
             LEFT JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
             WHERE playlist.generation_id = $1
             GROUP BY playlist.id
         )
         INSERT INTO playlist_tracks
         (playlist_id, track_id, position, source, provenance)
         SELECT ranked.playlist_id, ranked.track_id,
                base_position.value + ranked.destination_position,
                'manual',
                jsonb_build_object(
                    'assignment_revision_id', ranked.decision_id,
                    'replayed', true
                )
         FROM ranked
         JOIN base_position ON base_position.playlist_id = ranked.playlist_id
         ORDER BY ranked.playlist_id, ranked.destination_position",
    )
    .bind(generation_id)
    .bind(account_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn ensure_concept_playlist(
    transaction: &mut Transaction<'_, Postgres>,
    generation_id: Uuid,
    concept_id: Uuid,
) -> Result<Uuid> {
    if let Some(id) =
        sqlx::query_scalar("SELECT id FROM playlists WHERE generation_id = $1 AND concept_id = $2")
            .bind(generation_id)
            .bind(concept_id)
            .fetch_optional(&mut **transaction)
            .await?
    {
        return Ok(id);
    }
    let row = sqlx::query(
        "SELECT concept.stable_key,
                COALESCE(concept.manual_name, previous.name) AS name,
                COALESCE(concept.manual_description, previous.description, 'Manual assignment') AS description,
                CASE WHEN concept.origin = 'manual' THEN concept.manual_tags
                     ELSE COALESCE(previous.machine_tags, '[]'::jsonb) END AS tags
         FROM playlist_concepts concept
         LEFT JOIN LATERAL (
             SELECT playlist.name, playlist.description, playlist.machine_tags
             FROM playlists playlist WHERE playlist.concept_id = concept.id
             ORDER BY playlist.created_at DESC, playlist.id DESC LIMIT 1
         ) previous ON TRUE
         WHERE concept.id = $1",
    )
    .bind(concept_id)
    .fetch_one(&mut **transaction)
    .await?;
    let stable_key: String = row.try_get("stable_key")?;
    let name: String = row.try_get("name")?;
    let description: String = row.try_get("description")?;
    let tags: Value = row.try_get("tags")?;
    let playlist_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlists
         (generation_id, concept_id, name, description, kind, machine_label, machine_tags)
         VALUES ($1, $2, $3, $4, 'manual', $5, $6) RETURNING id",
    )
    .bind(generation_id)
    .bind(concept_id)
    .bind(&name)
    .bind(&description)
    .bind(format!("manual-{stable_key}"))
    .bind(&tags)
    .fetch_one(&mut **transaction)
    .await?;
    let artifact_sha256 = hash_parts(&[
        "assignment-replay",
        &generation_id.to_string(),
        &stable_key,
        &name,
        &description,
        &tags.to_string(),
    ]);
    sqlx::query(
        "INSERT INTO playlist_name_revisions
         (playlist_id, name, description, machine_tags, generator_provider,
          generator_model, generator_model_version, artifact_sha256)
         VALUES ($1, $2, $3, $4, 'chordrift', 'stable-concept-replay', '1', $5)",
    )
    .bind(playlist_id)
    .bind(name)
    .bind(description)
    .bind(tags)
    .bind(artifact_sha256)
    .execute(&mut **transaction)
    .await?;
    Ok(playlist_id)
}

async fn refresh_coverage_tx(
    transaction: &mut Transaction<'_, Postgres>,
    account_id: Uuid,
    generation_id: Uuid,
    required: usize,
) -> Result<usize> {
    let represented =
        represented_required_track_count_tx(transaction, account_id, generation_id).await?;
    sqlx::query(
        "UPDATE playlist_generations SET represented_track_count = $2,
         coverage_complete = ($2 = required_track_count) WHERE id = $1",
    )
    .bind(generation_id)
    .bind(as_i32(represented)?)
    .execute(&mut **transaction)
    .await?;
    if represented > required {
        return Err(ChordriftError::Configuration(
            "represented coverage exceeds required coverage".to_owned(),
        ));
    }
    Ok(represented)
}

async fn find_lineage_concept(
    transaction: &mut Transaction<'_, Postgres>,
    cluster_id: Uuid,
    previous_generation: Option<Uuid>,
    used: &HashSet<Uuid>,
) -> Result<Option<Uuid>> {
    let Some(previous_generation) = previous_generation else {
        return Ok(None);
    };
    let rows = sqlx::query(
        "WITH current_size AS (
             SELECT count(*)::double precision AS value FROM cluster_tracks WHERE cluster_id = $1
         ), candidates AS (
             SELECT playlist.concept_id, count(*)::double precision AS intersection,
                    (SELECT count(*)::double precision FROM playlist_tracks size_membership
                     WHERE size_membership.playlist_id = playlist.id) AS previous_size
             FROM cluster_tracks current_membership
             JOIN playlist_tracks previous_membership
               ON previous_membership.track_id = current_membership.track_id
             JOIN playlists playlist ON playlist.id = previous_membership.playlist_id
             WHERE current_membership.cluster_id = $1 AND playlist.generation_id = $2
             GROUP BY playlist.id, playlist.concept_id
         )
         SELECT concept_id, intersection / LEAST(previous_size, current_size.value) AS overlap
         FROM candidates CROSS JOIN current_size
         ORDER BY overlap DESC, concept_id",
    )
    .bind(cluster_id)
    .bind(previous_generation)
    .fetch_all(&mut **transaction)
    .await?;
    for row in rows {
        let concept_id: Uuid = row.try_get("concept_id")?;
        let overlap: f64 = row.try_get("overlap")?;
        if overlap >= LINEAGE_MIN_OVERLAP && !used.contains(&concept_id) {
            return Ok(Some(concept_id));
        }
    }
    Ok(None)
}

async fn required_track_count(database: &Database, account_id: Uuid) -> Result<usize> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint
         FROM tracks track
         WHERE account_track_is_library_candidate($1, track.id)
           AND NOT EXISTS (
               SELECT 1 FROM excluded_tracks exclusion
               WHERE exclusion.provider_account_id = $1
                 AND exclusion.track_id = track.id
                 AND exclusion.restored_at IS NULL
           )",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    as_usize_i64(count)
}

async fn represented_required_track_count_tx(
    transaction: &mut Transaction<'_, Postgres>,
    account_id: Uuid,
    generation_id: Uuid,
) -> Result<usize> {
    let count: i64 = sqlx::query_scalar(
        "WITH required AS (
             SELECT track.id AS track_id
             FROM tracks track
             WHERE account_track_is_library_candidate($1, track.id)
               AND NOT EXISTS (
                   SELECT 1 FROM excluded_tracks exclusion
                   WHERE exclusion.provider_account_id = $1
                     AND exclusion.track_id = track.id
                     AND exclusion.restored_at IS NULL
               ))
         SELECT count(DISTINCT required.track_id)::bigint FROM required
         JOIN playlist_tracks proposed ON proposed.track_id = required.track_id
         JOIN playlists playlist ON playlist.id = proposed.playlist_id
         WHERE playlist.generation_id = $2",
    )
    .bind(account_id)
    .bind(generation_id)
    .fetch_one(&mut **transaction)
    .await?;
    as_usize_i64(count)
}

async fn reused_report(
    database: &Database,
    account_id: Uuid,
    input_hash: &str,
) -> Result<Option<GenerationReport>> {
    let row = sqlx::query(
        "SELECT generation.id, generation.cluster_generation_id,
                generation.required_track_count, generation.represented_track_count,
                generation.coverage_complete,
                count(DISTINCT playlist.id)::bigint AS playlist_count,
                count(DISTINCT membership.track_id)::bigint AS assigned_track_count
         FROM playlist_generations generation
         LEFT JOIN playlists playlist ON playlist.generation_id = generation.id
         LEFT JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
         WHERE generation.provider_account_id = $1 AND generation.input_hash = $2
         GROUP BY generation.id",
    )
    .bind(account_id)
    .bind(input_hash)
    .fetch_optional(database.pool())
    .await?;
    row.map(|row| {
        Ok(GenerationReport {
            generation_id: row.try_get("id")?,
            cluster_generation_id: row.try_get("cluster_generation_id")?,
            reused: true,
            playlist_count: as_usize_i64(row.try_get("playlist_count")?)?,
            assigned_track_count: as_usize_i64(row.try_get("assigned_track_count")?)?,
            required_track_count: as_usize(row.try_get("required_track_count")?)?,
            represented_track_count: as_usize(row.try_get("represented_track_count")?)?,
            coverage_complete: row.try_get("coverage_complete")?,
            input_hash: input_hash.to_owned(),
        })
    })
    .transpose()
}

fn summary_from_row(row: sqlx::postgres::PgRow) -> Result<PlaylistSummary> {
    let tags: Value = row.try_get("machine_tags")?;
    Ok(PlaylistSummary {
        stable_key: row.try_get("stable_key")?,
        name: row.try_get("name")?,
        named: row.try_get("named")?,
        track_count: as_usize_i64(row.try_get("track_count")?)?,
        machine_label: row.try_get("machine_label")?,
        description: row.try_get("description")?,
        tags: serde_json::from_value(tags)?,
    })
}

async fn account_id(database: &Database, account_label: &str) -> Result<Uuid> {
    sqlx::query_scalar(
        "SELECT id FROM provider_accounts WHERE provider = 'spotify' AND account_label = $1",
    )
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(format!(
            "Spotify account {account_label:?} has not been imported"
        ))
    })
}

fn hash_parts(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn as_i32(value: usize) -> Result<i32> {
    i32::try_from(value).map_err(|_| {
        ChordriftError::Configuration("proposal count exceeds PostgreSQL integer".to_owned())
    })
}

fn as_usize(value: i32) -> Result<usize> {
    usize::try_from(value).map_err(|_| {
        ChordriftError::Configuration("database contains a negative proposal count".to_owned())
    })
}

fn as_usize_i64(value: i64) -> Result<usize> {
    usize::try_from(value).map_err(|_| {
        ChordriftError::Configuration("database contains a negative proposal count".to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_reserved_and_duplicate_names() {
        let current = vec![summary("playlist-a"), summary("playlist-b")];
        let reserved = vec![
            result("playlist-a", "Inbox"),
            result("playlist-b", "Night Air"),
        ];
        assert!(validate_results(&current, &reserved).is_err());
        let duplicate = vec![
            result("playlist-a", "Night Air"),
            result("playlist-b", "night air"),
        ];
        assert!(validate_results(&current, &duplicate).is_err());
    }

    #[test]
    fn accepts_complete_strict_results() {
        let current = vec![summary("playlist-a")];
        assert!(validate_results(&current, &[result("playlist-a", "Night Air")]).is_ok());
    }

    #[test]
    fn provider_order_alignment_requires_exact_unique_membership() {
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        assert!(orders_have_equal_unique_membership(
            &[first, second],
            &[second, first]
        ));
        assert!(!orders_have_equal_unique_membership(
            &[first, second],
            &[first]
        ));
        assert!(!orders_have_equal_unique_membership(
            &[first, first],
            &[first, second]
        ));
    }

    fn summary(key: &str) -> PlaylistSummary {
        PlaylistSummary {
            stable_key: key.to_owned(),
            name: "machine".to_owned(),
            named: false,
            track_count: 3,
            machine_label: "vibe-x".to_owned(),
            description: None,
            tags: vec![],
        }
    }

    fn result(key: &str, name: &str) -> NamingResult {
        NamingResult {
            stable_key: key.to_owned(),
            name: name.to_owned(),
            description: "A coherent test vibe.".to_owned(),
            tags: vec!["calm".to_owned(), "evening".to_owned()],
        }
    }
}
