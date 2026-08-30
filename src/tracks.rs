//! One-stop canonical track lookup and explainability reports.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result, classifications};

/// Stable or human-readable selector for one canonical track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrackSelector {
    /// Exact Spotify track ID.
    SpotifyId(String),
    /// Case-insensitive exact title, optionally narrowed by artist substring.
    Name {
        /// Exact title.
        title: String,
        /// Optional artist substring.
        artist: Option<String>,
    },
}

/// A playlist containing the track in the latest provider snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct CurrentPlaylist {
    /// Current playlist name.
    pub name: String,
    /// One-based position.
    pub position: usize,
    /// Chordrift role.
    pub role: String,
    /// Signal class.
    pub signal_class: String,
}

/// The newest Chordrift proposal or published destination for the track.
#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalPlacement {
    /// User-facing selected name.
    pub name: String,
    /// Stable playlist key.
    pub stable_key: String,
    /// One-based desired position.
    pub position: usize,
    /// Assignment source.
    pub source: String,
    /// Stored membership provenance.
    pub provenance: Value,
    /// Optional active manual override reason.
    pub manual_reason: Option<String>,
}

/// Historical provider playlist provenance retained in Neon.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoricalPlaylist {
    /// One or more observed names for the stable provider playlist.
    pub names: String,
    /// Spotify playlist ID.
    pub spotify_id: String,
    /// Signal class retained for the relationship.
    pub signal_class: String,
    /// Optional behavioral meaning.
    pub behavioral_signal: Option<String>,
    /// First snapshot containing this track.
    pub first_seen_at: DateTime<Utc>,
    /// Most recent snapshot containing this track.
    pub last_seen_at: DateTime<Utc>,
    /// Whether the playlist is present now.
    pub present: bool,
}

/// Latest account-specific listening and lifecycle signals.
#[derive(Clone, Debug, PartialEq)]
pub struct ListeningSignals {
    /// Meaningful historical plays.
    pub play_count: i64,
    /// Raw listening events.
    pub event_count: i64,
    /// Historical skips.
    pub skip_count: i64,
    /// Total milliseconds played.
    pub total_ms_played: i64,
    /// Last known playback.
    pub last_played_at: Option<DateTime<Utc>>,
    /// Present in the current saved library.
    pub saved: bool,
    /// Present in provider high rotation.
    pub rotation: bool,
    /// Present in provider discovery.
    pub discovery: bool,
    /// Prompt-derived interest.
    pub prompted: bool,
    /// Present in a user intake.
    pub intake: bool,
    /// Explicit friend recommendation.
    pub recommendation: bool,
}

/// Latest embedding and cluster explanation.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorExplanation {
    /// Personal embedding generation.
    pub embedding_generation_id: Uuid,
    /// Personal embedding model.
    pub embedding_model: String,
    /// Model revision.
    pub embedding_version: String,
    /// Vector width.
    pub dimensions: usize,
    /// Machine cluster, when assigned.
    pub cluster_label: Option<String>,
    /// Cosine similarity to its centroid.
    pub membership_score: Option<f64>,
    /// Rank within its cluster.
    pub representative_rank: Option<usize>,
}

/// One independently imported semantic fact.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticFact {
    /// Genre, mood, or sound descriptor.
    pub kind: String,
    /// Human-readable value.
    pub value: String,
    /// Model confidence.
    pub confidence: f64,
    /// Model provenance.
    pub model: String,
}

