//! Deterministic, account-scoped semantic music embeddings.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result};

const MODEL: &str = "semantic-feature-hash";
const MODEL_VERSION: &str = "2";
const DEFAULT_DIMENSIONS: usize = 128;
const DEFAULT_SEED: i64 = 42;
const PLAYLIST_WEIGHT: f64 = 1.0;
const ARTIST_WEIGHT: f64 = 0.55;
const ALBUM_WEIGHT: f64 = 0.35;
const NAME_TOKEN_WEIGHT: f64 = 0.20;

/// Readiness summary for one account's embedding inputs.
#[derive(Clone, Debug, PartialEq)]
pub struct AuditReport {
    /// Latest immutable provider snapshot.
    pub snapshot_id: Uuid,
    /// Canonical tracks eligible from current library or matched history.
    pub eligible_tracks: usize,
    /// Eligible tracks present in at least one weighted playlist.
    pub playlist_tracks: usize,
    /// Eligible tracks with a shared artist relationship.
    pub artist_related_tracks: usize,
    /// Eligible tracks with a shared album relationship.
    pub album_related_tracks: usize,
    /// Eligible tracks with matched listening statistics.
    pub history_tracks: usize,
    /// Playlist contribution configuration.
    pub playlists: Vec<PlaylistAudit>,
}

/// One playlist's contribution to the embedding feature space.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistAudit {
    /// Stable Spotify playlist ID.
    pub provider_playlist_id: String,
    /// Current playlist name.
    pub name: String,
    /// Unique canonical tracks in the latest snapshot.
    pub unique_tracks: usize,
    /// Configured semantic weight; zero excludes it.
    pub semantic_weight: f64,
}

/// Result of generating or reusing one immutable embedding generation.
#[derive(Clone, Debug, PartialEq)]
pub struct GenerationReport {
    /// Generation identity.
    pub generation_id: Uuid,
    /// Whether an identical input generation already existed.
    pub reused: bool,
    /// Source provider snapshot.
    pub snapshot_id: Uuid,
    /// Stable model name.
    pub model: String,
    /// Stable model implementation version.
    pub model_version: String,
    /// Vector dimensions.
    pub dimensions: usize,
    /// Reproducibility seed.
    pub seed: i64,
    /// Eligible tracks considered.
    pub eligible_tracks: usize,
    /// Tracks with enough signal to embed.
    pub embedded_tracks: usize,
    /// Tracks left intentionally unembedded.
    pub unembedded_tracks: usize,
    /// Content hash of normalized output and parameters.
    pub input_hash: String,
}

/// Latest persisted generation state.
#[derive(Clone, Debug, PartialEq)]
pub struct GenerationStatus {
    /// Generation identity.
    pub generation_id: Uuid,
    /// Source provider snapshot.
    pub snapshot_id: Uuid,
    /// Model name.
    pub model: String,
    /// Model version.
    pub model_version: String,
    /// Vector dimensions.
    pub dimensions: i32,
    /// Seed.
    pub seed: i64,
    /// Embedded track count.
    pub track_count: i32,
    /// Output/input content hash.
    pub input_hash: String,
    /// Generation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Track selector for nearest-neighbor queries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrackSelector {
    /// Exact Spotify track ID.
    ProviderId(String),
    /// Case-insensitive exact title; must be unambiguous.
    Name(String),
}

/// One nearest neighbor in the latest generation.
#[derive(Clone, Debug, PartialEq)]
pub struct Neighbor {
    /// Cosine similarity because stored vectors are unit-normalized.
    pub similarity: f64,
    /// Canonical title.
    pub title: String,
    /// Ordered display artists.
    pub artists: String,
    /// Stable Spotify track ID.
    pub provider_track_id: String,
}

/// A resolved query track and its nearest neighbors.
#[derive(Clone, Debug, PartialEq)]
pub struct NeighborReport {
    /// Query title.
    pub title: String,
    /// Query artists.
    pub artists: String,
    /// Query Spotify ID.
    pub provider_track_id: String,
    /// Generation used.
    pub generation_id: Uuid,
    /// Ranked neighbors.
    pub neighbors: Vec<Neighbor>,
}

#[derive(Clone, Debug)]
struct TrackInput {
    id: Uuid,
    album_id: Option<Uuid>,
    has_history: bool,
}

