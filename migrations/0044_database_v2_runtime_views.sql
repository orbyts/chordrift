-- Stable database-v2 runtime read surfaces. These views deliberately read
-- content-addressed current state, compact checkpoints, and normalized
-- evidence rather than the duplicated legacy snapshot/event tables.
--
-- The observation view is temporarily backed by the lightweight legacy
-- snapshot header. The exact cleanup phase can rename that table in place
-- after every runtime query uses this v2 name; no provider inventory body is
-- retained there.

CREATE VIEW provider_inventory_observations AS
SELECT id, provider, source, metadata, captured_at, provider_account_id
  FROM provider_library_snapshots;

-- New pulls use these explicitly transient import surfaces. During the
-- observation window they are updatable aliases over the old physical tables.
-- The separately approved cleanup renames those tables to these v2 names after
-- removing historical duplicate rows, so runtime SQL remains unchanged at the
-- destructive cutover.
CREATE VIEW provider_inventory_import_playlists AS
SELECT snapshot_id, provider_playlist_id, name, description,
       provider_snapshot_id, public, collaborative, total_items, metadata
  FROM provider_playlist_snapshots;

CREATE VIEW provider_inventory_import_playlist_tracks AS
SELECT snapshot_id, provider_playlist_id, provider_track_id, position,
       added_at, metadata, captured_at
  FROM provider_playlist_tracks;

CREATE VIEW provider_inventory_import_saved_tracks AS
SELECT snapshot_id, provider_track_id, position, saved_at, metadata
  FROM provider_saved_tracks;

CREATE VIEW provider_inventory_import_saved_albums AS
SELECT snapshot_id, provider_album_id, position, saved_at, metadata
  FROM provider_saved_albums;

CREATE VIEW provider_inventory_import_saved_album_tracks AS
SELECT snapshot_id, provider_album_id, provider_track_id, position, metadata
  FROM provider_saved_album_tracks;

CREATE VIEW provider_observed_playlists AS
SELECT inventory.source_snapshot_id AS snapshot_id,
       current_playlist.provider_playlist_id,
       current_playlist.name,
       current_playlist.description,
       current_playlist.provider_revision AS provider_snapshot_id,
       current_playlist.public,
       current_playlist.collaborative,
       current_playlist.reported_item_count AS total_items,
       current_playlist.metadata
  FROM provider_current_inventories inventory
  JOIN provider_current_playlists current_playlist
    ON current_playlist.provider_account_id = inventory.provider_account_id
 WHERE inventory.source_snapshot_id IS NOT NULL
UNION ALL
SELECT checkpoint.source_snapshot_id,
       playlist.provider_playlist_id,
       playlist.name,
       playlist.description,
       playlist.provider_revision,
       playlist.public,
       playlist.collaborative,
       revision.item_count,
       '{}'::jsonb
  FROM provider_inventory_checkpoints checkpoint
  JOIN provider_inventory_checkpoint_playlists playlist
    ON playlist.checkpoint_id = checkpoint.id
  JOIN provider_playlist_revisions revision ON revision.id = playlist.revision_id
 WHERE checkpoint.source_snapshot_id IS NOT NULL
   AND checkpoint.source_snapshot_id IS DISTINCT FROM (
       SELECT current_inventory.source_snapshot_id
         FROM provider_current_inventories current_inventory
        WHERE current_inventory.provider_account_id = checkpoint.provider_account_id
   );

CREATE VIEW provider_observed_playlist_tracks AS
SELECT inventory.source_snapshot_id AS snapshot_id,
       current_playlist.provider_playlist_id,
       membership.provider_track_id,
       membership.position,
       membership.added_at,
       membership.metadata,
       inventory.captured_at
  FROM provider_current_inventories inventory
  JOIN provider_current_playlists current_playlist
    ON current_playlist.provider_account_id = inventory.provider_account_id
  JOIN provider_playlist_revision_tracks membership
    ON membership.revision_id = current_playlist.revision_id
 WHERE inventory.source_snapshot_id IS NOT NULL
