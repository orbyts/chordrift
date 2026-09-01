-- Restart-safe ordinary-maintenance task state. The current typed projection
-- is query-efficient; every accepted revision is retained as an immutable
-- event so user intent and exact provider-effect reviews remain auditable.

CREATE TABLE maintenance_sessions (
    id UUID PRIMARY KEY,
    chordrift_account_id UUID NOT NULL,
    product_subject_id UUID NOT NULL,
    provider_account_id UUID NOT NULL,
    revision BIGINT NOT NULL CHECK (revision > 0),
    state_name TEXT NOT NULL CHECK (state_name IN
        ('reconciling', 'needs_decision', 'ready_for_authorization',
         'authorized', 'applying', 'verifying', 'in_sync', 'recoverable')),
    view_payload JSONB NOT NULL CHECK (jsonb_typeof(view_payload) = 'object'),
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (chordrift_account_id, product_subject_id)
        REFERENCES chordrift_account_memberships
            (chordrift_account_id, product_subject_id) ON DELETE RESTRICT,
    FOREIGN KEY (provider_account_id, chordrift_account_id)
        REFERENCES provider_accounts (id, chordrift_account_id) ON DELETE RESTRICT
);

CREATE TABLE maintenance_session_events (
    maintenance_session_id UUID NOT NULL
        REFERENCES maintenance_sessions (id) ON DELETE RESTRICT,
    revision BIGINT NOT NULL CHECK (revision > 0),
    transition_name TEXT NOT NULL CHECK (transition_name IN
        ('started', 'refreshed', 'resolved', 'authorized',
         'applying', 'verifying', 'verified', 'recoverable')),
    source_operation_id UUID REFERENCES service_operations (id) ON DELETE RESTRICT,
    view_payload JSONB NOT NULL CHECK (jsonb_typeof(view_payload) = 'object'),
    occurred_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (maintenance_session_id, revision)
);

CREATE INDEX maintenance_sessions_subject_idx
    ON maintenance_sessions
       (chordrift_account_id, product_subject_id, updated_at DESC, id);
CREATE INDEX maintenance_sessions_provider_idx
    ON maintenance_sessions (provider_account_id, updated_at DESC, id);

COMMENT ON TABLE maintenance_sessions IS
    'Current wrapper-neutral ordinary-maintenance session projection; contains no provider credential.';
COMMENT ON TABLE maintenance_session_events IS
    'Immutable accepted maintenance revisions preserving cumulative provider intent, decisions, and exact reviews.';
