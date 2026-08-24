ALTER TABLE account_track_signals
    ADD COLUMN provider_discovery BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN prompted_interest BOOLEAN NOT NULL DEFAULT FALSE;
