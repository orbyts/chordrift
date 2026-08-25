-- Saved albums are a separate Spotify library surface. Inventory them without
-- making them part of canonical playlist readiness or mutating provider state.

CREATE TABLE provider_saved_albums (
    snapshot_id UUID NOT NULL REFERENCES provider_library_snapshots (id) ON DELETE CASCADE,
    provider_album_id UUID NOT NULL REFERENCES provider_albums (id) ON DELETE RESTRICT,
    position INTEGER NOT NULL CHECK (position >= 0),
    saved_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (snapshot_id, position),
    UNIQUE (snapshot_id, provider_album_id)
);

CREATE INDEX provider_saved_albums_album_id_idx
    ON provider_saved_albums (provider_album_id);

CREATE TABLE provider_saved_album_tracks (
    snapshot_id UUID NOT NULL REFERENCES provider_library_snapshots (id) ON DELETE CASCADE,
    provider_album_id UUID NOT NULL REFERENCES provider_albums (id) ON DELETE RESTRICT,
    provider_track_id UUID NOT NULL REFERENCES provider_tracks (id) ON DELETE RESTRICT,
    position INTEGER NOT NULL CHECK (position >= 0),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (snapshot_id, provider_album_id, position)
);

CREATE INDEX provider_saved_album_tracks_track_id_idx
    ON provider_saved_album_tracks (provider_track_id);

CREATE TABLE provider_account_library_policies (
    provider_account_id UUID PRIMARY KEY REFERENCES provider_accounts (id) ON DELETE CASCADE,
    saved_album_policy TEXT NOT NULL DEFAULT 'preserve'
        CHECK (saved_album_policy IN ('preserve', 'inventory_only', 'review_then_unsave')),
    saved_track_clear_policy TEXT NOT NULL DEFAULT 'preserve'
        CHECK (saved_track_clear_policy IN ('preserve', 'after_verified_assignment')),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
