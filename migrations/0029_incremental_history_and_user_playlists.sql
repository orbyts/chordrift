ALTER TABLE listening_events
    ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'archive'
        CHECK (source_kind IN ('archive', 'recent_api')),
    ADD COLUMN superseded_at TIMESTAMPTZ;

CREATE INDEX listening_events_active_recent_idx
    ON listening_events (provider_account_id, played_at DESC)
    WHERE source_kind = 'recent_api' AND superseded_at IS NULL;

CREATE TABLE spotify_recent_play_syncs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL
        REFERENCES provider_accounts (id) ON DELETE CASCADE,
    requested_after TIMESTAMPTZ,
    newest_played_at TIMESTAMPTZ,
    observations_seen INTEGER NOT NULL DEFAULT 0 CHECK (observations_seen >= 0),
    observations_inserted INTEGER NOT NULL DEFAULT 0 CHECK (observations_inserted >= 0),
    captured_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX spotify_recent_play_syncs_account_captured_idx
    ON spotify_recent_play_syncs (provider_account_id, captured_at DESC, id DESC);

ALTER TABLE provider_account_playlists
    DROP CONSTRAINT provider_account_playlists_signal_class_check,
    DROP CONSTRAINT provider_account_playlists_semantic_policy_check;

ALTER TABLE provider_account_playlists
    ALTER COLUMN signal_class SET DEFAULT 'user_managed',
    ADD CONSTRAINT provider_account_playlists_signal_class_check
        CHECK (signal_class IN (
            'user_managed',
            'semantic_legacy',
            'provider_curated',
            'intake',
            'canonical',
            'transport',
            'ignored'
        )),
    ADD CONSTRAINT provider_account_playlists_semantic_policy_check
        CHECK (signal_class = 'semantic_legacy' OR semantic_weight = 0.0);

CREATE OR REPLACE FUNCTION account_track_is_library_candidate(
    p_account_id UUID,
    p_canonical_track_id UUID
)
RETURNS BOOLEAN
LANGUAGE sql
STABLE
AS $$
    SELECT EXISTS (
        SELECT 1
        FROM provider_library_snapshots snapshot
        JOIN provider_saved_tracks saved ON saved.snapshot_id = snapshot.id
        JOIN provider_tracks provider_track ON provider_track.id = saved.provider_track_id
        WHERE snapshot.id = (
            SELECT latest.id
            FROM provider_library_snapshots latest
            WHERE latest.provider_account_id = p_account_id
            ORDER BY latest.captured_at DESC, latest.id DESC
            LIMIT 1
        )
          AND provider_track.track_id = p_canonical_track_id
    ) OR EXISTS (
        SELECT 1
        FROM provider_account_playlists policy
        JOIN provider_library_snapshots snapshot
          ON snapshot.provider_account_id = policy.provider_account_id
        JOIN provider_playlist_tracks membership
          ON membership.snapshot_id = snapshot.id
         AND membership.provider_playlist_id = policy.provider_playlist_id
        JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
        WHERE policy.provider_account_id = p_account_id
          AND policy.signal_class IN
              ('user_managed', 'semantic_legacy', 'transport', 'intake', 'canonical')
          AND provider_track.track_id = p_canonical_track_id
    ) OR EXISTS (
        SELECT 1
        FROM excluded_tracks exclusion
        WHERE exclusion.provider_account_id = p_account_id
          AND exclusion.track_id = p_canonical_track_id
          AND exclusion.restored_at IS NULL
    )
$$;

COMMENT ON FUNCTION account_track_is_library_candidate(UUID, UUID) IS
    'True for the account preservation universe: latest saved tracks, protected user-managed and orchestration playlist history, or an active reversible exclusion. Listening history alone is enrichment, not library membership.';