/// Complete explainability report for one canonical track.
#[derive(Clone, Debug, PartialEq)]
pub struct Inspection {
    /// Canonical identity.
    pub track_id: Uuid,
    /// Spotify identity.
    pub spotify_id: String,
    /// Track title.
    pub title: String,
    /// Display artists.
    pub artists: String,
    /// Album title.
    pub album: Option<String>,
    /// Recording ISRC.
    pub isrc: Option<String>,
    /// Track duration.
    pub duration_ms: Option<i32>,
    /// Current provider surfaces.
    pub current_playlists: Vec<CurrentPlaylist>,
    /// Newest Chordrift proposal or published destinations.
    pub canonical_placements: Vec<CanonicalPlacement>,
    /// Historical source surfaces.
    pub historical_playlists: Vec<HistoricalPlaylist>,
    /// Current and historical preference signals.
    pub signals: ListeningSignals,
    /// Personal embedding and clustering state.
    pub vector: Option<VectorExplanation>,
    /// Independently inferred semantic facts.
    pub semantic_facts: Vec<SemanticFact>,
    /// Active private user-authored classification, when present.
    pub user_classification: Option<classifications::ClassificationRevision>,
    /// Active exclusion reason, if removed from managed output.
    pub exclusion_reason: Option<String>,
}

/// Result of an exact-confirmed reversible exclusion or restoration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExclusionChange {
    /// Stable Spotify track identity.
    pub spotify_id: String,
    /// `excluded`, `already-excluded`, or `restored`.
    pub state: String,
    /// Latest editable proposal affected, when present.
    pub proposal_generation_id: Option<Uuid>,
}

/// One active reversible exclusion retained as private account history.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveExclusion {
    /// Stable Spotify track identity.
    pub spotify_id: String,
    /// Human-facing track title.
    pub title: String,
    /// Display artists.
    pub artists: String,
    /// Durable exclusion explanation.
    pub reason: String,
    /// When the exclusion became active.
    pub excluded_at: DateTime<Utc>,
}

/// Result of emptying the account's exclusion archive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmptyExclusionsReport {
    /// Resolved exclusions; identity and audit history remain stored.
    pub cleared: usize,
}

/// Lists active account exclusions without contacting the provider.
pub async fn active_exclusions(
    database: &Database,
    account_label: &str,
) -> Result<Vec<ActiveExclusion>> {
    let account_id = account_id(database, account_label).await?;
    let rows = sqlx::query(
        "SELECT min(provider.provider_track_id) AS spotify_id, track.title,
                COALESCE(string_agg(DISTINCT artist.name, ', '), '') AS artists,
                exclusion.exclusion_reason, exclusion.excluded_at
         FROM excluded_tracks exclusion
         JOIN tracks track ON track.id = exclusion.track_id
         JOIN provider_tracks provider
           ON provider.track_id = track.id AND provider.provider = 'spotify'
         LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
         LEFT JOIN artists artist ON artist.id = track_artist.artist_id
         WHERE exclusion.provider_account_id = $1
           AND exclusion.restored_at IS NULL
           AND exclusion.source_provider <> 'chordrift_forget'
         GROUP BY exclusion.id, track.id, track.title,
                  exclusion.exclusion_reason, exclusion.excluded_at
         ORDER BY exclusion.excluded_at, exclusion.id",
    )
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(ActiveExclusion {
                spotify_id: row.try_get("spotify_id")?,
                title: row.try_get("title")?,
                artists: row.try_get("artists")?,
                reason: row.try_get("exclusion_reason")?,
                excluded_at: row.try_get("excluded_at")?,
            })
        })
        .collect()
}

