CREATE TABLE sync_readiness_assessments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL
        REFERENCES provider_accounts (id) ON DELETE CASCADE,
    sync_run_id UUID NOT NULL REFERENCES sync_runs (id) ON DELETE RESTRICT,
    artwork_batch_id UUID
        REFERENCES playlist_artwork_batches (id) ON DELETE RESTRICT,
    assessment_version TEXT NOT NULL CHECK (btrim(assessment_version) <> ''),
    input_hash TEXT NOT NULL CHECK (input_hash ~ '^[0-9a-f]{64}$'),
    status TEXT NOT NULL CHECK (status IN ('ready', 'blocked')),
    provider_probe_performed BOOLEAN NOT NULL DEFAULT FALSE,
    check_count INTEGER NOT NULL CHECK (check_count > 0),
    passed_check_count INTEGER NOT NULL CHECK (
        passed_check_count >= 0 AND passed_check_count <= check_count),
    operation_count INTEGER NOT NULL CHECK (operation_count >= 0),
    restart_checkpoints INTEGER NOT NULL CHECK (restart_checkpoints >= 0),
    replay_changes INTEGER NOT NULL CHECK (replay_changes >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_account_id, sync_run_id, assessment_version, input_hash)
);

CREATE INDEX sync_readiness_assessments_account_idx
    ON sync_readiness_assessments
    (provider_account_id, created_at DESC, id DESC);

CREATE TABLE sync_readiness_checks (
    assessment_id UUID NOT NULL
        REFERENCES sync_readiness_assessments (id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK (sequence >= 0),
    check_name TEXT NOT NULL CHECK (btrim(check_name) <> ''),
    status TEXT NOT NULL CHECK (status IN ('passed', 'blocked')),
    evidence JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (assessment_id, sequence),
    UNIQUE (assessment_id, check_name)
);

COMMENT ON TABLE sync_readiness_assessments IS
    'Immutable read-only proof that an exact dry-run satisfies future Spotify apply safety gates.';
COMMENT ON TABLE sync_readiness_checks IS
    'Inspectable approval, freshness, ordering, recovery, retry, convergence, and scope checks.';