#[derive(Clone, Debug)]
struct PlaylistInput {
    id: Uuid,
    provider_playlist_id: String,
    name: String,
    weight: f64,
    tracks: Vec<Uuid>,
    historical_names: Vec<String>,
}

struct Inputs {
    account_id: Uuid,
    snapshot_id: Uuid,
    tracks: Vec<TrackInput>,
    playlists: Vec<PlaylistInput>,
    artists: BTreeMap<Uuid, Vec<Uuid>>,
}

/// Audits source coverage without creating an embedding generation.
pub async fn audit(database: &Database, account_label: &str) -> Result<AuditReport> {
    let inputs = load_inputs(database, account_label).await?;
    let playlist_track_ids: HashSet<_> = inputs
        .playlists
        .iter()
        .filter(|playlist| playlist.weight > 0.0 && playlist.tracks.len() > 1)
        .flat_map(|playlist| playlist.tracks.iter().copied())
        .collect();
    let artist_related: HashSet<_> = inputs
        .artists
        .values()
        .filter(|tracks| tracks.len() > 1)
        .flat_map(|tracks| tracks.iter().copied())
        .collect();
    let mut albums: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for track in &inputs.tracks {
        if let Some(album_id) = track.album_id {
            albums.entry(album_id).or_default().push(track.id);
        }
    }
    let album_related: HashSet<_> = albums
        .values()
        .filter(|tracks| tracks.len() > 1)
        .flat_map(|tracks| tracks.iter().copied())
        .collect();
    let playlists = inputs
        .playlists
        .iter()
        .map(|playlist| PlaylistAudit {
            provider_playlist_id: playlist.provider_playlist_id.clone(),
            name: playlist.name.clone(),
            unique_tracks: playlist.tracks.len(),
            semantic_weight: playlist.weight,
        })
        .collect();
    Ok(AuditReport {
        snapshot_id: inputs.snapshot_id,
        eligible_tracks: inputs.tracks.len(),
        playlist_tracks: playlist_track_ids.len(),
        artist_related_tracks: artist_related.len(),
        album_related_tracks: album_related.len(),
        history_tracks: inputs
            .tracks
            .iter()
            .filter(|track| track.has_history)
            .count(),
        playlists,
    })
}

/// Creates a deterministic generation, or reuses the identical persisted one.
pub async fn generate(
    database: &Database,
    account_label: &str,
    dimensions: Option<usize>,
    seed: Option<i64>,
) -> Result<GenerationReport> {
    let dimensions = dimensions.unwrap_or(DEFAULT_DIMENSIONS);
    if !(16..=4096).contains(&dimensions) {
        return Err(ChordriftError::Configuration(
            "embedding dimensions must be between 16 and 4096".to_owned(),
        ));
    }
    let seed = seed.unwrap_or(DEFAULT_SEED);
    let inputs = load_inputs(database, account_label).await?;
    let eligible_tracks = inputs.tracks.len();
    let vectors = build_vectors(&inputs, dimensions, seed);
    let input_hash = vector_hash(&vectors, dimensions, seed);

    if let Some(id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM embedding_generations
         WHERE provider_account_id = $1 AND model = $2 AND model_version = $3
           AND input_hash = $4",
    )
    .bind(inputs.account_id)
    .bind(MODEL)
    .bind(MODEL_VERSION)
    .bind(&input_hash)
    .fetch_optional(database.pool())
    .await?
    {
        return Ok(GenerationReport {
            generation_id: id,
            reused: true,
            snapshot_id: inputs.snapshot_id,
            model: MODEL.to_owned(),
            model_version: MODEL_VERSION.to_owned(),
            dimensions,
            seed,
            eligible_tracks,
            embedded_tracks: vectors.len(),
            unembedded_tracks: eligible_tracks.saturating_sub(vectors.len()),
            input_hash,
        });
    }

    let parameters = json!({
        "playlist_weight": PLAYLIST_WEIGHT,
        "artist_weight": ARTIST_WEIGHT,
        "album_weight": ALBUM_WEIGHT,
        "historical_name_token_weight": NAME_TOKEN_WEIGHT,
        "normalization": "l2",
        "playlist_size_normalization": "sqrt(weight/(unique_tracks-1))"
    });
    let mut transaction = database.pool().begin().await?;
    let generation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO embedding_generations
         (provider_account_id, source_snapshot_id, model, model_version,
          dimensions, seed, input_hash, track_count, parameters)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id",
    )
    .bind(inputs.account_id)
    .bind(inputs.snapshot_id)
    .bind(MODEL)
    .bind(MODEL_VERSION)
    .bind(i32::try_from(dimensions).expect("validated dimensions fit i32"))
    .bind(seed)
    .bind(&input_hash)
    .bind(i32::try_from(vectors.len()).map_err(|_| {
        ChordriftError::Configuration("too many tracks for PostgreSQL counters".to_owned())
    })?)
    .bind(parameters)
    .fetch_one(&mut *transaction)
    .await?;
    for (track_id, embedding) in &vectors {
        sqlx::query(
            "INSERT INTO account_track_embeddings
             (generation_id, track_id, embedding, norm)
             VALUES ($1, $2, $3, 1.0)",
        )
        .bind(generation_id)
        .bind(track_id)
        .bind(embedding)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(GenerationReport {
        generation_id,
        reused: false,
        snapshot_id: inputs.snapshot_id,
        model: MODEL.to_owned(),
        model_version: MODEL_VERSION.to_owned(),
        dimensions,
        seed,
        eligible_tracks,
        embedded_tracks: vectors.len(),
        unembedded_tracks: eligible_tracks.saturating_sub(vectors.len()),
        input_hash,
    })
}

