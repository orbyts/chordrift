ALTER TABLE listening_events
    ADD COLUMN source_occurrence INTEGER NOT NULL DEFAULT 0
        CHECK (source_occurrence >= 0);

WITH ranked AS (
    SELECT id,
           row_number() OVER (
               PARTITION BY provider_account_id, provider, provider_track_id,
                            played_at, ms_played
               ORDER BY id
           ) - 1 AS occurrence
    FROM listening_events
    WHERE provider_account_id IS NOT NULL
      AND provider_track_id IS NOT NULL
      AND ms_played IS NOT NULL
)
UPDATE listening_events event
SET source_occurrence = ranked.occurrence
FROM ranked
WHERE event.id = ranked.id;

CREATE UNIQUE INDEX listening_events_account_core_event_idx
    ON listening_events
       (provider_account_id, provider, provider_track_id,
        played_at, ms_played, source_occurrence)
    WHERE provider_account_id IS NOT NULL
      AND provider_track_id IS NOT NULL
      AND ms_played IS NOT NULL;
