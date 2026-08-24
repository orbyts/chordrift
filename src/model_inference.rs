//! Strict import boundary for independently executed pretrained audio models.

use std::{collections::HashSet, fs, path::Path};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result};

const MAX_MANIFEST_BYTES: u64 = 64 * 1024 * 1024;
const SCHEMA_VERSION: u32 = 1;
const RIGHTS_BASIS: &str = "user_owned_or_authorized_local_audio";

/// Result of importing a model-produced artifact manifest.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportReport {
    /// Durable import identity.
    pub import_id: Uuid,
    /// Whether this exact manifest had already been imported.
    pub reused: bool,
    /// Pretrained model identifier.
    pub model: String,
    /// Pinned model version or revision.
    pub model_version: String,
    /// Number of new track inference records inserted.
    pub tracks_imported: usize,
    /// Number of model facts attached to newly inserted records.
    pub facts_imported: usize,
    /// SHA-256 of the exact manifest bytes.
    pub manifest_sha256: String,
}

/// Current pretrained model coverage for one account.
#[derive(Clone, Debug, PartialEq)]
pub struct StatusReport {
    /// Tracks eligible from the current library or listening history.
    pub eligible_tracks: usize,
    /// Eligible tracks with at least one imported model inference.
    pub inferred_tracks: usize,
    /// Eligible tracks with at least one acoustic embedding.
    pub embedded_tracks: usize,
    /// Imported model-produced semantic facts.
    pub facts: usize,
    /// Distinct pinned model and version pairs.
    pub models: Vec<String>,
    /// Latest successful import time.
    pub latest_import_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    model: String,
    model_version: String,
    model_license: String,
    model_revision: String,
    audio_source: String,
    rights_basis: String,
    sample_rate_hz: u32,
    aggregation: String,
    tracks: Vec<ManifestTrack>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestTrack {
    spotify_id: String,
    input_sha256: String,
    inferred_at: DateTime<Utc>,
    segment_count: u32,
    #[serde(default)]
    embedding: Option<Vec<f64>>,
    #[serde(default)]
    facts: Vec<ManifestFact>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FactKind {
    Genre,
    Mood,
    SoundDescriptor,
}

impl FactKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Genre => "genre",
            Self::Mood => "mood",
            Self::SoundDescriptor => "sound_descriptor",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFact {
    kind: FactKind,
    value: String,
    confidence: f64,
}

/// Imports a deterministic JSON artifact produced from authorized local audio.
pub async fn import(database: &Database, account_label: &str, path: &Path) -> Result<ImportReport> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(ChordriftError::Configuration(
            "model inference manifest must be a regular file".to_owned(),
        ));
    }
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(ChordriftError::Configuration(format!(
            "model inference manifest exceeds {MAX_MANIFEST_BYTES} bytes"
        )));
    }
    let bytes = fs::read(path)?;
    let manifest_sha256 = format!("{:x}", Sha256::digest(&bytes));
    let manifest: Manifest = serde_json::from_slice(&bytes)?;
    validate_manifest(&manifest)?;
    let account_id = account_id(database, account_label).await?;

    if let Some(row) = sqlx::query(
        "SELECT id, model, model_version, tracks_imported
         FROM model_inference_imports
         WHERE provider_account_id = $1 AND manifest_sha256 = $2",
    )
    .bind(account_id)
    .bind(&manifest_sha256)
    .fetch_optional(database.pool())
    .await?
    {
        return Ok(ImportReport {
            import_id: row.try_get("id")?,
            reused: true,
            model: row.try_get("model")?,
            model_version: row.try_get("model_version")?,
            tracks_imported: as_usize(row.try_get("tracks_imported")?)?,
            facts_imported: 0,
            manifest_sha256,
        });
    }

    let mut transaction = database.pool().begin().await?;
    let import_id: Uuid = sqlx::query_scalar(
        "INSERT INTO model_inference_imports
         (provider_account_id, model, model_version, model_license,
          manifest_sha256, schema_version, status, parameters)
         VALUES ($1, $2, $3, $4, $5, $6, 'succeeded', $7)
         RETURNING id",
    )
    .bind(account_id)
    .bind(&manifest.model)
    .bind(&manifest.model_version)
    .bind(&manifest.model_license)
    .bind(&manifest_sha256)
    .bind(i32::try_from(manifest.schema_version).expect("schema version fits i32"))
    .bind(json!({
        "model_revision": manifest.model_revision,
        "audio_source": manifest.audio_source,
        "rights_basis": manifest.rights_basis,
        "sample_rate_hz": manifest.sample_rate_hz,
        "aggregation": manifest.aggregation
    }))
    .fetch_one(&mut *transaction)
    .await?;

    let mut tracks_imported = 0;
    let mut facts_imported = 0;
    for track in &manifest.tracks {
        let track_id: Uuid = sqlx::query_scalar(
            "SELECT provider.track_id
             FROM provider_tracks provider
             WHERE provider.provider = 'spotify' AND provider.provider_track_id = $1
               AND account_track_is_eligible($2, provider.track_id)",
        )
        .bind(&track.spotify_id)
        .bind(account_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| {
            ChordriftError::Configuration(format!(
                "model inference track {} is not in account {account_label:?}",
                track.spotify_id
            ))
        })?;
        let dimensions = track
            .embedding
            .as_ref()
            .map(|embedding| i32::try_from(embedding.len()).expect("validated dimensions fit i32"));
        let inference_id: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO track_model_inferences
             (import_id, track_id, model, model_version, input_sha256,
              embedding, dimensions, metadata, inferred_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (track_id, model, model_version, input_sha256) DO NOTHING
             RETURNING id",
        )
        .bind(import_id)
        .bind(track_id)
        .bind(&manifest.model)
        .bind(&manifest.model_version)
        .bind(&track.input_sha256)
        .bind(&track.embedding)
        .bind(dimensions)
        .bind(json!({
            "segment_count": track.segment_count,
            "sample_rate_hz": manifest.sample_rate_hz,
            "aggregation": manifest.aggregation,
            "model_revision": manifest.model_revision,
            "model_license": manifest.model_license,
            "rights_basis": manifest.rights_basis
        }))
        .bind(track.inferred_at)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(inference_id) = inference_id else {
            continue;
        };
        tracks_imported += 1;
        for fact in &track.facts {
            sqlx::query(
                "INSERT INTO track_model_facts
                 (inference_id, fact_kind, value, normalized_value, confidence, metadata)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(inference_id)
            .bind(fact.kind.as_str())
            .bind(&fact.value)
            .bind(normalize(&fact.value))
            .bind(fact.confidence)
            .bind(json!({ "model_revision": manifest.model_revision }))
            .execute(&mut *transaction)
            .await?;
            facts_imported += 1;
        }
    }
    sqlx::query("UPDATE model_inference_imports SET tracks_imported = $2 WHERE id = $1")
        .bind(import_id)
        .bind(as_i32(tracks_imported)?)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(ImportReport {
        import_id,
        reused: false,
        model: manifest.model,
        model_version: manifest.model_version,
        tracks_imported,
        facts_imported,
        manifest_sha256,
    })
}