/// Empties the exclusion archive after proving none of its tracks is currently
/// present in the provider library. This changes Neon intent only and retains
/// immutable exclusion history.
pub async fn empty_exclusions(
    database: &Database,
    account_label: &str,
    confirm: &str,
) -> Result<EmptyExclusionsReport> {
    if confirm.trim() != account_label {
        return Err(configuration(
            "--confirm must exactly repeat the account label",
        ));
    }
    let account_id = account_id(database, account_label).await?;
    let mut tx = database.pool().begin().await?;
    let current_conflicts: i64 = sqlx::query_scalar(
        "WITH latest AS (
             SELECT id FROM provider_inventory_observations
             WHERE provider_account_id = $1
             ORDER BY captured_at DESC, id DESC LIMIT 1
         ), current_tracks AS (
             SELECT provider.track_id
             FROM latest
             JOIN provider_observed_playlist_tracks membership
               ON membership.snapshot_id = latest.id
             JOIN provider_tracks provider ON provider.id = membership.provider_track_id
             UNION
             SELECT provider.track_id
             FROM latest
             JOIN provider_observed_saved_tracks saved ON saved.snapshot_id = latest.id
             JOIN provider_tracks provider ON provider.id = saved.provider_track_id
         )
         SELECT count(*)::bigint
         FROM excluded_tracks exclusion
         JOIN current_tracks current ON current.track_id = exclusion.track_id
         WHERE exclusion.provider_account_id = $1
           AND exclusion.restored_at IS NULL
           AND exclusion.source_provider <> 'chordrift_forget'",
    )
    .bind(account_id)
    .fetch_one(&mut *tx)
    .await?;
    if current_conflicts != 0 {
        return Err(configuration(format!(
            "cannot empty exclusions while {current_conflicts} excluded track(s) are present in the current provider library; run ordinary maintenance first"
        )));
    }
    let track_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT track_id FROM excluded_tracks
         WHERE provider_account_id = $1 AND restored_at IS NULL
           AND source_provider <> 'chordrift_forget'
         ORDER BY excluded_at, id FOR UPDATE",
    )
    .bind(account_id)
    .fetch_all(&mut *tx)
    .await?;
    if track_ids.is_empty() {
        tx.commit().await?;
        return Ok(EmptyExclusionsReport { cleared: 0 });
    }
    sqlx::query(
        "WITH cleared AS (
             UPDATE excluded_tracks
             SET restored_at = now(),
                 restoration_reason = 'Cleared from exclusion archive; durable forget retained'
             WHERE provider_account_id = $1 AND track_id = ANY($2)
               AND restored_at IS NULL AND source_provider <> 'chordrift_forget'
             RETURNING provider_account_id, track_id,
                       source_provider_playlist_id, previous_concept_id
         )
         INSERT INTO excluded_tracks
         (provider_account_id, track_id, source_provider,
          source_provider_playlist_id, previous_concept_id, excluded_at,
          exclusion_reason)
         SELECT provider_account_id, track_id, 'chordrift_forget',
                source_provider_playlist_id, previous_concept_id, now(),
                'Cleared from visible exclusion archive; do not replay older placement'
         FROM cleared",
    )
    .bind(account_id)
    .bind(&track_ids)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE track_playlist_assignment_revisions SET superseded_at = now()
         WHERE provider_account_id = $1 AND track_id = ANY($2)
           AND superseded_at IS NULL",
    )
    .bind(account_id)
    .bind(&track_ids)
    .execute(&mut *tx)
    .await?;
    let proposed_generation: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM playlist_generations
         WHERE provider_account_id = $1 AND status = 'proposed'
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(generation_id) = proposed_generation {
        sqlx::query(
            "DELETE FROM playlist_tracks membership USING playlists playlist
             WHERE membership.playlist_id = playlist.id
               AND playlist.generation_id = $1 AND membership.track_id = ANY($2)",
        )
        .bind(generation_id)
        .bind(&track_ids)
        .execute(&mut *tx)
        .await?;
        refresh_proposal_coverage(&mut tx, account_id, generation_id).await?;
    }
    tx.commit().await?;
    Ok(EmptyExclusionsReport {
        cleared: track_ids.len(),
    })
}

