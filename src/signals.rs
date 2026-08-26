//! Versioned account-specific preference and lifecycle signals.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result};

const MODEL: &str = "account-track-signals";
const MODEL_VERSION: &str = "2";

/// Result of generating or reusing an immutable signal generation.
#[derive(Clone, Debug, PartialEq)]
pub struct GenerationReport {
    /// Generation identity.
    pub generation_id: Uuid,
    /// Whether identical inputs were already persisted.
    pub reused: bool,
    /// Source provider snapshot.
    pub snapshot_id: Uuid,
    /// Tracks represented by this generation.
    pub track_count: usize,
    /// Tracks with listening-history events.
    pub history_tracks: usize,
    /// Tracks currently saved in Spotify.
    pub saved_tracks: usize,
    /// Tracks present in a configured rotation source.
    pub rotation_tracks: usize,
    /// Tracks present in a configured provider or intake discovery source.
    pub discovery_tracks: usize,
    /// Tracks present in a configured prompted-interest source.
    pub prompted_tracks: usize,
    /// Tracks present in a configured intake.
    pub intake_tracks: usize,
    /// Tracks carrying recommendation provenance.
    pub recommendation_tracks: usize,
    /// Stable content hash.
    pub input_hash: String,
}

/// Latest persisted signal state.
#[derive(Clone, Debug, PartialEq)]
pub struct GenerationStatus {
    /// Generation identity.
    pub generation_id: Uuid,
    /// Source provider snapshot.
    pub snapshot_id: Uuid,
    /// Signal model name.
    pub model: String,
    /// Signal implementation version.
    pub model_version: String,
    /// Track count.
    pub track_count: i32,
    /// Stable content hash.
    pub input_hash: String,
    /// Generation timestamp.
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq)]
struct TrackSignal {
    track_id: Uuid,
    meaningful_play_count: i64,
    event_count: i64,
    last_played_at: Option<DateTime<Utc>>,
    recency_score: Option<f64>,
    completion_ratio: Option<f64>,
    non_skip_ratio: Option<f64>,
    saved: bool,
    provider_rotation: bool,
    provider_discovery: bool,
    prompted_interest: bool,
    intake: bool,
    recommendation: bool,
}

/// Generates independently versioned behavioral evidence for one account.
pub async fn generate(database: &Database, account_label: &str) -> Result<GenerationReport> {
    let (account_id, snapshot_id, mut signals) = load_inputs(database, account_label).await?;
    apply_derived_values(&mut signals);
    let input_hash = signal_hash(&signals, snapshot_id);
    if let Some(generation_id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM signal_generations
         WHERE provider_account_id = $1 AND model = $2 AND model_version = $3
           AND input_hash = $4",
    )
    .bind(account_id)
    .bind(MODEL)
    .bind(MODEL_VERSION)
    .bind(&input_hash)
    .fetch_optional(database.pool())
    .await?
    {
        return Ok(report(
            generation_id,
            true,
            snapshot_id,
            &signals,
            input_hash,
        ));
    }

    let mut transaction = database.pool().begin().await?;
    let generation_id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO signal_generations
         (provider_account_id, source_snapshot_id, model, model_version,
          input_hash, track_count, parameters)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (provider_account_id, model, model_version, input_hash)
         DO NOTHING
         RETURNING id",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .bind(MODEL)
    .bind(MODEL_VERSION)
    .bind(&input_hash)
    .bind(i32::try_from(signals.len()).map_err(|_| {
        ChordriftError::Configuration("too many tracks for PostgreSQL counters".to_owned())
    })?)
    .bind(json!({
        "recency": "exp(-days/365)",
        "ratios": "completed/event and 1-skipped/event",
        "playlist_evidence": "latest configured provider snapshot"
    }))
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(generation_id) = generation_id else {
        transaction.rollback().await?;
        let generation_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM signal_generations
             WHERE provider_account_id = $1 AND model = $2 AND model_version = $3
               AND input_hash = $4",
        )
        .bind(account_id)
        .bind(MODEL)
        .bind(MODEL_VERSION)
        .bind(&input_hash)
        .fetch_one(database.pool())
        .await?;
        return Ok(report(
            generation_id,
            true,
            snapshot_id,
            &signals,
            input_hash,
        ));
    };
    for signal in signals.values() {
        sqlx::query(
            "INSERT INTO account_track_signals
             (generation_id, track_id, meaningful_play_count, event_count,
              last_played_at, recency_score, completion_ratio, non_skip_ratio,
              saved, provider_rotation, provider_discovery, prompted_interest,
              intake, recommendation, provenance)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        )
        .bind(generation_id)
        .bind(signal.track_id)
        .bind(signal.meaningful_play_count)
        .bind(signal.event_count)
        .bind(signal.last_played_at)
        .bind(signal.recency_score)
        .bind(signal.completion_ratio)
        .bind(signal.non_skip_ratio)
        .bind(signal.saved)
        .bind(signal.provider_rotation)
        .bind(signal.provider_discovery)
        .bind(signal.prompted_interest)
        .bind(signal.intake)
        .bind(signal.recommendation)
        .bind(json!({
            "provider_rotation": signal.provider_rotation,
            "provider_discovery": signal.provider_discovery,
            "prompted_interest": signal.prompted_interest,
            "intake": signal.intake,
            "recommendation": signal.recommendation
        }))
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(report(
        generation_id,
        false,
        snapshot_id,
        &signals,
        input_hash,
    ))
}

