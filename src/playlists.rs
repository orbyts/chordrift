//! Account-scoped provider playlist roles and drift policy.

use std::collections::HashSet;

use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result};

/// How Chordrift treats a provider playlist in the orchestration workflow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaylistRole {
    /// Provider-owned playlist mirrored into Neon without remote management.
    Observed,
    /// Provider-native discovery surface intended for later consumption.
    Inbox,
    /// Canonical playlist whose approved desired state will be owned by Neon.
    Managed,
}

impl PlaylistRole {
    /// Stable database representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Observed => "observed",
            Self::Inbox => "inbox",
            Self::Managed => "managed",
        }
    }
}

/// Which side wins when a provider playlist differs from approved Neon state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriftPolicy {
    /// Import provider edits into Neon.
    ProviderWins,
    /// Restore approved Neon state to the provider during a future apply operation.
    NeonWins,
    /// Require an explicit decision before either side is changed.
    Manual,
}

impl DriftPolicy {
    /// Stable database representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProviderWins => "provider_wins",
            Self::NeonWins => "neon_wins",
            Self::Manual => "manual",
        }
    }
}

/// How a playlist contributes evidence without conflating sync authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaylistSignalClass {
    /// A protected user-created playlist retained without Chordrift ownership.
    UserManaged,
    /// A user-curated legacy playlist whose membership and name describe vibe.
    SemanticLegacy,
    /// A Spotify-owned surface observed for behavioral evidence.
    ProviderCurated,
    /// A user-owned temporary intake that is cleared only after verified placement.
    Intake,
    /// A Chordrift-managed output; previous assignments are stability evidence only.
    Canonical,
    /// Temporary provider-transfer infrastructure with no library meaning.
    Transport,
    /// A playlist excluded from semantic and behavioral analysis.
    Ignored,
}

impl PlaylistSignalClass {
    /// Stable database representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UserManaged => "user_managed",
            Self::SemanticLegacy => "semantic_legacy",
            Self::ProviderCurated => "provider_curated",
            Self::Intake => "intake",
            Self::Canonical => "canonical",
            Self::Transport => "transport",
            Self::Ignored => "ignored",
        }
    }
}

/// Optional behavioral evidence supplied by a provider-curated or intake playlist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BehavioralSignal {
    /// Current high-rotation evidence such as Spotify On Repeat.
    Rotation,
    /// Provider discovery evidence such as Discover Weekly.
    Discovery,
    /// Explicit prompted-interest evidence.
    Prompted,
    /// Social or friend recommendation provenance.
    Recommendation,
}

impl BehavioralSignal {
    /// Stable database representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rotation => "rotation",
            Self::Discovery => "discovery",
            Self::Prompted => "prompted",
            Self::Recommendation => "recommendation",
        }
    }
}

/// When Chordrift may clear a user-owned intake playlist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClearPolicy {
    /// Never clear this playlist automatically.
    Never,
    /// Clear entries only after canonical placement is published and verified.
    AfterVerifiedAssignment,
}

impl ClearPolicy {
    /// Stable database representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::AfterVerifiedAssignment => "after_verified_assignment",
        }
    }
}

/// One account-scoped playlist configuration.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistRecord {
    /// Spotify playlist ID.
    pub provider_playlist_id: String,
    /// Most recently imported name.
    pub name: String,
    /// Orchestration role.
    pub role: String,
    /// Configured drift policy.
    pub drift_policy: String,
    /// Evidence class, independent of role and drift authority.
    pub signal_class: String,
    /// Optional behavioral evidence produced by membership.
    pub behavioral_signal: Option<String>,
    /// Relative semantic contribution; zero excludes playlist co-membership.
    pub semantic_weight: f64,
    /// When a temporary intake may be cleared.
    pub clear_policy: String,
    /// Whether it exists in the latest imported snapshot.
    pub present: bool,
    /// Item count reported by the latest snapshot, when present.
    pub total_items: Option<i32>,
}

/// One ordered track entry from a playlist's latest imported snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaylistTrackRecord {
    /// Zero-based provider position retained in Neon.
    pub position: i32,
    /// Canonical track title.
    pub title: String,
    /// Ordered display artist string.
    pub artists: String,
    /// Canonical album title, when Spotify supplied one.
    pub album: Option<String>,
    /// Stable Spotify track ID.
    pub provider_track_id: String,
}

