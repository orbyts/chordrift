//! Cache-first, provenance-aware semantic metadata enrichment.

use std::{collections::BTreeSet, time::Duration};

use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use storexa::Database;
use tokio::time::{Instant, sleep};
use uuid::Uuid;

use crate::{ChordriftError, Result};

const SOURCE: &str = "musicbrainz";
const API_VERSION: &str = "ws2-json";
const PARSER_VERSION: &str = "musicbrainz-isrc-v1";
const API_ROOT: &str = "https://musicbrainz.org/ws/2/";
const REQUEST_INTERVAL: Duration = Duration::from_millis(1_100);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_TRANSIENT_ATTEMPTS: usize = 3;

/// Summary of one bounded cache-first MusicBrainz enrichment run.
#[derive(Clone, Debug, PartialEq)]
pub struct RunReport {
    /// Persisted run identity.
    pub run_id: Uuid,
    /// Eligible tracks considered in this batch.
    pub tracks_considered: usize,
    /// Network requests made after consulting the lookup cache.
    pub requests_made: usize,
    /// Tracks whose ISRC lookup was already cached.
    pub cache_hits: usize,
    /// Tracks conservatively linked to one MusicBrainz recording.
    pub matched_tracks: usize,
    /// Tracks whose ISRC mapped to multiple unresolved recordings.
    pub ambiguous_tracks: usize,
    /// Tracks with no MusicBrainz recording for the ISRC.
    pub unmatched_tracks: usize,
    /// Tracks whose lookup received a transient or permanent HTTP error.
    pub error_tracks: usize,
    /// Semantic facts inserted for this parser generation.
    pub facts_written: usize,
}

/// Latest aggregate enrichment state for an account.
#[derive(Clone, Debug, PartialEq)]
pub struct StatusReport {
    /// Tracks eligible for enrichment from current library or history.
    pub eligible_tracks: usize,
    /// Eligible tracks with an ISRC.
    pub tracks_with_isrc: usize,
    /// Tracks matched by the current parser.
    pub matched_tracks: usize,
    /// Tracks left ambiguous by the current parser.
    pub ambiguous_tracks: usize,
    /// Tracks with a cached unmatched outcome.
    pub unmatched_tracks: usize,
    /// Tracks whose lookup ended in a cached error.
    pub error_tracks: usize,
    /// Current semantic fact count.
    pub facts: usize,
    /// Most recent completed run, if any.
    pub latest_run_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
struct TrackInput {
    id: Uuid,
    title: String,
    duration_ms: Option<i32>,
    isrc: String,
    artists: Vec<String>,
}

#[derive(Clone, Debug)]
struct CachedLookup {
    id: Uuid,
    outcome: LookupOutcome,
    response: Option<Value>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LookupOutcome {
    Response,
    NotFound,
    Error,
}

impl LookupOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Response => "response",
            Self::NotFound => "not_found",
            Self::Error => "error",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "response" => Ok(Self::Response),
            "not_found" => Ok(Self::NotFound),
            "error" => Ok(Self::Error),
            _ => Err(ChordriftError::Configuration(
                "database contains an unsupported enrichment lookup outcome".to_owned(),
            )),
        }
    }
}

#[derive(Clone, Debug)]
struct MatchDecision {
    status: &'static str,
    candidate_count: usize,
    confidence: Option<f64>,
    selected: Option<MbRecording>,
}

