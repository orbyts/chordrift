ALTER TABLE playlists
    DROP CONSTRAINT playlists_kind_check,
    ADD CONSTRAINT playlists_kind_check CHECK (
        kind IN ('historical', 'canonical', 'generated', 'spatial', 'manual', 'routing')
    );

CREATE TABLE routing_surfaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL
        REFERENCES provider_accounts (id) ON DELETE CASCADE,
    playlist_id UUID NOT NULL
        REFERENCES playlists (id) ON DELETE CASCADE,
    stable_key TEXT NOT NULL CHECK (btrim(stable_key) <> ''),
    background_path TEXT NOT NULL CHECK (btrim(background_path) <> ''),
    artwork_path TEXT NOT NULL CHECK (btrim(artwork_path) <> ''),
    artwork_sha256 TEXT NOT NULL CHECK (artwork_sha256 ~ '^[0-9a-f]{64}$'),
    artwork_approved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_account_id, stable_key),
    UNIQUE (provider_account_id, playlist_id)
);

CREATE INDEX routing_surfaces_active_idx
    ON routing_surfaces (provider_account_id, lower(stable_key))
    WHERE active;

ALTER TABLE provider_account_playlists
    DROP CONSTRAINT provider_account_playlists_signal_class_check,
    ADD CONSTRAINT provider_account_playlists_signal_class_check
        CHECK (signal_class IN (
            'user_managed',
            'semantic_legacy',
            'provider_curated',
            'intake',
            'routing',
            'canonical',
            'transport',
            'ignored'
        ));

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
              ('user_managed', 'semantic_legacy', 'transport', 'intake', 'routing', 'canonical')
          AND provider_track.track_id = p_canonical_track_id
    ) OR EXISTS (
        SELECT 1
        FROM excluded_tracks exclusion
        WHERE exclusion.provider_account_id = p_account_id
          AND exclusion.track_id = p_canonical_track_id
          AND exclusion.restored_at IS NULL
    )
$$;

COMMENT ON TABLE routing_surfaces IS
    'Durable zero-signal review queues whose provider additions are captured into Neon before later explicit reassignment.';
COMMENT ON COLUMN routing_surfaces.background_path IS
    'Retained label-free master suitable for provider-specific future rendering.';
COMMENT ON COLUMN routing_surfaces.artwork_path IS
    'Deterministically labeled PNG approved for the current provider.';
COMMENT ON FUNCTION account_track_is_library_candidate(UUID, UUID) IS
    'True for the account preservation universe: latest saved tracks, protected user-managed and orchestration playlist history including routes, or an active reversible exclusion. Listening history alone is enrichment, not library membership.';