/// Returns the most recent generation for an account.
pub async fn status(database: &Database, account_label: &str) -> Result<GenerationStatus> {
    let account_id = account_id(database, account_label).await?;
    let row = sqlx::query(
        "SELECT id, source_snapshot_id, model, model_version, dimensions, seed,
                track_count, input_hash, created_at
         FROM embedding_generations
         WHERE provider_account_id = $1
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(
            "no embedding generation exists; run `chordrift embeddings generate`".to_owned(),
        )
    })?;
    Ok(GenerationStatus {
        generation_id: row.try_get("id")?,
        snapshot_id: row.try_get("source_snapshot_id")?,
        model: row.try_get("model")?,
        model_version: row.try_get("model_version")?,
        dimensions: row.try_get("dimensions")?,
        seed: row.try_get("seed")?,
        track_count: row.try_get("track_count")?,
        input_hash: row.try_get("input_hash")?,
        created_at: row.try_get("created_at")?,
    })
}

/// Finds nearest tracks in the latest generation using cosine similarity.
pub async fn neighbors(
    database: &Database,
    account_label: &str,
    selector: &TrackSelector,
    limit: u32,
) -> Result<NeighborReport> {
    if limit == 0 || limit > 100 {
        return Err(ChordriftError::Configuration(
            "neighbor limit must be between 1 and 100".to_owned(),
        ));
    }
    let generation = status(database, account_label).await?;
    let rows = sqlx::query(
        "SELECT embedding.track_id, embedding.embedding, track.title,
                COALESCE(string_agg(DISTINCT artist.name, ', '), '') AS artists,
                min(provider.provider_track_id) AS provider_track_id
         FROM account_track_embeddings embedding
         JOIN tracks track ON track.id = embedding.track_id
         JOIN provider_tracks provider
           ON provider.track_id = track.id AND provider.provider = 'spotify'
         LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
         LEFT JOIN artists artist ON artist.id = track_artist.artist_id
         WHERE embedding.generation_id = $1
         GROUP BY embedding.track_id, embedding.embedding, track.title
         ORDER BY track.title, embedding.track_id",
    )
    .bind(generation.generation_id)
    .fetch_all(database.pool())
    .await?;
    let candidates = rows
        .into_iter()
        .map(|row| {
            Ok(Candidate {
                track_id: row.try_get("track_id")?,
                embedding: row.try_get("embedding")?,
                title: row.try_get("title")?,
                artists: row.try_get("artists")?,
                provider_track_id: row.try_get("provider_track_id")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let matches: Vec<_> = candidates
        .iter()
        .filter(|candidate| match selector {
            TrackSelector::ProviderId(id) => candidate.provider_track_id == *id,
            TrackSelector::Name(name) => candidate.title.eq_ignore_ascii_case(name),
        })
        .collect();
    let [query] = matches.as_slice() else {
        return Err(ChordriftError::Configuration(if matches.is_empty() {
            "track selector did not match the latest embedding generation".to_owned()
        } else {
            "track title is ambiguous; select it by Spotify track ID".to_owned()
        }));
    };
    let mut neighbors: Vec<_> = candidates
        .iter()
        .filter(|candidate| candidate.track_id != query.track_id)
        .map(|candidate| Neighbor {
            similarity: dot(&query.embedding, &candidate.embedding),
            title: candidate.title.clone(),
            artists: candidate.artists.clone(),
            provider_track_id: candidate.provider_track_id.clone(),
        })
        .collect();
    neighbors.sort_by(|left, right| {
        right
            .similarity
            .total_cmp(&left.similarity)
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.provider_track_id.cmp(&right.provider_track_id))
    });
    neighbors.truncate(limit as usize);
    Ok(NeighborReport {
        title: query.title.clone(),
        artists: query.artists.clone(),
        provider_track_id: query.provider_track_id.clone(),
        generation_id: generation.generation_id,
        neighbors,
    })
}

#[derive(Clone, Debug)]
struct Candidate {
    track_id: Uuid,
    embedding: Vec<f64>,
    title: String,
    artists: String,
    provider_track_id: String,
}

async fn load_inputs(database: &Database, account_label: &str) -> Result<Inputs> {
    let account_id = account_id(database, account_label).await?;
    let snapshot_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM provider_library_snapshots
         WHERE provider_account_id = $1
         ORDER BY captured_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(
            "Spotify account has no imported provider snapshot".to_owned(),
        )
    })?;
    let rows = sqlx::query(
        "SELECT track.id, track.title, min(provider.provider_track_id) AS provider_track_id,
                track.album_id, listening.event_count IS NOT NULL AS has_history
         FROM tracks track
         JOIN provider_tracks provider
           ON provider.track_id = track.id AND provider.provider = 'spotify'
         LEFT JOIN LATERAL (
             SELECT sum(stats.event_count)::bigint AS event_count
             FROM account_listening_track_statistics stats
             WHERE stats.provider_account_id = $1 AND stats.track_id = track.id
         ) listening ON TRUE
         WHERE EXISTS (
             SELECT 1 FROM provider_playlist_tracks membership
             JOIN provider_tracks member_track ON member_track.id = membership.provider_track_id
             WHERE membership.snapshot_id = $2 AND member_track.track_id = track.id
         ) OR EXISTS (
             SELECT 1 FROM provider_saved_tracks saved
             JOIN provider_tracks saved_track ON saved_track.id = saved.provider_track_id
             WHERE saved.snapshot_id = $2 AND saved_track.track_id = track.id
         ) OR listening.event_count IS NOT NULL
         GROUP BY track.id, track.title, track.album_id, listening.event_count
         ORDER BY track.id",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .fetch_all(database.pool())
    .await?;
    let tracks = rows
        .into_iter()
        .map(|row| {
            Ok(TrackInput {
                id: row.try_get("id")?,
                album_id: row.try_get("album_id")?,
                has_history: row.try_get("has_history")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let eligible_ids: HashSet<_> = tracks.iter().map(|track| track.id).collect();

    let artist_rows =
        sqlx::query("SELECT track_id, artist_id FROM track_artists ORDER BY artist_id, track_id")
            .fetch_all(database.pool())
            .await?;
    let mut artists: BTreeMap<Uuid, Vec<Uuid>> = BTreeMap::new();
    for row in artist_rows {
        let track_id: Uuid = row.try_get("track_id")?;
        if eligible_ids.contains(&track_id) {
            artists
                .entry(row.try_get("artist_id")?)
                .or_default()
                .push(track_id);
        }
    }

    let playlist_rows = sqlx::query(
        "SELECT provider.id AS playlist_id, provider.provider_playlist_id,
                snapshot.name, account_playlist.signal_class,
                account_playlist.semantic_weight,
                member_track.track_id
         FROM provider_account_playlists account_playlist
         JOIN provider_playlists provider
           ON provider.id = account_playlist.provider_playlist_id
         JOIN provider_playlist_snapshots snapshot
           ON snapshot.provider_playlist_id = provider.id AND snapshot.snapshot_id = $2
         JOIN provider_playlist_tracks membership
           ON membership.provider_playlist_id = provider.id AND membership.snapshot_id = $2
         JOIN provider_tracks member_track ON member_track.id = membership.provider_track_id
         WHERE account_playlist.provider_account_id = $1
           AND account_playlist.present_in_latest_snapshot
           AND account_playlist.signal_class = 'semantic_legacy'
         ORDER BY provider.id, member_track.track_id",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .fetch_all(database.pool())
    .await?;
    let mut playlist_map: BTreeMap<Uuid, PlaylistInput> = BTreeMap::new();
    for row in playlist_rows {
        let id: Uuid = row.try_get("playlist_id")?;
        let track_id: Uuid = row.try_get("track_id")?;
        if !eligible_ids.contains(&track_id) {
            continue;
        }
        let playlist = playlist_map.entry(id).or_insert_with(|| PlaylistInput {
            id,
            provider_playlist_id: row
                .try_get("provider_playlist_id")
                .expect("selected Spotify playlist ID has correct type"),
            name: row
                .try_get("name")
                .expect("selected playlist name has correct type"),
            weight: row
                .try_get("semantic_weight")
                .expect("selected semantic weight has correct type"),
            tracks: Vec::new(),
            historical_names: Vec::new(),
        });
        if playlist.tracks.last() != Some(&track_id) {
            playlist.tracks.push(track_id);
        }
    }
    let name_rows = sqlx::query(
        "SELECT DISTINCT snapshot.provider_playlist_id, snapshot.name
         FROM provider_playlist_snapshots snapshot
         JOIN provider_library_snapshots library ON library.id = snapshot.snapshot_id
         WHERE library.provider_account_id = $1
         ORDER BY snapshot.provider_playlist_id, snapshot.name",
    )
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    for row in name_rows {
        let playlist_id: Uuid = row.try_get("provider_playlist_id")?;
        if let Some(playlist) = playlist_map.get_mut(&playlist_id) {
            playlist.historical_names.push(row.try_get("name")?);
        }
    }
    Ok(Inputs {
        account_id,
        snapshot_id,
        tracks,
        playlists: playlist_map.into_values().collect(),
        artists,
    })
}

fn build_vectors(inputs: &Inputs, dimensions: usize, seed: i64) -> BTreeMap<Uuid, Vec<f64>> {
    let mut vectors: BTreeMap<_, _> = inputs
        .tracks
        .iter()
        .map(|track| (track.id, vec![0.0; dimensions]))
        .collect();
    let hashed_dimensions = dimensions;

    for playlist in &inputs.playlists {
        if playlist.weight <= 0.0 || playlist.tracks.len() < 2 {
            continue;
        }
        let size = playlist.tracks.len() as f64;
        let contribution = (PLAYLIST_WEIGHT * playlist.weight / (size - 1.0)).sqrt();
        add_group_feature(
            &mut vectors,
            &playlist.tracks,
            &format!("playlist:{}", playlist.id),
            contribution,
            hashed_dimensions,
            seed,
        );
        let tokens: HashSet<_> = playlist
            .historical_names
            .iter()
            .flat_map(|name| semantic_tokens(name))
            .collect();
        if !tokens.is_empty() {
            let token_contribution =
                (NAME_TOKEN_WEIGHT * playlist.weight / (size - 1.0) / tokens.len() as f64).sqrt();
            for token in tokens {
                add_group_feature(
                    &mut vectors,
                    &playlist.tracks,
                    &format!("playlist-name:{token}"),
                    token_contribution,
                    hashed_dimensions,
                    seed,
                );
            }
        }
    }
    for (artist_id, tracks) in &inputs.artists {
        if tracks.len() > 1 {
            add_group_feature(
                &mut vectors,
                tracks,
                &format!("artist:{artist_id}"),
                (ARTIST_WEIGHT / (tracks.len() as f64 - 1.0)).sqrt(),
                hashed_dimensions,
                seed,
            );
        }
    }
    let mut albums: BTreeMap<Uuid, Vec<Uuid>> = BTreeMap::new();
    for track in &inputs.tracks {
        if let Some(album_id) = track.album_id {
            albums.entry(album_id).or_default().push(track.id);
        }
    }
    for (album_id, tracks) in albums {
        if tracks.len() > 1 {
            add_group_feature(
                &mut vectors,
                &tracks,
                &format!("album:{album_id}"),
                (ALBUM_WEIGHT / (tracks.len() as f64 - 1.0)).sqrt(),
                hashed_dimensions,
                seed,
            );
        }
    }

    vectors.retain(|_, vector| {
        let norm = vector.iter().map(|value| value * value).sum::<f64>().sqrt();
        if norm <= f64::EPSILON {
            return false;
        }
        for value in vector {
            *value /= norm;
        }
        true
    });
    vectors
}

fn add_group_feature(
    vectors: &mut BTreeMap<Uuid, Vec<f64>>,
    tracks: &[Uuid],
    feature: &str,
    value: f64,
    dimensions: usize,
    seed: i64,
) {
    let (index, sign) = feature_slot(feature, dimensions, seed);
    for track_id in tracks {
        if let Some(vector) = vectors.get_mut(track_id) {
            vector[index] += sign * value;
        }
    }
}

fn feature_slot(feature: &str, dimensions: usize, seed: i64) -> (usize, f64) {
    let mut hasher = Sha256::new();
    hasher.update(seed.to_be_bytes());
    hasher.update(feature.as_bytes());
    let digest = hasher.finalize();
    let mut index_bytes = [0_u8; 8];
    index_bytes.copy_from_slice(&digest[..8]);
    let index = (u64::from_be_bytes(index_bytes) % dimensions as u64) as usize;
    let sign = if digest[8] & 1 == 0 { 1.0 } else { -1.0 };
    (index, sign)
}

fn semantic_tokens(name: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "a", "all", "and", "by", "for", "from", "in", "my", "of", "on", "playlist", "the", "to",
        "vibe", "vibes", "with",
    ];
    name.split(|character: char| !character.is_alphanumeric())
        .filter_map(|token| {
            let token = token.to_lowercase();
            (token.chars().count() >= 3 && !STOP.contains(&token.as_str())).then_some(token)
        })
        .collect()
}

fn vector_hash(vectors: &BTreeMap<Uuid, Vec<f64>>, dimensions: usize, seed: i64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(MODEL.as_bytes());
    hasher.update(MODEL_VERSION.as_bytes());
    hasher.update(dimensions.to_be_bytes());
    hasher.update(seed.to_be_bytes());
    for (track_id, vector) in vectors {
        hasher.update(track_id.as_bytes());
        for value in vector {
            hasher.update(value.to_bits().to_be_bytes());
        }
    }
    format!("{:x}", hasher.finalize())
}

fn dot(left: &[f64], right: &[f64]) -> f64 {
    left.iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use uuid::Uuid;

    use super::{Inputs, PlaylistInput, TrackInput, build_vectors, semantic_tokens};

    #[test]
    fn semantic_tokens_ignore_generic_playlist_words() {
        assert_eq!(
            semantic_tokens("My Soft Vibes Playlist"),
            vec!["soft".to_owned()]
        );
    }

    #[test]
    fn equal_inputs_generate_equal_normalized_vectors() {
        let left = Uuid::new_v4();
        let right = Uuid::new_v4();
        let inputs = Inputs {
            account_id: Uuid::new_v4(),
            snapshot_id: Uuid::new_v4(),
            tracks: vec![track(left), track(right)],
            playlists: vec![PlaylistInput {
                id: Uuid::new_v4(),
                provider_playlist_id: "playlist".to_owned(),
                name: "Soft".to_owned(),
                weight: 1.0,
                tracks: vec![left, right],
                historical_names: vec!["Soft Vibes".to_owned()],
            }],
            artists: BTreeMap::new(),
        };
        let first = build_vectors(&inputs, 32, 42);
        let second = build_vectors(&inputs, 32, 42);
        assert_eq!(first, second);
        for vector in first.values() {
            let norm = vector.iter().map(|value| value * value).sum::<f64>();
            assert!((norm - 1.0).abs() < 1e-12);
        }
    }

    fn track(id: Uuid) -> TrackInput {
        TrackInput {
            id,
            album_id: None,
            has_history: false,
        }
    }
}
