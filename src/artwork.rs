//! Local-only, content-addressed playlist artwork approval records.

use std::{
    collections::HashMap,
    fs,
    path::{Component, Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result};

/// Strict local artwork manifest.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtworkManifest {
    /// Artifact schema revision; currently 1.
    pub schema_version: u32,
    /// Exact approved playlist proposal represented by this set.
    pub proposal_generation_id: Uuid,
    /// Shared visual-system name.
    pub visual_system: String,
    /// Generator provenance.
    pub generator: ArtworkGenerator,
    /// Contact sheet path relative to the manifest.
    pub contact_sheet: String,
    /// One cover for every canonical playlist.
    pub artifacts: Vec<ArtworkArtifact>,
}

/// Artwork generator provenance.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtworkGenerator {
    /// Generator provider.
    pub provider: String,
    /// Generator model or implementation.
    pub model: String,
    /// Prompt or visual-system revision.
    pub version: String,
}

/// One content-addressed playlist cover.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtworkArtifact {
    /// Stable Chordrift playlist key.
    pub stable_key: String,
    /// Approved playlist name.
    pub name: String,
    /// PNG path relative to the manifest.
    pub file: String,
    /// Expected media type; currently image/png.
    pub media_type: String,
    /// Expected pixel width.
    pub width: u32,
    /// Expected pixel height.
    pub height: u32,
    /// Expected lowercase SHA-256.
    pub sha256: String,
    /// Approved semantic inputs.
    pub tags: Vec<String>,
    /// Exact generation prompt summary.
    pub prompt: String,
}

/// Result of importing or reusing one immutable artwork review set.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportReport {
    /// Artwork batch identity.
    pub batch_id: Uuid,
    /// Proposal represented by the batch.
    pub proposal_generation_id: Uuid,
    /// Current lifecycle state.
    pub state: String,
    /// Number of verified covers.
    pub artifact_count: usize,
    /// Whether an identical batch already existed.
    pub reused: bool,
    /// Deterministic input hash.
    pub input_hash: String,
    /// Stored contact sheet location.
    pub contact_sheet_path: String,
}

/// Inspectable artwork batch status.
#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    /// Artwork batch identity.
    pub batch_id: Uuid,
    /// Proposal represented by the batch.
    pub proposal_generation_id: Uuid,
    /// Current lifecycle state.
    pub state: String,
    /// Shared visual-system name.
    pub visual_system: String,
    /// Generator provenance summary.
    pub generator: String,
    /// Verified cover count.
    pub artifact_count: usize,
    /// Deterministic input hash.
    pub input_hash: String,
    /// Contact sheet path.
    pub contact_sheet_path: String,
    /// Approval time, when approved.
    pub approved_at: Option<DateTime<Utc>>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
}

/// One registered cover summary.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactSummary {
    /// Stable playlist key.
    pub stable_key: String,
    /// Approved playlist name.
    pub name: String,
    /// Local artifact path.
    pub path: String,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Byte length.
    pub byte_size: i64,
    /// Content SHA-256.
    pub sha256: String,
}

