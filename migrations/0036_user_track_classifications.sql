-- User-authored classifications are a private, revisioned sidecar. They never
-- overwrite provider, public-database, or model-inferred facts.

CREATE TABLE track_classification_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL REFERENCES provider_accounts (id) ON DELETE CASCADE,
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    collection TEXT,
    regions TEXT[] NOT NULL DEFAULT '{}',
    traditions TEXT[] NOT NULL DEFAULT '{}',
    languages TEXT[] NOT NULL DEFAULT '{}',
    notes TEXT,
    reason TEXT NOT NULL CHECK (btrim(reason) <> ''),
    source TEXT NOT NULL CHECK (source IN ('cli', 'csv')),
    source_batch_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    superseded_at TIMESTAMPTZ,
    CHECK (collection IS NULL OR btrim(collection) <> '')
);

CREATE UNIQUE INDEX track_classification_revisions_active_uq
    ON track_classification_revisions (provider_account_id, track_id)
    WHERE superseded_at IS NULL;
CREATE INDEX track_classification_revisions_history_idx
    ON track_classification_revisions
       (provider_account_id, track_id, created_at DESC, id DESC);

CREATE TABLE track_classification_batches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL REFERENCES provider_accounts (id) ON DELETE CASCADE,
    schema_version INTEGER NOT NULL CHECK (schema_version = 1),
    source_path TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'approved')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    approved_at TIMESTAMPTZ
);

CREATE TABLE track_classification_batch_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    batch_id UUID NOT NULL REFERENCES track_classification_batches (id) ON DELETE CASCADE,
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    action TEXT NOT NULL CHECK (action IN ('set', 'clear')),
    collection TEXT,
    regions TEXT[] NOT NULL DEFAULT '{}',
    traditions TEXT[] NOT NULL DEFAULT '{}',
    languages TEXT[] NOT NULL DEFAULT '{}',
    notes TEXT,
    reason TEXT NOT NULL CHECK (btrim(reason) <> ''),
    UNIQUE (batch_id, track_id),
    CHECK (collection IS NULL OR btrim(collection) <> '')
);

ALTER TABLE track_classification_revisions
    ADD CONSTRAINT track_classification_revisions_source_batch_fk
    FOREIGN KEY (source_batch_id) REFERENCES track_classification_batches (id)
    ON DELETE RESTRICT;

COMMENT ON TABLE track_classification_revisions IS
    'Private account-scoped explicit facts. Active revisions influence personalized embeddings without mutating the base acoustic vector.';
COMMENT ON TABLE track_classification_batches IS
    'Immutable CSV review batches that require exact-ID approval before activation.';
