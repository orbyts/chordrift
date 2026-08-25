ALTER TABLE track_classification_revisions
    ADD COLUMN decision TEXT NOT NULL DEFAULT 'set'
        CHECK (decision IN ('set', 'clear'));

ALTER TABLE track_classification_revisions
    ADD CONSTRAINT track_classification_revisions_decision_shape_check CHECK (
        decision = 'set'
        OR (collection IS NULL AND cardinality(regions) = 0
            AND cardinality(traditions) = 0 AND cardinality(languages) = 0
            AND notes IS NULL AND superseded_at IS NOT NULL)
    );

COMMENT ON COLUMN track_classification_revisions.decision IS
    'Explicit set/clear history event. Clear events are born inactive and retain their reason.';