/// Reports imported pretrained-model coverage without reading audio or files.
pub async fn status(database: &Database, account_label: &str) -> Result<StatusReport> {
    let account_id = account_id(database, account_label).await?;
    let row = sqlx::query(
        "WITH eligible AS (
             SELECT track.id FROM tracks track
             WHERE account_track_is_eligible($1, track.id)
         )
         SELECT count(*)::bigint AS eligible_tracks,
                count(*) FILTER (WHERE EXISTS (
                    SELECT 1 FROM track_model_inferences inference
                    WHERE inference.track_id = eligible.id
                ))::bigint AS inferred_tracks,
                count(*) FILTER (WHERE EXISTS (
                    SELECT 1 FROM track_model_inferences inference
                    WHERE inference.track_id = eligible.id AND inference.embedding IS NOT NULL
                ))::bigint AS embedded_tracks
         FROM eligible",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let facts: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint
         FROM track_model_facts fact
         JOIN track_model_inferences inference ON inference.id = fact.inference_id
         WHERE account_track_is_eligible($1, inference.track_id)",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let models: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT inference.model || '@' || inference.model_version
         FROM track_model_inferences inference
         WHERE account_track_is_eligible($1, inference.track_id)
         ORDER BY 1",
    )
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    let latest_import_at: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT max(imported_at) FROM model_inference_imports
         WHERE provider_account_id = $1 AND status = 'succeeded'",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    Ok(StatusReport {
        eligible_tracks: as_usize_i64(row.try_get("eligible_tracks")?)?,
        inferred_tracks: as_usize_i64(row.try_get("inferred_tracks")?)?,
        embedded_tracks: as_usize_i64(row.try_get("embedded_tracks")?)?,
        facts: as_usize_i64(facts)?,
        models,
        latest_import_at,
    })
}

