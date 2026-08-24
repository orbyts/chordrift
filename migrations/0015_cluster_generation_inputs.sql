ALTER TABLE cluster_generations
    ADD COLUMN provider_account_id UUID REFERENCES provider_accounts (id) ON DELETE RESTRICT,
    ADD COLUMN embedding_generation_id UUID REFERENCES embedding_generations (id) ON DELETE RESTRICT,
    ADD COLUMN input_hash TEXT,
    ADD COLUMN track_count INTEGER CHECK (track_count IS NULL OR track_count >= 0),
    ADD COLUMN cluster_count INTEGER CHECK (cluster_count IS NULL OR cluster_count >= 0),
    ADD COLUMN unassigned_count INTEGER CHECK (unassigned_count IS NULL OR unassigned_count >= 0);

ALTER TABLE cluster_generations
    ADD CONSTRAINT cluster_generations_reproducible_input_check CHECK (
        (provider_account_id IS NULL AND embedding_generation_id IS NULL AND input_hash IS NULL)
        OR
        (provider_account_id IS NOT NULL AND embedding_generation_id IS NOT NULL
         AND input_hash IS NOT NULL AND btrim(input_hash) <> ''
         AND track_count IS NOT NULL AND cluster_count IS NOT NULL
         AND unassigned_count IS NOT NULL)
    );

CREATE UNIQUE INDEX cluster_generations_reproducible_input_idx
    ON cluster_generations (provider_account_id, algorithm, algorithm_version, input_hash)
    WHERE provider_account_id IS NOT NULL;

CREATE INDEX cluster_generations_account_created_idx
    ON cluster_generations (provider_account_id, created_at DESC, id DESC)
    WHERE provider_account_id IS NOT NULL;
