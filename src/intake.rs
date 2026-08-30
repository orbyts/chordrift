//! Read-only review of current provider intake against durable Chordrift state.

use sqlx::Row;
use storexa::Database;
use uuid::Uuid;

use crate::{ChordriftError, Result};

/// Explicit handling for one saved/liked intake item after verified placement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SavedTrackDisposition {
    /// Keep the track in the provider's saved/liked container as well.
    Preserve,
    /// Clear the saved/liked intake only after canonical placement is verified.
    ClearAfterVerifiedAssignment,
}

impl SavedTrackDisposition {
    /// Stable database and CLI label.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Preserve => "preserve",
            Self::ClearAfterVerifiedAssignment => "clear_after_verified_assignment",
        }
    }
}

/// One current provider intake item joined with Chordrift intent and history.
#[derive(Clone, Debug, PartialEq)]
pub struct IntakeItem {
    /// Stable provider track identity.
    pub spotify_id: String,
    /// Current display title.
    pub title: String,
    /// Current display artists.
    pub artists: String,
    /// Current provider intake surfaces containing the track.
    pub sources: Vec<String>,
    /// Exact high-level review state.
    pub state: IntakeState,
    /// Current provider-visible canonical Chordrift destinations.
    pub current_destinations: Vec<String>,
    /// Explicit keep-or-clear decision for the saved/liked source, when made.
    pub saved_track_disposition: Option<String>,
    /// Destinations in the latest proposal generation.
    pub proposal_destinations: Vec<String>,
    /// Latest proposal state when proposal destinations exist.
    pub proposal_state: Option<String>,
    /// Whether any exclusion exists in durable history.
    pub exclusion_history: bool,
    /// Active exclusion reason, when currently excluded.
    pub active_exclusion_reason: Option<String>,
    /// Normalized listening-evidence event count.
    pub listening_events: u64,
    /// Derived play count from normalized listening evidence.
    pub play_count: u64,
}

/// Operator-facing state for one current intake item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntakeState {
    /// The provider currently shows the track in a canonical Chordrift playlist.
    AlreadyCovered,
    /// The track was introduced through a managed destination and needs its
    /// already-observed placement recorded as canonical intent.
    DirectManagedAddition,
    /// A durable active exclusion exists and must be kept or explicitly restored.
    PreviouslyExcluded,
    /// The latest approved proposal assigns the track, but provider publication is pending.
    AssignedApproved,
    /// The latest editable proposal contains a proposed destination.
    SuggestedInDraft,
    /// Listening evidence exists, but no current or proposed destination does.
    KnownFromHistory,
    /// No listening evidence, exclusion, current destination, or proposal destination exists.
    GenuinelyNew,
}

impl IntakeState {
    /// Stable machine-readable label used by scripts and clients.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AlreadyCovered => "already_covered",
            Self::DirectManagedAddition => "direct_managed_addition",
            Self::PreviouslyExcluded => "previously_excluded",
            Self::AssignedApproved => "assigned_approved",
            Self::SuggestedInDraft => "suggested_in_draft",
            Self::KnownFromHistory => "known_from_history",
            Self::GenuinelyNew => "genuinely_new",
        }
    }
}

/// Complete current-provider intake audit for one account.
#[derive(Clone, Debug, PartialEq)]
pub struct IntakeAudit {
    /// Exact current provider snapshot joined by the report.
    pub snapshot_id: Uuid,
    /// Latest proposal generation, when one exists.
    pub proposal_generation_id: Option<Uuid>,
    /// Latest proposal state, when one exists.
    pub proposal_state: Option<String>,
    /// Current intake items in stable identity order.
    pub items: Vec<IntakeItem>,
}

