ALTER TABLE sync_operations
    DROP CONSTRAINT sync_operations_operation_type_check,
    ADD CONSTRAINT sync_operations_operation_type_check CHECK (
        operation_type IN (
            'create_playlist', 'rename_playlist', 'add_track', 'restore_track',
            'exclude_track', 'remove_track', 'reorder_track',
            'upload_artwork', 'remove_external_playlist', 'archive_playlist'
        )
    );

CREATE TABLE sync_apply_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL
        REFERENCES provider_accounts (id) ON DELETE CASCADE,
    plan_id UUID NOT NULL REFERENCES sync_runs (id) ON DELETE RESTRICT,
    readiness_assessment_id UUID NOT NULL
        REFERENCES sync_readiness_assessments (id) ON DELETE RESTRICT,
    apply_version TEXT NOT NULL CHECK (btrim(apply_version) <> ''),
    phase TEXT NOT NULL CHECK (phase IN ('publish', 'reconcile', 'cleanup', 'retirement')),
    input_hash TEXT NOT NULL CHECK (input_hash ~ '^[0-9a-f]{64}$'),
    status TEXT NOT NULL DEFAULT 'running'
        CHECK (status IN ('running', 'awaiting_pull', 'succeeded', 'failed')),
    operation_count INTEGER NOT NULL CHECK (operation_count >= 0),
    succeeded_count INTEGER NOT NULL DEFAULT 0 CHECK (succeeded_count >= 0),
    failed_count INTEGER NOT NULL DEFAULT 0 CHECK (failed_count >= 0),
    summary JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_error TEXT,
    confirmed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ,
    UNIQUE (provider_account_id, plan_id, readiness_assessment_id, apply_version, phase)
);

CREATE INDEX sync_apply_runs_account_idx
    ON sync_apply_runs (provider_account_id, started_at DESC, id DESC);

CREATE TABLE sync_apply_operations (
    apply_run_id UUID NOT NULL REFERENCES sync_apply_runs (id) ON DELETE CASCADE,
    planned_operation_id UUID NOT NULL REFERENCES sync_operations (id) ON DELETE RESTRICT,
    sequence INTEGER NOT NULL CHECK (sequence >= 0),
    operation_key TEXT NOT NULL CHECK (btrim(operation_key) <> ''),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'running', 'succeeded', 'failed', 'skipped')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    resolved_spotify_playlist_id TEXT,
    provider_response JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_error TEXT,
    started_at TIMESTAMPTZ,
    executed_at TIMESTAMPTZ,
    PRIMARY KEY (apply_run_id, planned_operation_id),
    UNIQUE (apply_run_id, sequence),
    UNIQUE (apply_run_id, operation_key)
);

CREATE INDEX sync_apply_operations_status_idx
    ON sync_apply_operations (apply_run_id, status, sequence);

CREATE TABLE sync_apply_playlist_targets (
    apply_run_id UUID NOT NULL REFERENCES sync_apply_runs (id) ON DELETE CASCADE,
    playlist_id UUID REFERENCES playlists (id) ON DELETE RESTRICT,
    concept_id UUID REFERENCES playlist_concepts (id) ON DELETE RESTRICT,
    playlist_name TEXT NOT NULL CHECK (btrim(playlist_name) <> ''),
    spotify_playlist_id TEXT NOT NULL CHECK (btrim(spotify_playlist_id) <> ''),
    provider_snapshot_id TEXT,
    resolved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (apply_run_id, spotify_playlist_id)
);

CREATE UNIQUE INDEX sync_apply_playlist_targets_playlist_uq
    ON sync_apply_playlist_targets (apply_run_id, playlist_id)
    WHERE playlist_id IS NOT NULL;
CREATE UNIQUE INDEX sync_apply_playlist_targets_concept_uq
    ON sync_apply_playlist_targets (apply_run_id, concept_id)
    WHERE concept_id IS NOT NULL;

COMMENT ON TABLE sync_apply_runs IS
    'Explicitly confirmed, resumable execution of one phase from one ready immutable Spotify plan.';
COMMENT ON TABLE sync_apply_operations IS
    'Durable per-operation execution ledger; successful operations are never blindly replayed.';
COMMENT ON TABLE sync_apply_playlist_targets IS
    'Provider IDs resolved or created during apply so later operations and interrupted resumes target the same playlist.';

CREATE TABLE sync_retirement_approvals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL REFERENCES provider_accounts (id) ON DELETE CASCADE,
    plan_id UUID NOT NULL REFERENCES sync_runs (id) ON DELETE RESTRICT,
    plan_input_hash TEXT NOT NULL CHECK (plan_input_hash ~ '^[0-9a-f]{64}$'),
    operation_count INTEGER NOT NULL CHECK (operation_count > 0),
    approved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_account_id, plan_id)
);

COMMENT ON TABLE sync_retirement_approvals IS
    'Exact durable approval for all legacy retirement operations in one inspected immutable plan.';
