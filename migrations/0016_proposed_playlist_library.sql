CREATE TABLE playlist_concepts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL REFERENCES provider_accounts (id) ON DELETE CASCADE,
    stable_key TEXT NOT NULL CHECK (btrim(stable_key) <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    retired_at TIMESTAMPTZ,
    UNIQUE (provider_account_id, stable_key)
);

ALTER TABLE playlist_generations
    ADD COLUMN provider_account_id UUID REFERENCES provider_accounts (id) ON DELETE CASCADE,
    ADD COLUMN cluster_generation_id UUID REFERENCES cluster_generations (id) ON DELETE RESTRICT,
    ADD COLUMN input_hash TEXT,
    ADD COLUMN naming_context_hash TEXT,
    ADD COLUMN coverage_complete BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN required_track_count INTEGER NOT NULL DEFAULT 0 CHECK (required_track_count >= 0),
    ADD COLUMN represented_track_count INTEGER NOT NULL DEFAULT 0 CHECK (represented_track_count >= 0),
    ADD COLUMN approved_by TEXT,
    ADD CONSTRAINT playlist_generation_account_scope_check CHECK (
        (provider_account_id IS NULL AND cluster_generation_id IS NULL AND input_hash IS NULL)
        OR
        (provider_account_id IS NOT NULL AND cluster_generation_id IS NOT NULL
         AND input_hash IS NOT NULL AND btrim(input_hash) <> '')
    ),
    ADD CONSTRAINT playlist_generation_coverage_count_check CHECK (
        represented_track_count <= required_track_count
    );

CREATE UNIQUE INDEX playlist_generations_reproducible_input_uq
    ON playlist_generations (provider_account_id, input_hash)
    WHERE provider_account_id IS NOT NULL;
CREATE INDEX playlist_generations_account_created_idx
    ON playlist_generations (provider_account_id, created_at DESC, id DESC)
    WHERE provider_account_id IS NOT NULL;

ALTER TABLE playlists
    ADD COLUMN concept_id UUID REFERENCES playlist_concepts (id) ON DELETE RESTRICT;

CREATE UNIQUE INDEX playlists_generation_concept_uq
    ON playlists (generation_id, concept_id)
    WHERE generation_id IS NOT NULL AND concept_id IS NOT NULL;

CREATE TABLE playlist_name_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    playlist_id UUID NOT NULL REFERENCES playlists (id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (btrim(name) <> ''),
    description TEXT NOT NULL CHECK (btrim(description) <> ''),
    machine_tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    generator_provider TEXT NOT NULL CHECK (btrim(generator_provider) <> ''),
    generator_model TEXT NOT NULL CHECK (btrim(generator_model) <> ''),
    generator_model_version TEXT NOT NULL CHECK (btrim(generator_model_version) <> ''),
    artifact_sha256 TEXT NOT NULL CHECK (length(artifact_sha256) = 64),
    selected BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX playlist_name_revisions_selected_uq
    ON playlist_name_revisions (playlist_id)
    WHERE selected;
CREATE INDEX playlist_name_revisions_playlist_created_idx
    ON playlist_name_revisions (playlist_id, created_at DESC, id DESC);