/// Joins the exact current Spotify intake inventory with Chordrift state without writing.
pub async fn audit(database: &Database, account_label: &str) -> Result<IntakeAudit> {
    let account = sqlx::query(
        "SELECT account.id, inventory.source_snapshot_id
         FROM provider_accounts account
         JOIN provider_current_inventories inventory
           ON inventory.provider_account_id = account.id
         WHERE account.provider = 'spotify' AND account.account_label = $1",
    )
    .bind(account_label)
    .fetch_optional(database.pool())
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(format!(
            "account `{account_label}` has no current Spotify inventory; run `chordrift sync pull --account {account_label}`"
        ))
    })?;
    let account_id: Uuid = account.try_get("id")?;
    let snapshot_id: Option<Uuid> = account.try_get("source_snapshot_id")?;
    let snapshot_id = snapshot_id.ok_or_else(|| {
        ChordriftError::Configuration(format!(
            "account `{account_label}` has no current Spotify snapshot; run `chordrift sync pull --account {account_label}`"
        ))
    })?;

    let proposal = sqlx::query(
        "SELECT id, status FROM playlist_generations
         WHERE provider_account_id = $1
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(account_id)
    .fetch_optional(database.pool())
    .await?;
    let proposal_generation_id = proposal.as_ref().map(|row| row.try_get("id")).transpose()?;
    let proposal_state = proposal
        .as_ref()
        .map(|row| row.try_get("status"))
        .transpose()?;

    let rows = sqlx::query(
        "WITH intake_memberships AS (
             SELECT saved.provider_track_id, 'Liked Songs'::text AS source
             FROM provider_observed_saved_tracks saved
             WHERE saved.snapshot_id = $2
             UNION ALL
             SELECT membership.provider_track_id, current.name AS source
             FROM current_spotify_playlists current
             JOIN provider_observed_playlist_tracks membership
               ON membership.snapshot_id = current.snapshot_id
              AND membership.provider_playlist_id = current.provider_playlist_id
             WHERE current.provider_account_id = $1
               AND current.snapshot_id = $2
               AND current.signal_class = 'intake'
             UNION ALL
             SELECT membership.provider_track_id, current.name AS source
             FROM current_spotify_playlists current
             JOIN provider_observed_playlist_tracks membership
               ON membership.snapshot_id = current.snapshot_id
              AND membership.provider_playlist_id = current.provider_playlist_id
             WHERE current.provider_account_id = $1
               AND current.snapshot_id = $2
               AND current.signal_class = 'canonical'
         ), candidates AS (
             SELECT provider_track_id,
                    array_agg(DISTINCT source ORDER BY source) AS sources
             FROM intake_memberships
             GROUP BY provider_track_id
         )
         SELECT provider.provider_track_id AS spotify_id,
                track.id AS track_id,
                track.title,
                COALESCE(artists.names, '') AS artists,
                candidates.sources,
                COALESCE(current_destinations.names, ARRAY[]::text[]) AS current_destinations,
                COALESCE(proposal_destinations.names, ARRAY[]::text[]) AS proposal_destinations,
                EXISTS (
                    SELECT 1 FROM excluded_tracks historical_exclusion
                    WHERE historical_exclusion.provider_account_id = $1
                      AND historical_exclusion.track_id = track.id
                ) AS exclusion_history,
                active_exclusion.exclusion_reason,
                saved_disposition.disposition AS saved_track_disposition,
                COALESCE(statistics.event_count, 0)::bigint AS event_count,
                COALESCE(statistics.play_count, 0)::bigint AS play_count
         FROM candidates
         JOIN provider_tracks provider ON provider.id = candidates.provider_track_id
         JOIN tracks track ON track.id = provider.track_id
         LEFT JOIN LATERAL (
             SELECT string_agg(artist.name, ', ' ORDER BY track_artist.position) AS names
             FROM track_artists track_artist
             JOIN artists artist ON artist.id = track_artist.artist_id
             WHERE track_artist.track_id = track.id
         ) artists ON TRUE
         LEFT JOIN LATERAL (
             SELECT array_agg(DISTINCT current.name ORDER BY current.name) AS names
             FROM current_spotify_playlists current
             JOIN provider_observed_playlist_tracks membership
               ON membership.snapshot_id = current.snapshot_id
              AND membership.provider_playlist_id = current.provider_playlist_id
             JOIN provider_tracks current_track
               ON current_track.id = membership.provider_track_id
             WHERE current.provider_account_id = $1
               AND current.snapshot_id = $2
               AND current.signal_class = 'canonical'
               AND current_track.track_id = track.id
         ) current_destinations ON TRUE
         LEFT JOIN LATERAL (
             SELECT array_agg(DISTINCT playlist.name ORDER BY playlist.name) AS names
             FROM playlists playlist
             JOIN playlist_tracks membership ON membership.playlist_id = playlist.id
             WHERE playlist.generation_id = $3
               AND membership.track_id = track.id
         ) proposal_destinations ON $3 IS NOT NULL
         LEFT JOIN LATERAL (
             SELECT exclusion.exclusion_reason
             FROM excluded_tracks exclusion
             WHERE exclusion.provider_account_id = $1
               AND exclusion.track_id = track.id
               AND exclusion.restored_at IS NULL
             ORDER BY exclusion.excluded_at DESC, exclusion.id DESC LIMIT 1
         ) active_exclusion ON TRUE
         LEFT JOIN LATERAL (
             SELECT CASE directive.directive
                        WHEN 'include' THEN 'preserve'
                        WHEN 'exclude' THEN 'clear_after_verified_assignment'
                    END AS disposition
             FROM playlist_surfaces surface
             JOIN playlist_track_directives directive
               ON directive.chordrift_account_id = surface.chordrift_account_id
              AND directive.surface_id = surface.id
              AND directive.track_id = track.id
              AND directive.superseded_at IS NULL
             WHERE surface.chordrift_account_id = (
                       SELECT chordrift_account_id FROM provider_accounts WHERE id = $1
                   )
               AND surface.stable_key = 'provider-saved-tracks:' || $1::text
               AND surface.active
             ORDER BY directive.created_at DESC, directive.id DESC LIMIT 1
         ) saved_disposition ON TRUE
         LEFT JOIN LATERAL (
             SELECT sum(item.event_count)::bigint AS event_count,
                    sum(item.play_count)::bigint AS play_count
             FROM account_listening_track_statistics item
             WHERE item.provider_account_id = $1 AND item.track_id = track.id
         ) statistics ON TRUE
         ORDER BY lower(track.title), lower(COALESCE(artists.names, '')), provider.provider_track_id",
    )
    .bind(account_id)
    .bind(snapshot_id)
    .bind(proposal_generation_id)
    .fetch_all(database.pool())
    .await?;

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let current_destinations: Vec<String> = row.try_get("current_destinations")?;
        let proposal_destinations: Vec<String> = row.try_get("proposal_destinations")?;
        let active_exclusion_reason: Option<String> = row.try_get("exclusion_reason")?;
        let event_count: i64 = row.try_get("event_count")?;
        let play_count: i64 = row.try_get("play_count")?;
        let state = classify(
            !current_destinations.is_empty(),
            active_exclusion_reason.is_some(),
            !proposal_destinations.is_empty(),
            proposal_state.as_deref(),
            event_count,
        );
        let item_proposal_state = (!proposal_destinations.is_empty())
            .then(|| proposal_state.clone())
            .flatten();
        items.push(IntakeItem {
            spotify_id: row.try_get("spotify_id")?,
            title: row.try_get("title")?,
            artists: row.try_get("artists")?,
            sources: row.try_get("sources")?,
            state,
            current_destinations,
            saved_track_disposition: row.try_get("saved_track_disposition")?,
            proposal_destinations,
            proposal_state: item_proposal_state,
            exclusion_history: row.try_get("exclusion_history")?,
            active_exclusion_reason,
            listening_events: nonnegative(event_count, "event count")?,
            play_count: nonnegative(play_count, "play count")?,
        });
    }

    Ok(IntakeAudit {
        snapshot_id,
        proposal_generation_id,
        proposal_state,
        items,
    })
}

