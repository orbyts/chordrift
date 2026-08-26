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
const MODEL_VERSION: &str = "5";
// This library exposes enough distinct playlist, artist, and album features
// that 128 slots create visible signed-hash collisions during neighbor review.
const DEFAULT_DIMENSIONS: usize = 1024;
const DEFAULT_SEED: i64 = 42;
const PLAYLIST_WEIGHT: f64 = 1.0;
const ARTIST_WEIGHT: f64 = 0.55;
const ALBUM_WEIGHT: f64 = 0.35;
const NAME_TOKEN_WEIGHT: f64 = 0.20;
const SEMANTIC_FACT_WEIGHT: f64 = 0.45;
const USER_CLASSIFICATION_WEIGHT: f64 = 1.25;
const ACOUSTIC_MODEL_WEIGHT: f64 = 1.0;
const LISTENING_SESSION_WEIGHT: f64 = 0.20;

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
    /// Eligible tracks sharing at least one meaningful listening session.
    pub session_related_tracks: usize,
    /// Eligible tracks with external semantic or model-produced facts.
    pub semantic_fact_tracks: usize,
    /// Eligible tracks with an imported pretrained acoustic embedding.
    pub acoustic_embedding_tracks: usize,
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

#[derive(Clone, Debug)]
struct SemanticFeature {
    key: String,
    value: f64,
}

#[derive(Clone, Debug)]
struct ModelVector {
    key: String,
    embedding: Vec<f64>,
}

