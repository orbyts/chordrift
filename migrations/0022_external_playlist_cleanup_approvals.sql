CREATE TABLE external_playlist_cleanup_batches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL
        REFERENCES provider_accounts (id) ON DELETE CASCADE,
    source_snapshot_id UUID NOT NULL
        REFERENCES provider_library_snapshots (id) ON DELETE RESTRICT,
    input_hash TEXT NOT NULL CHECK (input_hash ~ '^[0-9a-f]{64}$'),
    state TEXT NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'approved', 'superseded')),
    candidate_count INTEGER NOT NULL CHECK (candidate_count > 0),
    approved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((state = 'approved' AND approved_at IS NOT NULL)
        OR (state <> 'approved' AND approved_at IS NULL)),
    UNIQUE (provider_account_id, input_hash)
);

CREATE INDEX external_playlist_cleanup_batches_account_idx
    ON external_playlist_cleanup_batches
    (provider_account_id, created_at DESC, id DESC);

CREATE TABLE external_playlist_cleanup_items (
    batch_id UUID NOT NULL
        REFERENCES external_playlist_cleanup_batches (id) ON DELETE CASCADE,
    bookmark_id UUID NOT NULL
        REFERENCES external_playlist_bookmarks (id) ON DELETE RESTRICT,
    provider_playlist_id TEXT NOT NULL CHECK (btrim(provider_playlist_id) <> ''),
    provider_snapshot_id TEXT,
    name TEXT NOT NULL CHECK (btrim(name) <> ''),
    owner_provider_id TEXT NOT NULL CHECK (btrim(owner_provider_id) <> ''),
    content_status TEXT NOT NULL
        CHECK (content_status IN ('complete', 'metadata_only', 'inaccessible')),
    item_count INTEGER NOT NULL CHECK (item_count >= 0),
    PRIMARY KEY (batch_id, bookmark_id),
    UNIQUE (batch_id, provider_playlist_id)
);

ALTER TABLE sync_operations
    DROP CONSTRAINT sync_operations_operation_type_check,
    ADD CONSTRAINT sync_operations_operation_type_check CHECK (
        operation_type IN (
            'create_playlist', 'rename_playlist', 'add_track', 'restore_track',
            'exclude_track', 'remove_track', 'reorder_track',
            'remove_external_playlist', 'archive_playlist'
        )
    );

COMMENT ON TABLE external_playlist_cleanup_batches IS
    'Immutable review and approval boundary for removing external playlist relationships from a provider library.';
COMMENT ON TABLE external_playlist_cleanup_items IS
    'Exact externally owned playlist identities and snapshot signatures approved for relationship-only cleanup.';
