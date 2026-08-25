ALTER TABLE provider_account_playlists
    ALTER COLUMN semantic_weight SET DEFAULT 0.0;

COMMENT ON COLUMN provider_account_playlists.semantic_weight IS
    'Semantic contribution for explicit semantic_legacy playlists only; protected user-managed playlists default to zero.';
