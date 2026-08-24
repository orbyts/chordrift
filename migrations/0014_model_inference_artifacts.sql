CREATE TABLE model_inference_imports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL REFERENCES provider_accounts (id) ON DELETE RESTRICT,
    model TEXT NOT NULL CHECK (btrim(model) <> ''),
    model_version TEXT NOT NULL CHECK (btrim(model_version) <> ''),
    model_license TEXT NOT NULL CHECK (btrim(model_license) <> ''),
    manifest_sha256 TEXT NOT NULL CHECK (manifest_sha256 ~ '^[0-9a-f]{64}$'),
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    status TEXT NOT NULL CHECK (status IN ('succeeded', 'failed')),
    tracks_imported INTEGER NOT NULL DEFAULT 0 CHECK (tracks_imported >= 0),
    parameters JSONB NOT NULL DEFAULT '{}'::jsonb,
    imported_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_account_id, manifest_sha256)
);

CREATE TABLE track_model_inferences (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    import_id UUID NOT NULL REFERENCES model_inference_imports (id) ON DELETE RESTRICT,
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    model TEXT NOT NULL CHECK (btrim(model) <> ''),
    model_version TEXT NOT NULL CHECK (btrim(model_version) <> ''),
    input_sha256 TEXT NOT NULL CHECK (input_sha256 ~ '^[0-9a-f]{64}$'),
    embedding DOUBLE PRECISION[],
    dimensions INTEGER,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    inferred_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((embedding IS NULL) = (dimensions IS NULL)),
    CHECK (dimensions IS NULL OR dimensions BETWEEN 1 AND 4096),
    CHECK (embedding IS NULL OR cardinality(embedding) = dimensions),
    UNIQUE (track_id, model, model_version, input_sha256)
);

CREATE INDEX track_model_inferences_track_idx
    ON track_model_inferences (track_id, model, model_version);

CREATE TABLE track_model_facts (
    inference_id UUID NOT NULL REFERENCES track_model_inferences (id) ON DELETE CASCADE,
    fact_kind TEXT NOT NULL CHECK (fact_kind IN ('genre', 'mood', 'sound_descriptor')),
    value TEXT NOT NULL CHECK (btrim(value) <> ''),
    normalized_value TEXT NOT NULL CHECK (btrim(normalized_value) <> ''),
    confidence DOUBLE PRECISION NOT NULL CHECK (confidence BETWEEN 0 AND 1),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (inference_id, fact_kind, normalized_value)
);

CREATE INDEX track_model_facts_kind_value_idx
    ON track_model_facts (fact_kind, normalized_value);
