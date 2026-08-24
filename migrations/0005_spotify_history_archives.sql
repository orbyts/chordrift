CREATE TABLE spotify_archive_imports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL REFERENCES provider_accounts (id) ON DELETE CASCADE,
    archive_sha256 TEXT NOT NULL CHECK (archive_sha256 ~ '^[0-9a-f]{64}$'),
    archive_kind TEXT NOT NULL
        CHECK (archive_kind IN ('account_data', 'extended_streaming_history')),
    source_filename TEXT NOT NULL CHECK (btrim(source_filename) <> ''),
    source_files INTEGER NOT NULL DEFAULT 0 CHECK (source_files >= 0),
    events_seen BIGINT NOT NULL DEFAULT 0 CHECK (events_seen >= 0),
    events_imported BIGINT NOT NULL DEFAULT 0 CHECK (events_imported >= 0),
    events_matched BIGINT NOT NULL DEFAULT 0 CHECK (events_matched >= 0),
    events_ignored BIGINT NOT NULL DEFAULT 0 CHECK (events_ignored >= 0),
    first_event_at TIMESTAMPTZ,
    last_event_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    imported_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_account_id, archive_sha256)
);

CREATE INDEX spotify_archive_imports_account_imported_idx
    ON spotify_archive_imports (provider_account_id, imported_at DESC);

ALTER TABLE listening_events
    ADD COLUMN provider_account_id UUID REFERENCES provider_accounts (id) ON DELETE CASCADE,
    ADD COLUMN source_import_id UUID REFERENCES spotify_archive_imports (id) ON DELETE SET NULL,
    ADD COLUMN source_file TEXT,
    ADD COLUMN media_type TEXT NOT NULL DEFAULT 'track'
        CHECK (media_type IN ('track', 'episode', 'audiobook'));

CREATE UNIQUE INDEX listening_events_account_source_event_idx
    ON listening_events (provider_account_id, source_event_id)
    WHERE provider_account_id IS NOT NULL AND source_event_id IS NOT NULL;

CREATE INDEX listening_events_account_played_at_idx
    ON listening_events (provider_account_id, played_at DESC);

CREATE INDEX listening_events_account_provider_track_idx
    ON listening_events (provider_account_id, provider_track_id)
    WHERE provider_track_id IS NOT NULL;
