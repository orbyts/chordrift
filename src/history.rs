//! Privacy-conscious Spotify archive inspection and listening-history import.

use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, QueryBuilder, Row};
use storexa::Database;
use uuid::Uuid;
use zip::ZipArchive;

use crate::{ChordriftError, Result};

const EXTENDED_AUDIO_PREFIX: &str = "Spotify Extended Streaming History/Streaming_History_Audio_";
const EXTENDED_VIDEO_PREFIX: &str = "Spotify Extended Streaming History/Streaming_History_Video_";
const ACCOUNT_PREFIX: &str = "Spotify Account Data/";

/// Recognized Spotify export families.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveKind {
    /// General account-data export with recent simplified history.
    AccountData,
    /// Comprehensive extended streaming-history export.
    ExtendedStreamingHistory,
}

impl ArchiveKind {
    /// Stable database representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AccountData => "account_data",
            Self::ExtendedStreamingHistory => "extended_streaming_history",
        }
    }
}

/// Secret-free structural summary of a Spotify ZIP archive.
#[derive(Clone, Debug, PartialEq)]
pub struct ArchiveInspection {
    /// Recognized export family.
    pub kind: ArchiveKind,
    /// Basename only; the local directory is not retained.
    pub source_filename: String,
    /// SHA-256 used to make exact archive imports idempotent.
    pub sha256: String,
    /// JSON source files recognized in the archive.
    pub source_files: usize,
    /// Audio streaming records represented by the export.
    pub audio_events: usize,
    /// Music-track records eligible for canonical enrichment.
    pub track_events: usize,
    /// Distinct Spotify track URIs in eligible records.
    pub unique_tracks: usize,
    /// Podcast episode records retained only as an ignored count for now.
    pub episode_events: usize,
    /// Audiobook chapter records retained only as an ignored count for now.
    pub audiobook_events: usize,
    /// Video streaming records retained only as an ignored count for now.
    pub video_events: usize,
    /// First eligible audio timestamp.
    pub first_event_at: Option<DateTime<Utc>>,
    /// Last eligible audio timestamp.
    pub last_event_at: Option<DateTime<Utc>>,
    /// Total audio listening duration.
    pub total_ms_played: i64,
    /// Explicitly skipped music-track records.
    pub skipped_tracks: usize,
    /// Playlist count in an account-data export.
    pub account_playlists: usize,
    /// Ordered playlist entries in an account-data export.
    pub account_playlist_entries: usize,
    /// Saved tracks in an account-data export.
    pub account_library_tracks: usize,
    /// Simplified recent music records deliberately not imported when extended history exists.
    pub simplified_music_events: usize,
}

/// Result of registering or importing one Spotify archive.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportReport {
    /// Safe archive inspection details.
    pub inspection: ArchiveInspection,
    /// Whether this exact account/archive hash had already been imported.
    pub reused_archive: bool,
    /// New listening-event rows inserted.
    pub events_inserted: usize,
    /// Eligible events already represented by an earlier archive.
    pub events_already_present: usize,
    /// Eligible events matched to an existing canonical track.
    pub events_matched: usize,
    /// Eligible events retained unmatched by Spotify track ID.
    pub events_unmatched: usize,
}

/// One successfully imported inbox archive and its collision-safe local destination.
#[derive(Clone, Debug, PartialEq)]
pub struct IngestReport {
    /// Database import outcome.
    pub import: ImportReport,
    /// Archived path beneath the requested local data root.
    pub archived_to: PathBuf,
}

/// Aggregate imported listening-history state for one Spotify account.
#[derive(Clone, Debug, PartialEq)]
pub struct HistorySummary {
    /// Registered Spotify archives.
    pub archives: i64,
    /// Imported music listening events.
    pub events: i64,
    /// Distinct Spotify tracks represented in listening history.
    pub unique_tracks: i64,
    /// Distinct Spotify tracks linked to canonical tracks.
    pub matched_unique_tracks: i64,
    /// Distinct Spotify tracks awaiting canonical linkage.
    pub unmatched_unique_tracks: i64,
    /// Events already linked to canonical tracks.
    pub matched_events: i64,
    /// Events awaiting canonical track linkage.
    pub unmatched_events: i64,
    /// Total listening duration in milliseconds.
    pub total_ms_played: i64,
    /// Explicitly skipped events.
    pub skipped_events: i64,
    /// Earliest imported play.
    pub first_event_at: Option<DateTime<Utc>>,
    /// Latest imported play.
    pub last_event_at: Option<DateTime<Utc>>,
}

