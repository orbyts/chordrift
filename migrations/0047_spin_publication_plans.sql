-- Spin publication plans reuse the existing synchronization ledger without
-- pretending that a legacy canonical proposal generation owns the Spin.

ALTER TABLE playlist_surfaces
    ADD COLUMN recipe_id UUID;

ALTER TABLE playlist_surfaces
    ADD CONSTRAINT playlist_surfaces_recipe_fk
        FOREIGN KEY (chordrift_account_id, recipe_id)
        REFERENCES playlist_recipes (chordrift_account_id, id) ON DELETE RESTRICT;

CREATE INDEX playlist_surfaces_recipe_idx
    ON playlist_surfaces (chordrift_account_id, recipe_id)
    WHERE recipe_id IS NOT NULL;

ALTER TABLE sync_runs
    DROP CONSTRAINT sync_runs_dry_run_identity_check,
    ADD CONSTRAINT sync_runs_dry_run_identity_check CHECK (
        mode <> 'dry_run' OR planner_version IS NULL
        OR (provider_account_id IS NOT NULL
            AND source_snapshot_id IS NOT NULL
            AND planner_version IS NOT NULL AND btrim(planner_version) <> ''
            AND input_hash IS NOT NULL AND input_hash ~ '^[0-9a-f]{64}$'
            AND (proposal_generation_id IS NOT NULL
                OR (provider_checkpoint_id IS NOT NULL
                    AND preconditions ->> 'plan_origin' = 'spin_publication')))
    );

COMMENT ON COLUMN playlist_surfaces.recipe_id IS
    'Optional owning recipe. Spin publication requires this to match the approved Spin recipe; legacy and non-renewable surfaces may remain unbound.';
COMMENT ON CONSTRAINT sync_runs_dry_run_identity_check ON sync_runs IS
    'Maintenance plans bind a proposal generation; Spin publication plans bind an inventory checkpoint and explicit spin_publication origin.';
