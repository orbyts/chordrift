//! Revisioned user-authored classification facts and safe CSV review batches.

use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result};

const SCHEMA_VERSION: i32 = 1;

/// Explicit dimensions attached to one track.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ClassificationValues {
    /// Broad private collection, such as `south-asian`.
    pub collection: Option<String>,
    /// Geographic or cultural regions; values are normalized slugs.
    pub regions: Vec<String>,
    /// Musical traditions, such as `film` or `carnatic-classical`.
    pub traditions: Vec<String>,
    /// Personal cross-cutting cohorts, such as `ar-rahman-favorites`.
    pub cohorts: Vec<String>,
    /// BCP-47/ISO-style language tags, plus `instrumental` when appropriate.
    pub languages: Vec<String>,
    /// Optional free-form context that does not become an embedding feature.
    pub notes: Option<String>,
}

/// One active or historical classification revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassificationRevision {
    /// Revision identity.
    pub id: Uuid,
    /// Stable Spotify track identity.
    pub spotify_id: String,
    /// Track title.
    pub title: String,
    /// Display artist names.
    pub artists: String,
    /// Explicit dimensions.
    pub values: ClassificationValues,
    /// `set` or `clear`.
    pub decision: String,
    /// Why the user made this revision.
    pub reason: String,
    /// `cli` or `csv`.
    pub source: String,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// When replaced; `None` means active.
    pub superseded_at: Option<DateTime<Utc>>,
}

/// Result of exporting a review worksheet.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportReport {
    /// Written file.
    pub path: String,
    /// Unique exported tracks.
    pub tracks: usize,
}

/// A staged CSV batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchReport {
    /// Exact approval identity.
    pub batch_id: Uuid,
    /// Draft or approved.
    pub status: String,
    /// Rows that will change active state.
    pub entries: usize,
}

#[derive(Debug, Deserialize, Serialize)]
struct CsvRow {
    schema_version: i32,
    spotify_id: String,
    title: String,
    artists: String,
    album: String,
    inferred_release_country: String,
    inferred_release_language: String,
    action: String,
    user_collection: String,
    user_regions: String,
    user_traditions: String,
    #[serde(default)]
    user_cohorts: String,
    user_languages: String,
    user_notes: String,
    reason: String,
}

/// Immediately activates one explicit user revision.
pub async fn set(
    database: &Database,
    account_label: &str,
    spotify_ids: &[String],
    values: ClassificationValues,
    reason: &str,
) -> Result<Vec<ClassificationRevision>> {
    let account_id = account_id(database, account_label).await?;
    let tracks = track_ids(database, account_id, spotify_ids).await?;
    let values = normalize_values(values)?;
    if values == ClassificationValues::default() {
        return Err(configuration(
            "provide at least one classification dimension",
        ));
    }
    let reason = required(reason, "reason")?;
    let mut tx = database.pool().begin().await?;
    let mut ids = Vec::with_capacity(tracks.len());
    for (_, track_id) in tracks {
        sqlx::query(
            "UPDATE track_classification_revisions SET superseded_at = now()
             WHERE provider_account_id = $1 AND track_id = $2 AND superseded_at IS NULL",
        )
        .bind(account_id)
        .bind(track_id)
        .execute(&mut *tx)
        .await?;
        ids.push(
            sqlx::query_scalar(
                "INSERT INTO track_classification_revisions
             (provider_account_id, track_id, collection, regions, traditions, cohorts, languages,
              notes, reason, source)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'cli') RETURNING id",
            )
            .bind(account_id)
            .bind(track_id)
            .bind(&values.collection)
            .bind(&values.regions)
            .bind(&values.traditions)
            .bind(&values.cohorts)
            .bind(&values.languages)
            .bind(&values.notes)
            .bind(&reason)
            .fetch_one(&mut *tx)
            .await?,
        );
    }
    tx.commit().await?;
    let mut revisions = Vec::with_capacity(ids.len());
    for id in ids {
        revisions.push(revision(database, id).await?);
    }
    Ok(revisions)
}

