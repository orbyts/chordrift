CREATE TABLE playlist_artwork_batches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL
        REFERENCES provider_accounts (id) ON DELETE CASCADE,
    proposal_generation_id UUID NOT NULL
        REFERENCES playlist_generations (id) ON DELETE RESTRICT,
    input_hash TEXT NOT NULL CHECK (input_hash ~ '^[0-9a-f]{64}$'),
    state TEXT NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'approved', 'superseded')),
    visual_system TEXT NOT NULL CHECK (btrim(visual_system) <> ''),
    generator_provider TEXT NOT NULL CHECK (btrim(generator_provider) <> ''),
    generator_model TEXT NOT NULL CHECK (btrim(generator_model) <> ''),
    generator_version TEXT NOT NULL CHECK (btrim(generator_version) <> ''),
    manifest_path TEXT NOT NULL CHECK (btrim(manifest_path) <> ''),
    contact_sheet_path TEXT NOT NULL CHECK (btrim(contact_sheet_path) <> ''),
    artifact_count INTEGER NOT NULL CHECK (artifact_count > 0),
    approved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((state = 'approved' AND approved_at IS NOT NULL)
        OR (state <> 'approved' AND approved_at IS NULL)),
    UNIQUE (provider_account_id, proposal_generation_id, input_hash)
);

CREATE INDEX playlist_artwork_batches_account_idx
    ON playlist_artwork_batches
    (provider_account_id, created_at DESC, id DESC);

CREATE TABLE playlist_artwork_artifacts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    batch_id UUID NOT NULL
        REFERENCES playlist_artwork_batches (id) ON DELETE CASCADE,
    playlist_id UUID NOT NULL REFERENCES playlists (id) ON DELETE RESTRICT,
    stable_key TEXT NOT NULL CHECK (btrim(stable_key) <> ''),
    playlist_name TEXT NOT NULL CHECK (btrim(playlist_name) <> ''),
    artifact_path TEXT NOT NULL CHECK (btrim(artifact_path) <> ''),
    media_type TEXT NOT NULL CHECK (media_type = 'image/png'),
    pixel_width INTEGER NOT NULL CHECK (pixel_width > 0),
    pixel_height INTEGER NOT NULL CHECK (pixel_height > 0),
    byte_size BIGINT NOT NULL CHECK (byte_size > 0),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    prompt TEXT NOT NULL CHECK (btrim(prompt) <> ''),
    semantic_tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (batch_id, playlist_id),
    UNIQUE (batch_id, stable_key),
    UNIQUE (batch_id, artifact_path)
);

COMMENT ON TABLE playlist_artwork_batches IS
    'Immutable, local-only canonical playlist artwork review sets; approval never uploads provider images.';
COMMENT ON TABLE playlist_artwork_artifacts IS
    'Verified original cover metadata and provenance. Image bytes remain in the local project artifact tree.';
