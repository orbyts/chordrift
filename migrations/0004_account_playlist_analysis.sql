CREATE TABLE provider_account_playlists (
    provider_account_id UUID NOT NULL REFERENCES provider_accounts (id) ON DELETE CASCADE,
    provider_playlist_id UUID NOT NULL REFERENCES provider_playlists (id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'observed'
        CHECK (role IN ('observed', 'inbox', 'managed')),
    drift_policy TEXT NOT NULL DEFAULT 'provider_wins'
        CHECK (drift_policy IN ('provider_wins', 'neon_wins', 'manual')),
    present_in_latest_snapshot BOOLEAN NOT NULL DEFAULT TRUE,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (provider_account_id, provider_playlist_id)
);

CREATE INDEX provider_account_playlists_role_idx
    ON provider_account_playlists (provider_account_id, role, present_in_latest_snapshot);

INSERT INTO provider_account_playlists
    (provider_account_id, provider_playlist_id, first_seen_at, last_seen_at,
     present_in_latest_snapshot)
SELECT
    library.provider_account_id,
    playlist.provider_playlist_id,
    min(library.captured_at),
    max(library.captured_at),
    bool_or(playlist.snapshot_id = (
        SELECT latest.id
        FROM provider_library_snapshots latest
        WHERE latest.provider_account_id = library.provider_account_id
        ORDER BY latest.captured_at DESC, latest.id DESC
        LIMIT 1
    ))
FROM provider_playlist_snapshots playlist
JOIN provider_library_snapshots library ON library.id = playlist.snapshot_id
WHERE library.provider_account_id IS NOT NULL
GROUP BY library.provider_account_id, playlist.provider_playlist_id
ON CONFLICT (provider_account_id, provider_playlist_id) DO UPDATE SET
    first_seen_at = LEAST(
        provider_account_playlists.first_seen_at,
        EXCLUDED.first_seen_at
    ),
    last_seen_at = GREATEST(
        provider_account_playlists.last_seen_at,
        EXCLUDED.last_seen_at
    ),
    present_in_latest_snapshot =
        provider_account_playlists.present_in_latest_snapshot
        OR EXCLUDED.present_in_latest_snapshot;

CREATE TABLE account_track_statistics (
    provider_account_id UUID NOT NULL REFERENCES provider_accounts (id) ON DELETE CASCADE,
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    playlist_occurrence_count INTEGER NOT NULL DEFAULT 0
        CHECK (playlist_occurrence_count >= 0),
    total_playlist_entries INTEGER NOT NULL DEFAULT 0
        CHECK (total_playlist_entries >= 0),
    saved_in_library BOOLEAN NOT NULL DEFAULT FALSE,
    calculated_from_snapshot_id UUID NOT NULL
        REFERENCES provider_library_snapshots (id) ON DELETE CASCADE,
    calculated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (provider_account_id, track_id)
);

CREATE INDEX account_track_statistics_overlap_idx
    ON account_track_statistics
    (provider_account_id, playlist_occurrence_count DESC, total_playlist_entries DESC);

CREATE TABLE account_analysis_state (
    provider_account_id UUID PRIMARY KEY REFERENCES provider_accounts (id) ON DELETE CASCADE,
    calculated_from_snapshot_id UUID NOT NULL
        REFERENCES provider_library_snapshots (id) ON DELETE CASCADE,
    calculated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