UNION ALL
SELECT checkpoint.source_snapshot_id,
       playlist.provider_playlist_id,
       membership.provider_track_id,
       membership.position,
       membership.added_at,
       membership.metadata,
       checkpoint.captured_at
  FROM provider_inventory_checkpoints checkpoint
  JOIN provider_inventory_checkpoint_playlists playlist
    ON playlist.checkpoint_id = checkpoint.id
  JOIN provider_playlist_revision_tracks membership
    ON membership.revision_id = playlist.revision_id
 WHERE checkpoint.source_snapshot_id IS NOT NULL
   AND checkpoint.source_snapshot_id IS DISTINCT FROM (
       SELECT current_inventory.source_snapshot_id
         FROM provider_current_inventories current_inventory
        WHERE current_inventory.provider_account_id = checkpoint.provider_account_id
   );

CREATE VIEW provider_observed_saved_tracks AS
SELECT inventory.source_snapshot_id AS snapshot_id,
       membership.provider_track_id,
       membership.position,
       membership.saved_at,
       membership.metadata
  FROM provider_current_inventories inventory
  JOIN provider_saved_track_revision_tracks membership
    ON membership.revision_id = inventory.saved_track_revision_id
 WHERE inventory.source_snapshot_id IS NOT NULL
UNION ALL
SELECT checkpoint.source_snapshot_id,
       membership.provider_track_id,
       membership.position,
       membership.saved_at,
       membership.metadata
  FROM provider_inventory_checkpoints checkpoint
  JOIN provider_inventory_checkpoint_saved_surfaces surfaces
    ON surfaces.checkpoint_id = checkpoint.id
  JOIN provider_saved_track_revision_tracks membership
    ON membership.revision_id = surfaces.saved_track_revision_id
 WHERE checkpoint.source_snapshot_id IS NOT NULL
   AND checkpoint.source_snapshot_id IS DISTINCT FROM (
       SELECT current_inventory.source_snapshot_id
         FROM provider_current_inventories current_inventory
        WHERE current_inventory.provider_account_id = checkpoint.provider_account_id
   );

CREATE VIEW provider_observed_saved_albums AS
SELECT inventory.source_snapshot_id AS snapshot_id,
       membership.provider_album_id,
       membership.position,
       membership.saved_at,
       membership.metadata
  FROM provider_current_inventories inventory
  JOIN provider_saved_album_revision_albums membership
    ON membership.revision_id = inventory.saved_album_revision_id
 WHERE inventory.source_snapshot_id IS NOT NULL
UNION ALL
SELECT checkpoint.source_snapshot_id,
       membership.provider_album_id,
       membership.position,
       membership.saved_at,
       membership.metadata
  FROM provider_inventory_checkpoints checkpoint
  JOIN provider_inventory_checkpoint_saved_surfaces surfaces
    ON surfaces.checkpoint_id = checkpoint.id
  JOIN provider_saved_album_revision_albums membership
    ON membership.revision_id = surfaces.saved_album_revision_id
 WHERE checkpoint.source_snapshot_id IS NOT NULL
   AND checkpoint.source_snapshot_id IS DISTINCT FROM (
       SELECT current_inventory.source_snapshot_id
         FROM provider_current_inventories current_inventory
        WHERE current_inventory.provider_account_id = checkpoint.provider_account_id
   );

CREATE VIEW provider_observed_saved_album_tracks AS
SELECT inventory.source_snapshot_id AS snapshot_id,
       membership.provider_album_id,
       membership.provider_track_id,
       membership.position,
       membership.metadata
  FROM provider_current_inventories inventory
  JOIN provider_saved_album_revision_tracks membership
    ON membership.revision_id = inventory.saved_album_revision_id
 WHERE inventory.source_snapshot_id IS NOT NULL
UNION ALL
SELECT checkpoint.source_snapshot_id,
       membership.provider_album_id,
       membership.provider_track_id,
       membership.position,
       membership.metadata
  FROM provider_inventory_checkpoints checkpoint
  JOIN provider_inventory_checkpoint_saved_surfaces surfaces
    ON surfaces.checkpoint_id = checkpoint.id
  JOIN provider_saved_album_revision_tracks membership
    ON membership.revision_id = surfaces.saved_album_revision_id
 WHERE checkpoint.source_snapshot_id IS NOT NULL
   AND checkpoint.source_snapshot_id IS DISTINCT FROM (
       SELECT current_inventory.source_snapshot_id
         FROM provider_current_inventories current_inventory
        WHERE current_inventory.provider_account_id = checkpoint.provider_account_id
   );