/// Validates every local artifact and records an immutable pending review set.
pub async fn import(
    database: &Database,
    account_label: &str,
    manifest_path: &Path,
) -> Result<ImportReport> {
    let manifest_bytes = fs::read(manifest_path)?;
    let manifest: ArtworkManifest = serde_json::from_slice(&manifest_bytes)?;
    validate_manifest_shape(&manifest)?;
    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let manifest_dir = manifest_dir.canonicalize()?;
    let contact_sheet = resolve_local_file(&manifest_dir, &manifest.contact_sheet)?;
    let (contact_width, contact_height) = png_dimensions(&fs::read(&contact_sheet)?)?;
    if contact_width == 0 || contact_height == 0 {
        return Err(configuration(
            "artwork contact sheet has invalid dimensions",
        ));
    }

    let account_id: Uuid =
        sqlx::query_scalar("SELECT id FROM provider_accounts WHERE account_label = $1")
            .bind(account_label)
            .fetch_optional(database.pool())
            .await?
            .ok_or_else(|| {
                configuration(format!("account `{account_label}` has not been imported"))
            })?;

    let proposal_state: Option<String> = sqlx::query_scalar(
        "SELECT status FROM playlist_generations
         WHERE id = $1 AND provider_account_id = $2",
    )
    .bind(manifest.proposal_generation_id)
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?;
    if proposal_state.as_deref() != Some("approved") {
        return Err(configuration(
            "artwork must target an approved proposal for this account",
        ));
    }

    let playlist_rows = sqlx::query(
        "SELECT playlist.id, concept.stable_key, playlist.name, playlist.machine_tags
         FROM playlists playlist
         JOIN playlist_concepts concept ON concept.id = playlist.concept_id
         WHERE playlist.generation_id = $1",
    )
    .bind(manifest.proposal_generation_id)
    .fetch_all(database.pool())
    .await?;
    let mut playlists = HashMap::new();
    for row in playlist_rows {
        let id: Uuid = row.try_get("id")?;
        let stable_key: String = row.try_get("stable_key")?;
        let name: String = row.try_get("name")?;
        let tags: Value = row.try_get("machine_tags")?;
        playlists.insert(stable_key, (id, name, tags));
    }
    if playlists.len() != manifest.artifacts.len() {
        return Err(configuration(format!(
            "artwork manifest has {} covers but proposal requires {}",
            manifest.artifacts.len(),
            playlists.len()
        )));
    }

    let mut verified = Vec::with_capacity(manifest.artifacts.len());
    for artifact in &manifest.artifacts {
        let (playlist_id, approved_name, approved_tags) =
            playlists.remove(&artifact.stable_key).ok_or_else(|| {
                configuration(format!("unknown stable key `{}`", artifact.stable_key))
            })?;
        if artifact.name != approved_name {
            return Err(configuration(format!(
                "artwork name `{}` does not match approved name `{approved_name}`",
                artifact.name
            )));
        }
        let path = resolve_local_file(&manifest_dir, &artifact.file)?;
        let bytes = fs::read(&path)?;
        let content_hash = hex_sha256(&bytes);
        if content_hash != artifact.sha256 {
            return Err(configuration(format!(
                "artwork hash mismatch for `{}`",
                artifact.stable_key
            )));
        }
        let (width, height) = png_dimensions(&bytes)?;
        if artifact.media_type != "image/png"
            || width != artifact.width
            || height != artifact.height
        {
            return Err(configuration(format!(
                "artwork media or dimensions mismatch for `{}`",
                artifact.stable_key
            )));
        }
        verified.push((
            playlist_id,
            artifact,
            approved_tags.clone(),
            path.to_string_lossy().into_owned(),
            bytes.len() as i64,
        ));
    }
    if !playlists.is_empty() {
        return Err(configuration(
            "artwork manifest does not cover every playlist",
        ));
    }

    let canonical_bytes = serde_json::to_vec(&manifest)?;
    let input_hash = hex_sha256(&canonical_bytes);
    if let Some(batch_id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM playlist_artwork_batches
         WHERE provider_account_id = $1 AND proposal_generation_id = $2 AND input_hash = $3",
    )
    .bind(account_id)
    .bind(manifest.proposal_generation_id)
    .bind(&input_hash)
    .fetch_optional(database.pool())
    .await?
    {
        let status = status_for_batch(database, account_id, batch_id).await?;
        return Ok(report_from_status(status, true));
    }

    let mut transaction = database.pool().begin().await?;
    sqlx::query(
        "UPDATE playlist_artwork_batches SET state = 'superseded'
         WHERE provider_account_id = $1 AND state = 'pending'",
    )
    .bind(account_id)
    .execute(&mut *transaction)
    .await?;
    let batch_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlist_artwork_batches
         (provider_account_id, proposal_generation_id, input_hash, visual_system,
          generator_provider, generator_model, generator_version, manifest_path,
          contact_sheet_path, artifact_count)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
    )
    .bind(account_id)
    .bind(manifest.proposal_generation_id)
    .bind(&input_hash)
    .bind(&manifest.visual_system)
    .bind(&manifest.generator.provider)
    .bind(&manifest.generator.model)
    .bind(&manifest.generator.version)
    .bind(manifest_path.to_string_lossy().as_ref())
    .bind(contact_sheet.to_string_lossy().as_ref())
    .bind(verified.len() as i32)
    .fetch_one(&mut *transaction)
    .await?;
    for (playlist_id, artifact, approved_tags, path, byte_size) in verified {
        sqlx::query(
            "INSERT INTO playlist_artwork_artifacts
             (batch_id, playlist_id, stable_key, playlist_name, artifact_path,
              media_type, pixel_width, pixel_height, byte_size, content_sha256,
              prompt, semantic_tags)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(batch_id)
        .bind(playlist_id)
        .bind(&artifact.stable_key)
        .bind(&artifact.name)
        .bind(path)
        .bind(&artifact.media_type)
        .bind(artifact.width as i32)
        .bind(artifact.height as i32)
        .bind(byte_size)
        .bind(&artifact.sha256)
        .bind(&artifact.prompt)
        .bind(approved_tags)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(ImportReport {
        batch_id,
        proposal_generation_id: manifest.proposal_generation_id,
        state: "pending".to_owned(),
        artifact_count: manifest.artifacts.len(),
        reused: false,
        input_hash,
        contact_sheet_path: contact_sheet.to_string_lossy().into_owned(),
    })
}

/// Returns the latest artwork batch for an account.
pub async fn status(database: &Database, account_label: &str) -> Result<Status> {
    let account_id = account_id(database, account_label).await?;
    let batch_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM playlist_artwork_batches
         WHERE provider_account_id = $1 ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration("no artwork batch exists; run `chordrift artwork import`"))?;
    status_for_batch(database, account_id, batch_id).await
}