#[derive(Clone, Debug)]
struct Fact {
    kind: &'static str,
    value: String,
    normalized_value: String,
    weight: f64,
    confidence: f64,
    provenance: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MbResponse {
    #[serde(default)]
    recordings: Vec<MbRecording>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MbRecording {
    id: String,
    title: String,
    #[serde(default)]
    length: Option<i32>,
    #[serde(default, rename = "artist-credit")]
    artist_credit: Vec<MbArtistCredit>,
    #[serde(default)]
    releases: Vec<MbRelease>,
    #[serde(default)]
    genres: Vec<MbTag>,
    #[serde(default)]
    tags: Vec<MbTag>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MbArtistCredit {
    artist: MbArtist,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MbArtist {
    name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MbRelease {
    id: String,
    #[serde(default)]
    country: Option<String>,
    #[serde(default, rename = "text-representation")]
    text_representation: Option<MbTextRepresentation>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MbTextRepresentation {
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    script: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MbTag {
    #[serde(default)]
    count: i64,
    name: String,
}

/// Enriches a bounded batch, reusing durable ISRC lookups by default.
pub async fn musicbrainz(
    database: &Database,
    account_label: &str,
    limit: u32,
    refresh: bool,
) -> Result<RunReport> {
    if limit == 0 || limit > 1_000 {
        return Err(ChordriftError::Configuration(
            "enrichment limit must be between 1 and 1000".to_owned(),
        ));
    }
    let account_id = account_id(database, account_label).await?;
    let run_id: Uuid = sqlx::query_scalar(
        "INSERT INTO enrichment_runs
         (provider_account_id, source, source_version, status, parameters)
         VALUES ($1, $2, $3, 'running', $4) RETURNING id",
    )
    .bind(account_id)
    .bind(SOURCE)
    .bind(PARSER_VERSION)
    .bind(json!({ "limit": limit, "refresh": refresh, "lookup": "isrc" }))
    .fetch_one(database.pool())
    .await?;

    let result = run_musicbrainz(database, account_id, run_id, limit, refresh).await;
    if result.is_err() {
        sqlx::query(
            "UPDATE enrichment_runs SET status = 'failed', finished_at = now() WHERE id = $1",
        )
        .bind(run_id)
        .execute(database.pool())
        .await?;
    }
    result
}

async fn run_musicbrainz(
    database: &Database,
    account_id: Uuid,
    run_id: Uuid,
    limit: u32,
    refresh: bool,
) -> Result<RunReport> {
    let tracks = load_tracks(database, account_id, limit, refresh).await?;
    let mut report = RunReport {
        run_id,
        tracks_considered: tracks.len(),
        requests_made: 0,
        cache_hits: 0,
        matched_tracks: 0,
        ambiguous_tracks: 0,
        unmatched_tracks: 0,
        error_tracks: 0,
        facts_written: 0,
    };
    let client = MusicBrainzClient::new()?;
    let mut last_request = None;

    for (index, track) in tracks.iter().enumerate() {
        eprintln!(
            "musicbrainz enrich: {}/{} {} — {}",
            index + 1,
            tracks.len(),
            track.title,
            track.artists.join(", ")
        );
        let cached = cached_lookup(database, "isrc", &track.isrc).await?;
        let lookup = match cached {
            Some(cached) => {
                report.cache_hits += 1;
                cached
            }
            None => {
                wait_for_rate_limit(&mut last_request).await;
                let fetched = client.fetch_isrc(&track.isrc).await?;
                last_request = Some(Instant::now());
                report.requests_made += fetched.requests_made;
                persist_lookup(database, "isrc", &track.isrc, fetched).await?
            }
        };
        let mut decision = decide_match(track, &lookup)?;
        if let Some(recording_id) = decision
            .selected
            .as_ref()
            .map(|recording| recording.id.clone())
        {
            let detail = match cached_lookup(database, "recording", &recording_id).await? {
                Some(cached) => {
                    report.cache_hits += 1;
                    cached
                }
                None => {
                    wait_for_rate_limit(&mut last_request).await;
                    let fetched = client.fetch_recording(&recording_id).await?;
                    last_request = Some(Instant::now());
                    report.requests_made += fetched.requests_made;
                    persist_lookup(database, "recording", &recording_id, fetched).await?
                }
            };
            if detail.outcome == LookupOutcome::Response {
                let recording: MbRecording =
                    serde_json::from_value(detail.response.ok_or_else(|| {
                        ChordriftError::Configuration(
                            "MusicBrainz recording lookup is missing its cached response"
                                .to_owned(),
                        )
                    })?)?;
                if recording.id != recording_id {
                    return Err(ChordriftError::Configuration(
                        "MusicBrainz recording lookup returned a different identity".to_owned(),
                    ));
                }
                decision.selected = Some(recording);
            }
        }
        match decision.status {
            "matched" => report.matched_tracks += 1,
            "ambiguous" => report.ambiguous_tracks += 1,
            "unmatched" => report.unmatched_tracks += 1,
            "error" => report.error_tracks += 1,
            _ => unreachable!("decision statuses are internal constants"),
        }
        report.facts_written += persist_match(database, track, &lookup, &decision).await?;
    }

    sqlx::query(
        "UPDATE enrichment_runs
         SET status = 'succeeded', tracks_considered = $2, requests_made = $3,
             cache_hits = $4, matched_tracks = $5, ambiguous_tracks = $6,
             unmatched_tracks = $7, error_tracks = $8, facts_written = $9,
             finished_at = now()
         WHERE id = $1",
    )
    .bind(run_id)
    .bind(as_i32(report.tracks_considered)?)
    .bind(as_i32(report.requests_made)?)
    .bind(as_i32(report.cache_hits)?)
    .bind(as_i32(report.matched_tracks)?)
    .bind(as_i32(report.ambiguous_tracks)?)
    .bind(as_i32(report.unmatched_tracks)?)
    .bind(as_i32(report.error_tracks)?)
    .bind(as_i32(report.facts_written)?)
    .execute(database.pool())
    .await?;
    Ok(report)
}

/// Reports current enrichment coverage without making network requests.
pub async fn status(database: &Database, account_label: &str) -> Result<StatusReport> {
    let account_id = account_id(database, account_label).await?;
    let row = sqlx::query(
        "WITH eligible AS (
             SELECT DISTINCT track.id, track.isrc
             FROM tracks track
             WHERE EXISTS (
                 SELECT 1 FROM provider_tracks provider
                 WHERE provider.track_id = track.id AND provider.provider = 'spotify'
             ) AND account_track_is_eligible($1, track.id)
         )
         SELECT count(*)::bigint AS eligible_tracks,
                count(*) FILTER (WHERE eligible.isrc IS NOT NULL)::bigint AS tracks_with_isrc,
                count(*) FILTER (WHERE match.status = 'matched')::bigint AS matched_tracks,
                count(*) FILTER (WHERE match.status = 'ambiguous')::bigint AS ambiguous_tracks,
                count(*) FILTER (WHERE match.status = 'unmatched')::bigint AS unmatched_tracks,
                count(*) FILTER (WHERE match.status = 'error')::bigint AS error_tracks
         FROM eligible
         LEFT JOIN track_enrichment_matches match
           ON match.track_id = eligible.id AND match.source = $2 AND match.parser_version = $3",
    )
    .bind(account_id)
    .bind(SOURCE)
    .bind(PARSER_VERSION)
    .fetch_one(database.pool())
    .await?;
    let facts: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM track_semantic_facts fact
         WHERE fact.source = $1 AND fact.parser_version = $2
           AND account_track_is_eligible($3, fact.track_id)",
    )
    .bind(SOURCE)
    .bind(PARSER_VERSION)
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let latest_run_at: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT max(finished_at) FROM enrichment_runs
         WHERE provider_account_id = $1 AND source = $2 AND status = 'succeeded'",
    )
    .bind(account_id)
    .bind(SOURCE)
    .fetch_one(database.pool())
    .await?;
    Ok(StatusReport {
        eligible_tracks: as_usize(row.try_get("eligible_tracks")?)?,
        tracks_with_isrc: as_usize(row.try_get("tracks_with_isrc")?)?,
        matched_tracks: as_usize(row.try_get("matched_tracks")?)?,
        ambiguous_tracks: as_usize(row.try_get("ambiguous_tracks")?)?,
        unmatched_tracks: as_usize(row.try_get("unmatched_tracks")?)?,
        error_tracks: as_usize(row.try_get("error_tracks")?)?,
        facts: as_usize(facts)?,
        latest_run_at,
    })
}

async fn load_tracks(
    database: &Database,
    account_id: Uuid,
    limit: u32,
    refresh: bool,
) -> Result<Vec<TrackInput>> {
    let rows = sqlx::query(
        "SELECT track.id, track.title, track.duration_ms, track.isrc,
                COALESCE(string_agg(artist.name, E'\\x1f' ORDER BY track_artist.position), '') AS artists
         FROM tracks track
         LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
         LEFT JOIN artists artist ON artist.id = track_artist.artist_id
         LEFT JOIN account_track_signals signal
           ON signal.track_id = track.id AND signal.generation_id = (
               SELECT generation.id FROM signal_generations generation
               WHERE generation.provider_account_id = $1
               ORDER BY generation.created_at DESC, generation.id DESC LIMIT 1
           )
         WHERE track.isrc IS NOT NULL
           AND EXISTS (
               SELECT 1 FROM provider_tracks provider
               WHERE provider.track_id = track.id AND provider.provider = 'spotify'
           )
           AND account_track_is_eligible($1, track.id)
           AND ($2 OR NOT EXISTS (
               SELECT 1 FROM track_enrichment_matches match
               WHERE match.track_id = track.id AND match.source = $3
                 AND match.parser_version = $4
           ))
         GROUP BY track.id, track.title, track.duration_ms, track.isrc,
                  signal.intake, signal.provider_rotation, signal.saved,
                  signal.meaningful_play_count
         ORDER BY signal.intake DESC NULLS LAST,
                  signal.provider_rotation DESC NULLS LAST,
                  signal.saved DESC NULLS LAST,
                  signal.meaningful_play_count DESC NULLS LAST,
                  track.id
         LIMIT $5",
    )
    .bind(account_id)
    .bind(refresh)
    .bind(SOURCE)
    .bind(PARSER_VERSION)
    .bind(i64::from(limit))
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            let artists: String = row.try_get("artists")?;
            Ok(TrackInput {
                id: row.try_get("id")?,
                title: row.try_get("title")?,
                duration_ms: row.try_get("duration_ms")?,
                isrc: row.try_get("isrc")?,
                artists: artists
                    .split('\u{1f}')
                    .filter(|artist| !artist.is_empty())
                    .map(str::to_owned)
                    .collect(),
            })
        })
        .collect()
}

async fn cached_lookup(
    database: &Database,
    lookup_kind: &str,
    lookup_value: &str,
) -> Result<Option<CachedLookup>> {
    let row = sqlx::query(
        "SELECT id, outcome, response FROM track_enrichment_lookups
         WHERE source = $1 AND api_version = $2 AND lookup_kind = $3
           AND lookup_value = $4 AND (outcome <> 'error' OR retry_after > now())",
    )
    .bind(SOURCE)
    .bind(API_VERSION)
    .bind(lookup_kind)
    .bind(lookup_value)
    .fetch_optional(database.pool())
    .await?;
    row.map(|row| {
        Ok(CachedLookup {
            id: row.try_get("id")?,
            outcome: LookupOutcome::parse(row.try_get("outcome")?)?,
            response: row.try_get("response")?,
        })
    })
    .transpose()
}

#[derive(Clone, Debug)]
struct FetchedLookup {
    outcome: LookupOutcome,
    http_status: u16,
    response: Option<Value>,
    error_class: Option<&'static str>,
    requests_made: usize,
}

async fn persist_lookup(
    database: &Database,
    lookup_kind: &str,
    lookup_value: &str,
    fetched: FetchedLookup,
) -> Result<CachedLookup> {
    let response_sha256 = fetched.response.as_ref().map(payload_hash).transpose()?;
    let row = sqlx::query(
        "INSERT INTO track_enrichment_lookups
         (source, api_version, lookup_kind, lookup_value, outcome, http_status,
          response, response_sha256, error_class, fetched_at, retry_after)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now(),
                 CASE WHEN $5 = 'error' THEN now() + interval '1 hour' ELSE NULL END)
         ON CONFLICT (source, api_version, lookup_kind, lookup_value)
         DO UPDATE SET outcome = EXCLUDED.outcome, http_status = EXCLUDED.http_status,
             response = EXCLUDED.response, response_sha256 = EXCLUDED.response_sha256,
             error_class = EXCLUDED.error_class, fetched_at = now(),
             retry_after = EXCLUDED.retry_after
         RETURNING id, outcome, response",
    )
    .bind(SOURCE)
    .bind(API_VERSION)
    .bind(lookup_kind)
    .bind(lookup_value)
    .bind(fetched.outcome.as_str())
    .bind(i32::from(fetched.http_status))
    .bind(fetched.response)
    .bind(response_sha256)
    .bind(fetched.error_class)
    .fetch_one(database.pool())
    .await?;
    Ok(CachedLookup {
        id: row.try_get("id")?,
        outcome: LookupOutcome::parse(row.try_get("outcome")?)?,
        response: row.try_get("response")?,
    })
}

fn decide_match(track: &TrackInput, lookup: &CachedLookup) -> Result<MatchDecision> {
    if lookup.outcome == LookupOutcome::NotFound {
        return Ok(MatchDecision {
            status: "unmatched",
            candidate_count: 0,
            confidence: None,
            selected: None,
        });
    }
    if lookup.outcome == LookupOutcome::Error {
        return Ok(MatchDecision {
            status: "error",
            candidate_count: 0,
            confidence: None,
            selected: None,
        });
    }
    let response: MbResponse =
        serde_json::from_value(lookup.response.clone().ok_or_else(|| {
            ChordriftError::Configuration(
                "MusicBrainz response lookup is missing its cached response".to_owned(),
            )
        })?)?;
    if response.recordings.is_empty() {
        return Ok(MatchDecision {
            status: "unmatched",
            candidate_count: 0,
            confidence: None,
            selected: None,
        });
    }
    if response.recordings.len() == 1 {
        let candidate = response
            .recordings
            .into_iter()
            .next()
            .expect("single candidate exists");
        let confidence = candidate_score(track, &candidate);
        if confidence < 0.85 {
            return Ok(MatchDecision {
                status: "ambiguous",
                candidate_count: 1,
                confidence: Some(confidence),
                selected: None,
            });
        }
        return Ok(MatchDecision {
            status: "matched",
            candidate_count: 1,
            confidence: Some(confidence),
            selected: Some(candidate),
        });
    }
    let candidate_count = response.recordings.len();
    let mut scored: Vec<_> = response
        .recordings
        .into_iter()
        .map(|candidate| (candidate_score(track, &candidate), candidate))
        .collect();
    scored.sort_by(|left, right| right.0.total_cmp(&left.0));
    let top = scored.first().map(|value| value.0).unwrap_or(0.0);
    let runner_up = scored.get(1).map(|value| value.0).unwrap_or(0.0);
    if top >= 0.85 && top - runner_up >= 0.15 {
        Ok(MatchDecision {
            status: "matched",
            candidate_count,
            confidence: Some(top),
            selected: scored.into_iter().next().map(|value| value.1),
        })
    } else {
        Ok(MatchDecision {
            status: "ambiguous",
            candidate_count,
            confidence: Some(top),
            selected: None,
        })
    }
}

fn candidate_score(track: &TrackInput, candidate: &MbRecording) -> f64 {
    let mut score = 0.0;
    if normalize_title(&track.title) == normalize_title(&candidate.title) {
        score += 0.55;
    }
    let track_artists: BTreeSet<_> = track.artists.iter().map(|name| normalize(name)).collect();
    if candidate
        .artist_credit
        .iter()
        .any(|credit| track_artists.contains(&normalize(&credit.artist.name)))
    {
        score += 0.30;
    }
    if let (Some(expected), Some(actual)) = (track.duration_ms, candidate.length)
        && (i64::from(expected) - i64::from(actual)).abs() <= 3_000
    {
        score += 0.15;
    }
    score
}

async fn persist_match(
    database: &Database,
    track: &TrackInput,
    lookup: &CachedLookup,
    decision: &MatchDecision,
) -> Result<usize> {
    let mut transaction = database.pool().begin().await?;
    sqlx::query(
        "INSERT INTO track_enrichment_matches
         (track_id, source, parser_version, lookup_id, status, source_entity_id,
          candidate_count, confidence, metadata, resolved_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now())
         ON CONFLICT (track_id, source, parser_version)
         DO UPDATE SET lookup_id = EXCLUDED.lookup_id, status = EXCLUDED.status,
             source_entity_id = EXCLUDED.source_entity_id,
             candidate_count = EXCLUDED.candidate_count,
             confidence = EXCLUDED.confidence, metadata = EXCLUDED.metadata,
             resolved_at = now()",
    )
    .bind(track.id)
    .bind(SOURCE)
    .bind(PARSER_VERSION)
    .bind(lookup.id)
    .bind(decision.status)
    .bind(decision.selected.as_ref().map(|recording| &recording.id))
    .bind(as_i32(decision.candidate_count)?)
    .bind(decision.confidence)
    .bind(json!({ "lookup_kind": "isrc", "lookup_value": &track.isrc }))
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "DELETE FROM track_semantic_facts
         WHERE track_id = $1 AND source = $2 AND parser_version = $3",
    )
    .bind(track.id)
    .bind(SOURCE)
    .bind(PARSER_VERSION)
    .execute(&mut *transaction)
    .await?;

