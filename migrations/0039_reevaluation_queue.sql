ALTER TABLE routing_surfaces
    ADD COLUMN purpose TEXT NOT NULL DEFAULT 'legacy_route'
        CHECK (purpose IN ('legacy_route', 'reevaluate'));

CREATE UNIQUE INDEX routing_surfaces_active_reevaluate_uq
    ON routing_surfaces (provider_account_id)
    WHERE active AND purpose = 'reevaluate';

CREATE TABLE reevaluation_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL
        REFERENCES provider_accounts (id) ON DELETE CASCADE,
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    playlist_id UUID NOT NULL REFERENCES playlists (id) ON DELETE RESTRICT,
    provider_snapshot_id UUID NOT NULL
        REFERENCES provider_library_snapshots (id) ON DELETE RESTRICT,
    event_type TEXT NOT NULL CHECK (event_type IN ('entered', 'left')),
    observed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    UNIQUE (provider_account_id, track_id, playlist_id,
            provider_snapshot_id, event_type)
);

CREATE INDEX reevaluation_events_account_track_idx
    ON reevaluation_events (provider_account_id, track_id, observed_at DESC, id DESC);

COMMENT ON TABLE reevaluation_events IS
    'Immutable provider-observed entry and exit history for the Re-evaluate holding queue; current queue state remains provider-owned.';
COMMENT ON COLUMN routing_surfaces.purpose IS
    'Legacy multi-route surface or the single account-scoped Re-evaluate holding queue.';

COMMENT ON TABLE excluded_tracks IS
    'Reversible account-level exclusions inferred only when a track is absent from its verified managed destination and the latest provider Re-evaluate queue, or recorded by an exact-confirmed user action.';
