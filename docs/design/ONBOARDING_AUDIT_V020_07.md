# Inventory-only onboarding audit — V020-07

Status: implemented on the v0.2.0 development line and verified with the
deterministic fake provider plus disposable PostgreSQL 18. It is a Rust query
boundary, not a released CLI workflow, and it has not touched live Spotify or
production Neon.

## What the audit reads

`InventoryOnlyAuditBoundary` accepts the contract's `OnboardingAudit` query and
one typed `AccountContext`. It consumes a V020-06 session only when all of these
facts still hold:

- the Chordrift account and selected provider connection match the persisted
  owner, provider namespace, and provider-owned account ID;
- `ignore_existing_intent` is true and provenance says both
  `chordrift_intent_read: false` and `provider_write_requested: false`;
- the input-manifest fingerprint and inventory-state fingerprint still match;
- no extended-history evidence was selected; and
- the persisted provider and evidence capability snapshots belong to the same
  provider connection.

After validation, it reads only the immutable playlist, saved-track, and
saved-album revisions referenced by that inventory checkpoint. It does not call
the provider again. It does not read listening events, optional imported
history, collections, playlist-surface directives, recipes, Spins, publication
plans, or later changes to Chordrift intent.

## Report returned

The typed `OnboardingAudit` view reports:

- library shape: playlists, reported and readable playlist entries, saved
  tracks, saved albums, saved-album tracks, and distinct observed tracks;
- playlist facts: name, visibility, collaboration, reported/readable entries,
  distinct tracks, duplicate positions, and unreadable positions;
- overlap: tracks in several playlists, maximum playlist occurrence, saved and
  playlist overlap, saved tracks outside playlists, playlist-only tracks, and
  within-playlist duplicates;
- the exact capability snapshot, including degraded or unavailable inputs; and
- uncertainty: unreadable references, missing saved surfaces, capability gaps,
  and explicit inference limits.

The inventory-only result always says that it did not infer listening behavior,
user intent, or collection membership, and that it did not use extended
history. Current placement is observable structure, not proof of why the user
placed a track there.

## Starter organization is a proposal

The preserve-first proposal is deliberately conservative and overlapping:

1. `Preserved Library` represents all observed inventory.
2. Each current provider playlist is preserved as an observed view.
3. `Saved Outside Playlists` appears when that observable set is non-empty.
4. `Needs Review` appears when the provider declared items whose identities
   were not readable.

`preserve_existing_playlists` is true and `approved` is false. These values are
returned to a client for review; no `library_collections` or membership rows are
created, and nothing authorizes a provider write.

## Determinism and state

The audit has a stable SHA-256 fingerprint over the session identity, captured
input fingerprint, capabilities, findings, uncertainty, and proposal. Repeating
the query over the same immutable revisions returns the same audit value and
fingerprint (apart from the generic view's observation timestamp).

The query performs no database writes. In particular, the onboarding session
remains in `created` status and retains its V020-06 output provenance. This
keeps V020-07 repeatable without inventing a second persisted audit lifecycle.

## Verification and exclusions

The PostgreSQL proof uses two playlists, duplicate membership, cross-playlist
overlap, saved tracks inside and outside playlists, one unreadable playlist
position, an empty saved-album revision, and a degraded saved-album capability.
It proves the exact counts, visible gaps and limitations, deterministic replay,
zero additional fake-provider reads, zero collection-intent writes, unchanged
session state, rejection of an extended-history session, and cross-account
denial. The fresh 46-migration chain and retained `45 → 46` rehearsal both pass
on isolated PostgreSQL 18.

V020-07 adds no CLI command, production configuration, credential handling,
live-provider access, provider mutation, production Neon migration, approved
collection intent, recipe execution, Spin, or publication behavior.

## Enriched counterpart

`V020-08 — Enriched new-account audit` now runs the same inventory baseline
with explicitly selected extended listening evidence and explains exactly
which conclusions become stronger. This inventory-only path remains
independently usable and unchanged. See the
[enriched audit record](ENRICHED_ONBOARDING_AUDIT_V020_08.md).