/// One track's derived personal listening statistics.
#[derive(Clone, Debug, PartialEq)]
pub struct TopTrack {
    /// Stable Spotify track ID.
    pub provider_track_id: String,
    /// Latest retained Spotify title.
    pub track_name: String,
    /// Latest retained Spotify album-artist display name.
    pub artist_name: String,
    /// Playback events of any duration.
    pub event_count: i64,
    /// Events lasting at least 30 seconds.
    pub play_count: i64,
    /// Total playback duration in milliseconds.
    pub total_ms_played: i64,
    /// Explicit Spotify skips.
    pub skip_count: i64,
    /// Events Spotify reported ending because the track completed.
    pub completed_count: i64,
    /// Whether the Spotify ID is linked to a canonical track.
    pub matched: bool,
    /// Most recent playback.
    pub last_played_at: DateTime<Utc>,
}

#[derive(Debug)]
struct LoadedArchive {
    inspection: ArchiveInspection,
    events: Vec<HistoryEvent>,
}

#[derive(Debug)]
struct HistoryEvent {
    source_file: String,
    source_event_id: String,
    played_at: DateTime<Utc>,
    ms_played: i32,
    skipped: Option<bool>,
    provider_track_id: String,
    source_occurrence: i32,
    metadata: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ExtendedRecord {
    ts: String,
    platform: Option<String>,
    ms_played: i64,
    conn_country: Option<String>,
    master_metadata_track_name: Option<String>,
    master_metadata_album_artist_name: Option<String>,
    master_metadata_album_album_name: Option<String>,
    spotify_track_uri: Option<String>,
    episode_name: Option<String>,
    episode_show_name: Option<String>,
    spotify_episode_uri: Option<String>,
    audiobook_title: Option<String>,
    audiobook_uri: Option<String>,
    audiobook_chapter_uri: Option<String>,
    audiobook_chapter_title: Option<String>,
    reason_start: Option<String>,
    reason_end: Option<String>,
    shuffle: Option<bool>,
    skipped: Option<bool>,
    offline: Option<bool>,
    offline_timestamp: Option<i64>,
    incognito_mode: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct AccountPlaylists {
    #[serde(default)]
    playlists: Vec<AccountPlaylist>,
}

#[derive(Debug, Deserialize)]
struct AccountPlaylist {
    #[serde(default)]
    items: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct AccountLibrary {
    #[serde(default)]
    tracks: Vec<Value>,
}

/// Inspects a supported Spotify archive without writing to Neon.
pub fn inspect(path: &Path) -> Result<ArchiveInspection> {
    Ok(load_archive(path)?.inspection)
}

/// Idempotently imports useful archive state for one account.
pub async fn import(database: &Database, account_label: &str, path: &Path) -> Result<ImportReport> {
    let loaded = load_archive(path)?;
    let account_id = account_id(database, account_label).await?;
    if let Some(report) = existing_report(database, account_id, &loaded.inspection).await? {
        return Ok(report);
    }

    let track_rows = sqlx::query(
        "SELECT provider_track_id, track_id FROM provider_tracks WHERE provider = 'spotify'",
    )
    .fetch_all(database.pool())
    .await?;
    let track_map: HashMap<String, Uuid> = track_rows
        .into_iter()
        .map(|row| Ok((row.try_get("provider_track_id")?, row.try_get("track_id")?)))
        .collect::<Result<_>>()?;
    let events_matched = loaded
        .events
        .iter()
        .filter(|event| track_map.contains_key(&event.provider_track_id))
        .count();
    let events_unmatched = loaded.events.len().saturating_sub(events_matched);

    let mut transaction = database.pool().begin().await?;
    let import_id = Uuid::new_v4();
    let ignored = loaded
        .inspection
        .episode_events
        .saturating_add(loaded.inspection.audiobook_events)
        .saturating_add(loaded.inspection.video_events)
        .saturating_add(loaded.inspection.simplified_music_events);
    sqlx::query(
        "INSERT INTO spotify_archive_imports
         (id, provider_account_id, archive_sha256, archive_kind,
          source_filename, source_files, events_seen, events_matched,
          events_ignored, first_event_at, last_event_at, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(import_id)
    .bind(account_id)
    .bind(&loaded.inspection.sha256)
    .bind(loaded.inspection.kind.as_str())
    .bind(&loaded.inspection.source_filename)
    .bind(to_i32(loaded.inspection.source_files, "source file count")?)
    .bind(to_i64(loaded.inspection.track_events, "event count")?)
    .bind(to_i64(events_matched, "matched event count")?)
    .bind(to_i64(ignored, "ignored event count")?)
    .bind(loaded.inspection.first_event_at)
    .bind(loaded.inspection.last_event_at)
    .bind(inspection_metadata(&loaded.inspection))
    .execute(&mut *transaction)
    .await?;

    let mut events_inserted = 0_usize;
    for chunk in loaded.events.chunks(750) {
        let mut builder = QueryBuilder::<Postgres>::new(
            "INSERT INTO listening_events
             (id, provider_account_id, source_import_id, track_id, provider,
              provider_track_id, source_event_id, played_at, ms_played, skipped,
              source_file, media_type, source_occurrence, raw_metadata) ",
        );
        builder.push_values(chunk, |mut row, event| {
            row.push_bind(Uuid::new_v4())
                .push_bind(account_id)
                .push_bind(import_id)
                .push_bind(track_map.get(&event.provider_track_id).copied())
                .push_bind("spotify")
                .push_bind(&event.provider_track_id)
                .push_bind(&event.source_event_id)
                .push_bind(event.played_at)
                .push_bind(event.ms_played)
                .push_bind(event.skipped)
                .push_bind(&event.source_file)
                .push_bind("track")
                .push_bind(event.source_occurrence)
                .push_bind(&event.metadata);
        });
        builder.push(
            " ON CONFLICT (provider_account_id, provider, provider_track_id,
                           played_at, ms_played, source_occurrence)
              WHERE provider_account_id IS NOT NULL
                AND provider_track_id IS NOT NULL
                AND ms_played IS NOT NULL
              DO NOTHING",
        );
        events_inserted = events_inserted.saturating_add(
            usize::try_from(
                builder
                    .build()
                    .execute(&mut *transaction)
                    .await?
                    .rows_affected(),
            )
            .map_err(|_| {
                ChordriftError::Configuration("inserted event count overflowed".to_owned())
            })?,
        );
    }
    sqlx::query("UPDATE spotify_archive_imports SET events_imported = $2 WHERE id = $1")
        .bind(import_id)
        .bind(to_i64(events_inserted, "inserted event count")?)
        .execute(&mut *transaction)
        .await?;
    if loaded.inspection.kind == ArchiveKind::ExtendedStreamingHistory
        && let Some(covered_through) = loaded.inspection.last_event_at
    {
        sqlx::query(
            "UPDATE listening_events
             SET superseded_at = now()
             WHERE provider_account_id = $1 AND provider = 'spotify'
               AND source_kind = 'recent_api' AND superseded_at IS NULL
               AND played_at <= $2",
        )
        .bind(account_id)
        .bind(covered_through)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    let report = ImportReport {
        events_already_present: loaded.events.len().saturating_sub(events_inserted),
        inspection: loaded.inspection,
        reused_archive: false,
        events_inserted,
        events_matched,
        events_unmatched,
    };
    if report.inspection.kind == ArchiveKind::ExtendedStreamingHistory {
        refresh(database, account_label).await?;
    }
    Ok(report)
}

/// Imports every ZIP in an account inbox and archives it under type, date, and hash folders.
pub async fn ingest(
    database: &Database,
    account_label: &str,
    data_root: &Path,
) -> Result<Vec<IngestReport>> {
    if account_label.is_empty()
        || !account_label
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(ChordriftError::Configuration(
            "account label must contain only letters, numbers, '-' or '_' for local ingestion"
                .to_owned(),
        ));
    }
    let account_root = data_root.join("spotify").join(account_label);
    let inbox = account_root.join("inbox");
    std::fs::create_dir_all(inbox.join("account-data"))?;
    std::fs::create_dir_all(inbox.join("extended-streaming-history"))?;
    let mut archives = Vec::new();
    collect_zip_files(&inbox, &mut archives)?;
    archives.sort();
    if archives.is_empty() {
        return Err(ChordriftError::Configuration(format!(
            "no ZIP archives found; save Spotify exports under {}",
            inbox.display()
        )));
    }

    let date = Local::now().format("%Y-%m-%d").to_string();
    let mut reports = Vec::with_capacity(archives.len());
    for archive in archives {
        let report = import(database, account_label, &archive).await?;
        let kind_folder = match report.inspection.kind {
            ArchiveKind::AccountData => "account-data",
            ArchiveKind::ExtendedStreamingHistory => "extended-streaming-history",
        };
        let destination_directory = account_root
            .join("archive")
            .join(kind_folder)
            .join(&date)
            .join(&report.inspection.sha256);
        std::fs::create_dir_all(&destination_directory)?;
        let destination = destination_directory.join("my_spotify_data.zip");
        if destination.exists() {
            if archive_sha256(&destination)? != report.inspection.sha256 {
                return Err(ChordriftError::Configuration(
                    "archive destination unexpectedly contained different data".to_owned(),
                ));
            }
            std::fs::remove_file(&archive)?;
        } else {
            std::fs::rename(&archive, &destination)?;
        }
        reports.push(IngestReport {
            import: report,
            archived_to: destination,
        });
    }
    Ok(reports)
}

/// Replays every content-addressed local archive into the canonical database.
pub async fn restore(
    database: &Database,
    account_label: &str,
    data_root: &Path,
) -> Result<Vec<ImportReport>> {
    if account_label.is_empty()
        || !account_label
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(ChordriftError::Configuration(
            "account label must contain only letters, numbers, '-' or '_' for local restoration"
                .to_owned(),
        ));
    }
    let archive_root = data_root
        .join("spotify")
        .join(account_label)
        .join("archive");
    let mut archives = Vec::new();
    if archive_root.exists() {
        collect_zip_files(&archive_root, &mut archives)?;
    }
    archives.sort();
    if archives.is_empty() {
        return Err(ChordriftError::Configuration(format!(
            "no recovery ZIP archives found under {}",
            archive_root.display()
        )));
    }
    let mut reports = Vec::with_capacity(archives.len());
    for archive in archives {
        reports.push(import(database, account_label, &archive).await?);
    }
    Ok(reports)
}

/// Summarizes all imported music listening history for one account.
pub async fn summary(database: &Database, account_label: &str) -> Result<HistorySummary> {
    let account_id = account_id(database, account_label).await?;
    let row = sqlx::query(
        "SELECT
           (SELECT count(*) FROM spotify_archive_imports
             WHERE provider_account_id = $1) AS archives,
           count(*) AS events,
           count(DISTINCT provider_track_id) AS unique_tracks,
           count(DISTINCT provider_track_id) FILTER (WHERE track_id IS NOT NULL)
             AS matched_unique_tracks,
           count(DISTINCT provider_track_id) FILTER (WHERE track_id IS NULL)
             AS unmatched_unique_tracks,
           count(*) FILTER (WHERE track_id IS NOT NULL) AS matched_events,
           count(*) FILTER (WHERE track_id IS NULL) AS unmatched_events,
           COALESCE(sum(ms_played), 0)::bigint AS total_ms_played,
           count(*) FILTER (WHERE skipped IS TRUE) AS skipped_events,
           min(played_at) AS first_event_at,
           max(played_at) AS last_event_at
         FROM listening_events
         WHERE provider_account_id = $1 AND provider = 'spotify' AND media_type = 'track'
           AND superseded_at IS NULL",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    Ok(HistorySummary {
        archives: row.try_get("archives")?,
        events: row.try_get("events")?,
        unique_tracks: row.try_get("unique_tracks")?,
        matched_unique_tracks: row.try_get("matched_unique_tracks")?,
        unmatched_unique_tracks: row.try_get("unmatched_unique_tracks")?,
        matched_events: row.try_get("matched_events")?,
        unmatched_events: row.try_get("unmatched_events")?,
        total_ms_played: row.try_get("total_ms_played")?,
        skipped_events: row.try_get("skipped_events")?,
        first_event_at: row.try_get("first_event_at")?,
        last_event_at: row.try_get("last_event_at")?,
    })
}

/// Relinks newly known Spotify IDs and rebuilds account-scoped per-track statistics.
pub async fn refresh(database: &Database, account_label: &str) -> Result<HistorySummary> {
    let account_id = account_id(database, account_label).await?;
    let mut transaction = database.pool().begin().await?;
    sqlx::query(
        "UPDATE listening_events event
         SET track_id = provider_track.track_id
         FROM provider_tracks provider_track
         WHERE event.provider_account_id = $1
           AND event.provider = 'spotify'
           AND event.track_id IS NULL
           AND provider_track.provider = 'spotify'
           AND provider_track.provider_track_id = event.provider_track_id",
    )
    .bind(account_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("DELETE FROM account_listening_track_statistics WHERE provider_account_id = $1")
        .bind(account_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "WITH latest_metadata AS (
             SELECT DISTINCT ON (provider_track_id)
                    provider_track_id,
                    raw_metadata->>'track_name' AS track_name,
                    raw_metadata->>'artist_name' AS artist_name,
                    raw_metadata->>'album_name' AS album_name
             FROM listening_events
             WHERE provider_account_id = $1
               AND provider = 'spotify' AND media_type = 'track'
               AND superseded_at IS NULL
             ORDER BY provider_track_id, played_at DESC, id DESC
         )
         INSERT INTO account_listening_track_statistics
             (provider_account_id, provider_track_id, track_id,
              track_name, artist_name, album_name, event_count, play_count,
              total_ms_played, average_ms_played, skip_count, completed_count,
              first_played_at, last_played_at)
         SELECT $1, event.provider_track_id,
                (array_agg(event.track_id) FILTER (WHERE event.track_id IS NOT NULL))[1],
                metadata.track_name, metadata.artist_name, metadata.album_name,
                count(*), count(*) FILTER (WHERE event.ms_played >= 30000),
                COALESCE(sum(event.ms_played), 0)::bigint,
                COALESCE(avg(event.ms_played), 0)::double precision,
                count(*) FILTER (WHERE event.skipped IS TRUE),
                count(*) FILTER (WHERE event.raw_metadata->>'reason_end' = 'trackdone'),
                min(event.played_at), max(event.played_at)
         FROM listening_events event
         JOIN latest_metadata metadata USING (provider_track_id)
         WHERE event.provider_account_id = $1
           AND event.provider = 'spotify' AND event.media_type = 'track'
           AND event.superseded_at IS NULL
         GROUP BY event.provider_track_id, metadata.track_name,
                  metadata.artist_name, metadata.album_name",
    )
    .bind(account_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    summary(database, account_label).await
}

/// Lists the most-listened tracks by total retained playback duration.
pub async fn top(database: &Database, account_label: &str, limit: u32) -> Result<Vec<TopTrack>> {
    let account_id = account_id(database, account_label).await?;
    let rows = sqlx::query(
        "SELECT provider_track_id,
                COALESCE(track_name, 'Unknown track') AS track_name,
                COALESCE(artist_name, 'Unknown artist') AS artist_name,
                event_count, play_count, total_ms_played, skip_count,
                completed_count, track_id IS NOT NULL AS matched, last_played_at
         FROM account_listening_track_statistics
         WHERE provider_account_id = $1
         ORDER BY total_ms_played DESC, play_count DESC, provider_track_id
         LIMIT $2",
    )
    .bind(account_id)
    .bind(i64::from(limit))
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(TopTrack {
                provider_track_id: row.try_get("provider_track_id")?,
                track_name: row.try_get("track_name")?,
                artist_name: row.try_get("artist_name")?,
                event_count: row.try_get("event_count")?,
                play_count: row.try_get("play_count")?,
                total_ms_played: row.try_get("total_ms_played")?,
                skip_count: row.try_get("skip_count")?,
                completed_count: row.try_get("completed_count")?,
                matched: row.try_get("matched")?,
                last_played_at: row.try_get("last_played_at")?,
            })
        })
        .collect()
}

fn load_archive(path: &Path) -> Result<LoadedArchive> {
    let source_filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| ChordriftError::Configuration("archive filename is invalid".to_owned()))?
        .to_owned();
    let sha256 = archive_sha256(path)?;
    let mut archive = ZipArchive::new(File::open(path)?)?;
    let names = (0..archive.len())
        .map(|index| Ok(archive.by_index(index)?.name().to_owned()))
        .collect::<Result<Vec<_>>>()?;
    let has_extended = names.iter().any(|name| {
        name.starts_with(EXTENDED_AUDIO_PREFIX) || name.starts_with(EXTENDED_VIDEO_PREFIX)
    });
    let has_account = names.iter().any(|name| name.starts_with(ACCOUNT_PREFIX));
    match (has_extended, has_account) {
        (true, false) => load_extended(archive, names, source_filename, sha256),
        (false, true) => load_account(archive, names, source_filename, sha256),
        _ => Err(ChordriftError::Configuration(
            "ZIP is not one unambiguous Spotify account or extended-history export".to_owned(),
        )),
    }
}