fn validate_manifest(manifest: &Manifest) -> Result<()> {
    if manifest.schema_version != SCHEMA_VERSION {
        return Err(ChordriftError::Configuration(format!(
            "unsupported model inference schema version {}; expected {SCHEMA_VERSION}",
            manifest.schema_version
        )));
    }
    for (field, value) in [
        ("model", manifest.model.as_str()),
        ("model_version", manifest.model_version.as_str()),
        ("model_license", manifest.model_license.as_str()),
        ("model_revision", manifest.model_revision.as_str()),
        ("aggregation", manifest.aggregation.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(ChordriftError::Configuration(format!(
                "model inference {field} must not be empty"
            )));
        }
    }
    if manifest.audio_source != "local_audio" || manifest.rights_basis != RIGHTS_BASIS {
        return Err(ChordriftError::Configuration(format!(
            "model inference requires local_audio with rights_basis {RIGHTS_BASIS}"
        )));
    }
    if !(8_000..=192_000).contains(&manifest.sample_rate_hz) {
        return Err(ChordriftError::Configuration(
            "model inference sample_rate_hz must be between 8000 and 192000".to_owned(),
        ));
    }
    if manifest.tracks.is_empty() {
        return Err(ChordriftError::Configuration(
            "model inference manifest contains no tracks".to_owned(),
        ));
    }
    let mut tracks = HashSet::new();
    for track in &manifest.tracks {
        if !valid_spotify_id(&track.spotify_id) || !tracks.insert(track.spotify_id.as_str()) {
            return Err(ChordriftError::Configuration(
                "model inference spotify_id values must be unique 22-character base62 IDs"
                    .to_owned(),
            ));
        }
        if !valid_sha256(&track.input_sha256) {
            return Err(ChordriftError::Configuration(
                "model inference input_sha256 must be 64 lowercase hexadecimal characters"
                    .to_owned(),
            ));
        }
        if track.segment_count == 0 {
            return Err(ChordriftError::Configuration(
                "model inference segment_count must be positive".to_owned(),
            ));
        }
        if let Some(embedding) = &track.embedding
            && (embedding.is_empty()
                || embedding.len() > 4096
                || embedding.iter().any(|value| !value.is_finite()))
        {
            return Err(ChordriftError::Configuration(
                "model inference embeddings must contain 1 to 4096 finite values".to_owned(),
            ));
        }
        if track.embedding.is_none() && track.facts.is_empty() {
            return Err(ChordriftError::Configuration(
                "each model inference track requires an embedding or at least one fact".to_owned(),
            ));
        }
        let mut facts = HashSet::new();
        for fact in &track.facts {
            let normalized = normalize(&fact.value);
            if normalized.is_empty()
                || !fact.confidence.is_finite()
                || !(0.0..=1.0).contains(&fact.confidence)
                || !facts.insert((fact.kind.as_str(), normalized))
            {
                return Err(ChordriftError::Configuration(
                    "model inference facts must be unique, non-empty, and have confidence in 0..=1"
                        .to_owned(),
                ));
            }
        }
    }
    Ok(())
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

fn valid_spotify_id(value: &str) -> bool {
    value.len() == 22
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn as_i32(value: usize) -> Result<i32> {
    i32::try_from(value).map_err(|_| {
        ChordriftError::Configuration("model inference count exceeds PostgreSQL integer".to_owned())
    })
}

fn as_usize(value: i32) -> Result<usize> {
    usize::try_from(value).map_err(|_| {
        ChordriftError::Configuration(
            "database contains a negative model inference count".to_owned(),
        )
    })
}

fn as_usize_i64(value: i64) -> Result<usize> {
    usize::try_from(value).map_err(|_| {
        ChordriftError::Configuration(
            "database contains a negative model inference count".to_owned(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{Manifest, validate_manifest};

    #[test]
    fn accepts_a_strict_authorized_audio_manifest() {
        let manifest: Manifest = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "model": "OpenMuQ/MuQ-MuLan-large",
            "model_version": "revision-abc",
            "model_license": "CC-BY-NC-4.0",
            "model_revision": "abc",
            "audio_source": "local_audio",
            "rights_basis": "user_owned_or_authorized_local_audio",
            "sample_rate_hz": 24000,
            "aggregation": "mean_l2_normalized",
            "tracks": [{
                "spotify_id": "0VjIjW4GlUZAMYd2vXMi3b",
                "input_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "inferred_at": "2026-08-24T20:00:00Z",
                "segment_count": 4,
                "embedding": [0.25, -0.5],
                "facts": [{"kind": "mood", "value": "Melancholic", "confidence": 0.87}]
            }]
        }))
        .expect("fixture parses");
        validate_manifest(&manifest).expect("manifest is valid");
    }

    #[test]
    fn rejects_provider_audio_and_non_finite_embeddings() {
        let manifest: Manifest = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "model": "model",
            "model_version": "v1",
            "model_license": "license",
            "model_revision": "revision",
            "audio_source": "spotify_preview",
            "rights_basis": "user_owned_or_authorized_local_audio",
            "sample_rate_hz": 24000,
            "aggregation": "mean",
            "tracks": [{
                "spotify_id": "0VjIjW4GlUZAMYd2vXMi3b",
                "input_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "inferred_at": "2026-08-24T20:00:00Z",
                "segment_count": 1,
                "embedding": [1.0],
                "facts": []
            }]
        }))
        .expect("fixture parses");
        assert!(validate_manifest(&manifest).is_err());
    }
}