/// Returns the latest signal generation for an account.
pub async fn status(database: &Database, account_label: &str) -> Result<GenerationStatus> {
    let account_id = account_id(database, account_label).await?;
    let row = sqlx::query(
        "SELECT id, source_snapshot_id, model, model_version, track_count,
                input_hash, created_at
         FROM signal_generations
         WHERE provider_account_id = $1
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(
            "no signal generation exists; run `chordrift signals generate`".to_owned(),
        )
    })?;
    Ok(GenerationStatus {
        generation_id: row.try_get("id")?,
        snapshot_id: row.try_get("source_snapshot_id")?,
        model: row.try_get("model")?,
        model_version: row.try_get("model_version")?,
        track_count: row.try_get("track_count")?,
        input_hash: row.try_get("input_hash")?,
        created_at: row.try_get("created_at")?,
    })
}

async fn load_inputs(
    database: &Database,
    account_label: &str,
) -> Result<(Uuid, Uuid, BTreeMap<Uuid, TrackSignal>)> {
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
        "SELECT track.id,
                COALESCE(listening.play_count, 0) AS play_count,
                COALESCE(listening.event_count, 0) AS event_count,
                COALESCE(listening.skip_count, 0) AS skip_count,
                COALESCE(listening.completed_count, 0) AS completed_count,
                listening.last_played_at,
                EXISTS (
                    SELECT 1 FROM provider_observed_saved_tracks saved
                    JOIN provider_tracks saved_track ON saved_track.id = saved.provider_track_id
                    WHERE saved.snapshot_id = $2 AND saved_track.track_id = track.id
                ) AS saved,
                EXISTS (
                    SELECT 1 FROM provider_observed_playlist_tracks membership
                    JOIN provider_tracks member_track ON member_track.id = membership.provider_track_id
                    JOIN provider_account_playlists policy
                      ON policy.provider_playlist_id = membership.provider_playlist_id
                     AND policy.provider_account_id = $1
                    WHERE membership.snapshot_id = $2 AND member_track.track_id = track.id
                      AND policy.signal_class = 'provider_curated'
                      AND policy.behavioral_signal = 'rotation'
                ) AS provider_rotation,
                EXISTS (
                    SELECT 1 FROM provider_observed_playlist_tracks membership
                    JOIN provider_tracks member_track ON member_track.id = membership.provider_track_id
                    JOIN provider_account_playlists policy
                      ON policy.provider_playlist_id = membership.provider_playlist_id
                     AND policy.provider_account_id = $1
                    WHERE membership.snapshot_id = $2 AND member_track.track_id = track.id
                      AND policy.behavioral_signal = 'discovery'
                ) AS provider_discovery,
                EXISTS (
                    SELECT 1 FROM provider_observed_playlist_tracks membership
                    JOIN provider_tracks member_track ON member_track.id = membership.provider_track_id
                    JOIN provider_account_playlists policy
                      ON policy.provider_playlist_id = membership.provider_playlist_id
                     AND policy.provider_account_id = $1
                    WHERE membership.snapshot_id = $2 AND member_track.track_id = track.id
                      AND policy.behavioral_signal = 'prompted'
                ) AS prompted_interest,
                EXISTS (
                    SELECT 1 FROM provider_observed_playlist_tracks membership
                    JOIN provider_tracks member_track ON member_track.id = membership.provider_track_id
                    JOIN provider_account_playlists policy
                      ON policy.provider_playlist_id = membership.provider_playlist_id
                     AND policy.provider_account_id = $1
                    WHERE membership.snapshot_id = $2 AND member_track.track_id = track.id
                      AND policy.signal_class = 'intake'
                ) AS intake,
                EXISTS (
                    SELECT 1 FROM provider_observed_playlist_tracks membership
                    JOIN provider_tracks member_track ON member_track.id = membership.provider_track_id
                    JOIN provider_account_playlists policy
                      ON policy.provider_playlist_id = membership.provider_playlist_id
                     AND policy.provider_account_id = $1
                    WHERE membership.snapshot_id = $2 AND member_track.track_id = track.id
                      AND policy.behavioral_signal = 'recommendation'
                ) AS recommendation
         FROM tracks track
         LEFT JOIN LATERAL (
             SELECT sum(stats.play_count)::bigint AS play_count,
                    sum(stats.event_count)::bigint AS event_count,
                    sum(stats.skip_count)::bigint AS skip_count,
                    sum(stats.completed_count)::bigint AS completed_count,
                    max(stats.last_played_at) AS last_played_at
             FROM account_listening_track_statistics stats
             WHERE stats.provider_account_id = $1 AND stats.track_id = track.id
         ) listening ON TRUE
         WHERE EXISTS (
             SELECT 1 FROM provider_observed_playlist_tracks membership
             JOIN provider_tracks member_track ON member_track.id = membership.provider_track_id
             WHERE membership.snapshot_id = $2 AND member_track.track_id = track.id
         ) OR EXISTS (
             SELECT 1 FROM provider_observed_saved_tracks saved
             JOIN provider_tracks saved_track ON saved_track.id = saved.provider_track_id
             WHERE saved.snapshot_id = $2 AND saved_track.track_id = track.id
         ) OR listening.event_count IS NOT NULL
         ORDER BY track.id",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .fetch_all(database.pool())
    .await?;
    let mut signals = BTreeMap::new();
    for row in rows {
        let track_id = row.try_get("id")?;
        signals.insert(
            track_id,
            TrackSignal {
                track_id,
                meaningful_play_count: row.try_get("play_count")?,
                event_count: row.try_get("event_count")?,
                last_played_at: row.try_get("last_played_at")?,
                recency_score: None,
                completion_ratio: ratio(
                    row.try_get("completed_count")?,
                    row.try_get("event_count")?,
                ),
                non_skip_ratio: ratio(
                    row.try_get::<i64, _>("event_count")? - row.try_get::<i64, _>("skip_count")?,
                    row.try_get("event_count")?,
                ),
                saved: row.try_get("saved")?,
                provider_rotation: row.try_get("provider_rotation")?,
                provider_discovery: row.try_get("provider_discovery")?,
                prompted_interest: row.try_get("prompted_interest")?,
                intake: row.try_get("intake")?,
                recommendation: row.try_get("recommendation")?,
            },
        );
    }
    Ok((account_id, snapshot_id, signals))
}