/// Records one explicit saved/liked keep-or-clear decision without writing to a provider.
pub async fn set_saved_track_disposition(
    database: &Database,
    account_label: &str,
    spotify_id: &str,
    disposition: SavedTrackDisposition,
    reason: &str,
) -> Result<()> {
    if reason.trim().is_empty() {
        return Err(ChordriftError::Configuration(
            "saved-track disposition reason must not be empty".to_owned(),
        ));
    }
    let mut transaction = database.pool().begin().await?;
    let row = sqlx::query(
        "SELECT account.id AS provider_account_id, account.chordrift_account_id,
                provider.track_id
         FROM provider_accounts account
         JOIN provider_current_inventories inventory
           ON inventory.provider_account_id = account.id
         JOIN provider_tracks provider
           ON provider.provider = account.provider
          AND provider.provider_track_id = $2
         JOIN provider_observed_saved_tracks saved
           ON saved.snapshot_id = inventory.source_snapshot_id
          AND saved.provider_track_id = provider.id
         WHERE account.provider = 'spotify' AND account.account_label = $1
         FOR UPDATE OF provider",
    )
    .bind(account_label)
    .bind(spotify_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(|| {
        ChordriftError::Configuration(format!(
            "track `{spotify_id}` is not in the current saved/liked intake for account `{account_label}`"
        ))
    })?;
    let provider_account_id: Uuid = row.try_get("provider_account_id")?;
    let chordrift_account_id: Uuid = row.try_get("chordrift_account_id")?;
    let track_id: Uuid = row.try_get("track_id")?;
    let surface_id: Uuid = sqlx::query_scalar(
        "INSERT INTO playlist_surfaces
             (chordrift_account_id, stable_key, name, authority, purpose,
              refresh_policy)
         VALUES ($1, 'provider-saved-tracks:' || $2::text, 'Liked Songs',
                 'user', 'intake', 'monitored')
         ON CONFLICT (chordrift_account_id, stable_key) DO UPDATE SET
           name = EXCLUDED.name, active = TRUE, updated_at = now()
         RETURNING id",
    )
    .bind(chordrift_account_id)
    .bind(provider_account_id)
    .fetch_one(&mut *transaction)
    .await?;
    sqlx::query(
        "UPDATE playlist_track_directives
         SET superseded_at = now()
         WHERE chordrift_account_id = $1 AND surface_id = $2 AND track_id = $3
           AND superseded_at IS NULL",
    )
    .bind(chordrift_account_id)
    .bind(surface_id)
    .bind(track_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO playlist_track_directives
             (chordrift_account_id, surface_id, track_id, directive, reason)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(chordrift_account_id)
    .bind(surface_id)
    .bind(track_id)
    .bind(match disposition {
        SavedTrackDisposition::Preserve => "include",
        SavedTrackDisposition::ClearAfterVerifiedAssignment => "exclude",
    })
    .bind(reason.trim())
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

fn classify(
    current: bool,
    excluded: bool,
    proposed: bool,
    proposal_state: Option<&str>,
    event_count: i64,
) -> IntakeState {
    if excluded {
        IntakeState::PreviouslyExcluded
    } else if current && proposed && proposal_state == Some("approved") {
        IntakeState::AlreadyCovered
    } else if current {
        IntakeState::DirectManagedAddition
    } else if proposed && proposal_state == Some("approved") {
        IntakeState::AssignedApproved
    } else if proposed {
        IntakeState::SuggestedInDraft
    } else if event_count > 0 {
        IntakeState::KnownFromHistory
    } else {
        IntakeState::GenuinelyNew
    }
}

fn nonnegative(value: i64, label: &str) -> Result<u64> {
    u64::try_from(value).map_err(|_| {
        ChordriftError::Configuration(format!("intake audit returned a negative {label}"))
    })
}

#[cfg(test)]
mod tests {
    use super::{IntakeState, classify};

    #[test]
    fn disposition_precedence_is_explicit() {
        assert_eq!(
            classify(true, true, true, Some("approved"), 8),
            IntakeState::PreviouslyExcluded
        );
        assert_eq!(
            classify(true, false, true, Some("approved"), 8),
            IntakeState::AlreadyCovered
        );
        assert_eq!(
            classify(true, false, false, Some("approved"), 0),
            IntakeState::DirectManagedAddition
        );
        assert_eq!(
            classify(true, false, true, Some("proposed"), 0),
            IntakeState::DirectManagedAddition
        );
        assert_eq!(
            classify(false, false, true, Some("approved"), 8),
            IntakeState::AssignedApproved
        );
        assert_eq!(
            classify(false, false, true, Some("proposed"), 8),
            IntakeState::SuggestedInDraft
        );
        assert_eq!(
            classify(false, false, false, None, 8),
            IntakeState::KnownFromHistory
        );
        assert_eq!(
            classify(false, false, false, None, 0),
            IntakeState::GenuinelyNew
        );
    }
}