CREATE OR REPLACE VIEW current_spotify_playlists AS
SELECT account.id AS provider_account_id,
       inventory.source_snapshot_id AS snapshot_id,
       provider_playlist.id AS provider_playlist_id,
       provider_playlist.provider_playlist_id AS spotify_playlist_id,
       current_playlist.name,
       current_playlist.description,
       current_playlist.reported_item_count AS total_items,
       current_playlist.public,
       current_playlist.collaborative,
       account_playlist.role,
       account_playlist.drift_policy,
       account_playlist.signal_class,
       account_playlist.behavioral_signal,
       account_playlist.semantic_weight,
       account_playlist.clear_policy
  FROM provider_accounts account
  JOIN provider_current_inventories inventory
    ON inventory.provider_account_id = account.id
  JOIN provider_current_playlists current_playlist
    ON current_playlist.provider_account_id = account.id
  JOIN provider_playlists provider_playlist
    ON provider_playlist.id = current_playlist.provider_playlist_id
   AND provider_playlist.provider = 'spotify'
  JOIN provider_account_playlists account_playlist
    ON account_playlist.provider_account_id = account.id
   AND account_playlist.provider_playlist_id = provider_playlist.id
   AND account_playlist.present_in_latest_snapshot
 WHERE account.provider = 'spotify';

COMMENT ON VIEW current_spotify_playlists IS
    'Current Spotify playlist headers backed by the database-v2 current inventory pointer.';

CREATE VIEW listening_evidence_events AS
SELECT event.id,
       identity.canonical_track_id AS track_id,
       identity.provider,
       identity.provider_track_id,
       event.source_event_id,
       event.played_at,
       event.ms_played,
       event.skipped,
       event.context_uri,
       jsonb_strip_nulls(jsonb_build_object(
           'track_name', identity.track_name,
           'artist_name', identity.artist_name,
           'album_name', identity.album_name,
           'reason_end', event.completion_reason,
           'context_type', event.context_type
       )) || event.provider_extensions AS raw_metadata,
       event.played_at AS imported_at,
       event.provider_account_id,
       event.source_import_id,
       source_file.source_path AS source_file,
       'track'::text AS media_type,
       event.source_kind,
       event.source_occurrence,
       event.superseded_at
  FROM normalized_listening_events event
  JOIN historical_provider_track_identities identity
    ON identity.id = event.historical_identity_id
  LEFT JOIN listening_evidence_source_files source_file
    ON source_file.id = event.source_file_id;

COMMENT ON VIEW provider_inventory_observations IS
    'Lightweight provider pull receipts; runtime code must not infer that each observation owns a duplicated inventory body.';
COMMENT ON VIEW provider_observed_playlist_tracks IS
    'Reconstructed current/protected playlist evidence backed by content-addressed revisions and compact checkpoints.';
COMMENT ON VIEW listening_evidence_events IS
    'Typed normalized listening evidence with provider identity display metadata joined once for runtime reads.';

CREATE TABLE database_v2_cleanup_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_account_id UUID NOT NULL
        REFERENCES provider_accounts (id) ON DELETE CASCADE,
    plan_sha256 TEXT NOT NULL CHECK (plan_sha256 ~ '^[0-9a-f]{64}$'),
    cleanup_version TEXT NOT NULL CHECK (btrim(cleanup_version) <> ''),
    legacy_snapshot_count BIGINT NOT NULL CHECK (legacy_snapshot_count >= 0),
    legacy_provider_row_count BIGINT NOT NULL CHECK (legacy_provider_row_count >= 0),
    legacy_listening_event_count BIGINT NOT NULL CHECK (legacy_listening_event_count >= 0),
    legacy_archive_import_count BIGINT NOT NULL CHECK (legacy_archive_import_count >= 0),
    invariant_sha256 TEXT NOT NULL CHECK (invariant_sha256 ~ '^[0-9a-f]{64}$'),
    applied_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    verified_at TIMESTAMPTZ,
    verification JSONB NOT NULL DEFAULT '{}'::jsonb,
    UNIQUE (provider_account_id, plan_sha256)
);

COMMENT ON TABLE database_v2_cleanup_runs IS
    'Exact-confirmed receipts for removing database-v1 duplicate bodies after database-v2 invariant verification.';
