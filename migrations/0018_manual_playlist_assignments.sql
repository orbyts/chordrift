ALTER TABLE playlist_concepts
    ADD COLUMN origin TEXT NOT NULL DEFAULT 'cluster_lineage'
        CHECK (origin IN ('cluster_lineage', 'manual')),
    ADD COLUMN manual_name TEXT,
    ADD COLUMN manual_description TEXT,
    ADD COLUMN manual_tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD CONSTRAINT playlist_concepts_manual_metadata_check CHECK (
        (origin = 'cluster_lineage')
        OR
        (manual_name IS NOT NULL AND btrim(manual_name) <> ''
         AND manual_description IS NOT NULL AND btrim(manual_description) <> '')
    );

CREATE TABLE track_playlist_assignment_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL REFERENCES provider_accounts (id) ON DELETE CASCADE,
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    destination_concept_id UUID REFERENCES playlist_concepts (id) ON DELETE RESTRICT,
    decision TEXT NOT NULL CHECK (decision IN ('assign', 'needs_review')),
    source_generation_id UUID NOT NULL REFERENCES playlist_generations (id) ON DELETE RESTRICT,
    reason TEXT NOT NULL CHECK (btrim(reason) <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    superseded_at TIMESTAMPTZ,
    CHECK (
        (decision = 'assign' AND destination_concept_id IS NOT NULL)
        OR
        (decision = 'needs_review' AND destination_concept_id IS NULL)
    )
);

CREATE UNIQUE INDEX track_playlist_assignment_revisions_active_uq
    ON track_playlist_assignment_revisions (provider_account_id, track_id)
    WHERE superseded_at IS NULL;
CREATE INDEX track_playlist_assignment_revisions_destination_idx
    ON track_playlist_assignment_revisions
       (provider_account_id, destination_concept_id, created_at DESC)
    WHERE superseded_at IS NULL;
