//! Durable, zero-signal routing playlists for ongoing listening review.

use std::{fs, path::Path};

use sha2::{Digest, Sha256};
use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result};

const PREFIX: &str = "Route — ";
const REEVALUATE_NAME: &str = "Re-evaluate";
const REEVALUATE_KEY: &str = "re-evaluate";

/// One configured routing surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteRecord {
    /// Stable Neon playlist identity.
    pub playlist_id: Uuid,
    /// Stable account-scoped routing key.
    pub stable_key: String,
    /// Provider-facing playlist name.
    pub name: String,
    /// Human-readable routing instructions.
    pub description: String,
    /// Retained label-free artwork master.
    pub background_path: String,
    /// Deterministically labeled provider artwork.
    pub artwork_path: String,
    /// Approved artwork digest.
    pub artwork_sha256: String,
    /// Current Spotify playlist ID after publication, if present.
    pub spotify_playlist_id: Option<String>,
    /// Desired membership retained in Neon.
    pub track_count: i64,
    /// Whether new plans should manage this route.
    pub active: bool,
}

/// One track retained in a routing surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteTrackRecord {
    /// One-based display position.
    pub position: i32,
    /// Canonical title.
    pub title: String,
    /// Display artist list.
    pub artists: String,
    /// Stable Spotify track ID.
    pub spotify_track_id: String,
}

/// Result of adding tracks to a route.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddReport {
    /// Route receiving the tracks.
    pub route: RouteRecord,
    /// New desired memberships inserted.
    pub added: usize,
    /// Existing memberships reused idempotently.
    pub reused: usize,
}

/// Result of retiring the obsolete multi-route review workflow in Neon.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetireLegacyReport {
    /// Legacy routes made inactive.
    pub routes: usize,
    /// Distinct route tracks already represented or explicitly excluded.
    pub tracks: usize,
}

/// Creates one durable route without contacting Spotify.
pub async fn create(
    database: &Database,
    account_label: &str,
    name: &str,
    description: &str,
    background_path: &Path,
    artwork_path: &Path,
) -> Result<RouteRecord> {
    let name = normalized_name(name)?;
    let stable_key = stable_key(&name);
    create_surface(
        database,
        account_label,
        SurfaceSpec {
            name: &name,
            stable_key: &stable_key,
            purpose: "legacy_route",
            description,
            background_path,
            artwork_path,
        },
    )
    .await
}

/// Creates or updates the account's single provider-native Re-evaluate queue.
pub async fn create_reevaluate(
    database: &Database,
    account_label: &str,
    description: &str,
    background_path: &Path,
    artwork_path: &Path,
) -> Result<RouteRecord> {
    create_surface(
        database,
        account_label,
        SurfaceSpec {
            name: REEVALUATE_NAME,
            stable_key: REEVALUATE_KEY,
            purpose: "reevaluate",
            description,
            background_path,
            artwork_path,
        },
    )
    .await
}

struct SurfaceSpec<'a> {
    name: &'a str,
    stable_key: &'a str,
    purpose: &'a str,
    description: &'a str,
    background_path: &'a Path,
    artwork_path: &'a Path,
}

async fn create_surface(
    database: &Database,
    account_label: &str,
    spec: SurfaceSpec<'_>,
) -> Result<RouteRecord> {
    let SurfaceSpec {
        name,
        stable_key,
        purpose,
        description,
        background_path,
        artwork_path,
    } = spec;
    let description = description.trim();
    if description.is_empty() {
        return Err(configuration("route description cannot be empty"));
    }
    let account_id = account_id(database, account_label).await?;
    let background_path = checked_png_path(background_path, "route background")?;
    let artwork_path = checked_png_path(artwork_path, "route artwork")?;
    let artwork_sha256 = sha256_file(Path::new(&artwork_path))?;

    let mut tx = database.pool().begin().await?;
    if let Some(existing_id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT playlist_id FROM routing_surfaces
         WHERE provider_account_id = $1 AND stable_key = $2",
    )
    .bind(account_id)
    .bind(stable_key)
    .fetch_optional(&mut *tx)
    .await?
    {
        sqlx::query(
            "UPDATE playlists SET name = $2, description = $3, updated_at = now()
             WHERE id = $1",
        )
        .bind(existing_id)
        .bind(name)
        .bind(description)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE routing_surfaces SET background_path = $3, artwork_path = $4,
                 artwork_sha256 = $5, artwork_approved_at = now(), active = TRUE,
                 purpose = $6, updated_at = now()
             WHERE provider_account_id = $1 AND playlist_id = $2",
        )
        .bind(account_id)
        .bind(existing_id)
        .bind(&background_path)
        .bind(&artwork_path)
        .bind(&artwork_sha256)
        .bind(purpose)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return resolve_exact(database, account_id, name).await;
    }

    let playlist_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlists (name, description, kind, machine_label, machine_tags)
         VALUES ($1, $2, 'routing', $3, '[\"routing\", \"zero_signal\"]'::jsonb)
         RETURNING id",
    )
    .bind(name)
    .bind(description)
    .bind(stable_key)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO routing_surfaces
         (provider_account_id, playlist_id, stable_key, background_path,
          artwork_path, artwork_sha256, purpose)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(account_id)
    .bind(playlist_id)
    .bind(stable_key)
    .bind(&background_path)
    .bind(&artwork_path)
    .bind(&artwork_sha256)
    .bind(purpose)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    resolve_exact(database, account_id, name).await
}

