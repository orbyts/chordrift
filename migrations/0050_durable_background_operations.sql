-- Durable authenticated application work. Commands are accepted before a
-- worker lease, lifecycle events are append-only, and idempotency survives
-- process restarts. Payloads are typed application DTO JSON, never credentials.

CREATE TABLE service_operations (
    id UUID PRIMARY KEY,
    chordrift_account_id UUID NOT NULL,
    product_subject_id UUID NOT NULL,
    request_id UUID NOT NULL,
    cancellation_id UUID NOT NULL UNIQUE,
    idempotency_key UUID NOT NULL,
    command_fingerprint BYTEA NOT NULL
        CHECK (octet_length(command_fingerprint) = 32),
    command_payload JSONB NOT NULL
        CHECK (jsonb_typeof(command_payload) = 'object'),
    state_name TEXT NOT NULL
        CHECK (state_name IN
            ('queued', 'running', 'waiting', 'completed', 'failed',
             'cancelled', 'recoverable')),
    state_payload JSONB NOT NULL
        CHECK (jsonb_typeof(state_payload) = 'object'),
    attempt INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 100),
    retry_delay_seconds INTEGER NOT NULL
        CHECK (retry_delay_seconds BETWEEN 0 AND 86400),
    next_attempt_at TIMESTAMPTZ NOT NULL,
    lease_id UUID,
    lease_owner TEXT,
    lease_expires_at TIMESTAMPTZ,
    cancellation_requested_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ,
    CHECK ((lease_id IS NULL AND lease_owner IS NULL AND lease_expires_at IS NULL)
        OR (lease_id IS NOT NULL AND lease_owner IS NOT NULL
            AND btrim(lease_owner) <> '' AND lease_expires_at IS NOT NULL)),
    CHECK ((state_name IN ('completed', 'failed', 'cancelled')) =
           (finished_at IS NOT NULL)),
    FOREIGN KEY (chordrift_account_id, product_subject_id)
        REFERENCES chordrift_account_memberships
            (chordrift_account_id, product_subject_id) ON DELETE RESTRICT,
    UNIQUE (chordrift_account_id, product_subject_id, idempotency_key)
);

CREATE TABLE service_operation_events (
    operation_id UUID NOT NULL
        REFERENCES service_operations (id) ON DELETE CASCADE,
    sequence BIGINT NOT NULL CHECK (sequence > 0),
    occurred_at TIMESTAMPTZ NOT NULL,
    state_name TEXT NOT NULL
        CHECK (state_name IN
            ('queued', 'running', 'waiting', 'completed', 'failed',
             'cancelled', 'recoverable')),
    state_payload JSONB NOT NULL
        CHECK (jsonb_typeof(state_payload) = 'object'),
    PRIMARY KEY (operation_id, sequence)
);

CREATE INDEX service_operations_claim_idx
    ON service_operations (next_attempt_at, created_at)
    WHERE state_name IN ('queued', 'recoverable');
CREATE INDEX service_operations_expired_lease_idx
    ON service_operations (lease_expires_at)
    WHERE state_name = 'running';
CREATE INDEX service_operations_subject_history_idx
    ON service_operations
       (chordrift_account_id, product_subject_id, created_at, id);
CREATE INDEX service_operation_events_cursor_idx
    ON service_operation_events (operation_id, sequence);

COMMENT ON TABLE service_operations IS
    'Restart-safe typed application commands and current lifecycle state; contains no provider credential.';
COMMENT ON TABLE service_operation_events IS
    'Immutable ordered operation lifecycle/progress stream used by reconnecting clients.';
