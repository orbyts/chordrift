ALTER TABLE provider_account_playlists
    DROP CONSTRAINT provider_account_playlists_clear_policy_class_check,
    ADD CONSTRAINT provider_account_playlists_clear_policy_class_check
        CHECK (
            clear_policy = 'never'
            OR signal_class IN ('intake', 'routing')
        );

COMMENT ON CONSTRAINT provider_account_playlists_clear_policy_class_check
    ON provider_account_playlists IS
    'Only intake and zero-signal routing inboxes may clear after verified canonical assignment.';
