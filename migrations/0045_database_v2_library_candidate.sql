-- Replace the final database-v1 function body left behind by physical cleanup.
-- Candidate membership now comes from database-v2 current revision pointers,
-- the latest verified managed baseline (so a provider removal remains
-- reviewable), or durable active exclusion intent.

CREATE OR REPLACE FUNCTION account_track_is_library_candidate(
    p_account_id UUID,
    p_canonical_track_id UUID
)
RETURNS BOOLEAN
LANGUAGE sql
STABLE
AS $$
    SELECT EXISTS (
        SELECT 1
          FROM provider_current_inventories inventory
          JOIN provider_saved_track_revision_tracks saved
            ON saved.revision_id = inventory.saved_track_revision_id
          JOIN provider_tracks provider_track
            ON provider_track.id = saved.provider_track_id
         WHERE inventory.provider_account_id = p_account_id
           AND provider_track.track_id = p_canonical_track_id
    ) OR EXISTS (
        SELECT 1
          FROM provider_account_playlists policy
          JOIN provider_current_playlists current_playlist
            ON current_playlist.provider_account_id = policy.provider_account_id
           AND current_playlist.provider_playlist_id = policy.provider_playlist_id
          JOIN provider_playlist_revision_tracks membership
            ON membership.revision_id = current_playlist.revision_id
          JOIN provider_tracks provider_track
            ON provider_track.id = membership.provider_track_id
         WHERE policy.provider_account_id = p_account_id
           AND policy.present_in_latest_snapshot
           AND policy.signal_class IN
               ('user_managed', 'semantic_legacy', 'transport', 'intake', 'routing', 'canonical')
           AND provider_track.track_id = p_canonical_track_id
    ) OR EXISTS (
        SELECT 1
          FROM provider_account_playlists policy
          JOIN managed_playlist_verifications verification
            ON verification.provider_account_id = policy.provider_account_id
           AND verification.provider_playlist_id = policy.provider_playlist_id
          JOIN managed_playlist_verified_tracks membership
            ON membership.verification_id = verification.id
         WHERE policy.provider_account_id = p_account_id
           AND policy.signal_class IN
               ('user_managed', 'semantic_legacy', 'transport', 'intake', 'routing', 'canonical')
           AND verification.id = (
               SELECT latest.id
                 FROM managed_playlist_verifications latest
                WHERE latest.provider_account_id = verification.provider_account_id
                  AND latest.provider_playlist_id = verification.provider_playlist_id
                ORDER BY latest.verified_at DESC, latest.id DESC
                LIMIT 1
           )
           AND membership.track_id = p_canonical_track_id
    ) OR EXISTS (
        SELECT 1
          FROM excluded_tracks exclusion
         WHERE exclusion.provider_account_id = p_account_id
           AND exclusion.track_id = p_canonical_track_id
           AND exclusion.restored_at IS NULL
    )
$$;

COMMENT ON FUNCTION account_track_is_library_candidate(UUID, UUID) IS
    'True for current v2 saved/provider membership, the latest managed verification baseline, or active durable exclusion intent; listening history alone is enrichment.';