/// Current ordered contents of one account-scoped playlist.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistTracks {
    /// Playlist resolved from the user selector.
    pub playlist: PlaylistRecord,
    /// Immutable library snapshot supplying the entries.
    pub snapshot_id: Uuid,
    /// Ordered entries. Canonical duplicates remain separate rows.
    pub tracks: Vec<PlaylistTrackRecord>,
}

/// Result of changing non-destructive retirement intent for user playlists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetirementPolicyReport {
    /// User playlists now explicitly eligible for a future retirement plan.
    pub retirement_candidates: usize,
    /// User playlists explicitly protected from retirement.
    pub protected_playlists: usize,
    /// Rows whose policy changed in this command.
    pub changed: usize,
}

/// Selects one playlist without relying exclusively on a mutable display name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlaylistSelector {
    /// Select by exact Spotify playlist ID.
    ProviderId(String),
    /// Select by case-insensitive current name; the match must be unambiguous.
    Name(String),
}

/// Lists only playlists present in the account's latest imported snapshot.
pub async fn list(database: &Database, account_label: &str) -> Result<Vec<PlaylistRecord>> {
    let account_id = account_id(database, account_label).await?;
    let rows = sqlx::query(
        "SELECT spotify_playlist_id AS provider_playlist_id, name,
                role, drift_policy, signal_class, behavioral_signal,
                semantic_weight, clear_policy, TRUE AS present_in_latest_snapshot,
                total_items
         FROM current_spotify_playlists
         WHERE provider_account_id = $1
         ORDER BY lower(name), spotify_playlist_id",
    )
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(PlaylistRecord {
                provider_playlist_id: row.try_get("provider_playlist_id")?,
                name: row.try_get("name")?,
                role: row.try_get("role")?,
                drift_policy: row.try_get("drift_policy")?,
                signal_class: row.try_get("signal_class")?,
                behavioral_signal: row.try_get("behavioral_signal")?,
                semantic_weight: row.try_get("semantic_weight")?,
                clear_policy: row.try_get("clear_policy")?,
                present: row.try_get("present_in_latest_snapshot")?,
                total_items: row.try_get("total_items")?,
            })
        })
        .collect()
}

/// Configures one playlist's independently modeled evidence policy.
pub async fn configure_signals(
    database: &Database,
    account_label: &str,
    selector: &PlaylistSelector,
    signal_class: PlaylistSignalClass,
    behavioral_signal: Option<BehavioralSignal>,
    semantic_weight: Option<f64>,
    clear_policy: Option<ClearPolicy>,
) -> Result<PlaylistRecord> {
    let selected = resolve_selector(database, account_label, selector).await?;
    let semantic_weight = match signal_class {
        PlaylistSignalClass::SemanticLegacy => semantic_weight.unwrap_or({
            if selected.semantic_weight > 0.0 {
                selected.semantic_weight
            } else {
                1.0
            }
        }),
        _ => {
            if semantic_weight.is_some_and(|weight| weight != 0.0) {
                return Err(ChordriftError::Configuration(
                    "only semantic-legacy playlists may have a non-zero semantic weight".to_owned(),
                ));
            }
            0.0
        }
    };
    if !semantic_weight.is_finite() || !(0.0..=10.0).contains(&semantic_weight) {
        return Err(ChordriftError::Configuration(
            "playlist semantic weight must be between 0 and 10".to_owned(),
        ));
    }
    if behavioral_signal.is_some()
        && !matches!(
            signal_class,
            PlaylistSignalClass::ProviderCurated | PlaylistSignalClass::Intake
        )
    {
        return Err(ChordriftError::Configuration(
            "behavioral signals require a provider-curated or intake playlist".to_owned(),
        ));
    }
    let clear_policy = clear_policy.unwrap_or(match signal_class {
        PlaylistSignalClass::Intake => ClearPolicy::AfterVerifiedAssignment,
        _ => ClearPolicy::Never,
    });
    if clear_policy == ClearPolicy::AfterVerifiedAssignment
        && signal_class != PlaylistSignalClass::Intake
    {
        return Err(ChordriftError::Configuration(
            "only intake playlists may clear after verified assignment".to_owned(),
        ));
    }
    let account_id = account_id(database, account_label).await?;
    sqlx::query(
        "UPDATE provider_account_playlists account_playlist
         SET signal_class = $3, behavioral_signal = $4,
             semantic_weight = $5, clear_policy = $6, updated_at = now()
         FROM provider_playlists provider
         WHERE account_playlist.provider_account_id = $1
           AND account_playlist.provider_playlist_id = provider.id
           AND provider.provider_playlist_id = $2",
    )
    .bind(account_id)
    .bind(&selected.provider_playlist_id)
    .bind(signal_class.as_str())
    .bind(behavioral_signal.map(BehavioralSignal::as_str))
    .bind(semantic_weight)
    .bind(clear_policy.as_str())
    .execute(database.pool())
    .await?;
    Ok(PlaylistRecord {
        signal_class: signal_class.as_str().to_owned(),
        behavioral_signal: behavioral_signal.map(|signal| signal.as_str().to_owned()),
        semantic_weight,
        clear_policy: clear_policy.as_str().to_owned(),
        ..selected
    })
}