fn load_extended(
    mut archive: ZipArchive<File>,
    names: Vec<String>,
    source_filename: String,
    sha256: String,
) -> Result<LoadedArchive> {
    let mut events = Vec::new();
    let mut unique_tracks = HashSet::new();
    let mut fingerprints = HashMap::<String, u32>::new();
    let mut source_files = 0_usize;
    let mut audio_events = 0_usize;
    let mut track_events = 0_usize;
    let mut episode_events = 0_usize;
    let mut audiobook_events = 0_usize;
    let mut video_events = 0_usize;
    let mut first_event_at = None;
    let mut last_event_at = None;
    let mut total_ms_played = 0_i64;
    let mut skipped_tracks = 0_usize;

    for name in names {
        if !name.ends_with(".json") {
            continue;
        }
        if name.starts_with(EXTENDED_VIDEO_PREFIX) {
            source_files = source_files.saturating_add(1);
            let records: Vec<Value> =
                serde_json::from_reader(BufReader::new(archive.by_name(&name)?))?;
            video_events = video_events.saturating_add(records.len());
            continue;
        }
        if !name.starts_with(EXTENDED_AUDIO_PREFIX) {
            continue;
        }
        source_files = source_files.saturating_add(1);
        let records: Vec<ExtendedRecord> =
            serde_json::from_reader(BufReader::new(archive.by_name(&name)?))?;
        for record in records {
            audio_events = audio_events.saturating_add(1);
            total_ms_played = total_ms_played.saturating_add(record.ms_played.max(0));
            let played_at = record.ts.parse::<DateTime<Utc>>().map_err(|_| {
                ChordriftError::Configuration(
                    "extended history contained an invalid timestamp".to_owned(),
                )
            })?;
            first_event_at =
                Some(first_event_at.map_or(played_at, |value: DateTime<Utc>| value.min(played_at)));
            last_event_at =
                Some(last_event_at.map_or(played_at, |value: DateTime<Utc>| value.max(played_at)));
            if record.spotify_episode_uri.is_some() {
                episode_events = episode_events.saturating_add(1);
                continue;
            }
            if record.audiobook_chapter_uri.is_some() {
                audiobook_events = audiobook_events.saturating_add(1);
                continue;
            }
            let Some(provider_track_id) = record
                .spotify_track_uri
                .as_deref()
                .and_then(spotify_track_id)
                .map(str::to_owned)
            else {
                continue;
            };
            track_events = track_events.saturating_add(1);
            unique_tracks.insert(provider_track_id.clone());
            if record.skipped == Some(true) {
                skipped_tracks = skipped_tracks.saturating_add(1);
            }
            let content_hash = record_fingerprint(&record)?;
            let occurrence = fingerprints.entry(content_hash.clone()).or_default();
            let source_occurrence = i32::try_from(*occurrence).map_err(|_| {
                ChordriftError::Configuration(
                    "extended history contained too many identical events".to_owned(),
                )
            })?;
            let source_event_id = format!("spotify-extended-v2:{content_hash}:{occurrence}");
            *occurrence = occurrence.saturating_add(1);
            let ms_played = i32::try_from(record.ms_played.max(0)).map_err(|_| {
                ChordriftError::Configuration(
                    "extended history contained an unsupported play duration".to_owned(),
                )
            })?;
            let metadata = json!({
                "track_name": record.master_metadata_track_name,
                "artist_name": record.master_metadata_album_artist_name,
                "album_name": record.master_metadata_album_album_name,
                "platform": record.platform,
                "connection_country": record.conn_country,
                "reason_start": record.reason_start,
                "reason_end": record.reason_end,
                "shuffle": record.shuffle,
                "offline": record.offline,
                "offline_timestamp": record.offline_timestamp,
                "incognito_mode": record.incognito_mode,
            });
            events.push(HistoryEvent {
                source_file: name.clone(),
                source_event_id,
                played_at,
                ms_played,
                skipped: record.skipped,
                provider_track_id,
                source_occurrence,
                metadata,
            });
        }
    }
    Ok(LoadedArchive {
        inspection: ArchiveInspection {
            kind: ArchiveKind::ExtendedStreamingHistory,
            source_filename,
            sha256,
            source_files,
            audio_events,
            track_events,
            unique_tracks: unique_tracks.len(),
            episode_events,
            audiobook_events,
            video_events,
            first_event_at,
            last_event_at,
            total_ms_played,
            skipped_tracks,
            account_playlists: 0,
            account_playlist_entries: 0,
            account_library_tracks: 0,
            simplified_music_events: 0,
        },
        events,
    })
}

