ALTER TABLE sync_operations
    DROP CONSTRAINT sync_operations_operation_type_check,
    ADD CONSTRAINT sync_operations_operation_type_check CHECK (
        operation_type IN (
            'create_playlist', 'rename_playlist', 'add_track', 'restore_track',
            'exclude_track', 'remove_track', 'reorder_track', 'archive_playlist'
        )
    );

CREATE TABLE managed_playlist_verifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL REFERENCES provider_accounts (id) ON DELETE CASCADE,
    provider_playlist_id UUID NOT NULL REFERENCES provider_playlists (id) ON DELETE CASCADE,
    concept_id UUID NOT NULL REFERENCES playlist_concepts (id) ON DELETE RESTRICT,
    proposal_generation_id UUID NOT NULL REFERENCES playlist_generations (id) ON DELETE RESTRICT,
    verified_snapshot_id UUID NOT NULL REFERENCES provider_library_snapshots (id) ON DELETE RESTRICT,
    desired_state_hash TEXT NOT NULL CHECK (desired_state_hash ~ '^[0-9a-f]{64}$'),
    verified_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider_account_id, provider_playlist_id, verified_snapshot_id)
);

CREATE INDEX managed_playlist_verifications_latest_idx
    ON managed_playlist_verifications
       (provider_account_id, provider_playlist_id, verified_at DESC, id DESC);

CREATE TABLE managed_playlist_verified_tracks (
    verification_id UUID NOT NULL REFERENCES managed_playlist_verifications (id) ON DELETE CASCADE,
    track_id UUID NOT NULL REFERENCES tracks (id) ON DELETE RESTRICT,
    position INTEGER NOT NULL CHECK (position >= 0),
    PRIMARY KEY (verification_id, track_id),
    UNIQUE (verification_id, position)
);

COMMENT ON TABLE managed_playlist_verifications IS
    'Immutable proof that one managed provider playlist matched an approved desired state at one imported snapshot.';
COMMENT ON TABLE managed_playlist_verified_tracks IS
    'Expected membership baseline used to distinguish intentional managed-playlist removals from unrelated provider drift.';