/// Changes retirement intent without creating or executing a Spotify plan.
///
/// Newly imported user playlists are protected. `include` marks only named
/// playlists as legacy candidates, `all` marks every eligible user playlist
/// except the named exclusions, and `none` protects every eligible playlist.
pub async fn configure_retirement(
    database: &Database,
    account_label: &str,
    include: &[String],
    all: bool,
    except: &[String],
    none: bool,
) -> Result<RetirementPolicyReport> {
    let modes = usize::from(!include.is_empty()) + usize::from(all) + usize::from(none);
    if modes != 1 || (!all && !except.is_empty()) {
        return Err(ChordriftError::Configuration(
            "choose exactly one retirement mode: --include NAME (repeatable), --all [--except NAME], or --none"
                .to_owned(),
        ));
    }
    let account_id = account_id(database, account_label).await?;
    let rows = sqlx::query(
        "SELECT policy.provider_playlist_id, lower(snapshot.name) AS normalized_name,
                policy.signal_class
         FROM provider_account_playlists policy
         JOIN provider_playlists provider ON provider.id = policy.provider_playlist_id
         JOIN provider_accounts account ON account.id = policy.provider_account_id
         JOIN LATERAL (
             SELECT item.name, item.metadata
             FROM provider_playlist_snapshots item
             JOIN provider_library_snapshots library ON library.id = item.snapshot_id
             WHERE item.provider_playlist_id = provider.id
               AND library.provider_account_id = policy.provider_account_id
             ORDER BY library.captured_at DESC, library.id DESC LIMIT 1
         ) snapshot ON TRUE
         WHERE policy.provider_account_id = $1 AND policy.present_in_latest_snapshot
           AND provider.concept_id IS NULL
           AND snapshot.metadata->'owner'->>'id' = account.metadata->>'id'
           AND policy.signal_class NOT IN ('canonical', 'intake', 'provider_curated')",
    )
    .bind(account_id)
    .fetch_all(database.pool())
    .await?;
    let include = include
        .iter()
        .map(|name| name.trim().to_lowercase())
        .collect::<HashSet<_>>();
    let except = except
        .iter()
        .map(|name| name.trim().to_lowercase())
        .collect::<HashSet<_>>();
    let available = rows
        .iter()
        .map(|row| row.try_get::<String, _>("normalized_name"))
        .collect::<std::result::Result<HashSet<_>, _>>()?;
    let missing = include
        .iter()
        .chain(except.iter())
        .filter(|name| !available.contains(*name))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(ChordriftError::Configuration(format!(
            "retirement selectors did not match protected user playlists: {}",
            missing.join(", ")
        )));
    }
    let mut transaction = database.pool().begin().await?;
    let mut changed = 0_usize;
    let mut retirement_candidates = 0_usize;
    let mut protected_playlists = 0_usize;
    for row in rows {
        let provider_playlist_id: Uuid = row.try_get("provider_playlist_id")?;
        let name: String = row.try_get("normalized_name")?;
        let current: String = row.try_get("signal_class")?;
        let retire = if none {
            false
        } else if all {
            !except.contains(&name)
        } else {
            include.contains(&name)
        };
        let desired = if retire {
            retirement_candidates += 1;
            "semantic_legacy"
        } else {
            protected_playlists += 1;
            "user_managed"
        };
        if current != desired {
            sqlx::query(
                "UPDATE provider_account_playlists
                 SET signal_class = $3, behavioral_signal = NULL,
                     semantic_weight = CASE WHEN $3 = 'semantic_legacy' THEN 1.0 ELSE 0.0 END,
                     clear_policy = 'never', role = 'observed',
                     drift_policy = 'provider_wins', updated_at = now()
                 WHERE provider_account_id = $1 AND provider_playlist_id = $2",
            )
            .bind(account_id)
            .bind(provider_playlist_id)
            .bind(desired)
            .execute(&mut *transaction)
            .await?;
            changed += 1;
        }
    }
    transaction.commit().await?;
    Ok(RetirementPolicyReport {
        retirement_candidates,
        protected_playlists,
        changed,
    })
}

