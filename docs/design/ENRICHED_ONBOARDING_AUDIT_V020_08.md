# Enriched onboarding audit — V020-08

Status: implemented on the v0.2.0 development line and verified with
deterministic evidence plus disposable PostgreSQL 18. It is a Rust query
boundary, not a released CLI workflow, and it has not touched live Spotify or
production Neon.

## Same acceptance path, explicit extra evidence

`EnrichedAuditBoundary` accepts the same contract `OnboardingAudit` query and
typed `AccountContext` used by V020-07. It requires a V020-06 session that
explicitly selected exactly one `extended_playback_history` source. The
inventory-only boundary continues to require no selected history, so neither
path can silently change evidence modes.

The enriched boundary first runs the same inventory analysis as V020-07. Its
`inventory_baseline` retains the same library, playlist, overlap, uncertainty,
and preserve-first proposal findings for the same checkpoint. The nested
baseline continues to say `extended_history_used: false` because those findings
do not depend on history.

The history layer is accepted only when all of these values agree:

- the session owner, provider connection, manifest, provenance, and fingerprints;
- the selected evidence capability is `extended_playback_history`;
- the captured evidence fingerprint equals the account-owned
  `listening_evidence_imports.archive_sha256`;
- the import kind is `extended_streaming_history`; and
- the captured record count, import record count, and linked normalized-event
  count are identical.

This binds the audit to one content-addressed import. It does not read every
event currently available for the account and does not call a provider again.

## What becomes stronger

The enriched report counts usable and superseded records, observed time range,
distinct historical tracks, current-inventory matches, history-only tracks,
repeatedly observed tracks, tracks observed at least 180 days apart, maximum
plays for one track, and explicit completion and skip observations.

`strengthened_conclusions` contains only claims with direct support. Each entry
states:

- the stable conclusion category;
- `unavailable_from_current_inventory` as its inventory-only strength;
- `directly_observed_from_extended_history` as its enriched strength; and
- the exact number of supporting records and tracks.

The possible strengthened conclusions are observed listening, repeated
listening, long-term listening, history outside current inventory, completion
evidence, and skip evidence. A category with zero support is omitted rather
than presented as a conclusion.

## What does not become stronger

Extended history still does not approve user intent, establish collection
membership, or prove that repetition means preference or favorite status. Its
claims cover the selected import, not all possible listening. Existing provider
playlists remain preserved, and the starter organization remains unapproved.

The enriched result and its nested inventory baseline each have deterministic
SHA-256 fingerprints. Replay over the same checkpoint and history import
returns the same values apart from the generic view timestamp.

## Verification and exclusions

The side-by-side PostgreSQL fixture uses the same four-track inventory for both
paths and seven extended-history records across four historical identities. It
proves equal inventory findings, three current tracks with history, one
history-only track, two repeated tracks, one long-span track, exact completion
and skip support, deterministic replay, no extra fake-provider read, no intent
write, unchanged session state, inventory/enriched mode rejection, and
cross-account denial. The fresh 46-migration path, retained `45 → 46` rehearsal,
and following Spotify persistence proof pass in the same isolated PostgreSQL 18
database.

V020-08 adds no migration, CLI command, production configuration, credential
handling, live-provider request, provider mutation, approved collection intent,
recipe execution, Spin, or publication behavior.

## Next slice

`V020-09 — Discovery + Rediscovery recipe v1` may consume provider-neutral
inventory and evidence facts through an immutable recipe boundary. It must not
begin the persisted Spin preview assigned to V020-10 or provider publication.
