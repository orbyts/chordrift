ALTER TABLE sync_operations
    DROP CONSTRAINT sync_operations_operation_type_check,
    ADD CONSTRAINT sync_operations_operation_type_check CHECK (
        operation_type IN (
            'create_playlist', 'rename_playlist', 'add_track', 'restore_track',
            'exclude_track', 'remove_track', 'reorder_track', 'reorder_playlist',
            'upload_artwork', 'remove_external_playlist', 'archive_playlist'
        )
    );

COMMENT ON CONSTRAINT sync_operations_operation_type_check ON sync_operations IS
    'Provider-neutral exact operations, including non-destructive whole-playlist order convergence.';
