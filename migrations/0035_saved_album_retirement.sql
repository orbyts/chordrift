-- Saved-album retirement removes only the Spotify library container. The
-- immutable snapshots and ordered track inventory remain queryable in Neon.

ALTER TABLE provider_account_library_policies
    DROP CONSTRAINT provider_account_library_policies_saved_album_policy_check,
    ADD CONSTRAINT provider_account_library_policies_saved_album_policy_check CHECK (
        saved_album_policy IN (
            'preserve', 'inventory_only', 'review_then_unsave', 'archive_only'
        )
    );

ALTER TABLE sync_operations
    DROP CONSTRAINT sync_operations_operation_type_check,
    ADD CONSTRAINT sync_operations_operation_type_check CHECK (
        operation_type IN (
            'create_playlist', 'rename_playlist', 'add_track', 'restore_track',
            'exclude_track', 'remove_track', 'remove_saved_track',
            'reorder_track', 'reorder_playlist', 'upload_artwork',
            'remove_external_playlist', 'archive_playlist', 'remove_saved_album'
        )
    );

COMMENT ON CONSTRAINT sync_operations_operation_type_check ON sync_operations IS
    'Provider-neutral exact operations, including separately approved saved-album container retirement.';