/// Returns the configured Re-evaluate queue.
pub async fn reevaluate(database: &Database, account_label: &str) -> Result<RouteRecord> {
    let account_id = account_id(database, account_label).await?;
    resolve_exact(database, account_id, REEVALUATE_NAME).await
}

/// Retires every legacy route only after the replacement queue exists and every
/// routed track is represented by the current proposal or a durable exclusion.
pub async fn retire_legacy(
    database: &Database,
    account_label: &str,
    confirm: &str,
) -> Result<RetireLegacyReport> {
    const PHRASE: &str = "RETIRE LEGACY ROUTES";
    if confirm != PHRASE {
        return Err(configuration(format!(
            "legacy route retirement requires --confirm {PHRASE:?}"
        )));
    }
    let account_id = account_id(database, account_label).await?;
    let replacement_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM routing_surfaces
         WHERE provider_account_id = $1 AND active AND purpose = 'reevaluate')",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    if !replacement_exists {
        return Err(configuration(
            "create the Re-evaluate queue before retiring legacy routes",
        ));
    }
    let generation_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM playlist_generations
         WHERE provider_account_id = $1 AND status IN ('proposed', 'approved')
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration("no current playlist proposal exists"))?;
    let uncovered: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT membership.track_id)::bigint
         FROM routing_surfaces route
         JOIN playlist_tracks membership ON membership.playlist_id = route.playlist_id
         WHERE route.provider_account_id = $1 AND route.active
           AND route.purpose = 'legacy_route'
           AND NOT EXISTS (
               SELECT 1 FROM playlists proposed
               JOIN playlist_tracks placed ON placed.playlist_id = proposed.id
               WHERE proposed.generation_id = $2
                 AND placed.track_id = membership.track_id)
           AND NOT EXISTS (
               SELECT 1 FROM excluded_tracks exclusion
               WHERE exclusion.provider_account_id = $1
                 AND exclusion.track_id = membership.track_id
                 AND exclusion.restored_at IS NULL)",
    )
    .bind(account_id)
    .bind(generation_id)
    .fetch_one(database.pool())
    .await?;
    if uncovered != 0 {
        return Err(configuration(format!(
            "cannot retire legacy routes: {uncovered} routed track(s) are neither represented nor excluded"
        )));
    }
    let tracks: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT membership.track_id)::bigint
         FROM routing_surfaces route
         JOIN playlist_tracks membership ON membership.playlist_id = route.playlist_id
         WHERE route.provider_account_id = $1 AND route.active
           AND route.purpose = 'legacy_route'",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let changed = sqlx::query(
        "UPDATE routing_surfaces SET active = FALSE, updated_at = now()
         WHERE provider_account_id = $1 AND active AND purpose = 'legacy_route'",
    )
    .bind(account_id)
    .execute(database.pool())
    .await?
    .rows_affected();
    Ok(RetireLegacyReport {
        routes: usize::try_from(changed)
            .map_err(|_| configuration("legacy route count exceeds usize"))?,
        tracks: usize::try_from(tracks)
            .map_err(|_| configuration("legacy route track count exceeds usize"))?,
    })
}

/// Lists configured routes, including routes not yet published to Spotify.
pub async fn list(database: &Database, account_label: &str) -> Result<Vec<RouteRecord>> {
    let account_id = account_id(database, account_label).await?;
    let rows = route_query(database, account_id, None).await?;
    rows.into_iter().map(route_record).collect()
}