    let mut written = 0;
    if let Some(recording) = &decision.selected {
        for fact in facts(recording, decision.confidence.unwrap_or(1.0)) {
            sqlx::query(
                "INSERT INTO track_semantic_facts
                 (track_id, match_track_id, source, parser_version, source_entity_id,
                  fact_kind, value, normalized_value, weight, confidence, provenance)
                 VALUES ($1, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 ON CONFLICT DO NOTHING",
            )
            .bind(track.id)
            .bind(SOURCE)
            .bind(PARSER_VERSION)
            .bind(&recording.id)
            .bind(fact.kind)
            .bind(fact.value)
            .bind(fact.normalized_value)
            .bind(fact.weight)
            .bind(fact.confidence)
            .bind(fact.provenance)
            .execute(&mut *transaction)
            .await?;
            written += 1;
        }
    }
    transaction.commit().await?;
    Ok(written)
}

fn facts(recording: &MbRecording, match_confidence: f64) -> Vec<Fact> {
    let mut facts = Vec::new();
    for genre in &recording.genres {
        push_fact(
            &mut facts,
            "genre",
            &genre.name,
            genre.count.max(0) as f64,
            match_confidence,
            json!({ "entity": "recording", "recording_mbid": recording.id }),
        );
    }
    for tag in &recording.tags {
        push_fact(
            &mut facts,
            "tag",
            &tag.name,
            tag.count.max(0) as f64,
            match_confidence * 0.9,
            json!({ "entity": "recording", "recording_mbid": recording.id }),
        );
    }
    let mut countries = BTreeSet::new();
    let mut languages = BTreeSet::new();
    for release in &recording.releases {
        if let Some(country) = release.country.as_deref()
            && countries.insert(country.to_owned())
        {
            push_fact(
                &mut facts,
                "release_country",
                country,
                1.0,
                match_confidence * 0.75,
                json!({ "entity": "release", "release_mbid": release.id }),
            );
        }
        if let Some(representation) = &release.text_representation
            && let Some(language) = representation.language.as_deref()
            && language != "zxx"
            && languages.insert(language.to_owned())
        {
            push_fact(
                &mut facts,
                "release_language",
                language,
                1.0,
                match_confidence * 0.65,
                json!({
                    "entity": "release",
                    "release_mbid": release.id,
                    "meaning": "release_title_language",
                    "script": representation.script
                }),
            );
        }
    }
    facts
}

fn push_fact(
    facts: &mut Vec<Fact>,
    kind: &'static str,
    value: &str,
    weight: f64,
    confidence: f64,
    provenance: Value,
) {
    let normalized_value = normalize(value);
    if normalized_value.is_empty()
        || facts
            .iter()
            .any(|fact| fact.kind == kind && fact.normalized_value == normalized_value)
    {
        return;
    }
    facts.push(Fact {
        kind,
        value: value.to_owned(),
        normalized_value,
        weight,
        confidence: confidence.clamp(0.0, 1.0),
        provenance,
    });
}

struct MusicBrainzClient {
    http: Client,
}

impl MusicBrainzClient {
    fn new() -> Result<Self> {
        let http = Client::builder()
            .user_agent(concat!(
                "Chordrift/",
                env!("CARGO_PKG_VERSION"),
                " (https://github.com/orbyts/chordrift)"
            ))
            .connect_timeout(Duration::from_secs(10))
            .timeout(REQUEST_TIMEOUT)
            .build()?;
        Ok(Self { http })
    }