/// Lists every cover in the latest artwork batch.
pub async fn list(database: &Database, account_label: &str) -> Result<Vec<ArtifactSummary>> {
    let account_id = account_id(database, account_label).await?;
    let rows = sqlx::query(
        "SELECT artifact.stable_key, artifact.playlist_name, artifact.artifact_path,
                artifact.pixel_width, artifact.pixel_height, artifact.byte_size,
                artifact.content_sha256
         FROM playlist_artwork_artifacts artifact
         JOIN playlist_artwork_batches batch ON batch.id = artifact.batch_id
         WHERE batch.id = (SELECT id FROM playlist_artwork_batches
             WHERE provider_account_id = $1 ORDER BY created_at DESC, id DESC LIMIT 1)
         ORDER BY artifact.playlist_name",
    )
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| ArtifactSummary {
            stable_key: row.get("stable_key"),
            name: row.get("playlist_name"),
            path: row.get("artifact_path"),
            width: row.get::<i32, _>("pixel_width") as u32,
            height: row.get::<i32, _>("pixel_height") as u32,
            byte_size: row.get("byte_size"),
            sha256: row.get("content_sha256"),
        })
        .collect())
}

/// Explicitly approves one exact immutable artwork batch without provider writes.
pub async fn approve(database: &Database, account_label: &str, confirm: Uuid) -> Result<Status> {
    let account_id = account_id(database, account_label).await?;
    let status = status_for_batch(database, account_id, confirm).await?;
    if status.state == "superseded" {
        return Err(configuration("superseded artwork cannot be approved"));
    }
    let current_proposal: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM playlist_generations
         WHERE provider_account_id = $1 AND status = 'approved'
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?;
    if current_proposal != Some(status.proposal_generation_id) {
        return Err(configuration(
            "artwork proposal is no longer the current approved proposal",
        ));
    }
    sqlx::query(
        "UPDATE playlist_artwork_batches
         SET state = 'approved', approved_at = COALESCE(approved_at, now())
         WHERE id = $1 AND provider_account_id = $2 AND state IN ('pending', 'approved')",
    )
    .bind(confirm)
    .bind(account_id)
    .execute(database.pool())
    .await?;
    status_for_batch(database, account_id, confirm).await
}