/// Lists desired Neon membership for one route.
pub async fn tracks(
    database: &Database,
    account_label: &str,
    route_name: &str,
) -> Result<(RouteRecord, Vec<RouteTrackRecord>)> {
    let account_id = account_id(database, account_label).await?;
    let route = resolve(database, account_id, route_name).await?;
    let rows = sqlx::query(
        "SELECT membership.position, track.title,
                string_agg(artist.name, ', ' ORDER BY credit.position) AS artists,
                provider.provider_track_id
         FROM playlist_tracks membership
         JOIN tracks track ON track.id = membership.track_id
         JOIN provider_tracks provider ON provider.track_id = track.id
              AND provider.provider = 'spotify'
         LEFT JOIN track_artists credit ON credit.track_id = track.id
         LEFT JOIN artists artist ON artist.id = credit.artist_id
         WHERE membership.playlist_id = $1
         GROUP BY membership.position, track.title, provider.provider_track_id
         ORDER BY membership.position",
    )
    .bind(route.playlist_id)
    .fetch_all(database.pool())
    .await?;
    let tracks = rows
        .into_iter()
        .map(|row| {
            Ok(RouteTrackRecord {
                position: row.try_get::<i32, _>("position")? + 1,
                title: row.try_get("title")?,
                artists: row
                    .try_get::<Option<String>, _>("artists")?
                    .unwrap_or_default(),
                spotify_track_id: row.try_get("provider_track_id")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((route, tracks))
}

/// Adds known Spotify tracks to one route without contacting Spotify.
pub async fn add(
    database: &Database,
    account_label: &str,
    route_name: &str,
    spotify_track_ids: &[String],
    reason: Option<&str>,
) -> Result<AddReport> {
    if spotify_track_ids.is_empty() {
        return Err(configuration("provide at least one --spotify-id"));
    }
    let account_id = account_id(database, account_label).await?;
    let route = resolve(database, account_id, route_name).await?;
    let mut tx = database.pool().begin().await?;
    let mut next_position: i32 = sqlx::query_scalar(
        "SELECT COALESCE(max(position) + 1, 0) FROM playlist_tracks WHERE playlist_id = $1",
    )
    .bind(route.playlist_id)
    .fetch_one(&mut *tx)
    .await?;
    let mut added = 0;
    let mut reused = 0;
    for spotify_id in spotify_track_ids {
        let track_id: Uuid = sqlx::query_scalar(
            "SELECT provider.track_id
             FROM provider_tracks provider
             WHERE provider.provider = 'spotify' AND provider.provider_track_id = $1
               AND account_track_is_library_candidate($2, provider.track_id)
             ORDER BY provider.updated_at DESC LIMIT 1",
        )
        .bind(spotify_id)
        .bind(account_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| configuration(format!(
            "Spotify track {spotify_id:?} is not known in this account's preservation inventory; run `chordrift sync pull` first"
        )))?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM playlist_tracks
             WHERE playlist_id = $1 AND track_id = $2)",
        )
        .bind(route.playlist_id)
        .bind(track_id)
        .fetch_one(&mut *tx)
        .await?;
        if exists {
            reused += 1;
            continue;
        }
        sqlx::query(
            "INSERT INTO playlist_tracks
             (playlist_id, track_id, position, source, provenance)
             VALUES ($1, $2, $3, 'manual', jsonb_build_object(
                 'captured_via', 'chordrift_routes_add',
                 'reason', $4::text,
                 'spotify_track_id', $5::text
             ))",
        )
        .bind(route.playlist_id)
        .bind(track_id)
        .bind(next_position)
        .bind(reason.map(str::trim).filter(|value| !value.is_empty()))
        .bind(spotify_id)
        .execute(&mut *tx)
        .await?;
        next_position += 1;
        added += 1;
    }
    tx.commit().await?;
    let route = resolve(database, account_id, &route.name).await?;
    Ok(AddReport {
        route,
        added,
        reused,
    })
}

async fn account_id(database: &Database, account_label: &str) -> Result<Uuid> {
    sqlx::query_scalar(
        "SELECT id FROM provider_accounts
         WHERE provider = 'spotify' AND account_label = $1",
    )
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| configuration("Spotify account is not imported"))
}

async fn resolve(database: &Database, account_id: Uuid, selector: &str) -> Result<RouteRecord> {
    let normalized = if selector.trim().starts_with(PREFIX) {
        selector.trim().to_owned()
    } else {
        format!("{PREFIX}{}", selector.trim())
    };
    let rows = route_query(database, account_id, Some(&normalized)).await?;
    match rows.len() {
        0 => Err(configuration(format!("no route matches {selector:?}"))),
        1 => route_record(rows.into_iter().next().expect("one route")),
        _ => Err(configuration(format!(
            "route selector {selector:?} is ambiguous"
        ))),
    }
}

async fn resolve_exact(database: &Database, account_id: Uuid, name: &str) -> Result<RouteRecord> {
    let rows = route_query(database, account_id, Some(name)).await?;
    match rows.len() {
        0 => Err(configuration(format!("no review surface matches {name:?}"))),
        1 => route_record(rows.into_iter().next().expect("one route")),
        _ => Err(configuration(format!(
            "review surface {name:?} is ambiguous"
        ))),
    }
}

async fn route_query(
    database: &Database,
    account_id: Uuid,
    name: Option<&str>,
) -> Result<Vec<sqlx::postgres::PgRow>> {
    sqlx::query(
        "SELECT route.playlist_id, route.stable_key, playlist.name,
                COALESCE(playlist.description, '') AS description,
                route.background_path, route.artwork_path, route.artwork_sha256,
                route.active, provider.provider_playlist_id AS spotify_playlist_id,
                count(membership.id)::bigint AS track_count
         FROM routing_surfaces route
         JOIN playlists playlist ON playlist.id = route.playlist_id
         LEFT JOIN provider_playlists provider
           ON provider.playlist_id = route.playlist_id AND provider.provider = 'spotify'
         LEFT JOIN playlist_tracks membership ON membership.playlist_id = route.playlist_id
         WHERE route.provider_account_id = $1
           AND ($2::text IS NULL OR lower(playlist.name) = lower($2))
         GROUP BY route.playlist_id, route.stable_key, playlist.name,
                  playlist.description, route.background_path, route.artwork_path,
                  route.artwork_sha256, route.active, provider.provider_playlist_id
         ORDER BY lower(playlist.name), route.playlist_id",
    )
    .bind(account_id)
    .bind(name)
    .fetch_all(database.pool())
    .await
    .map_err(Into::into)
}

fn route_record(row: sqlx::postgres::PgRow) -> Result<RouteRecord> {
    Ok(RouteRecord {
        playlist_id: row.try_get("playlist_id")?,
        stable_key: row.try_get("stable_key")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        background_path: row.try_get("background_path")?,
        artwork_path: row.try_get("artwork_path")?,
        artwork_sha256: row.try_get("artwork_sha256")?,
        spotify_playlist_id: row.try_get("spotify_playlist_id")?,
        track_count: row.try_get("track_count")?,
        active: row.try_get("active")?,
    })
}

fn normalized_name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(configuration("route name cannot be empty"));
    }
    let suffix = value.strip_prefix(PREFIX).unwrap_or(value).trim();
    if suffix.is_empty() {
        return Err(configuration(
            "route name must include a label after `Route —`",
        ));
    }
    Ok(format!("{PREFIX}{suffix}"))
}