/// Explicitly excludes one known track while retaining identity and history.
pub async fn exclude(
    database: &Database,
    account_label: &str,
    spotify_id: &str,
    reason: &str,
    confirm: &str,
) -> Result<ExclusionChange> {
    let spotify_id = spotify_id.trim();
    if spotify_id.is_empty() || confirm.trim() != spotify_id {
        return Err(configuration(
            "--confirm must exactly repeat the Spotify track ID",
        ));
    }
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(configuration("exclusion reason cannot be empty"));
    }
    let account_id = account_id(database, account_label).await?;
    let track = resolve_track(database, &TrackSelector::SpotifyId(spotify_id.to_owned())).await?;
    let track_id: Uuid = track.try_get("track_id")?;
    if !sqlx::query_scalar::<_, bool>("SELECT account_track_is_library_candidate($1, $2)")
        .bind(account_id)
        .bind(track_id)
        .fetch_one(database.pool())
        .await?
    {
        return Err(configuration(
            "track is not in this account's preservation universe",
        ));
    }
    let mut tx = database.pool().begin().await?;
    let existing: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM excluded_tracks
         WHERE provider_account_id = $1 AND track_id = $2 AND restored_at IS NULL)",
    )
    .bind(account_id)
    .bind(track_id)
    .fetch_one(&mut *tx)
    .await?;
    let proposal_generation_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM playlist_generations
         WHERE provider_account_id = $1 AND status = 'proposed'
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(&mut *tx)
    .await?;
    if existing {
        tx.commit().await?;
        return Ok(ExclusionChange {
            spotify_id: spotify_id.to_owned(),
            state: "already-excluded".to_owned(),
            proposal_generation_id,
        });
    }
    let previous: Option<(Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "SELECT provider.id, playlist.concept_id
         FROM playlist_generations generation
         JOIN playlists playlist ON playlist.generation_id = generation.id
         JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
         LEFT JOIN provider_playlists provider
           ON provider.concept_id = playlist.concept_id AND provider.provider = 'spotify'
         WHERE generation.provider_account_id = $1
           AND membership.track_id = $2
         ORDER BY generation.created_at DESC, generation.id DESC LIMIT 1",
    )
    .bind(account_id)
    .bind(track_id)
    .fetch_optional(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO excluded_tracks
         (provider_account_id, track_id, source_provider,
          source_provider_playlist_id, previous_concept_id, excluded_at,
          exclusion_reason)
         VALUES ($1, $2, 'user_explicit', $3, $4, now(), $5)",
    )
    .bind(account_id)
    .bind(track_id)
    .bind(previous.and_then(|value| value.0))
    .bind(previous.and_then(|value| value.1))
    .bind(reason)
    .execute(&mut *tx)
    .await?;
    if let Some(generation_id) = proposal_generation_id {
        sqlx::query(
            "DELETE FROM playlist_tracks membership
             USING playlists playlist
             WHERE membership.playlist_id = playlist.id
               AND playlist.generation_id = $1 AND membership.track_id = $2",
        )
        .bind(generation_id)
        .bind(track_id)
        .execute(&mut *tx)
        .await?;
        refresh_proposal_coverage(&mut tx, account_id, generation_id).await?;
    }
    tx.commit().await?;
    Ok(ExclusionChange {
        spotify_id: spotify_id.to_owned(),
        state: "excluded".to_owned(),
        proposal_generation_id,
    })
}

/// Restores one active explicit or inferred exclusion without guessing a destination.
pub async fn restore(
    database: &Database,
    account_label: &str,
    spotify_id: &str,
    reason: &str,
    confirm: &str,
) -> Result<ExclusionChange> {
    let spotify_id = spotify_id.trim();
    if spotify_id.is_empty() || confirm.trim() != spotify_id {
        return Err(configuration(
            "--confirm must exactly repeat the Spotify track ID",
        ));
    }
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(configuration("restoration reason cannot be empty"));
    }
    let account_id = account_id(database, account_label).await?;
    let track = resolve_track(database, &TrackSelector::SpotifyId(spotify_id.to_owned())).await?;
    let track_id: Uuid = track.try_get("track_id")?;
    let mut tx = database.pool().begin().await?;
    let changed = sqlx::query(
        "UPDATE excluded_tracks SET restored_at = now(), restoration_reason = $3
         WHERE provider_account_id = $1 AND track_id = $2 AND restored_at IS NULL",
    )
    .bind(account_id)
    .bind(track_id)
    .bind(reason)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if changed == 0 {
        return Err(configuration("track does not have an active exclusion"));
    }
    let proposal_generation_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM playlist_generations
         WHERE provider_account_id = $1 AND status = 'proposed'
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(generation_id) = proposal_generation_id {
        refresh_proposal_coverage(&mut tx, account_id, generation_id).await?;
    }
    tx.commit().await?;
    Ok(ExclusionChange {
        spotify_id: spotify_id.to_owned(),
        state: "restored".to_owned(),
        proposal_generation_id,
    })
}

