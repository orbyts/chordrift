CREATE TABLE external_playlist_bookmark_refreshes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bookmark_id UUID NOT NULL
        REFERENCES external_playlist_bookmarks (id) ON DELETE CASCADE,
    status TEXT NOT NULL
        CHECK (status IN ('complete', 'inaccessible', 'not_found')),
    provider_snapshot_id TEXT,
    item_count INTEGER NOT NULL DEFAULT 0 CHECK (item_count >= 0),
    captured_item_count INTEGER NOT NULL DEFAULT 0
        CHECK (captured_item_count >= 0 AND captured_item_count <= item_count),
    unavailable_item_count INTEGER NOT NULL DEFAULT 0 CHECK (unavailable_item_count >= 0),
    unsupported_item_count INTEGER NOT NULL DEFAULT 0 CHECK (unsupported_item_count >= 0),
    http_status INTEGER CHECK (http_status IS NULL OR http_status BETWEEN 100 AND 599),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    refreshed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX external_playlist_bookmark_refreshes_bookmark_idx
    ON external_playlist_bookmark_refreshes
    (bookmark_id, refreshed_at DESC, id DESC);

CREATE TABLE external_playlist_bookmark_refresh_tracks (
    refresh_id UUID NOT NULL
        REFERENCES external_playlist_bookmark_refreshes (id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    provider_track_id TEXT NOT NULL CHECK (btrim(provider_track_id) <> ''),
    title TEXT NOT NULL CHECK (btrim(title) <> ''),
    artists TEXT NOT NULL,
    album TEXT,
    added_at TIMESTAMPTZ,
    provider_url TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (refresh_id, position)
);

CREATE INDEX external_playlist_bookmark_refresh_tracks_provider_idx
    ON external_playlist_bookmark_refresh_tracks (provider_track_id);

COMMENT ON TABLE external_playlist_bookmark_refreshes IS
    'Explicit one-bookmark refresh attempts, separate from normal provider-library pulls and their request budget.';
COMMENT ON TABLE external_playlist_bookmark_refresh_tracks IS
    'Readable track metadata retained from an explicit bookmark refresh; never a canonical or embedding input.';