fn load_account(
    mut archive: ZipArchive<File>,
    names: Vec<String>,
    source_filename: String,
    sha256: String,
) -> Result<LoadedArchive> {
    let mut source_files = 0_usize;
    let mut account_playlists = 0_usize;
    let mut account_playlist_entries = 0_usize;
    let mut account_library_tracks = 0_usize;
    let mut simplified_music_events = 0_usize;
    for name in names {
        if !name.starts_with(ACCOUNT_PREFIX) || !name.ends_with(".json") {
            continue;
        }
        source_files = source_files.saturating_add(1);
        if name.contains("/Playlist") {
            let data: AccountPlaylists =
                serde_json::from_reader(BufReader::new(archive.by_name(&name)?))?;
            account_playlists = account_playlists.saturating_add(data.playlists.len());
            account_playlist_entries = account_playlist_entries.saturating_add(
                data.playlists
                    .iter()
                    .map(|playlist| playlist.items.len())
                    .sum::<usize>(),
            );
        } else if name.ends_with("/YourLibrary.json") {
            let data: AccountLibrary =
                serde_json::from_reader(BufReader::new(archive.by_name(&name)?))?;
            account_library_tracks = data.tracks.len();
        } else if name.contains("/StreamingHistory_music_") {
            let records: Vec<Value> =
                serde_json::from_reader(BufReader::new(archive.by_name(&name)?))?;
            simplified_music_events = simplified_music_events.saturating_add(records.len());
        }
    }
    Ok(LoadedArchive {
        inspection: ArchiveInspection {
            kind: ArchiveKind::AccountData,
            source_filename,
            sha256,
            source_files,
            audio_events: 0,
            track_events: 0,
            unique_tracks: 0,
            episode_events: 0,
            audiobook_events: 0,
            video_events: 0,
            first_event_at: None,
            last_event_at: None,
            total_ms_played: 0,
            skipped_tracks: 0,
            account_playlists,
            account_playlist_entries,
            account_library_tracks,
            simplified_music_events,
        },
        events: Vec::new(),
    })
}