async fn refresh_proposal_coverage(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account_id: Uuid,
    generation_id: Uuid,
) -> Result<()> {
    sqlx::query(
        "WITH required AS (
             SELECT track.id
             FROM tracks track
             WHERE account_track_is_library_candidate($1, track.id)
               AND NOT EXISTS (
                   SELECT 1 FROM excluded_tracks exclusion
                   WHERE exclusion.provider_account_id = $1
                     AND exclusion.track_id = track.id
                     AND exclusion.restored_at IS NULL
               )
         ), represented AS (
             SELECT count(DISTINCT membership.track_id)::integer AS value
             FROM playlist_tracks membership
             JOIN playlists playlist ON playlist.id = membership.playlist_id
             JOIN required ON required.id = membership.track_id
             WHERE playlist.generation_id = $2
         )
         UPDATE playlist_generations generation
         SET required_track_count = (SELECT count(*)::integer FROM required),
             represented_track_count = represented.value,
             coverage_complete = represented.value = (SELECT count(*) FROM required)
         FROM represented WHERE generation.id = $2",
    )
    .bind(account_id)
    .bind(generation_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

/// Resolves one track and assembles its provider, history, signal, and ML rationale.
pub async fn inspect(
    database: &Database,
    account_label: &str,
    selector: &TrackSelector,
) -> Result<Inspection> {
    let account_id = account_id(database, account_label).await?;
    let track = resolve_track(database, selector).await?;
    let track_id: Uuid = track.try_get("track_id")?;
    let spotify_id: String = track.try_get("spotify_id")?;

    let current_playlists = current_playlists(database, account_id, track_id).await?;
    let canonical_placements = canonical_placements(database, account_id, track_id).await?;
    let historical_playlists = historical_playlists(database, account_id, track_id).await?;
    let signals = listening_signals(database, account_id, track_id).await?;
    let vector = vector_explanation(database, account_id, track_id).await?;
    let semantic_facts = semantic_facts(database, track_id).await?;
    let user_classification = classifications::history(database, account_label, &spotify_id)
        .await?
        .into_iter()
        .find(|revision| revision.superseded_at.is_none());
    let exclusion_reason = sqlx::query_scalar(
        "SELECT exclusion_reason FROM excluded_tracks
         WHERE provider_account_id = $1 AND track_id = $2 AND restored_at IS NULL
         ORDER BY excluded_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .bind(track_id)
    .fetch_optional(database.pool())
    .await?;

    Ok(Inspection {
        track_id,
        spotify_id,
        title: track.try_get("title")?,
        artists: track.try_get("artists")?,
        album: track.try_get("album")?,
        isrc: track.try_get("isrc")?,
        duration_ms: track.try_get("duration_ms")?,
        current_playlists,
        canonical_placements,
        historical_playlists,
        signals,
        vector,
        semantic_facts,
        user_classification,
        exclusion_reason,
    })
}

async fn account_id(database: &Database, account_label: &str) -> Result<Uuid> {
    sqlx::query_scalar(
        "SELECT id FROM provider_accounts WHERE provider = 'spotify' AND account_label = $1",
    )
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration(format!("account `{account_label}` has not been imported")))
}

async fn resolve_track(
    database: &Database,
    selector: &TrackSelector,
) -> Result<sqlx::postgres::PgRow> {
    let (spotify_id, title, artist) = match selector {
        TrackSelector::SpotifyId(id) => (Some(id.trim()), None, None),
        TrackSelector::Name { title, artist } => {
            (None, Some(title.trim()), artist.as_deref().map(str::trim))
        }
    };
    let rows = sqlx::query(
        "SELECT track.id AS track_id, track.title, track.isrc, track.duration_ms,
                album.title AS album,
                COALESCE(string_agg(artist.name, ', ' ORDER BY track_artist.position), '') AS artists,
                min(provider.provider_track_id) AS spotify_id
         FROM tracks track
         JOIN provider_tracks provider
           ON provider.track_id = track.id AND provider.provider = 'spotify'
         LEFT JOIN albums album ON album.id = track.album_id
         LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
         LEFT JOIN artists artist ON artist.id = track_artist.artist_id
         WHERE ($1::text IS NOT NULL AND provider.provider_track_id = $1)
            OR ($1::text IS NULL AND lower(track.title) = lower($2)
                AND ($3::text IS NULL OR EXISTS (
                    SELECT 1 FROM track_artists selected_artist
                    JOIN artists selected ON selected.id = selected_artist.artist_id
                    WHERE selected_artist.track_id = track.id
                      AND lower(selected.name) LIKE '%' || lower($3) || '%'
                )))
         GROUP BY track.id, track.title, track.isrc, track.duration_ms, album.title
         ORDER BY track.id",
    )
    .bind(spotify_id)
    .bind(title)
    .bind(artist)
    .fetch_all(database.pool())
    .await?;
    match rows.len() {
        0 => Err(configuration(
            "track selector did not match a Spotify track",
        )),
        1 => Ok(rows.into_iter().next().expect("one row")),
        count => Err(configuration(format!(
            "track selector matched {count} tracks; add `--artist` or use `--spotify-id`"
        ))),
    }
}

async fn current_playlists(
    database: &Database,
    account_id: Uuid,
    track_id: Uuid,
) -> Result<Vec<CurrentPlaylist>> {
    let rows = sqlx::query(
        "SELECT current.name, membership.position, current.role, current.signal_class
         FROM current_spotify_playlists current
         JOIN provider_observed_playlist_tracks membership
           ON membership.snapshot_id = current.snapshot_id
          AND membership.provider_playlist_id = current.provider_playlist_id
         JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
         WHERE current.provider_account_id = $1 AND provider_track.track_id = $2
         ORDER BY lower(current.name), membership.position",
    )
    .bind(account_id)
    .bind(track_id)
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(CurrentPlaylist {
                name: row.try_get("name")?,
                position: one_based(row.try_get("position")?)?,
                role: row.try_get("role")?,
                signal_class: row.try_get("signal_class")?,
            })
        })
        .collect()
}