/// Supersedes the active user classification without deleting its history.
pub async fn clear(
    database: &Database,
    account_label: &str,
    spotify_ids: &[String],
    reason: &str,
) -> Result<usize> {
    let account_id = account_id(database, account_label).await?;
    let tracks = track_ids(database, account_id, spotify_ids).await?;
    let reason = required(reason, "reason")?;
    let mut tx = database.pool().begin().await?;
    let mut changed = 0;
    for (_, track_id) in tracks {
        changed += usize::from(
            sqlx::query(
                "UPDATE track_classification_revisions SET superseded_at = now()
             WHERE provider_account_id = $1 AND track_id = $2 AND superseded_at IS NULL",
            )
            .bind(account_id)
            .bind(track_id)
            .execute(&mut *tx)
            .await?
            .rows_affected()
                != 0,
        );
        sqlx::query(
            "INSERT INTO track_classification_revisions
             (provider_account_id, track_id, reason, source, decision, superseded_at)
             VALUES ($1, $2, $3, 'cli', 'clear', now())",
        )
        .bind(account_id)
        .bind(track_id)
        .bind(&reason)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(changed)
}

/// Returns every revision newest first.
pub async fn history(
    database: &Database,
    account_label: &str,
    spotify_id: &str,
) -> Result<Vec<ClassificationRevision>> {
    let account_id = account_id(database, account_label).await?;
    let track_id = track_id(database, account_id, spotify_id).await?;
    let rows = sqlx::query(
        "SELECT revision.id
         FROM track_classification_revisions revision
         WHERE revision.provider_account_id = $1 AND revision.track_id = $2
         ORDER BY revision.created_at DESC, revision.id DESC",
    )
    .bind(account_id)
    .bind(track_id)
    .fetch_all(database.pool())
    .await?;
    let mut revisions = Vec::with_capacity(rows.len());
    for row in rows {
        revisions.push(revision(database, row.try_get("id")?).await?);
    }
    Ok(revisions)
}

/// Exports deduplicated tracks from one or more current Spotify playlists.
pub async fn export(
    database: &Database,
    account_label: &str,
    playlist_names: &[String],
    path: &Path,
) -> Result<ExportReport> {
    if playlist_names.is_empty() {
        return Err(configuration("provide at least one --playlist"));
    }
    let account_id = account_id(database, account_label).await?;
    let rows = sqlx::query(
        "WITH latest AS (
             SELECT id FROM provider_library_snapshots
             WHERE provider_account_id = $1
             ORDER BY captured_at DESC, id DESC LIMIT 1
         ), selected AS (
             SELECT playlist.provider_playlist_id
             FROM current_spotify_playlists playlist
             WHERE playlist.provider_account_id = $1
               AND lower(playlist.name) = ANY($2)
         ), base AS (
             SELECT provider_track.track_id,
                    min(provider_track.provider_track_id) AS spotify_id,
                    track.title, album.title AS album,
                    COALESCE(string_agg(DISTINCT artist.name, ', '), '') AS artists
             FROM latest
             JOIN provider_playlist_tracks membership ON membership.snapshot_id = latest.id
             JOIN selected ON selected.provider_playlist_id = membership.provider_playlist_id
             JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
             JOIN tracks track ON track.id = provider_track.track_id
             LEFT JOIN albums album ON album.id = track.album_id
             LEFT JOIN track_artists credit ON credit.track_id = track.id
             LEFT JOIN artists artist ON artist.id = credit.artist_id
             GROUP BY provider_track.track_id, track.title, album.title
         )
         SELECT base.*, active.collection, active.regions, active.traditions,
                active.cohorts, active.languages, active.notes,
                COALESCE((SELECT string_agg(DISTINCT fact.value, ';' ORDER BY fact.value)
                          FROM track_semantic_facts fact
                          WHERE fact.track_id = base.track_id
                            AND fact.fact_kind = 'release_country'), '') AS inferred_country,
                COALESCE((SELECT string_agg(DISTINCT fact.value, ';' ORDER BY fact.value)
                          FROM track_semantic_facts fact
                          WHERE fact.track_id = base.track_id
                            AND fact.fact_kind = 'release_language'), '') AS inferred_language
         FROM base
         LEFT JOIN track_classification_revisions active
           ON active.provider_account_id = $1 AND active.track_id = base.track_id
          AND active.superseded_at IS NULL
         ORDER BY lower(base.artists), lower(base.title)",
    )
    .bind(account_id)
    .bind(
        playlist_names
            .iter()
            .map(|name| name.to_lowercase())
            .collect::<Vec<_>>(),
    )
    .fetch_all(database.pool())
    .await?;
    let mut writer = csv::Writer::from_path(path).map_err(csv_error)?;
    for row in &rows {
        writer
            .serialize(CsvRow {
                schema_version: SCHEMA_VERSION,
                spotify_id: row.try_get("spotify_id")?,
                title: row.try_get("title")?,
                artists: row.try_get("artists")?,
                album: row
                    .try_get::<Option<String>, _>("album")?
                    .unwrap_or_default(),
                inferred_release_country: row.try_get("inferred_country")?,
                inferred_release_language: row.try_get("inferred_language")?,
                action: String::new(),
                user_collection: row
                    .try_get::<Option<String>, _>("collection")?
                    .unwrap_or_default(),
                user_regions: row
                    .try_get::<Option<Vec<String>>, _>("regions")?
                    .unwrap_or_default()
                    .join(";"),
                user_traditions: row
                    .try_get::<Option<Vec<String>>, _>("traditions")?
                    .unwrap_or_default()
                    .join(";"),
                user_cohorts: row
                    .try_get::<Option<Vec<String>>, _>("cohorts")?
                    .unwrap_or_default()
                    .join(";"),
                user_languages: row
                    .try_get::<Option<Vec<String>>, _>("languages")?
                    .unwrap_or_default()
                    .join(";"),
                user_notes: row
                    .try_get::<Option<String>, _>("notes")?
                    .unwrap_or_default(),
                reason: String::new(),
            })
            .map_err(csv_error)?;
    }
    writer.flush()?;
    Ok(ExportReport {
        path: path.display().to_string(),
        tracks: rows.len(),
    })
}

