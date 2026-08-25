CREATE FUNCTION account_track_is_library_candidate(
    p_account_id UUID,
    p_canonical_track_id UUID
)
RETURNS BOOLEAN
LANGUAGE sql
STABLE
AS $$
    SELECT EXISTS (
        SELECT 1
        FROM provider_library_snapshots snapshot
        JOIN provider_saved_tracks saved ON saved.snapshot_id = snapshot.id
        JOIN provider_tracks provider_track ON provider_track.id = saved.provider_track_id
        WHERE snapshot.id = (
            SELECT latest.id
            FROM provider_library_snapshots latest
            WHERE latest.provider_account_id = p_account_id
            ORDER BY latest.captured_at DESC, latest.id DESC
            LIMIT 1
        )
          AND provider_track.track_id = p_canonical_track_id
    ) OR EXISTS (
        SELECT 1
        FROM provider_account_playlists policy
        JOIN provider_library_snapshots snapshot
          ON snapshot.provider_account_id = policy.provider_account_id
        JOIN provider_playlist_tracks membership
          ON membership.snapshot_id = snapshot.id
         AND membership.provider_playlist_id = policy.provider_playlist_id
        JOIN provider_tracks provider_track ON provider_track.id = membership.provider_track_id
        WHERE policy.provider_account_id = p_account_id
          AND policy.signal_class IN ('semantic_legacy', 'transport', 'intake', 'canonical')
          AND provider_track.track_id = p_canonical_track_id
    ) OR EXISTS (
        SELECT 1
        FROM excluded_tracks exclusion
        WHERE exclusion.provider_account_id = p_account_id
          AND exclusion.track_id = p_canonical_track_id
          AND exclusion.restored_at IS NULL
    )
$$;

COMMENT ON FUNCTION account_track_is_library_candidate(UUID, UUID) IS
    'True for the account preservation universe: latest saved tracks, durable user semantic/transport/intake/canonical membership history, or an active reversible exclusion. Listening history alone is enrichment, not library membership.';

CREATE OR REPLACE FUNCTION account_track_is_eligible(
    p_account_id UUID,
    p_canonical_track_id UUID
)
RETURNS BOOLEAN
LANGUAGE sql
STABLE
AS $$
    SELECT account_track_is_library_candidate(p_account_id, p_canonical_track_id)
$$;

COMMENT ON FUNCTION account_track_is_eligible(UUID, UUID) IS
    'Compatibility wrapper for enrichment jobs; eligibility follows the complete preserved-library inventory.';