struct Inputs {
    account_id: Uuid,
    snapshot_id: Uuid,
    tracks: Vec<TrackInput>,
    playlists: Vec<PlaylistInput>,
    artists: BTreeMap<Uuid, Vec<Uuid>>,
    semantic_features: BTreeMap<Uuid, Vec<SemanticFeature>>,
    model_vectors: BTreeMap<Uuid, Vec<ModelVector>>,
    listening_sessions: Vec<Vec<Uuid>>,
    semantic_sources: Vec<String>,
    acoustic_models: Vec<String>,
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
    let session_related_tracks: HashSet<_> = inputs
        .listening_sessions
        .iter()
        .filter(|session| session.len() > 1)
        .flat_map(|session| session.iter().copied())
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
        session_related_tracks: session_related_tracks.len(),
        semantic_fact_tracks: inputs.semantic_features.len(),
        acoustic_embedding_tracks: inputs.model_vectors.len(),
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
        "semantic_fact_weight": SEMANTIC_FACT_WEIGHT,
        "user_classification_weight": USER_CLASSIFICATION_WEIGHT,
        "acoustic_model_weight": ACOUSTIC_MODEL_WEIGHT,
        "acoustic_projection": "signed_feature_hash_after_l2_normalization",
        "semantic_sources": inputs.semantic_sources,
        "acoustic_models": inputs.acoustic_models,
        "normalization": "l2",
        "playlist_size_normalization": "sqrt(weight/(unique_tracks-1))"
    });
    let mut transaction = database.pool().begin().await?;
    let generation_id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO embedding_generations
         (provider_account_id, source_snapshot_id, model, model_version,
          dimensions, seed, input_hash, track_count, parameters)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (provider_account_id, model, model_version, input_hash)
         DO NOTHING
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
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(generation_id) = generation_id else {
        transaction.rollback().await?;
        let generation_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM embedding_generations
             WHERE provider_account_id = $1 AND model = $2 AND model_version = $3
               AND input_hash = $4",
        )
        .bind(inputs.account_id)
        .bind(MODEL)
        .bind(MODEL_VERSION)
        .bind(&input_hash)
        .fetch_one(database.pool())
        .await?;
        return Ok(GenerationReport {
            generation_id,
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
    };
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
        "SELECT id FROM provider_inventory_observations
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
         WHERE account_track_is_library_candidate($1, track.id)
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
        "SELECT DISTINCT provider.id AS playlist_id, provider.provider_playlist_id,
                latest_name.name, account_playlist.signal_class,
                account_playlist.semantic_weight,
                member_track.track_id
         FROM provider_account_playlists account_playlist
         JOIN provider_playlists provider
           ON provider.id = account_playlist.provider_playlist_id
         JOIN LATERAL (
             SELECT historical_name.name
             FROM provider_observed_playlists historical_name
             JOIN provider_inventory_observations historical_library
               ON historical_library.id = historical_name.snapshot_id
             WHERE historical_name.provider_playlist_id = provider.id
               AND historical_library.provider_account_id = $1
             ORDER BY historical_library.captured_at DESC, historical_library.id DESC
             LIMIT 1
         ) latest_name ON TRUE
         JOIN provider_inventory_observations historical_library
           ON historical_library.provider_account_id = $1
         JOIN provider_observed_playlist_tracks membership
           ON membership.provider_playlist_id = provider.id
          AND membership.snapshot_id = historical_library.id
         JOIN provider_tracks member_track ON member_track.id = membership.provider_track_id
         WHERE account_playlist.provider_account_id = $1
           AND account_playlist.signal_class = 'semantic_legacy'
         ORDER BY provider.id, member_track.track_id",
    )
    .bind(account_id)
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
         FROM provider_observed_playlists snapshot
         JOIN provider_inventory_observations library ON library.id = snapshot.snapshot_id
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

    let semantic_rows = sqlx::query(
        "SELECT fact.track_id,
                'semantic:' || fact.source || '@' || fact.parser_version || ':' ||
                    fact.fact_kind || ':' || fact.normalized_value AS feature_key,
                fact.source || '@' || fact.parser_version AS provenance,
                fact.weight, fact.confidence
         FROM track_semantic_facts fact
         WHERE account_track_is_eligible($1, fact.track_id)
         UNION ALL
         SELECT inference.track_id,
                'model:' || inference.model || '@' || inference.model_version || ':' ||
                    fact.fact_kind || ':' || fact.normalized_value AS feature_key,
                inference.model || '@' || inference.model_version AS provenance,
                1::double precision AS weight, fact.confidence
         FROM track_model_facts fact
         JOIN track_model_inferences inference ON inference.id = fact.inference_id
         WHERE account_track_is_eligible($1, inference.track_id)
         UNION ALL
         SELECT revision.track_id,
                'user-classification:v1:' || dimension.kind || ':' || dimension.value AS feature_key,
                'user-classification@v1' AS provenance,
                1::double precision AS weight,
                $2::double precision / $3::double precision AS confidence
         FROM track_classification_revisions revision
         CROSS JOIN LATERAL (
             SELECT 'collection'::text AS kind, revision.collection AS value
             WHERE revision.collection IS NOT NULL
             UNION ALL SELECT 'region', unnest(revision.regions)
             UNION ALL SELECT 'tradition', unnest(revision.traditions)
             UNION ALL SELECT 'language', unnest(revision.languages)
         ) dimension
         WHERE revision.provider_account_id = $1
           AND revision.superseded_at IS NULL
           AND account_track_is_eligible($1, revision.track_id)
         ORDER BY track_id, feature_key",
    )
    .bind(account_id)
    .bind(USER_CLASSIFICATION_WEIGHT)
    .bind(SEMANTIC_FACT_WEIGHT)
    .fetch_all(database.pool())
    .await?;
    let mut semantic_features: BTreeMap<Uuid, Vec<SemanticFeature>> = BTreeMap::new();
    let mut semantic_sources = HashSet::new();
    for row in semantic_rows {
        let track_id: Uuid = row.try_get("track_id")?;
        if eligible_ids.contains(&track_id) {
            semantic_sources.insert(row.try_get::<String, _>("provenance")?);
            let weight: f64 = row.try_get("weight")?;
            let confidence: f64 = row.try_get("confidence")?;
            semantic_features
                .entry(track_id)
                .or_default()
                .push(SemanticFeature {
                    key: row.try_get("feature_key")?,
                    value: SEMANTIC_FACT_WEIGHT * confidence * weight.max(0.0).ln_1p().max(1.0),
                });
        }
    }

    let model_rows = sqlx::query(
        "SELECT DISTINCT ON (inference.track_id, inference.model)
                inference.track_id, inference.model, inference.model_version,
                inference.embedding
         FROM track_model_inferences inference
         WHERE inference.embedding IS NOT NULL
           AND account_track_is_eligible($1, inference.track_id)
         ORDER BY inference.track_id, inference.model,
                  inference.inferred_at DESC, inference.id DESC",
    )
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    let mut model_vectors: BTreeMap<Uuid, Vec<ModelVector>> = BTreeMap::new();
    let mut acoustic_models = HashSet::new();
    for row in model_rows {
        let track_id: Uuid = row.try_get("track_id")?;
        if eligible_ids.contains(&track_id) {
            let model: String = row.try_get("model")?;
            let version: String = row.try_get("model_version")?;
            acoustic_models.insert(format!("{model}@{version}"));
            model_vectors
                .entry(track_id)
                .or_default()
                .push(ModelVector {
                    key: format!("{model}@{version}"),
                    embedding: row.try_get("embedding")?,
                });
        }
    }
    let session_rows = sqlx::query(
        "WITH ordered AS (
             SELECT event.track_id, event.played_at,
                    lag(event.played_at) OVER (
                        ORDER BY event.played_at, event.id
                    ) AS previous_played_at
             FROM listening_evidence_events event
             WHERE event.provider_account_id = $1 AND event.track_id IS NOT NULL
               AND event.superseded_at IS NULL
               AND COALESCE(event.ms_played, 0) >= 30000
         ), boundaries AS (
             SELECT track_id, played_at,
                    CASE WHEN previous_played_at IS NULL
                              OR played_at - previous_played_at > interval '45 minutes'
                         THEN 1 ELSE 0 END AS new_session
             FROM ordered
         ), sessionized AS (
             SELECT track_id,
                    sum(new_session) OVER (ORDER BY played_at, track_id) AS session_id
             FROM boundaries
         )
         SELECT DISTINCT session_id, track_id
         FROM sessionized ORDER BY session_id, track_id",
    )
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    let mut session_map: BTreeMap<i64, Vec<Uuid>> = BTreeMap::new();
    for row in session_rows {
        let track_id: Uuid = row.try_get("track_id")?;
        if eligible_ids.contains(&track_id) {
            session_map
                .entry(row.try_get("session_id")?)
                .or_default()
                .push(track_id);
        }
    }
    Ok(Inputs {
        account_id,
        snapshot_id,
        tracks,
        playlists: playlist_map.into_values().collect(),
        artists,
        semantic_features,
        model_vectors,
        listening_sessions: session_map.into_values().collect(),
        semantic_sources: {
            let mut values: Vec<_> = semantic_sources.into_iter().collect();
            values.sort();
            values
        },
        acoustic_models: {
            let mut values: Vec<_> = acoustic_models.into_iter().collect();
            values.sort();
            values
        },
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

    let mut track_session_counts: HashMap<Uuid, usize> = HashMap::new();
    for session in inputs
        .listening_sessions
        .iter()
        .filter(|session| session.len() > 1)
    {
        for track_id in session {
            *track_session_counts.entry(*track_id).or_default() += 1;
        }
    }
    for (session_index, session) in inputs.listening_sessions.iter().enumerate() {
        if session.len() < 2 {
            continue;
        }
        let (feature_index, sign) = feature_slot(
            &format!("listening-session:{session_index}"),
            hashed_dimensions,
            seed,
        );
        for track_id in session {
            if let Some(vector) = vectors.get_mut(track_id) {
                let appearances = track_session_counts[track_id] as f64;
                vector[feature_index] += sign
                    * (LISTENING_SESSION_WEIGHT / (session.len() as f64 - 1.0) / appearances)
                        .sqrt();
            }
        }
    }

    for (track_id, features) in &inputs.semantic_features {
        if let Some(vector) = vectors.get_mut(track_id) {
            for feature in features {
                let (index, sign) = feature_slot(&feature.key, hashed_dimensions, seed);
                vector[index] += sign * feature.value;
            }
        }
    }
    for (track_id, model_vectors) in &inputs.model_vectors {
        let Some(vector) = vectors.get_mut(track_id) else {
            continue;
        };
        for model_vector in model_vectors {
            let norm = model_vector
                .embedding
                .iter()
                .map(|value| value * value)
                .sum::<f64>()
                .sqrt();
            if norm <= f64::EPSILON {
                continue;
            }
            for (source_index, value) in model_vector.embedding.iter().enumerate() {
                let (index, sign) = feature_slot(
                    &format!("acoustic:{}:{source_index}", model_vector.key),
                    hashed_dimensions,
                    seed,
                );
                vector[index] += sign * ACOUSTIC_MODEL_WEIGHT * value / norm;
            }
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

    use super::{
        Inputs, PlaylistInput, SemanticFeature, TrackInput, build_vectors, semantic_tokens,
    };

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
            semantic_features: BTreeMap::new(),
            model_vectors: BTreeMap::new(),
            listening_sessions: Vec::new(),
            semantic_sources: Vec::new(),
            acoustic_models: Vec::new(),
        };
        let first = build_vectors(&inputs, 32, 42);
        let second = build_vectors(&inputs, 32, 42);
        assert_eq!(first, second);
        for vector in first.values() {
            let norm = vector.iter().map(|value| value * value).sum::<f64>();
            assert!((norm - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn semantic_facts_embed_an_otherwise_unconnected_track() {
        let track_id = Uuid::new_v4();
        let inputs = Inputs {
            account_id: Uuid::new_v4(),
            snapshot_id: Uuid::new_v4(),
            tracks: vec![track(track_id)],
            playlists: Vec::new(),
            artists: BTreeMap::new(),
            semantic_features: BTreeMap::from([(
                track_id,
                vec![SemanticFeature {
                    key: "musicbrainz:tag:ambient".to_owned(),
                    value: 0.8,
                }],
            )]),
            model_vectors: BTreeMap::new(),
            listening_sessions: Vec::new(),
            semantic_sources: vec!["musicbrainz@v1".to_owned()],
            acoustic_models: Vec::new(),
        };
        assert!(build_vectors(&inputs, 32, 42).contains_key(&track_id));
    }

    fn track(id: Uuid) -> TrackInput {
        TrackInput {
            id,
            album_id: None,
            has_history: false,
        }
    }
}