fn stable_key(name: &str) -> String {
    let suffix = name.strip_prefix(PREFIX).unwrap_or(name);
    let mut key = String::from("route-");
    let mut separator = false;
    for character in suffix.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            key.push(character);
            separator = false;
        } else if !separator && !key.ends_with('-') {
            key.push('-');
            separator = true;
        }
    }
    key.trim_end_matches('-').to_owned()
}

fn checked_png_path(path: &Path, label: &str) -> Result<String> {
    let canonical = path.canonicalize().map_err(|error| {
        configuration(format!(
            "{label} {} is unavailable: {error}",
            path.display()
        ))
    })?;
    if canonical.extension().and_then(|value| value.to_str()) != Some("png") {
        return Err(configuration(format!("{label} must be a PNG")));
    }
    let bytes = fs::read(&canonical)?;
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(configuration(format!("{label} is not a valid PNG")));
    }
    Ok(canonical.to_string_lossy().into_owned())
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn configuration(message: impl Into<String>) -> ChordriftError {
    ChordriftError::Configuration(message.into())
}

#[cfg(test)]
mod tests {
    use super::{normalized_name, stable_key};

    #[test]
    fn route_names_are_prefixed_and_keys_are_stable() {
        let name = normalized_name("South Indian").expect("valid route");
        assert_eq!(name, "Route — South Indian");
        assert_eq!(stable_key(&name), "route-south-indian");
        assert_eq!(normalized_name(&name).expect("idempotent"), name);
    }
}
