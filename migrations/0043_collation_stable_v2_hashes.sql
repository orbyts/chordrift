-- Make database-v2 inventory hashes stable across PostgreSQL installations
-- with different default collations. Playlist/saved revision hashes already
-- order primarily by unique numeric positions; this wrapper normalizes the
-- remaining set-level inventory ordering explicitly.

ALTER FUNCTION materialize_provider_current_state_v2(UUID, UUID)
    RENAME TO materialize_provider_current_state_v2_collation_legacy;

CREATE OR REPLACE FUNCTION materialize_provider_current_state_v2(
    p_provider_account_id UUID,
    p_snapshot_id UUID
)
RETURNS VOID
LANGUAGE plpgsql
AS $$
DECLARE
    v_state_hash TEXT;
BEGIN
    PERFORM materialize_provider_current_state_v2_collation_legacy(
        p_provider_account_id, p_snapshot_id
    );

    WITH state_lines AS (
        SELECT 'playlist:' || playlist.provider_playlist_id || ':' ||
               revision.content_sha256 AS line
          FROM provider_current_playlists current_playlist
          JOIN provider_playlists playlist
            ON playlist.id = current_playlist.provider_playlist_id
          JOIN provider_playlist_revisions revision
            ON revision.id = current_playlist.revision_id
         WHERE current_playlist.provider_account_id = p_provider_account_id
        UNION ALL
        SELECT 'saved_tracks:' || saved_track.content_sha256
          FROM provider_current_inventories inventory
          JOIN provider_saved_track_revisions saved_track
            ON saved_track.id = inventory.saved_track_revision_id
         WHERE inventory.provider_account_id = p_provider_account_id
        UNION ALL
        SELECT 'saved_albums:' || saved_album.content_sha256
          FROM provider_current_inventories inventory
          JOIN provider_saved_album_revisions saved_album
            ON saved_album.id = inventory.saved_album_revision_id
         WHERE inventory.provider_account_id = p_provider_account_id
    )
    SELECT encode(sha256(convert_to(string_agg(
               line, E'\n' ORDER BY line COLLATE "C"
           ), 'UTF8')), 'hex')
      INTO v_state_hash
      FROM state_lines;

    UPDATE provider_current_inventories
       SET state_sha256 = v_state_hash,
           updated_at = now()
     WHERE provider_account_id = p_provider_account_id;
END;
$$;

COMMENT ON FUNCTION materialize_provider_current_state_v2(UUID, UUID) IS
    'Transactionally replaces one current provider state, reuses content-addressed revisions, and hashes inventory lines with explicit bytewise ordering.';

WITH current_sources AS (
    SELECT provider_account_id, source_snapshot_id
      FROM provider_current_inventories
     WHERE source_snapshot_id IS NOT NULL
)
SELECT materialize_provider_current_state_v2(
           provider_account_id, source_snapshot_id
       )
  FROM current_sources;
