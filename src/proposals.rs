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
    let mut assigned = HashSet::new();
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
            assigned.insert(track_id);
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
        playlist_count: used_concepts.len(),
        assigned_track_count: assigned.len(),
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
             SELECT id FROM provider_library_snapshots
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
         JOIN provider_playlist_snapshots snapshot
           ON snapshot.provider_playlist_id = provider_playlist.id AND snapshot.snapshot_id = latest.id
         JOIN provider_playlist_tracks membership
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
        "WITH latest AS (
             SELECT id FROM provider_library_snapshots WHERE provider_account_id = $1
             ORDER BY captured_at DESC, id DESC LIMIT 1)
         SELECT count(DISTINCT provider_track.track_id)::bigint
         FROM provider_account_playlists account_playlist
         JOIN provider_playlist_tracks membership
           ON membership.provider_playlist_id = account_playlist.provider_playlist_id
         JOIN latest ON latest.id = membership.snapshot_id
         JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
         WHERE account_playlist.provider_account_id = $1
           AND account_playlist.signal_class IN ('semantic_legacy', 'intake')",
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
        "WITH latest AS (
             SELECT id FROM provider_library_snapshots WHERE provider_account_id = $1
             ORDER BY captured_at DESC, id DESC LIMIT 1), required AS (
             SELECT DISTINCT provider_track.track_id
             FROM provider_account_playlists account_playlist
             JOIN provider_playlist_tracks membership
               ON membership.provider_playlist_id = account_playlist.provider_playlist_id
             JOIN latest ON latest.id = membership.snapshot_id
             JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
             WHERE account_playlist.provider_account_id = $1
               AND account_playlist.signal_class IN ('semantic_legacy', 'intake'))
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
