CREATE TABLE account_listening_track_statistics (
    provider_account_id UUID NOT NULL REFERENCES provider_accounts (id) ON DELETE CASCADE,
    provider_track_id TEXT NOT NULL CHECK (btrim(provider_track_id) <> ''),
    track_id UUID REFERENCES tracks (id) ON DELETE SET NULL,
    track_name TEXT,
    artist_name TEXT,
    album_name TEXT,
    event_count BIGINT NOT NULL DEFAULT 0 CHECK (event_count >= 0),
    play_count BIGINT NOT NULL DEFAULT 0 CHECK (play_count >= 0),
    total_ms_played BIGINT NOT NULL DEFAULT 0 CHECK (total_ms_played >= 0),
    average_ms_played DOUBLE PRECISION NOT NULL DEFAULT 0
        CHECK (average_ms_played >= 0),
    skip_count BIGINT NOT NULL DEFAULT 0 CHECK (skip_count >= 0),
    completed_count BIGINT NOT NULL DEFAULT 0 CHECK (completed_count >= 0),
    first_played_at TIMESTAMPTZ NOT NULL,
    last_played_at TIMESTAMPTZ NOT NULL,
    calculated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (provider_account_id, provider_track_id)
);

CREATE INDEX account_listening_track_statistics_time_idx
    ON account_listening_track_statistics
       (provider_account_id, total_ms_played DESC, play_count DESC);

CREATE INDEX account_listening_track_statistics_recency_idx
    ON account_listening_track_statistics
       (provider_account_id, last_played_at DESC);