async fn canonical_placements(
    database: &Database,
    account_id: Uuid,
    track_id: Uuid,
) -> Result<Vec<CanonicalPlacement>> {
    let rows = sqlx::query(
        "SELECT revision.name, concept.stable_key, membership.position,
                membership.source, membership.provenance, assignment.reason AS manual_reason
         FROM playlist_generations generation
         JOIN playlists playlist ON playlist.generation_id = generation.id
         JOIN playlist_concepts concept ON concept.id = playlist.concept_id
         JOIN playlist_name_revisions revision
           ON revision.playlist_id = playlist.id AND revision.selected
         JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
         LEFT JOIN track_playlist_assignment_revisions assignment
           ON assignment.provider_account_id = generation.provider_account_id
          AND assignment.track_id = membership.track_id
          AND assignment.destination_concept_id = concept.id
          AND assignment.decision = 'assign' AND assignment.superseded_at IS NULL
         WHERE generation.id = (
             SELECT id FROM playlist_generations
             WHERE provider_account_id = $1
             ORDER BY created_at DESC, id DESC LIMIT 1
         ) AND membership.track_id = $2
         ORDER BY lower(revision.name), membership.position",
    )
    .bind(account_id)
    .bind(track_id)
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(CanonicalPlacement {
                name: row.try_get("name")?,
                stable_key: row.try_get("stable_key")?,
                position: one_based(row.try_get("position")?)?,
                source: row.try_get("source")?,
                provenance: row.try_get("provenance")?,
                manual_reason: row.try_get("manual_reason")?,
            })
        })
        .collect()
}

