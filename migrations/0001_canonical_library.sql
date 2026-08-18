-- Chordrift owns this schema. Storexa owns connection and migration execution.

CREATE TABLE artists (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL CHECK (btrim(name) <> ''),
    normalized_name TEXT NOT NULL CHECK (btrim(normalized_name) <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX artists_normalized_name_idx ON artists (normalized_name);

CREATE TABLE albums (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL CHECK (btrim(title) <> ''),
    normalized_title TEXT NOT NULL CHECK (btrim(normalized_title) <> ''),
    release_date DATE,
    album_type TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX albums_normalized_title_idx ON albums (normalized_title);

CREATE TABLE tracks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    album_id UUID REFERENCES albums (id) ON DELETE SET NULL,
    title TEXT NOT NULL CHECK (btrim(title) <> ''),
    normalized_title TEXT NOT NULL CHECK (btrim(normalized_title) <> ''),
    duration_ms INTEGER CHECK (duration_ms IS NULL OR duration_ms >= 0),
    isrc TEXT CHECK (isrc IS NULL OR btrim(isrc) <> ''),
    explicit BOOLEAN,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX tracks_isrc_idx ON tracks (isrc) WHERE isrc IS NOT NULL;
CREATE INDEX tracks_normalized_title_idx ON tracks (normalized_title);

CREATE TABLE track_artists (
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    artist_id UUID NOT NULL REFERENCES artists (id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'primary' CHECK (btrim(role) <> ''),
    position INTEGER NOT NULL CHECK (position >= 0),
    PRIMARY KEY (track_id, artist_id, role),
    UNIQUE (track_id, position)
);

CREATE INDEX track_artists_artist_id_idx ON track_artists (artist_id);

CREATE TABLE provider_artists (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    artist_id UUID NOT NULL REFERENCES artists (id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (btrim(provider) <> ''),
    provider_artist_id TEXT NOT NULL CHECK (btrim(provider_artist_id) <> ''),
    provider_uri TEXT,
    provider_url TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, provider_artist_id)
);

CREATE INDEX provider_artists_artist_id_idx ON provider_artists (artist_id);

CREATE TABLE provider_albums (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    album_id UUID NOT NULL REFERENCES albums (id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (btrim(provider) <> ''),
    provider_album_id TEXT NOT NULL CHECK (btrim(provider_album_id) <> ''),
    provider_uri TEXT,
    provider_url TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, provider_album_id)
);

CREATE INDEX provider_albums_album_id_idx ON provider_albums (album_id);

CREATE TABLE provider_tracks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (btrim(provider) <> ''),
    provider_track_id TEXT NOT NULL CHECK (btrim(provider_track_id) <> ''),
    provider_uri TEXT,
    provider_url TEXT,
    spatial_audio_available BOOLEAN,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, provider_track_id)
);

CREATE INDEX provider_tracks_track_id_idx ON provider_tracks (track_id);
CREATE INDEX provider_tracks_spatial_idx
    ON provider_tracks (provider, spatial_audio_available)
    WHERE spatial_audio_available = TRUE;

CREATE TABLE playlist_generations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    model TEXT,
    model_version TEXT,
    status TEXT NOT NULL DEFAULT 'proposed'
        CHECK (status IN ('proposed', 'approved', 'published', 'superseded')),
    parameters JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    approved_at TIMESTAMPTZ
);

CREATE TABLE playlists (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    generation_id UUID REFERENCES playlist_generations (id) ON DELETE SET NULL,
    name TEXT NOT NULL CHECK (btrim(name) <> ''),
    description TEXT,
    kind TEXT NOT NULL DEFAULT 'historical'
        CHECK (kind IN ('historical', 'canonical', 'generated', 'spatial', 'manual')),
    machine_label TEXT,
    machine_tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    archived_at TIMESTAMPTZ
);

CREATE INDEX playlists_generation_id_idx ON playlists (generation_id);
CREATE INDEX playlists_kind_idx ON playlists (kind);

CREATE TABLE playlist_tracks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    playlist_id UUID NOT NULL REFERENCES playlists (id) ON DELETE CASCADE,
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE RESTRICT,
    position INTEGER NOT NULL CHECK (position >= 0),
    source TEXT NOT NULL
        CHECK (source IN ('spotify_import', 'apple_music_import', 'historical', 'generated', 'manual')),
    provenance JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (playlist_id, position)
);

