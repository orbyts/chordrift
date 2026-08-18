CREATE TABLE provider_accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider TEXT NOT NULL CHECK (btrim(provider) <> ''),
    provider_account_id TEXT NOT NULL CHECK (btrim(provider_account_id) <> ''),
    account_label TEXT NOT NULL CHECK (btrim(account_label) <> ''),
    display_name TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_authenticated_at TIMESTAMPTZ,
    last_imported_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, provider_account_id),
    UNIQUE (provider, account_label)
);

ALTER TABLE provider_library_snapshots
    ADD COLUMN provider_account_id UUID REFERENCES provider_accounts (id) ON DELETE RESTRICT;

CREATE INDEX provider_library_snapshots_account_captured_at_idx
    ON provider_library_snapshots (provider_account_id, captured_at DESC);

CREATE TABLE provider_import_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL REFERENCES provider_accounts (id) ON DELETE RESTRICT,
    snapshot_id UUID REFERENCES provider_library_snapshots (id) ON DELETE SET NULL,
    status TEXT NOT NULL CHECK (status IN ('fetching', 'persisting', 'succeeded', 'failed')),
    playlists_seen INTEGER NOT NULL DEFAULT 0 CHECK (playlists_seen >= 0),
    playlists_imported INTEGER NOT NULL DEFAULT 0 CHECK (playlists_imported >= 0),
    playlist_entries INTEGER NOT NULL DEFAULT 0 CHECK (playlist_entries >= 0),
    saved_tracks INTEGER NOT NULL DEFAULT 0 CHECK (saved_tracks >= 0),
    unavailable_items INTEGER NOT NULL DEFAULT 0 CHECK (unavailable_items >= 0),
    unsupported_items INTEGER NOT NULL DEFAULT 0 CHECK (unsupported_items >= 0),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ
);

CREATE INDEX provider_import_runs_account_started_at_idx
    ON provider_import_runs (provider_account_id, started_at DESC);

CREATE TABLE provider_playlist_snapshots (
    snapshot_id UUID NOT NULL REFERENCES provider_library_snapshots (id) ON DELETE CASCADE,
    provider_playlist_id UUID NOT NULL REFERENCES provider_playlists (id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (btrim(name) <> ''),
    description TEXT,
    provider_snapshot_id TEXT,
    public BOOLEAN,
    collaborative BOOLEAN NOT NULL DEFAULT FALSE,
    total_items INTEGER NOT NULL DEFAULT 0 CHECK (total_items >= 0),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (snapshot_id, provider_playlist_id)
);

CREATE TABLE provider_saved_tracks (
    snapshot_id UUID NOT NULL REFERENCES provider_library_snapshots (id) ON DELETE CASCADE,
    provider_track_id UUID NOT NULL REFERENCES provider_tracks (id) ON DELETE RESTRICT,
    position INTEGER NOT NULL CHECK (position >= 0),
    saved_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (snapshot_id, position)
);

CREATE INDEX provider_saved_tracks_track_id_idx
    ON provider_saved_tracks (provider_track_id);