fn archive_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_digest(&hasher.finalize()))
}

fn collect_zip_files(directory: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_zip_files(&entry.path(), output)?;
        } else if file_type.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
        {
            output.push(entry.path());
        }
    }
    Ok(())
}

fn record_fingerprint(record: &ExtendedRecord) -> Result<String> {
    Ok(hex_digest(&Sha256::digest(serde_json::to_vec(&(
        &record.ts,
        &record.spotify_track_uri,
        record.ms_played,
    ))?)))
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn spotify_track_id(uri: &str) -> Option<&str> {
    uri.strip_prefix("spotify:track:")
        .filter(|value| !value.is_empty() && !value.contains(':'))
}

fn inspection_metadata(inspection: &ArchiveInspection) -> Value {
    json!({
        "audio_events": inspection.audio_events,
        "track_events": inspection.track_events,
        "unique_tracks": inspection.unique_tracks,
        "episode_events": inspection.episode_events,
        "audiobook_events": inspection.audiobook_events,
        "video_events": inspection.video_events,
        "total_ms_played": inspection.total_ms_played,
        "skipped_tracks": inspection.skipped_tracks,
        "account_playlists": inspection.account_playlists,
        "account_playlist_entries": inspection.account_playlist_entries,
        "account_library_tracks": inspection.account_library_tracks,
        "simplified_music_events_not_imported": inspection.simplified_music_events,
        "privacy": {
            "ip_addresses_stored": false,
            "account_profile_pii_stored": false
        }
    })
}

async fn existing_report(
    database: &Database,
    account_id: Uuid,
    inspection: &ArchiveInspection,
) -> Result<Option<ImportReport>> {
    let row = sqlx::query(
        "SELECT events_imported, events_matched
         FROM spotify_archive_imports
         WHERE provider_account_id = $1 AND archive_sha256 = $2",
    )
    .bind(account_id)
    .bind(&inspection.sha256)
    .fetch_optional(database.pool())
    .await?;
    row.map(|row| {
        let events_matched: i64 = row.try_get("events_matched")?;
        let events_matched = usize::try_from(events_matched).map_err(|_| {
            ChordriftError::Configuration("stored matched event count was invalid".to_owned())
        })?;
        Ok(ImportReport {
            inspection: inspection.clone(),
            reused_archive: true,
            events_inserted: 0,
            events_already_present: inspection.track_events,
            events_matched,
            events_unmatched: inspection.track_events.saturating_sub(events_matched),
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

fn to_i32(value: usize, name: &str) -> Result<i32> {
    i32::try_from(value)
        .map_err(|_| ChordriftError::Configuration(format!("{name} exceeded PostgreSQL limits")))
}

fn to_i64(value: usize, name: &str) -> Result<i64> {
    i64::try_from(value)
        .map_err(|_| ChordriftError::Configuration(format!("{name} exceeded PostgreSQL limits")))
}

#[cfg(test)]
mod tests {
    use super::{ExtendedRecord, record_fingerprint, spotify_track_id};

    #[test]
    fn parses_only_track_uris() {
        assert_eq!(spotify_track_id("spotify:track:abc123"), Some("abc123"));
        assert_eq!(spotify_track_id("spotify:episode:abc123"), None);
        assert_eq!(
            spotify_track_id("https://open.spotify.com/track/abc123"),
            None
        );
    }

    #[test]
    fn fingerprints_are_stable_for_equal_records() {
        let record = ExtendedRecord {
            ts: "2026-08-20T04:33:23Z".to_owned(),
            platform: Some("ios".to_owned()),
            ms_played: 1234,
            conn_country: Some("US".to_owned()),
            master_metadata_track_name: Some("Track".to_owned()),
            master_metadata_album_artist_name: Some("Artist".to_owned()),
            master_metadata_album_album_name: Some("Album".to_owned()),
            spotify_track_uri: Some("spotify:track:abc".to_owned()),
            episode_name: None,
            episode_show_name: None,
            spotify_episode_uri: None,
            audiobook_title: None,
            audiobook_uri: None,
            audiobook_chapter_uri: None,
            audiobook_chapter_title: None,
            reason_start: Some("clickrow".to_owned()),
            reason_end: Some("trackdone".to_owned()),
            shuffle: Some(false),
            skipped: Some(false),
            offline: Some(false),
            offline_timestamp: None,
            incognito_mode: Some(false),
        };
        assert_eq!(
            record_fingerprint(&record).expect("fingerprint"),
            record_fingerprint(&record.clone()).expect("fingerprint")
        );
    }
}