/// Lists the ordered tracks in one playlist's latest imported snapshot.
pub async fn tracks(
    database: &Database,
    account_label: &str,
    selector: &PlaylistSelector,
) -> Result<PlaylistTracks> {
    let account_id = account_id(database, account_label).await?;
    let playlist = resolve_selector(database, account_label, selector).await?;
    if !playlist.present {
        return Err(ChordriftError::Configuration(
            "playlist is not present in this account's latest imported snapshot".to_owned(),
        ));
    }
    let snapshot_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM provider_library_snapshots
         WHERE provider_account_id = $1
         ORDER BY captured_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_one(database.pool())
    .await?;
    let rows = sqlx::query(
        "SELECT membership.position, track.title,
                COALESCE(string_agg(artist.name, ', ' ORDER BY track_artist.position), '') AS artists,
                album.title AS album, provider_track.provider_track_id
         FROM provider_playlist_tracks membership
         JOIN provider_playlists provider_playlist
           ON provider_playlist.id = membership.provider_playlist_id
         JOIN provider_tracks provider_track
           ON provider_track.id = membership.provider_track_id
         JOIN tracks track ON track.id = provider_track.track_id
         LEFT JOIN albums album ON album.id = track.album_id
         LEFT JOIN track_artists track_artist ON track_artist.track_id = track.id
         LEFT JOIN artists artist ON artist.id = track_artist.artist_id
         WHERE membership.snapshot_id = $1
           AND provider_playlist.provider = 'spotify'
           AND provider_playlist.provider_playlist_id = $2
         GROUP BY membership.position, track.title, album.title,
                  provider_track.provider_track_id
         ORDER BY membership.position",
    )
    .bind(snapshot_id)
    .bind(&playlist.provider_playlist_id)
    .fetch_all(database.pool())
    .await?;
    let tracks = rows
        .into_iter()
        .map(|row| {
            Ok(PlaylistTrackRecord {
                position: row.try_get("position")?,
                title: row.try_get("title")?,
                artists: row.try_get("artists")?,
                album: row.try_get("album")?,
                provider_track_id: row.try_get("provider_track_id")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(PlaylistTracks {
        playlist,
        snapshot_id,
        tracks,
    })
}

/// Updates one playlist's role and drift policy.
pub async fn configure(
    database: &Database,
    account_label: &str,
    selector: &PlaylistSelector,
    role: PlaylistRole,
    drift_policy: DriftPolicy,
) -> Result<PlaylistRecord> {
    let account_id = account_id(database, account_label).await?;
    let selected = resolve_selector(database, account_label, selector).await?;
    sqlx::query(
        "UPDATE provider_account_playlists account_playlist
         SET role = $3, drift_policy = $4, updated_at = now()
         FROM provider_playlists provider
         WHERE account_playlist.provider_account_id = $1
           AND account_playlist.provider_playlist_id = provider.id
           AND provider.provider_playlist_id = $2",
    )
    .bind(account_id)
    .bind(&selected.provider_playlist_id)
    .bind(role.as_str())
    .bind(drift_policy.as_str())
    .execute(database.pool())
    .await?;
    Ok(PlaylistRecord {
        role: role.as_str().to_owned(),
        drift_policy: drift_policy.as_str().to_owned(),
        ..selected
    })
}

async fn resolve_selector(
    database: &Database,
    account_label: &str,
    selector: &PlaylistSelector,
) -> Result<PlaylistRecord> {
    let rows = list(database, account_label).await?;
    let matches: Vec<_> = rows
        .into_iter()
        .filter(|playlist| match selector {
            PlaylistSelector::ProviderId(id) => playlist.provider_playlist_id == *id,
            PlaylistSelector::Name(name) => playlist.name.eq_ignore_ascii_case(name),
        })
        .collect();
    let [selected] = matches.as_slice() else {
        return Err(ChordriftError::Configuration(if matches.is_empty() {
            "playlist selector did not match this account's imported playlists".to_owned()
        } else {
            "playlist name is ambiguous; select it by Spotify playlist ID".to_owned()
        }));
    };
    Ok(selected.clone())
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
