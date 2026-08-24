CREATE VIEW current_spotify_playlists AS
SELECT
    account.id AS provider_account_id,
    library.id AS snapshot_id,
    provider_playlist.id AS provider_playlist_id,
    provider_playlist.provider_playlist_id AS spotify_playlist_id,
    snapshot.name,
    snapshot.description,
    snapshot.total_items,
    snapshot.public,
    snapshot.collaborative,
    account_playlist.role,
    account_playlist.drift_policy,
    account_playlist.signal_class,
    account_playlist.behavioral_signal,
    account_playlist.semantic_weight,
    account_playlist.clear_policy
FROM provider_accounts account
JOIN LATERAL (
    SELECT candidate.id
    FROM provider_library_snapshots candidate
    WHERE candidate.provider_account_id = account.id
    ORDER BY candidate.captured_at DESC, candidate.id DESC
    LIMIT 1
) library ON TRUE
JOIN provider_playlist_snapshots snapshot ON snapshot.snapshot_id = library.id
JOIN provider_playlists provider_playlist
  ON provider_playlist.id = snapshot.provider_playlist_id
 AND provider_playlist.provider = 'spotify'
JOIN provider_account_playlists account_playlist
  ON account_playlist.provider_account_id = account.id
 AND account_playlist.provider_playlist_id = provider_playlist.id
 AND account_playlist.present_in_latest_snapshot
WHERE account.provider = 'spotify';

COMMENT ON VIEW current_spotify_playlists IS
    'Only playlists and mutable names in each Spotify account latest successful imported snapshot; historical names remain in provider_playlist_snapshots.';