fn apply_derived_values(signals: &mut BTreeMap<Uuid, TrackSignal>) {
    let newest = signals
        .values()
        .filter_map(|signal| signal.last_played_at)
        .max();
    for signal in signals.values_mut() {
        signal.recency_score = newest.zip(signal.last_played_at).map(|(newest, played)| {
            let days = (newest - played).num_days().max(0) as f64;
            (-days / 365.0).exp()
        });
    }
}

fn ratio(numerator: i64, denominator: i64) -> Option<f64> {
    (denominator > 0).then(|| (numerator.max(0) as f64 / denominator as f64).clamp(0.0, 1.0))
}

fn signal_hash(signals: &BTreeMap<Uuid, TrackSignal>, snapshot_id: Uuid) -> String {
    let mut hasher = Sha256::new();
    hasher.update(MODEL.as_bytes());
    hasher.update(MODEL_VERSION.as_bytes());
    hasher.update(snapshot_id.as_bytes());
    for signal in signals.values() {
        hasher.update(signal.track_id.as_bytes());
        hasher.update(signal.meaningful_play_count.to_be_bytes());
        hasher.update(signal.event_count.to_be_bytes());
        hasher.update(
            signal
                .last_played_at
                .map_or(i64::MIN, |value| value.timestamp_millis())
                .to_be_bytes(),
        );
        for value in [
            signal.recency_score,
            signal.completion_ratio,
            signal.non_skip_ratio,
        ] {
            hasher.update(value.map_or(u64::MAX, f64::to_bits).to_be_bytes());
        }
        hasher.update([
            signal.saved as u8,
            signal.provider_rotation as u8,
            signal.provider_discovery as u8,
            signal.prompted_interest as u8,
            signal.intake as u8,
            signal.recommendation as u8,
        ]);
    }
    format!("{:x}", hasher.finalize())
}

fn report(
    generation_id: Uuid,
    reused: bool,
    snapshot_id: Uuid,
    signals: &BTreeMap<Uuid, TrackSignal>,
    input_hash: String,
) -> GenerationReport {
    GenerationReport {
        generation_id,
        reused,
        snapshot_id,
        track_count: signals.len(),
        history_tracks: signals
            .values()
            .filter(|signal| signal.event_count > 0)
            .count(),
        saved_tracks: signals.values().filter(|signal| signal.saved).count(),
        rotation_tracks: signals
            .values()
            .filter(|signal| signal.provider_rotation)
            .count(),
        discovery_tracks: signals
            .values()
            .filter(|signal| signal.provider_discovery)
            .count(),
        prompted_tracks: signals
            .values()
            .filter(|signal| signal.prompted_interest)
            .count(),
        intake_tracks: signals.values().filter(|signal| signal.intake).count(),
        recommendation_tracks: signals
            .values()
            .filter(|signal| signal.recommendation)
            .count(),
        input_hash,
    }
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
    use super::ratio;

    #[test]
    fn ratios_are_bounded_and_absent_without_events() {
        assert_eq!(ratio(1, 0), None);
        assert_eq!(ratio(8, 10), Some(0.8));
        assert_eq!(ratio(12, 10), Some(1.0));
        assert_eq!(ratio(-1, 10), Some(0.0));
    }
}
