ALTER TABLE provider_playlists
    ADD COLUMN concept_id UUID REFERENCES playlist_concepts (id) ON DELETE RESTRICT;

CREATE UNIQUE INDEX provider_playlists_provider_concept_uq
    ON provider_playlists (provider, concept_id)
    WHERE concept_id IS NOT NULL;

ALTER TABLE sync_runs
    ADD COLUMN provider_account_id UUID REFERENCES provider_accounts (id) ON DELETE CASCADE,
    ADD COLUMN source_snapshot_id UUID REFERENCES provider_library_snapshots (id) ON DELETE RESTRICT,
    ADD COLUMN proposal_generation_id UUID REFERENCES playlist_generations (id) ON DELETE RESTRICT,
    ADD COLUMN planner_version TEXT,
    ADD COLUMN input_hash TEXT,
    ADD COLUMN preconditions JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD CONSTRAINT sync_runs_dry_run_identity_check CHECK (
        mode <> 'dry_run' OR planner_version IS NULL
        OR (provider_account_id IS NOT NULL
            AND source_snapshot_id IS NOT NULL
            AND proposal_generation_id IS NOT NULL
            AND planner_version IS NOT NULL AND btrim(planner_version) <> ''
            AND input_hash IS NOT NULL AND input_hash ~ '^[0-9a-f]{64}$')
    );

CREATE UNIQUE INDEX sync_runs_dry_run_input_uq
    ON sync_runs (provider_account_id, provider, planner_version, input_hash)
    WHERE mode = 'dry_run' AND planner_version IS NOT NULL;

ALTER TABLE sync_operations
    DROP CONSTRAINT sync_operations_operation_type_check,
    ADD CONSTRAINT sync_operations_operation_type_check CHECK (
        operation_type IN (
            'create_playlist',
            'rename_playlist',
            'add_track',
            'restore_track',
            'remove_track',
            'reorder_track',
            'archive_playlist'
        )
    ),
    ADD COLUMN phase TEXT NOT NULL DEFAULT 'publish'
        CHECK (phase IN ('publish', 'reconcile', 'cleanup', 'retirement')),
    ADD COLUMN sequence INTEGER NOT NULL DEFAULT 0 CHECK (sequence >= 0),
    ADD COLUMN safety JSONB NOT NULL DEFAULT '{}'::jsonb;

WITH numbered AS (
    SELECT id,
           row_number() OVER (PARTITION BY sync_run_id ORDER BY created_at, id) - 1 AS value
    FROM sync_operations
)
UPDATE sync_operations operation
SET sequence = numbered.value
FROM numbered
WHERE numbered.id = operation.id;

CREATE UNIQUE INDEX sync_operations_run_sequence_uq
    ON sync_operations (sync_run_id, sequence);

CREATE TABLE excluded_tracks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL REFERENCES provider_accounts (id) ON DELETE CASCADE,
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE CASCADE,
    source_provider TEXT NOT NULL CHECK (btrim(source_provider) <> ''),
    source_provider_playlist_id UUID REFERENCES provider_playlists (id) ON DELETE SET NULL,
    previous_concept_id UUID REFERENCES playlist_concepts (id) ON DELETE SET NULL,
    excluded_at TIMESTAMPTZ NOT NULL,
    exclusion_reason TEXT NOT NULL CHECK (btrim(exclusion_reason) <> ''),
    restored_at TIMESTAMPTZ,
    restoration_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((restored_at IS NULL AND restoration_reason IS NULL)
        OR (restored_at IS NOT NULL AND restoration_reason IS NOT NULL
            AND btrim(restoration_reason) <> '' AND restored_at >= excluded_at))
);

CREATE UNIQUE INDEX excluded_tracks_active_uq
    ON excluded_tracks (provider_account_id, track_id)
    WHERE restored_at IS NULL;
CREATE INDEX excluded_tracks_account_history_idx
    ON excluded_tracks (provider_account_id, excluded_at DESC, id DESC);

COMMENT ON TABLE excluded_tracks IS
    'Reversible account-level exclusions inferred only from verified Chordrift-managed playlist removals; never a provider playlist.';
COMMENT ON COLUMN sync_runs.source_snapshot_id IS
    'Immutable provider snapshot against which this plan was calculated; apply must reject a newer live snapshot.';
COMMENT ON COLUMN sync_operations.safety IS
    'Machine-readable preservation and verification gates; dry-run planning never satisfies or executes them.';