CREATE INDEX playlist_tracks_track_id_idx ON playlist_tracks (track_id);
CREATE UNIQUE INDEX playlist_tracks_generated_membership_uq
    ON playlist_tracks (playlist_id, track_id)
    WHERE source = 'generated';

CREATE TABLE provider_playlists (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    playlist_id UUID NOT NULL REFERENCES playlists (id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (btrim(provider) <> ''),
    provider_playlist_id TEXT NOT NULL CHECK (btrim(provider_playlist_id) <> ''),
    provider_uri TEXT,
    provider_url TEXT,
    snapshot_id TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    imported_at TIMESTAMPTZ,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, provider_playlist_id),
    UNIQUE (playlist_id, provider)
);

CREATE INDEX provider_playlists_playlist_id_idx ON provider_playlists (playlist_id);

CREATE TABLE provider_library_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider TEXT NOT NULL CHECK (btrim(provider) <> ''),
    source TEXT NOT NULL CHECK (btrim(source) <> ''),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    captured_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX provider_library_snapshots_provider_captured_at_idx
    ON provider_library_snapshots (provider, captured_at DESC);

CREATE TABLE provider_playlist_tracks (
    snapshot_id UUID NOT NULL REFERENCES provider_library_snapshots (id) ON DELETE CASCADE,
    provider_playlist_id UUID NOT NULL REFERENCES provider_playlists (id) ON DELETE CASCADE,
    provider_track_id UUID NOT NULL REFERENCES provider_tracks (id) ON DELETE RESTRICT,
    position INTEGER NOT NULL CHECK (position >= 0),
    added_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    captured_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (snapshot_id, provider_playlist_id, position)
);

CREATE INDEX provider_playlist_tracks_track_id_idx
    ON provider_playlist_tracks (provider_track_id);

CREATE TABLE listening_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    track_id UUID REFERENCES tracks (id) ON DELETE SET NULL,
    provider TEXT NOT NULL CHECK (btrim(provider) <> ''),
    provider_track_id TEXT,
    source_event_id TEXT,
    played_at TIMESTAMPTZ NOT NULL,
    ms_played INTEGER CHECK (ms_played IS NULL OR ms_played >= 0),
    skipped BOOLEAN,
    context_uri TEXT,
    raw_metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    imported_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX listening_events_source_event_id_uq
    ON listening_events (provider, source_event_id)
    WHERE source_event_id IS NOT NULL;
CREATE INDEX listening_events_track_id_played_at_idx
    ON listening_events (track_id, played_at DESC);
CREATE INDEX listening_events_unmatched_idx
    ON listening_events (provider, played_at)
    WHERE track_id IS NULL;

