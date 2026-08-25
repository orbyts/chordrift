ALTER TABLE playlist_artwork_artifacts
    ALTER COLUMN playlist_id DROP NOT NULL,
    ADD COLUMN target_kind TEXT NOT NULL DEFAULT 'canonical'
        CHECK (target_kind IN ('canonical', 'intake'));

COMMENT ON COLUMN playlist_artwork_artifacts.target_kind IS
    'Canonical outputs resolve by concept; intake covers resolve by stable Chordrift surface name and may predate provider creation.';

UPDATE provider_account_playlists policy
SET behavioral_signal = CASE current.name
        WHEN 'From Friends' THEN 'recommendation'
        WHEN 'Liked from Radio' THEN 'discovery'
        WHEN 'From Prompts' THEN 'prompted'
        ELSE NULL
    END,
    updated_at = now()
FROM current_spotify_playlists current
WHERE current.provider_account_id = policy.provider_account_id
  AND current.provider_playlist_id = policy.provider_playlist_id
  AND current.signal_class = 'intake'
  AND current.name IN ('Inbox', 'From Friends', 'Liked from Radio', 'From Prompts');