/// Imports only CSV rows whose `action` is `set` or `clear` into a draft batch.
pub async fn import(database: &Database, account_label: &str, path: &Path) -> Result<BatchReport> {
    let account_id = account_id(database, account_label).await?;
    let mut reader = csv::Reader::from_path(path).map_err(csv_error)?;
    let mut entries = Vec::new();
    for result in reader.deserialize::<CsvRow>() {
        let row = result.map_err(csv_error)?;
        if row.schema_version != SCHEMA_VERSION {
            return Err(configuration("classification CSV schema_version must be 1"));
        }
        let action = row.action.trim().to_lowercase();
        if action.is_empty() {
            continue;
        }
        if action != "set" && action != "clear" {
            return Err(configuration(format!(
                "invalid action `{}` for Spotify track {}",
                row.action, row.spotify_id
            )));
        }
        let reason = required(&row.reason, "reason")?;
        let values = normalize_values(ClassificationValues {
            collection: optional(&row.user_collection),
            regions: split_values(&row.user_regions),
            traditions: split_values(&row.user_traditions),
            cohorts: split_values(&row.user_cohorts),
            languages: split_values(&row.user_languages),
            notes: optional(&row.user_notes),
        })?;
        if action == "set" && values == ClassificationValues::default() {
            return Err(configuration(format!(
                "set action for {} has no user classification values",
                row.spotify_id
            )));
        }
        entries.push((row.spotify_id.trim().to_owned(), action, values, reason));
    }
    if entries.is_empty() {
        return Err(configuration("CSV has no rows marked set or clear"));
    }
    let ids = entries
        .iter()
        .map(|entry| entry.0.clone())
        .collect::<Vec<_>>();
    let resolved_rows = sqlx::query(
        "SELECT provider_track_id, track_id FROM provider_tracks
         WHERE provider = 'spotify' AND provider_track_id = ANY($1)
           AND account_track_is_library_candidate($2, track_id)",
    )
    .bind(&ids)
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    let resolved = resolved_rows
        .into_iter()
        .map(|row| {
            Ok((
                row.try_get::<String, _>("provider_track_id")?,
                row.try_get::<Uuid, _>("track_id")?,
            ))
        })
        .collect::<Result<HashMap<_, _>>>()?;
    let mut unique = BTreeSet::new();
    let entries = entries
        .into_iter()
        .map(|(spotify_id, action, values, reason)| {
            let track_id = resolved.get(&spotify_id).copied().ok_or_else(|| {
                configuration(format!(
                    "unknown Spotify track `{spotify_id}`; run `chordrift sync pull` first"
                ))
            })?;
            if !unique.insert(track_id) {
                return Err(configuration(
                    "CSV contains a duplicate Spotify track action",
                ));
            }
            Ok((track_id, action, values, reason))
        })
        .collect::<Result<Vec<_>>>()?;
    if entries.len() != unique.len() {
        return Err(configuration(
            "CSV contains a duplicate Spotify track action",
        ));
    }
    let mut tx = database.pool().begin().await?;
    let batch_id: Uuid = sqlx::query_scalar(
        "INSERT INTO track_classification_batches
         (provider_account_id, schema_version, source_path)
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(account_id)
    .bind(SCHEMA_VERSION)
    .bind(path.display().to_string())
    .fetch_one(&mut *tx)
    .await?;
    for (track_id, action, values, reason) in &entries {
        sqlx::query(
            "INSERT INTO track_classification_batch_entries
             (batch_id, track_id, action, collection, regions, traditions, cohorts, languages, notes, reason)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(batch_id)
        .bind(track_id)
        .bind(action)
        .bind(&values.collection)
        .bind(&values.regions)
        .bind(&values.traditions)
        .bind(&values.cohorts)
        .bind(&values.languages)
        .bind(&values.notes)
        .bind(reason)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(BatchReport {
        batch_id,
        status: "draft".to_owned(),
        entries: entries.len(),
    })
}

/// Activates every exact change in one draft batch.
pub async fn approve(
    database: &Database,
    account_label: &str,
    batch_id: Uuid,
    confirm: Uuid,
) -> Result<BatchReport> {
    if batch_id != confirm {
        return Err(configuration("--confirm must exactly match --batch"));
    }
    let account_id = account_id(database, account_label).await?;
    let mut tx = database.pool().begin().await?;
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM track_classification_batches
         WHERE id = $1 AND provider_account_id = $2 FOR UPDATE",
    )
    .bind(batch_id)
    .bind(account_id)
    .fetch_optional(&mut *tx)
    .await?;
    match status.as_deref() {
        None => {
            return Err(configuration(
                "classification batch does not exist for this account",
            ));
        }
        Some("approved") => return Err(configuration("classification batch is already approved")),
        Some("draft") => {}
        Some(_) => unreachable!("database status constraint"),
    }
    let rows = sqlx::query(
        "SELECT track_id, action, collection, regions, traditions, cohorts, languages, notes, reason
         FROM track_classification_batch_entries WHERE batch_id = $1 ORDER BY id",
    )
    .bind(batch_id)
    .fetch_all(&mut *tx)
    .await?;
    for row in &rows {
        let track_id: Uuid = row.try_get("track_id")?;
        sqlx::query(
            "UPDATE track_classification_revisions SET superseded_at = now()
             WHERE provider_account_id = $1 AND track_id = $2 AND superseded_at IS NULL",
        )
        .bind(account_id)
        .bind(track_id)
        .execute(&mut *tx)
        .await?;
        if row.try_get::<String, _>("action")? == "set" {
            sqlx::query(
                "INSERT INTO track_classification_revisions
                 (provider_account_id, track_id, collection, regions, traditions, cohorts, languages,
                  notes, reason, source, source_batch_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'csv', $10)",
            )
            .bind(account_id)
            .bind(track_id)
            .bind(row.try_get::<Option<String>, _>("collection")?)
            .bind(row.try_get::<Vec<String>, _>("regions")?)
            .bind(row.try_get::<Vec<String>, _>("traditions")?)
            .bind(row.try_get::<Vec<String>, _>("cohorts")?)
            .bind(row.try_get::<Vec<String>, _>("languages")?)
            .bind(row.try_get::<Option<String>, _>("notes")?)
            .bind(row.try_get::<String, _>("reason")?)
            .bind(batch_id)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO track_classification_revisions
                 (provider_account_id, track_id, reason, source, source_batch_id,
                  decision, superseded_at)
                 VALUES ($1, $2, $3, 'csv', $4, 'clear', now())",
            )
            .bind(account_id)
            .bind(track_id)
            .bind(row.try_get::<String, _>("reason")?)
            .bind(batch_id)
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query(
        "UPDATE track_classification_batches
         SET status = 'approved', approved_at = now() WHERE id = $1",
    )
    .bind(batch_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(BatchReport {
        batch_id,
        status: "approved".to_owned(),
        entries: rows.len(),
    })
}

async fn revision(database: &Database, id: Uuid) -> Result<ClassificationRevision> {
    let row = sqlx::query(
        "SELECT revision.id, provider.provider_track_id AS spotify_id, track.title,
                COALESCE(string_agg(artist.name, ', ' ORDER BY credit.position), '') AS artists,
                revision.collection, revision.regions, revision.traditions, revision.cohorts,
                revision.languages,
                revision.notes, revision.decision, revision.reason, revision.source,
                revision.created_at, revision.superseded_at
         FROM track_classification_revisions revision
         JOIN tracks track ON track.id = revision.track_id
         JOIN provider_tracks provider ON provider.track_id = track.id AND provider.provider = 'spotify'
         LEFT JOIN track_artists credit ON credit.track_id = track.id
         LEFT JOIN artists artist ON artist.id = credit.artist_id
         WHERE revision.id = $1
         GROUP BY revision.id, provider.provider_track_id, track.title",
    )
    .bind(id)
    .fetch_one(database.pool())
    .await?;
    Ok(ClassificationRevision {
        id: row.try_get("id")?,
        spotify_id: row.try_get("spotify_id")?,
        title: row.try_get("title")?,
        artists: row.try_get("artists")?,
        values: ClassificationValues {
            collection: row.try_get("collection")?,
            regions: row.try_get("regions")?,
            traditions: row.try_get("traditions")?,
            cohorts: row.try_get("cohorts")?,
            languages: row.try_get("languages")?,
            notes: row.try_get("notes")?,
        },
        decision: row.try_get("decision")?,
        reason: row.try_get("reason")?,
        source: row.try_get("source")?,
        created_at: row.try_get("created_at")?,
        superseded_at: row.try_get("superseded_at")?,
    })
}

async fn account_id(database: &Database, label: &str) -> Result<Uuid> {
    sqlx::query_scalar(
        "SELECT id FROM provider_accounts WHERE provider = 'spotify' AND account_label = $1",
    )
    .bind(label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration(format!("account `{label}` has not been imported")))
}

async fn track_id(database: &Database, account_id: Uuid, spotify_id: &str) -> Result<Uuid> {
    sqlx::query_scalar(
        "SELECT track_id FROM provider_tracks
        WHERE provider = 'spotify' AND provider_track_id = $1
          AND account_track_is_library_candidate($2, track_id)",
    )
    .bind(spotify_id.trim())
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        configuration(format!(
            "unknown Spotify track `{}`; run `chordrift sync pull` first",
            spotify_id.trim()
        ))
    })
}

async fn track_ids(
    database: &Database,
    account_id: Uuid,
    spotify_ids: &[String],
) -> Result<Vec<(String, Uuid)>> {
    if spotify_ids.is_empty() {
        return Err(configuration("provide at least one --spotify-id"));
    }
    let normalized = spotify_ids
        .iter()
        .map(|id| id.trim().to_owned())
        .collect::<Vec<_>>();
    if normalized.iter().any(String::is_empty) {
        return Err(configuration("Spotify track ID cannot be empty"));
    }
    let rows = sqlx::query(
        "SELECT provider_track_id, track_id FROM provider_tracks
         WHERE provider = 'spotify' AND provider_track_id = ANY($1)
           AND account_track_is_library_candidate($2, track_id)",
    )
    .bind(&normalized)
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    let resolved = rows
        .into_iter()
        .map(|row| {
            Ok((
                row.try_get::<String, _>("provider_track_id")?,
                row.try_get::<Uuid, _>("track_id")?,
            ))
        })
        .collect::<Result<HashMap<_, _>>>()?;
    let mut unique = BTreeSet::new();
    normalized
        .into_iter()
        .map(|spotify_id| {
            let track_id = resolved.get(&spotify_id).copied().ok_or_else(|| {
                configuration(format!(
                    "unknown Spotify track `{spotify_id}`; run `chordrift sync pull` first"
                ))
            })?;
            if !unique.insert(track_id) {
                return Err(configuration(format!(
                    "duplicate Spotify track `{spotify_id}`"
                )));
            }
            Ok((spotify_id, track_id))
        })
        .collect()
}

fn normalize_values(mut values: ClassificationValues) -> Result<ClassificationValues> {
    values.collection = values.collection.as_deref().map(slug).transpose()?;
    values.regions = normalize_many(values.regions)?;
    values.traditions = normalize_many(values.traditions)?;
    values.cohorts = normalize_many(values.cohorts)?;
    values.languages = normalize_many(values.languages)?;
    values.notes = values.notes.as_deref().and_then(optional);
    Ok(values)
}

fn normalize_many(values: Vec<String>) -> Result<Vec<String>> {
    let mut normalized = BTreeSet::new();
    for value in values {
        normalized.insert(slug(&value)?);
    }
    Ok(normalized.into_iter().collect())
}

fn slug(value: &str) -> Result<String> {
    let value = value.trim().to_lowercase().replace([' ', '_'], "-");
    if value.is_empty() || !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(configuration(format!(
            "invalid classification value `{value}`; use letters, numbers, and hyphens"
        )));
    }
    Ok(value)
}

fn split_values(value: &str) -> Vec<String> {
    value.split(';').filter_map(optional).collect()
}

fn optional(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn required(value: &str, label: &str) -> Result<String> {
    optional(value).ok_or_else(|| configuration(format!("{label} cannot be empty")))
}

fn csv_error(error: csv::Error) -> ChordriftError {
    configuration(format!("classification CSV is invalid: {error}"))
}

fn configuration(message: impl Into<String>) -> ChordriftError {
    ChordriftError::Configuration(message.into())
}

#[cfg(test)]
mod tests {
    use super::{ClassificationValues, CsvRow, normalize_values, split_values};

    #[test]
    fn normalizes_and_deduplicates_dimensions() {
        let values = normalize_values(ClassificationValues {
            collection: Some("South Asian".to_owned()),
            regions: vec!["South Indian".to_owned(), "south-indian".to_owned()],
            traditions: vec!["Film".to_owned()],
            cohorts: vec!["A R Rahman Favorites".to_owned()],
            languages: split_values("ta; instrumental"),
            notes: Some("  personal correction ".to_owned()),
        })
        .expect("valid values");
        assert_eq!(values.collection.as_deref(), Some("south-asian"));
        assert_eq!(values.regions, ["south-indian"]);
        assert_eq!(values.cohorts, ["a-r-rahman-favorites"]);
        assert_eq!(values.languages, ["instrumental", "ta"]);
        assert_eq!(values.notes.as_deref(), Some("personal correction"));
    }

    #[test]
    fn cohort_column_is_backward_compatible_with_existing_csv() {
        let csv = "schema_version,spotify_id,title,artists,album,inferred_release_country,inferred_release_language,action,user_collection,user_regions,user_traditions,user_languages,user_notes,reason\n1,id,title,artist,album,,,set,south-asian,south-indian,film,ta,,reviewed\n";
        let row: CsvRow = csv::Reader::from_reader(csv.as_bytes())
            .deserialize()
            .next()
            .expect("one row")
            .expect("old schema-v1 row remains valid");
        assert!(row.user_cohorts.is_empty());
    }
}