    async fn fetch_isrc(&self, isrc: &str) -> Result<FetchedLookup> {
        if !isrc
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
        {
            return Err(ChordriftError::Configuration(
                "canonical track contains an invalid ISRC".to_owned(),
            ));
        }
        let url = format!("{API_ROOT}isrc/{isrc}");
        self.get_with_retry(url, &[("inc", "artist-credits"), ("fmt", "json")])
            .await
    }

    async fn fetch_recording(&self, recording_id: &str) -> Result<FetchedLookup> {
        Uuid::parse_str(recording_id).map_err(|_| {
            ChordriftError::Configuration(
                "MusicBrainz returned an invalid recording identity".to_owned(),
            )
        })?;
        let url = format!("{API_ROOT}recording/{recording_id}");
        self.get_with_retry(
            url,
            &[
                ("inc", "artist-credits+releases+genres+tags"),
                ("fmt", "json"),
            ],
        )
        .await
    }

    async fn get_with_retry(&self, url: String, query: &[(&str, &str)]) -> Result<FetchedLookup> {
        for attempt in 1..=MAX_TRANSIENT_ATTEMPTS {
            let response = self.http.get(&url).query(query).send().await?;
            let status = response.status();
            if !matches!(
                status,
                StatusCode::SERVICE_UNAVAILABLE | StatusCode::TOO_MANY_REQUESTS
            ) || attempt == MAX_TRANSIENT_ATTEMPTS
            {
                return response_outcome(response, attempt).await;
            }
            sleep(Duration::from_secs(1_u64 << attempt)).await;
        }
        unreachable!("bounded attempt loop always returns")
    }
}

async fn response_outcome(
    response: reqwest::Response,
    requests_made: usize,
) -> Result<FetchedLookup> {
    let status = response.status();
    if status == StatusCode::OK {
        return Ok(FetchedLookup {
            outcome: LookupOutcome::Response,
            http_status: status.as_u16(),
            response: Some(response.json().await?),
            error_class: None,
            requests_made,
        });
    }
    if status == StatusCode::NOT_FOUND {
        return Ok(FetchedLookup {
            outcome: LookupOutcome::NotFound,
            http_status: status.as_u16(),
            response: None,
            error_class: None,
            requests_made,
        });
    }
    Ok(FetchedLookup {
        outcome: LookupOutcome::Error,
        http_status: status.as_u16(),
        response: None,
        error_class: Some(if status == StatusCode::SERVICE_UNAVAILABLE {
            "service_unavailable"
        } else if status == StatusCode::TOO_MANY_REQUESTS {
            "rate_limited"
        } else if status.is_server_error() {
            "server_error"
        } else {
            "http_error"
        }),
        requests_made,
    })
}

async fn wait_for_rate_limit(last_request: &mut Option<Instant>) {
    if let Some(previous) = *last_request {
        let elapsed = Instant::now().duration_since(previous);
        if elapsed < REQUEST_INTERVAL {
            sleep(REQUEST_INTERVAL - elapsed).await;
        }
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

fn payload_hash(value: &Value) -> Result<String> {
    let bytes = serde_json::to_vec(value)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn normalize_title(value: &str) -> String {
    let lowercase = value.to_lowercase();
    let base = ["(feat.", "(feat ", "(featuring ", "[feat.", "[featuring "]
        .into_iter()
        .filter_map(|marker| lowercase.find(marker))
        .min()
        .map_or(value, |index| &value[..index]);
    normalize(base)
}

fn as_i32(value: usize) -> Result<i32> {
    i32::try_from(value).map_err(|_| {
        ChordriftError::Configuration("enrichment count exceeds PostgreSQL integer".to_owned())
    })
}

fn as_usize(value: i64) -> Result<usize> {
    usize::try_from(value).map_err(|_| {
        ChordriftError::Configuration("database contains a negative enrichment count".to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::{
        CachedLookup, LookupOutcome, MbArtist, MbArtistCredit, MbRecording, MbResponse, TrackInput,
        candidate_score, decide_match, facts, normalize, normalize_title,
    };
    use serde_json::to_value;
    use uuid::Uuid;

    #[test]
    fn candidate_scoring_requires_identity_evidence() {
        let track = track();
        let candidate = recording(
            "recording",
            "Leaving Hope",
            "Nine Inch Nails",
            Some(216_000),
        );
        assert!((candidate_score(&track, &candidate) - 1.0).abs() < f64::EPSILON);
        let unrelated = recording("other", "Leaving Hope", "Other Artist", Some(216_000));
        assert!((candidate_score(&track, &unrelated) - 0.70).abs() < f64::EPSILON);
    }

    #[test]
    fn duplicate_isrc_candidates_remain_ambiguous_without_a_clear_margin() {
        let response = MbResponse {
            recordings: vec![
                recording("one", "Leaving Hope", "Nine Inch Nails", Some(216_000)),
                recording("two", "Leaving Hope", "Nine Inch Nails", Some(216_000)),
            ],
        };
        let lookup = CachedLookup {
            id: Uuid::new_v4(),
            outcome: LookupOutcome::Response,
            response: Some(to_value(response).expect("fixture serializes")),
        };
        let decision = decide_match(&track(), &lookup).expect("decision succeeds");
        assert_eq!(decision.status, "ambiguous");
        assert!(decision.selected.is_none());
    }

    #[test]
    fn release_language_is_not_claimed_as_vocal_language() {
        let mut recording = recording("recording", "Leaving Hope", "Nine Inch Nails", None);
        recording.releases.push(super::MbRelease {
            id: "release".to_owned(),
            country: Some("US".to_owned()),
            text_representation: Some(super::MbTextRepresentation {
                language: Some("eng".to_owned()),
                script: Some("Latn".to_owned()),
            }),
        });
        let facts = facts(&recording, 1.0);
        let language = facts
            .iter()
            .find(|fact| fact.kind == "release_language")
            .expect("language fact exists");
        assert_eq!(language.provenance["meaning"], "release_title_language");
    }

    #[test]
    fn normalization_is_stable_across_punctuation_and_case() {
        assert_eq!(normalize("A. R. Rahman"), normalize("a-r rahman"));
    }

    #[test]
    fn featured_artist_suffix_does_not_change_recording_title() {
        assert_eq!(
            normalize_title("Earnestly Yours (feat. Ren Ford)"),
            normalize_title("Earnestly Yours")
        );
        assert_ne!(
            normalize_title("Song (Instrumental)"),
            normalize_title("Song")
        );
    }

    fn track() -> TrackInput {
        TrackInput {
            id: Uuid::new_v4(),
            title: "Leaving Hope".to_owned(),
            duration_ms: Some(216_000),
            isrc: "USIR10211552".to_owned(),
            artists: vec!["Nine Inch Nails".to_owned()],
        }
    }

    fn recording(id: &str, title: &str, artist: &str, length: Option<i32>) -> MbRecording {
        MbRecording {
            id: id.to_owned(),
            title: title.to_owned(),
            length,
            artist_credit: vec![MbArtistCredit {
                artist: MbArtist {
                    name: artist.to_owned(),
                },
            }],
            releases: Vec::new(),
            genres: Vec::new(),
            tags: Vec::new(),
        }
    }
}
