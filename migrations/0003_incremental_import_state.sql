-- Backfill the raw saved-library size for snapshots created before copy-forward imports.
-- Positions preserve Spotify's original ordering even when an unsupported item was skipped.

UPDATE provider_library_snapshots AS snapshots
SET metadata = jsonb_set(
    snapshots.metadata,
    '{saved_items_seen}',
    to_jsonb(saved_totals.total_items),
    TRUE
)
FROM (
    SELECT snapshot_id, max(position) + 1 AS total_items
    FROM provider_saved_tracks
    GROUP BY snapshot_id
) AS saved_totals
WHERE snapshots.id = saved_totals.snapshot_id
  AND NOT snapshots.metadata ? 'saved_items_seen';
