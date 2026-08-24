ALTER TABLE provider_account_playlists
    ADD COLUMN embedding_weight DOUBLE PRECISION NOT NULL DEFAULT 1.0
        CHECK (embedding_weight >= 0.0 AND embedding_weight <= 10.0);

CREATE TABLE embedding_generations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL REFERENCES provider_accounts (id) ON DELETE CASCADE,
    source_snapshot_id UUID NOT NULL REFERENCES provider_library_snapshots (id) ON DELETE RESTRICT,
    model TEXT NOT NULL CHECK (btrim(model) <> ''),
    model_version TEXT NOT NULL CHECK (btrim(model_version) <> ''),
    dimensions INTEGER NOT NULL CHECK (dimensions >= 16 AND dimensions <= 4096),
    seed BIGINT NOT NULL,
    input_hash TEXT NOT NULL CHECK (input_hash ~ '^[0-9a-f]{64}$'),
    track_count INTEGER NOT NULL CHECK (track_count >= 0),
    parameters JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_account_id, model, model_version, input_hash)
);

CREATE INDEX embedding_generations_account_created_idx
    ON embedding_generations (provider_account_id, created_at DESC, id DESC);

CREATE TABLE account_track_embeddings (
    generation_id UUID NOT NULL REFERENCES embedding_generations (id) ON DELETE CASCADE,
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    embedding DOUBLE PRECISION[] NOT NULL CHECK (cardinality(embedding) > 0),
    norm DOUBLE PRECISION NOT NULL CHECK (norm > 0),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (generation_id, track_id),
    CHECK (abs(norm - 1.0) < 0.000001)
);

CREATE INDEX account_track_embeddings_track_idx
    ON account_track_embeddings (track_id, generation_id);
