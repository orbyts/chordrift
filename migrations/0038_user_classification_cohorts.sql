ALTER TABLE track_classification_revisions
    ADD COLUMN cohorts TEXT[] NOT NULL DEFAULT '{}';

ALTER TABLE track_classification_batch_entries
    ADD COLUMN cohorts TEXT[] NOT NULL DEFAULT '{}';

COMMENT ON COLUMN track_classification_revisions.cohorts IS
    'Private personal groupings such as ar-rahman-favorites; unlike tradition, a cohort does not claim that its tracks sound alike.';
