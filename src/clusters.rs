//! Reproducible account-scoped vibe cluster generations.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result, embeddings};

const ALGORITHM: &str = "spherical-kmeans";
const ALGORITHM_VERSION: &str = "1";
const DEFAULT_SEED: i64 = 42;
const DEFAULT_ITERATIONS: usize = 50;

/// Result of creating or reusing one cluster generation.
#[derive(Clone, Debug, PartialEq)]
pub struct GenerationReport {
    /// Immutable cluster generation identity.
    pub generation_id: Uuid,
    /// Exact embedding generation consumed.
    pub embedding_generation_id: Uuid,
    /// Whether identical inputs already existed.
    pub reused: bool,
    /// Tracks available in the embedding generation.
    pub track_count: usize,
    /// Non-empty clusters persisted.
    pub cluster_count: usize,
    /// Tracks below the minimum membership similarity.
    pub unassigned_count: usize,
    /// Reproducibility hash of input identity and parameters.
    pub input_hash: String,
}

/// Latest cluster generation state.
#[derive(Clone, Debug, PartialEq)]
pub struct GenerationStatus {
    /// Immutable cluster generation identity.
    pub generation_id: Uuid,
    /// Exact embedding generation consumed.
    pub embedding_generation_id: Uuid,
    /// Clustering algorithm.
    pub algorithm: String,
    /// Algorithm implementation version.
    pub algorithm_version: String,
    /// Tracks considered.
    pub track_count: usize,
    /// Persisted non-empty clusters.
    pub cluster_count: usize,
    /// Explicitly unassigned tracks.
    pub unassigned_count: usize,
    /// Reproducibility hash.
    pub input_hash: String,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

/// Inspectable cluster summary.
#[derive(Clone, Debug, PartialEq)]
pub struct ClusterSummary {
    /// Content-derived machine label; generated names arrive later.
    pub machine_label: String,
    /// Assigned track count.
    pub track_count: usize,
    /// Highest-scoring representative title.
    pub representative_title: String,
    /// Representative artists.
    pub representative_artists: String,
    /// Representative Spotify ID.
    pub representative_spotify_id: String,
    /// Representative cosine similarity to the centroid.
    pub representative_score: f64,
}

/// One assigned track for inspection.
#[derive(Clone, Debug, PartialEq)]
pub struct ClusterTrack {
    /// Track title.
    pub title: String,
    /// Display artists.
    pub artists: String,
    /// Spotify track ID.
    pub spotify_id: String,
    /// Cosine similarity to the assigned centroid.
    pub membership_score: f64,
    /// Rank by centroid representativeness.
    pub representative_rank: usize,
}

#[derive(Clone, Debug)]
struct Item {
    id: Uuid,
    embedding: Vec<f64>,
}

#[derive(Clone, Debug)]
struct Assignment {
    cluster: usize,
    score: f64,
}

/// Generates deterministic spherical k-means clusters from the latest embeddings.
pub async fn generate(
    database: &Database,
    account_label: &str,
    count: u32,
    min_similarity: f64,
    seed: Option<i64>,
) -> Result<GenerationReport> {
    if !(2..=100).contains(&count) {
        return Err(ChordriftError::Configuration(
            "cluster count must be between 2 and 100".to_owned(),
        ));
    }
    if !min_similarity.is_finite() || !(-1.0..=1.0).contains(&min_similarity) {
        return Err(ChordriftError::Configuration(
            "minimum cluster similarity must be between -1 and 1".to_owned(),
        ));
    }
    let account_id = account_id(database, account_label).await?;
    let embedding = embeddings::status(database, account_label).await?;
    let rows = sqlx::query(
        "SELECT track_id, embedding FROM account_track_embeddings
         WHERE generation_id = $1 ORDER BY track_id",
    )
    .bind(embedding.generation_id)
    .fetch_all(database.pool())
    .await?;
    let items = rows
        .into_iter()
        .map(|row| {
            Ok(Item {
                id: row.try_get("track_id")?,
                embedding: row.try_get("embedding")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if items.len() < count as usize {
        return Err(ChordriftError::Configuration(format!(
            "cluster count {count} exceeds the {} embedded tracks",
            items.len()
        )));
    }
    let seed = seed.unwrap_or(DEFAULT_SEED);
    let input_hash = generation_hash(
        embedding.generation_id,
        &embedding.input_hash,
        count,
        min_similarity,
        seed,
    );
    if let Some(report) = reused_report(database, account_id, &input_hash).await? {
        return Ok(report);
    }

    let (centroids, assignments) = spherical_kmeans(&items, count as usize, seed);
    let mut members = vec![Vec::<(usize, f64)>::new(); centroids.len()];
    let mut unassigned_count = 0;
    for (item_index, assignment) in assignments.into_iter().enumerate() {
        if assignment.score < min_similarity {
            unassigned_count += 1;
        } else {
            members[assignment.cluster].push((item_index, assignment.score));
        }
    }
    members.retain(|cluster| !cluster.is_empty());
    for cluster in &mut members {
        cluster.sort_by(|left, right| {
            right
                .1
                .total_cmp(&left.1)
                .then_with(|| items[left.0].id.cmp(&items[right.0].id))
        });
    }

    let parameters = json!({
        "requested_clusters": count,
        "iterations": DEFAULT_ITERATIONS,
        "initialization": "deterministic-farthest-first",
        "distance": "cosine",
        "min_similarity": min_similarity,
        "unassigned_policy": "below-minimum-similarity"
    });
    let mut transaction = database.pool().begin().await?;
    let generation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO cluster_generations
         (embedding_model, embedding_version, algorithm, algorithm_version,
          seed, parameters, provider_account_id, embedding_generation_id,
          input_hash, track_count, cluster_count, unassigned_count)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING id",
    )
    .bind(&embedding.model)
    .bind(&embedding.model_version)
    .bind(ALGORITHM)
    .bind(ALGORITHM_VERSION)
    .bind(seed)
    .bind(parameters)
    .bind(account_id)
    .bind(embedding.generation_id)
    .bind(&input_hash)
    .bind(as_i32(items.len())?)
    .bind(as_i32(members.len())?)
    .bind(as_i32(unassigned_count)?)
    .fetch_one(&mut *transaction)
    .await?;
    for cluster in &members {
        let representative = &items[cluster[0].0];
        let machine_label = machine_label(representative.id);
        let cluster_id: Uuid = sqlx::query_scalar(
            "INSERT INTO clusters (generation_id, machine_label)
             VALUES ($1, $2) RETURNING id",
        )
        .bind(generation_id)
        .bind(machine_label)
        .fetch_one(&mut *transaction)
        .await?;
        for (rank, (item_index, score)) in cluster.iter().enumerate() {
            sqlx::query(
                "INSERT INTO cluster_tracks
                 (cluster_id, track_id, membership_score, representative_rank)
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(cluster_id)
            .bind(items[*item_index].id)
            .bind(score)
            .bind(as_i32(rank + 1)?)
            .execute(&mut *transaction)
            .await?;
        }
    }
    transaction.commit().await?;
    Ok(GenerationReport {
        generation_id,
        embedding_generation_id: embedding.generation_id,
        reused: false,
        track_count: items.len(),
        cluster_count: members.len(),
        unassigned_count,
        input_hash,
    })
}

/// Returns the latest account-scoped cluster generation.
pub async fn status(database: &Database, account_label: &str) -> Result<GenerationStatus> {
    let account_id = account_id(database, account_label).await?;
    let row = sqlx::query(
        "SELECT id, embedding_generation_id, algorithm, algorithm_version,
                track_count, cluster_count, unassigned_count, input_hash, created_at
         FROM cluster_generations
         WHERE provider_account_id = $1
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(
            "no cluster generation exists; run `chordrift clusters generate`".to_owned(),
        )
    })?;
    Ok(GenerationStatus {
        generation_id: row.try_get("id")?,
        embedding_generation_id: row.try_get("embedding_generation_id")?,
        algorithm: row.try_get("algorithm")?,
        algorithm_version: row.try_get("algorithm_version")?,
        track_count: as_usize(row.try_get("track_count")?)?,
        cluster_count: as_usize(row.try_get("cluster_count")?)?,
        unassigned_count: as_usize(row.try_get("unassigned_count")?)?,
        input_hash: row.try_get("input_hash")?,
        created_at: row.try_get("created_at")?,
    })
}

/// Lists inspectable summaries for the latest generation.
pub async fn list(database: &Database, account_label: &str) -> Result<Vec<ClusterSummary>> {
    let generation = status(database, account_label).await?;
    let rows = sqlx::query(
        "SELECT cluster.machine_label, count(DISTINCT membership.track_id)::bigint AS track_count,
                max(track.title) FILTER (WHERE membership.representative_rank = 1)
                    AS representative_title,
                max(provider.provider_track_id) FILTER (WHERE membership.representative_rank = 1)
                    AS representative_spotify_id,
                max(membership.membership_score) FILTER (WHERE membership.representative_rank = 1)
                    AS representative_score,
                COALESCE(string_agg(DISTINCT artist.name, ', ')
                    FILTER (WHERE membership.representative_rank = 1), '')
                    AS representative_artists
         FROM clusters cluster
         JOIN cluster_tracks membership ON membership.cluster_id = cluster.id
         JOIN tracks track ON track.id = membership.track_id
         JOIN provider_tracks provider
           ON provider.track_id = track.id AND provider.provider = 'spotify'
         LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
         LEFT JOIN artists artist ON artist.id = track_artist.artist_id
         WHERE cluster.generation_id = $1
         GROUP BY cluster.id, cluster.machine_label
         ORDER BY cluster.machine_label",
    )
    .bind(generation.generation_id)
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(ClusterSummary {
                machine_label: row.try_get("machine_label")?,
                track_count: as_usize_i64(row.try_get("track_count")?)?,
                representative_title: row.try_get("representative_title")?,
                representative_artists: row.try_get("representative_artists")?,
                representative_spotify_id: row.try_get("representative_spotify_id")?,
                representative_score: row.try_get("representative_score")?,
            })
        })
        .collect()
}

/// Lists assigned tracks for one machine label in the latest generation.
pub async fn tracks(
    database: &Database,
    account_label: &str,
    machine_label: &str,
    limit: u32,
) -> Result<Vec<ClusterTrack>> {
    if limit == 0 || limit > 1_000 {
        return Err(ChordriftError::Configuration(
            "cluster track limit must be between 1 and 1000".to_owned(),
        ));
    }
    let generation = status(database, account_label).await?;
    let rows = sqlx::query(
        "SELECT track.title,
                COALESCE(string_agg(DISTINCT artist.name, ', '), '') AS artists,
                min(provider.provider_track_id) AS spotify_id,
                membership.membership_score, membership.representative_rank
         FROM clusters cluster
         JOIN cluster_tracks membership ON membership.cluster_id = cluster.id
         JOIN tracks track ON track.id = membership.track_id
         JOIN provider_tracks provider
           ON provider.track_id = track.id AND provider.provider = 'spotify'
         LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
         LEFT JOIN artists artist ON artist.id = track_artist.artist_id
         WHERE cluster.generation_id = $1 AND cluster.machine_label = $2
         GROUP BY membership.track_id, membership.membership_score,
                  membership.representative_rank, track.title
         ORDER BY membership.representative_rank
         LIMIT $3",
    )
    .bind(generation.generation_id)
    .bind(machine_label)
    .bind(i64::from(limit))
    .fetch_all(database.pool())
    .await?;
    if rows.is_empty() {
        return Err(ChordriftError::Configuration(
            "cluster machine label was not found in the latest generation".to_owned(),
        ));
    }
    rows.into_iter()
        .map(|row| {
            Ok(ClusterTrack {
                title: row.try_get("title")?,
                artists: row.try_get("artists")?,
                spotify_id: row.try_get("spotify_id")?,
                membership_score: row.try_get("membership_score")?,
                representative_rank: as_usize(row.try_get("representative_rank")?)?,
            })
        })
        .collect()
}

fn spherical_kmeans(items: &[Item], count: usize, seed: i64) -> (Vec<Vec<f64>>, Vec<Assignment>) {
    let mut chosen = HashSet::new();
    let first = seed.unsigned_abs() as usize % items.len();
    chosen.insert(first);
    let mut centroids = vec![items[first].embedding.clone()];
    while centroids.len() < count {
        let next = items
            .iter()
            .enumerate()
            .filter(|(index, _)| !chosen.contains(index))
            .max_by(|left, right| {
                nearest_distance(&left.1.embedding, &centroids)
                    .total_cmp(&nearest_distance(&right.1.embedding, &centroids))
                    .then_with(|| right.1.id.cmp(&left.1.id))
            })
            .map(|(index, _)| index)
            .expect("cluster count does not exceed items");
        chosen.insert(next);
        centroids.push(items[next].embedding.clone());
    }
    let mut previous = Vec::new();
    for _ in 0..DEFAULT_ITERATIONS {
        let assignments = assign(items, &centroids);
        let identities: Vec<_> = assignments.iter().map(|value| value.cluster).collect();
        if identities == previous {
            return (centroids, assignments);
        }
        previous = identities;
        let dimensions = centroids[0].len();
        let mut sums = vec![vec![0.0; dimensions]; count];
        let mut sizes = vec![0_usize; count];
        for (item, assignment) in items.iter().zip(&assignments) {
            sizes[assignment.cluster] += 1;
            for (sum, value) in sums[assignment.cluster].iter_mut().zip(&item.embedding) {
                *sum += value;
            }
        }
        for index in 0..count {
            if sizes[index] > 0 {
                normalize(&mut sums[index]);
                centroids[index] = sums[index].clone();
            }
        }
    }
    let assignments = assign(items, &centroids);
    (centroids, assignments)
}

fn assign(items: &[Item], centroids: &[Vec<f64>]) -> Vec<Assignment> {
    items
        .iter()
        .map(|item| {
            let (cluster, score) = centroids
                .iter()
                .enumerate()
                .map(|(index, centroid)| (index, dot(&item.embedding, centroid)))
                .max_by(|left, right| {
                    left.1
                        .total_cmp(&right.1)
                        .then_with(|| right.0.cmp(&left.0))
                })
                .expect("at least one centroid exists");
            Assignment { cluster, score }
        })
        .collect()
}

fn nearest_distance(vector: &[f64], centroids: &[Vec<f64>]) -> f64 {
    1.0 - centroids
        .iter()
        .map(|centroid| dot(vector, centroid))
        .max_by(f64::total_cmp)
        .unwrap_or(0.0)
}

fn normalize(vector: &mut [f64]) {
    let norm = vector.iter().map(|value| value * value).sum::<f64>().sqrt();
    if norm > f64::EPSILON {
        for value in vector {
            *value /= norm;
        }
    }
}

fn dot(left: &[f64], right: &[f64]) -> f64 {
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}

fn machine_label(representative: Uuid) -> String {
    let digest = format!("{:x}", Sha256::digest(representative.as_bytes()));
    format!("vibe-{}", &digest[..12])
}

fn generation_hash(
    embedding_generation_id: Uuid,
    embedding_hash: &str,
    count: u32,
    min_similarity: f64,
    seed: i64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ALGORITHM.as_bytes());
    hasher.update(ALGORITHM_VERSION.as_bytes());
    hasher.update(embedding_generation_id.as_bytes());
    hasher.update(embedding_hash.as_bytes());
    hasher.update(count.to_be_bytes());
    hasher.update(min_similarity.to_bits().to_be_bytes());
    hasher.update(seed.to_be_bytes());
    format!("{:x}", hasher.finalize())
}

async fn reused_report(
    database: &Database,
    account_id: Uuid,
    input_hash: &str,
) -> Result<Option<GenerationReport>> {
    let row = sqlx::query(
        "SELECT id, embedding_generation_id, track_count, cluster_count, unassigned_count
         FROM cluster_generations
         WHERE provider_account_id = $1 AND algorithm = $2
           AND algorithm_version = $3 AND input_hash = $4",
    )
    .bind(account_id)
    .bind(ALGORITHM)
    .bind(ALGORITHM_VERSION)
    .bind(input_hash)
    .fetch_optional(database.pool())
    .await?;
    row.map(|row| {
        Ok(GenerationReport {
            generation_id: row.try_get("id")?,
            embedding_generation_id: row.try_get("embedding_generation_id")?,
            reused: true,
            track_count: as_usize(row.try_get("track_count")?)?,
            cluster_count: as_usize(row.try_get("cluster_count")?)?,
            unassigned_count: as_usize(row.try_get("unassigned_count")?)?,
            input_hash: input_hash.to_owned(),
        })
    })
    .transpose()
}

async fn account_id(database: &Database, account_label: &str) -> Result<Uuid> {
    sqlx::query_scalar(
        "SELECT id FROM provider_accounts
         WHERE provider = 'spotify' AND account_label = $1",
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

fn as_i32(value: usize) -> Result<i32> {
    i32::try_from(value).map_err(|_| {
        ChordriftError::Configuration("cluster count exceeds PostgreSQL integer".to_owned())
    })
}

fn as_usize(value: i32) -> Result<usize> {
    usize::try_from(value).map_err(|_| {
        ChordriftError::Configuration("database contains a negative cluster count".to_owned())
    })
}

fn as_usize_i64(value: i64) -> Result<usize> {
    usize::try_from(value).map_err(|_| {
        ChordriftError::Configuration("database contains a negative cluster count".to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::{Item, machine_label, spherical_kmeans};
    use uuid::Uuid;

    #[test]
    fn clustering_is_deterministic_and_separates_simple_groups() {
        let items = vec![
            item([1.0, 0.0]),
            item([0.9, 0.1]),
            item([0.0, 1.0]),
            item([0.1, 0.9]),
        ];
        let first = spherical_kmeans(&items, 2, 42).1;
        let second = spherical_kmeans(&items, 2, 42).1;
        assert_eq!(
            first.iter().map(|value| value.cluster).collect::<Vec<_>>(),
            second.iter().map(|value| value.cluster).collect::<Vec<_>>()
        );
        assert_eq!(first[0].cluster, first[1].cluster);
        assert_eq!(first[2].cluster, first[3].cluster);
        assert_ne!(first[0].cluster, first[2].cluster);
    }

    #[test]
    fn machine_labels_are_content_derived() {
        let id = Uuid::nil();
        assert_eq!(machine_label(id), machine_label(id));
        assert!(machine_label(id).starts_with("vibe-"));
    }

    fn item(vector: [f64; 2]) -> Item {
        Item {
            id: Uuid::new_v4(),
            embedding: vector.to_vec(),
        }
    }
}
