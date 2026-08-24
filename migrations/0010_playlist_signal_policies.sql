ALTER TABLE provider_account_playlists
    RENAME COLUMN embedding_weight TO semantic_weight;

ALTER TABLE provider_account_playlists
    ADD COLUMN signal_class TEXT NOT NULL DEFAULT 'semantic_legacy'
        CHECK (signal_class IN (
            'semantic_legacy',
            'provider_curated',
            'intake',
            'canonical',
            'transport',
            'ignored'
        )),
    ADD COLUMN behavioral_signal TEXT
        CHECK (behavioral_signal IS NULL OR behavioral_signal IN (
            'rotation',
            'discovery',
            'prompted',
            'recommendation'
        )),
    ADD COLUMN clear_policy TEXT NOT NULL DEFAULT 'never'
        CHECK (clear_policy IN ('never', 'after_verified_assignment'));

ALTER TABLE provider_account_playlists
    ADD CONSTRAINT provider_account_playlists_semantic_policy_check
        CHECK (signal_class = 'semantic_legacy' OR semantic_weight = 0.0),
    ADD CONSTRAINT provider_account_playlists_clear_policy_class_check
        CHECK (
            clear_policy = 'never'
            OR signal_class = 'intake'
        );

CREATE TABLE signal_generations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL REFERENCES provider_accounts (id) ON DELETE CASCADE,
    source_snapshot_id UUID NOT NULL REFERENCES provider_library_snapshots (id) ON DELETE RESTRICT,
    model TEXT NOT NULL CHECK (btrim(model) <> ''),
    model_version TEXT NOT NULL CHECK (btrim(model_version) <> ''),
    input_hash TEXT NOT NULL CHECK (input_hash ~ '^[0-9a-f]{64}$'),
    track_count INTEGER NOT NULL CHECK (track_count >= 0),
    parameters JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_account_id, model, model_version, input_hash)
);

CREATE INDEX signal_generations_account_created_idx
    ON signal_generations (provider_account_id, created_at DESC, id DESC);

CREATE TABLE account_track_signals (
    generation_id UUID NOT NULL REFERENCES signal_generations (id) ON DELETE CASCADE,
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    meaningful_play_count BIGINT NOT NULL DEFAULT 0 CHECK (meaningful_play_count >= 0),
    event_count BIGINT NOT NULL DEFAULT 0 CHECK (event_count >= 0),
    last_played_at TIMESTAMPTZ,
    recency_score DOUBLE PRECISION CHECK (recency_score BETWEEN 0.0 AND 1.0),
    completion_ratio DOUBLE PRECISION CHECK (completion_ratio BETWEEN 0.0 AND 1.0),
    non_skip_ratio DOUBLE PRECISION CHECK (non_skip_ratio BETWEEN 0.0 AND 1.0),
    saved BOOLEAN NOT NULL DEFAULT FALSE,
    provider_rotation BOOLEAN NOT NULL DEFAULT FALSE,
    intake BOOLEAN NOT NULL DEFAULT FALSE,
    recommendation BOOLEAN NOT NULL DEFAULT FALSE,
    provenance JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (generation_id, track_id)
);

CREATE INDEX account_track_signals_track_idx
    ON account_track_signals (track_id, generation_id);