CREATE TABLE track_statistics (
    track_id UUID PRIMARY KEY REFERENCES tracks (id) ON DELETE CASCADE,
    playlist_occurrence_count INTEGER NOT NULL DEFAULT 0
        CHECK (playlist_occurrence_count >= 0),
    total_playlist_entries INTEGER NOT NULL DEFAULT 0
        CHECK (total_playlist_entries >= 0),
    play_count BIGINT NOT NULL DEFAULT 0 CHECK (play_count >= 0),
    total_ms_played BIGINT NOT NULL DEFAULT 0 CHECK (total_ms_played >= 0),
    first_played_at TIMESTAMPTZ,
    last_played_at TIMESTAMPTZ,
    average_ms_played DOUBLE PRECISION CHECK (average_ms_played IS NULL OR average_ms_played >= 0),
    skip_count BIGINT NOT NULL DEFAULT 0 CHECK (skip_count >= 0),
    completion_ratio DOUBLE PRECISION
        CHECK (completion_ratio IS NULL OR completion_ratio BETWEEN 0 AND 1),
    calculated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE track_embeddings (
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    model TEXT NOT NULL CHECK (btrim(model) <> ''),
    model_version TEXT NOT NULL CHECK (btrim(model_version) <> ''),
    embedding DOUBLE PRECISION[] NOT NULL CHECK (cardinality(embedding) > 0),
    dimensions INTEGER NOT NULL CHECK (dimensions > 0),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (track_id, model, model_version),
    CHECK (cardinality(embedding) = dimensions)
);

CREATE TABLE cluster_generations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    embedding_model TEXT NOT NULL CHECK (btrim(embedding_model) <> ''),
    embedding_version TEXT NOT NULL CHECK (btrim(embedding_version) <> ''),
    algorithm TEXT NOT NULL CHECK (btrim(algorithm) <> ''),
    algorithm_version TEXT NOT NULL CHECK (btrim(algorithm_version) <> ''),
    seed BIGINT,
    parameters JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE clusters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    generation_id UUID NOT NULL REFERENCES cluster_generations (id) ON DELETE CASCADE,
    machine_label TEXT NOT NULL CHECK (btrim(machine_label) <> ''),
    display_name TEXT,
    description TEXT,
    machine_tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (generation_id, machine_label)
);

CREATE TABLE cluster_tracks (
    cluster_id UUID NOT NULL REFERENCES clusters (id) ON DELETE CASCADE,
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    membership_score DOUBLE PRECISION,
    representative_rank INTEGER CHECK (representative_rank IS NULL OR representative_rank > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (cluster_id, track_id)
);

CREATE INDEX cluster_tracks_track_id_idx ON cluster_tracks (track_id);

CREATE TABLE track_matches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_provider_track_id UUID NOT NULL REFERENCES provider_tracks (id) ON DELETE CASCADE,
    target_provider_track_id UUID NOT NULL REFERENCES provider_tracks (id) ON DELETE CASCADE,
    match_method TEXT NOT NULL
        CHECK (match_method IN ('known_mapping', 'isrc', 'metadata_duration', 'metadata_album', 'fuzzy', 'manual')),
    confidence DOUBLE PRECISION NOT NULL CHECK (confidence BETWEEN 0 AND 1),
    confirmed BOOLEAN NOT NULL DEFAULT FALSE,
    evidence JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (source_provider_track_id <> target_provider_track_id),
    UNIQUE (source_provider_track_id, target_provider_track_id)
);

CREATE TABLE sync_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider TEXT NOT NULL CHECK (btrim(provider) <> ''),
    mode TEXT NOT NULL CHECK (mode IN ('dry_run', 'apply')),
    status TEXT NOT NULL CHECK (status IN ('planning', 'planned', 'running', 'succeeded', 'failed')),
    desired_state_hash TEXT,
    summary JSONB NOT NULL DEFAULT '{}'::jsonb,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ
);

CREATE INDEX sync_runs_provider_started_at_idx ON sync_runs (provider, started_at DESC);

CREATE TABLE sync_operations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sync_run_id UUID NOT NULL REFERENCES sync_runs (id) ON DELETE CASCADE,
    playlist_id UUID REFERENCES playlists (id) ON DELETE SET NULL,
    provider_playlist_id UUID REFERENCES provider_playlists (id) ON DELETE SET NULL,
    operation_type TEXT NOT NULL
        CHECK (operation_type IN ('create_playlist', 'rename_playlist', 'add_track', 'remove_track', 'reorder_track', 'archive_playlist')),
    operation_key TEXT NOT NULL CHECK (btrim(operation_key) <> ''),
    status TEXT NOT NULL DEFAULT 'planned'
        CHECK (status IN ('planned', 'running', 'succeeded', 'failed', 'skipped')),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    executed_at TIMESTAMPTZ,
    UNIQUE (sync_run_id, operation_key)
);

CREATE INDEX sync_operations_run_status_idx ON sync_operations (sync_run_id, status);