async fn historical_playlists(
    database: &Database,
    account_id: Uuid,
    track_id: Uuid,
) -> Result<Vec<HistoricalPlaylist>> {
    let rows = sqlx::query(
        "SELECT string_agg(DISTINCT snapshot.name, ' / ' ORDER BY snapshot.name) AS names,
                provider_playlist.provider_playlist_id AS spotify_id,
                policy.signal_class, policy.behavioral_signal,
                min(library.captured_at) AS first_seen_at,
                max(library.captured_at) AS last_seen_at,
                policy.present_in_latest_snapshot AS present
         FROM provider_inventory_observations library
         JOIN provider_observed_playlists snapshot ON snapshot.snapshot_id = library.id
         JOIN provider_playlists provider_playlist ON provider_playlist.id = snapshot.provider_playlist_id
         JOIN provider_account_playlists policy
           ON policy.provider_account_id = library.provider_account_id
          AND policy.provider_playlist_id = provider_playlist.id
         JOIN provider_observed_playlist_tracks membership
           ON membership.snapshot_id = library.id
          AND membership.provider_playlist_id = provider_playlist.id
         JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
         WHERE library.provider_account_id = $1 AND provider_track.track_id = $2
         GROUP BY provider_playlist.id, provider_playlist.provider_playlist_id,
                  policy.signal_class, policy.behavioral_signal,
                  policy.present_in_latest_snapshot
         ORDER BY max(library.captured_at) DESC, names",
    )
    .bind(account_id)
    .bind(track_id)
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(HistoricalPlaylist {
                names: row.try_get("names")?,
                spotify_id: row.try_get("spotify_id")?,
                signal_class: row.try_get("signal_class")?,
                behavioral_signal: row.try_get("behavioral_signal")?,
                first_seen_at: row.try_get("first_seen_at")?,
                last_seen_at: row.try_get("last_seen_at")?,
                present: row.try_get("present")?,
            })
        })
        .collect()
}

async fn listening_signals(
    database: &Database,
    account_id: Uuid,
    track_id: Uuid,
) -> Result<ListeningSignals> {
    let row = sqlx::query(
        "SELECT COALESCE(sum(stats.play_count), 0)::bigint AS play_count,
                COALESCE(sum(stats.event_count), 0)::bigint AS event_count,
                COALESCE(sum(stats.skip_count), 0)::bigint AS skip_count,
                COALESCE(sum(stats.total_ms_played), 0)::bigint AS total_ms_played,
                max(stats.last_played_at) AS last_played_at,
                COALESCE(bool_or(signal.saved), false) AS saved,
                COALESCE(bool_or(signal.provider_rotation), false) AS rotation,
                COALESCE(bool_or(signal.provider_discovery), false) AS discovery,
                COALESCE(bool_or(signal.prompted_interest), false) AS prompted,
                COALESCE(bool_or(signal.intake), false) AS intake,
                COALESCE(bool_or(signal.recommendation), false) AS recommendation
         FROM tracks track
         LEFT JOIN account_listening_track_statistics stats
           ON stats.provider_account_id = $1 AND stats.track_id = track.id
         LEFT JOIN LATERAL (
             SELECT item.* FROM account_track_signals item
             JOIN signal_generations generation ON generation.id = item.generation_id
             WHERE generation.provider_account_id = $1 AND item.track_id = track.id
             ORDER BY generation.created_at DESC, generation.id DESC LIMIT 1
         ) signal ON TRUE
         WHERE track.id = $2 GROUP BY track.id",
    )
    .bind(account_id)
    .bind(track_id)
    .fetch_one(database.pool())
    .await?;
    Ok(ListeningSignals {
        play_count: row.try_get("play_count")?,
        event_count: row.try_get("event_count")?,
        skip_count: row.try_get("skip_count")?,
        total_ms_played: row.try_get("total_ms_played")?,
        last_played_at: row.try_get("last_played_at")?,
        saved: row.try_get("saved")?,
        rotation: row.try_get("rotation")?,
        discovery: row.try_get("discovery")?,
        prompted: row.try_get("prompted")?,
        intake: row.try_get("intake")?,
        recommendation: row.try_get("recommendation")?,
    })
}