async fn account_id(database: &Database, account_label: &str) -> Result<Uuid> {
    sqlx::query_scalar("SELECT id FROM provider_accounts WHERE account_label = $1")
        .bind(account_label)
        .fetch_optional(database.pool())
        .await?
        .ok_or_else(|| configuration(format!("account `{account_label}` has not been imported")))
}

async fn status_for_batch(database: &Database, account_id: Uuid, batch_id: Uuid) -> Result<Status> {
    let row = sqlx::query(
        "SELECT id, proposal_generation_id, state, visual_system, generator_provider,
                generator_model, generator_version, artifact_count, input_hash,
                contact_sheet_path, approved_at, created_at
         FROM playlist_artwork_batches WHERE id = $1 AND provider_account_id = $2",
    )
    .bind(batch_id)
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration(format!("artwork batch `{batch_id}` was not found")))?;
    Ok(Status {
        batch_id: row.get("id"),
        proposal_generation_id: row.get("proposal_generation_id"),
        state: row.get("state"),
        visual_system: row.get("visual_system"),
        generator: format!(
            "{}/{}@{}",
            row.get::<String, _>("generator_provider"),
            row.get::<String, _>("generator_model"),
            row.get::<String, _>("generator_version")
        ),
        artifact_count: row.get::<i32, _>("artifact_count") as usize,
        input_hash: row.get("input_hash"),
        contact_sheet_path: row.get("contact_sheet_path"),
        approved_at: row.get("approved_at"),
        created_at: row.get("created_at"),
    })
}

fn report_from_status(status: Status, reused: bool) -> ImportReport {
    ImportReport {
        batch_id: status.batch_id,
        proposal_generation_id: status.proposal_generation_id,
        state: status.state,
        artifact_count: status.artifact_count,
        reused,
        input_hash: status.input_hash,
        contact_sheet_path: status.contact_sheet_path,
    }
}

fn validate_manifest_shape(manifest: &ArtworkManifest) -> Result<()> {
    if manifest.schema_version != 1 {
        return Err(configuration(format!(
            "unsupported artwork schema version {}",
            manifest.schema_version
        )));
    }
    if manifest.artifacts.is_empty() {
        return Err(configuration("artwork manifest contains no artifacts"));
    }
    for value in [
        &manifest.visual_system,
        &manifest.generator.provider,
        &manifest.generator.model,
        &manifest.generator.version,
        &manifest.contact_sheet,
    ] {
        if value.trim().is_empty() {
            return Err(configuration(
                "artwork manifest contains a blank required field",
            ));
        }
    }
    Ok(())
}

fn resolve_local_file(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(configuration(format!(
            "artwork path `{relative}` must be a simple relative path"
        )));
    }
    let resolved = root.join(path).canonicalize()?;
    if !resolved.starts_with(root) || !resolved.is_file() {
        return Err(configuration(format!(
            "artwork path `{relative}` is invalid"
        )));
    }
    Ok(resolved)
}

fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return Err(configuration("artwork file is not a valid PNG header"));
    }
    Ok((
        u32::from_be_bytes(bytes[16..20].try_into().expect("four bytes")),
        u32::from_be_bytes(bytes[20..24].try_into().expect("four bytes")),
    ))
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn configuration(message: impl Into<String>) -> ChordriftError {
    ChordriftError::Configuration(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_png_dimensions_from_ihdr() {
        let mut bytes = Vec::from(&b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"[..]);
        bytes.extend_from_slice(&1254_u32.to_be_bytes());
        bytes.extend_from_slice(&1254_u32.to_be_bytes());
        assert_eq!(png_dimensions(&bytes).unwrap(), (1254, 1254));
    }

    #[test]
    fn rejects_parent_paths() {
        let error = resolve_local_file(Path::new("."), "../cover.png").unwrap_err();
        assert!(error.to_string().contains("simple relative path"));
    }
}
