CREATE TABLE external_playlist_bookmarks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL
        REFERENCES provider_accounts (id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (btrim(provider) <> ''),
    provider_playlist_id TEXT NOT NULL CHECK (btrim(provider_playlist_id) <> ''),
    relationship TEXT NOT NULL
        CHECK (relationship IN ('followed_external', 'collaborative_external')),
    name TEXT NOT NULL CHECK (btrim(name) <> ''),
    owner_provider_id TEXT NOT NULL CHECK (btrim(owner_provider_id) <> ''),
    owner_display_name TEXT,
    provider_uri TEXT,
    provider_url TEXT,
    provider_snapshot_id TEXT,
    public BOOLEAN,
    collaborative BOOLEAN NOT NULL DEFAULT FALSE,
    item_count INTEGER NOT NULL DEFAULT 0 CHECK (item_count >= 0),
    content_status TEXT NOT NULL
        CHECK (content_status IN ('complete', 'metadata_only', 'inaccessible')),
    present_in_provider_library BOOLEAN NOT NULL DEFAULT TRUE,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_changed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_checked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_account_id, provider, provider_playlist_id)
);

CREATE INDEX external_playlist_bookmarks_account_presence_idx
    ON external_playlist_bookmarks
    (provider_account_id, present_in_provider_library, name);

CREATE TABLE external_playlist_bookmark_snapshots (
    snapshot_id UUID NOT NULL
        REFERENCES provider_library_snapshots (id) ON DELETE CASCADE,
    bookmark_id UUID NOT NULL
        REFERENCES external_playlist_bookmarks (id) ON DELETE CASCADE,
    relationship TEXT NOT NULL
        CHECK (relationship IN ('followed_external', 'collaborative_external')),
    name TEXT NOT NULL CHECK (btrim(name) <> ''),
    owner_provider_id TEXT NOT NULL CHECK (btrim(owner_provider_id) <> ''),
    owner_display_name TEXT,
    provider_url TEXT,
    provider_snapshot_id TEXT,
    content_status TEXT NOT NULL
        CHECK (content_status IN ('complete', 'metadata_only', 'inaccessible')),
    item_count INTEGER NOT NULL DEFAULT 0 CHECK (item_count >= 0),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    captured_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (snapshot_id, bookmark_id)
);

CREATE INDEX external_playlist_bookmark_snapshots_bookmark_idx
    ON external_playlist_bookmark_snapshots (bookmark_id, captured_at DESC);

CREATE TABLE external_playlist_bookmark_tracks (
    snapshot_id UUID NOT NULL,
    bookmark_id UUID NOT NULL,
    provider_track_id UUID NOT NULL
        REFERENCES provider_tracks (id) ON DELETE RESTRICT,
    position INTEGER NOT NULL CHECK (position >= 0),
    added_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    captured_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (snapshot_id, bookmark_id, position),
    FOREIGN KEY (snapshot_id, bookmark_id)
        REFERENCES external_playlist_bookmark_snapshots (snapshot_id, bookmark_id)
        ON DELETE CASCADE
);

CREATE INDEX external_playlist_bookmark_tracks_track_idx
    ON external_playlist_bookmark_tracks (provider_track_id);

COMMENT ON TABLE external_playlist_bookmarks IS
    'Durable account-scoped references to externally owned playlists; these never participate in the active canonical library.';

COMMENT ON TABLE external_playlist_bookmark_snapshots IS
    'Immutable metadata observations for external playlist bookmarks, tied to provider library pulls.';