async fn vector_explanation(
    database: &Database,
    account_id: Uuid,
    track_id: Uuid,
) -> Result<Option<VectorExplanation>> {
    let row = sqlx::query(
        "SELECT embedding.id AS embedding_generation_id, embedding.model,
                embedding.model_version, embedding.dimensions,
                cluster.machine_label, membership.membership_score,
                membership.representative_rank
         FROM embedding_generations embedding
         JOIN account_track_embeddings vector
           ON vector.generation_id = embedding.id AND vector.track_id = $2
         LEFT JOIN LATERAL (
             SELECT generation.id FROM cluster_generations generation
             WHERE generation.provider_account_id = $1
               AND generation.embedding_generation_id = embedding.id
             ORDER BY generation.created_at DESC, generation.id DESC LIMIT 1
         ) cluster_generation ON TRUE
         LEFT JOIN clusters cluster ON cluster.generation_id = cluster_generation.id
         LEFT JOIN cluster_tracks membership
           ON membership.cluster_id = cluster.id AND membership.track_id = $2
         WHERE embedding.provider_account_id = $1
         ORDER BY embedding.created_at DESC, embedding.id DESC,
                  membership.membership_score DESC NULLS LAST LIMIT 1",
    )
    .bind(account_id)
    .bind(track_id)
    .fetch_optional(database.pool())
    .await?;
    row.map(|row| {
        Ok(VectorExplanation {
            embedding_generation_id: row.try_get("embedding_generation_id")?,
            embedding_model: row.try_get("model")?,
            embedding_version: row.try_get("model_version")?,
            dimensions: usize::try_from(row.try_get::<i32, _>("dimensions")?)
                .map_err(|_| configuration("invalid embedding dimensions"))?,
            cluster_label: row.try_get("machine_label")?,
            membership_score: row.try_get("membership_score")?,
            representative_rank: row
                .try_get::<Option<i32>, _>("representative_rank")?
                .map(one_based)
                .transpose()?,
        })
    })
    .transpose()
}

async fn semantic_facts(database: &Database, track_id: Uuid) -> Result<Vec<SemanticFact>> {
    let rows = sqlx::query(
        "SELECT fact.fact_kind, fact.value, fact.confidence,
                inference.model || '@' || inference.model_version AS model
         FROM track_model_facts fact
         JOIN track_model_inferences inference ON inference.id = fact.inference_id
         WHERE inference.track_id = $1
         ORDER BY fact.confidence DESC, fact.fact_kind, lower(fact.value) LIMIT 12",
    )
    .bind(track_id)
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(SemanticFact {
                kind: row.try_get("fact_kind")?,
                value: row.try_get("value")?,
                confidence: row.try_get("confidence")?,
                model: row.try_get("model")?,
            })
        })
        .collect()
}

fn one_based(value: i32) -> Result<usize> {
    usize::try_from(value)
        .map(|value| value + 1)
        .map_err(|_| configuration("invalid negative track position"))
}

fn configuration(message: impl Into<String>) -> ChordriftError {
    ChordriftError::Configuration(message.into())
}
