# Chordrift database architecture v2

Status: approved direction, 2026-08-26. This document defines the storage
redesign that precedes recipe work and the native UI. It is not authorization
to delete the current Neon database.

## Why this redesign exists

The v0.1.2 personal database is healthy and fully migrated, but its physical
representation is unnecessarily expensive. A read-only audit measured about
391 MB of PostgreSQL data while Neon reported about 0.43 GB of project storage.
The project has one branch and a six-hour restore window, so extra branches are
not the cause.

The dominant costs are:

- `listening_events`: about 275 MB for 149,314 events. Every archive event
  repeats track, artist, album, source-file, and raw playback JSON. Eight
  indexes consume about 90 MB, including two large event-identity indexes.
- `provider_playlist_tracks`: about 32 MB for 103,561 rows. Fifty-eight full
  provider snapshots were recorded in eight days even though the current
  library contains only 1,790 playlist entries.
- derived statistics, embeddings, sync operations, and verified-history tables
  account for most of the remainder.

The reported 13,855 unmatched historical tracks are not current Spotify
library members. They are provider track identities observed across twelve
years of listening archives for which current Spotify metadata has not yet
been resolved. Preserve them as lightweight historical identities and enrich
them lazily; do not treat them as current-library bloat.

## Data classes

Chordrift must give every table one explicit lifecycle class.

### Current provider state

Store one transactionally replaceable, verified view per account and provider:
accounts, playlists, current membership and order, saved surfaces, provider
identities, provider revisions, and capture time. A routine pull updates this
state or reuses it by provider revision. It does not copy a complete membership
body merely to prove that nothing changed.

### Durable Chordrift intent

Provider-neutral user intent is authoritative and retained: canonical
collections, intake surfaces, the Re-evaluate queue, classifications,
corrections, exclusions, retirements, recipe definitions, approvals, and
verified apply receipts. Provider adapters translate this intent but do not
own it.

### Listening evidence

Retain normalized event facts: account, provider identity, playback time,
duration, completion/skip evidence, context when available, and source import.
Store track/artist/album display metadata once per historical provider identity,
not once per event. The immutable Spotify ZIP and its checksum are the raw
source of truth; PostgreSQL retains its manifest, coverage, import result, and
normalized facts rather than duplicating every raw JSON object.

### Rebuildable derived data

Statistics, analyses, embeddings, similarity results, recipe candidates, and
recommendations are caches. Each generation records its algorithm/schema
version and inputs. Old generations may expire after no durable plan or audit
references them.

## Modeling rules

- Keep canonical musical identity separate from provider identity and account
  membership.
- Keep current state separate from immutable evidence and user intent.
- Use typed columns for fields used by recipes, filters, constraints, or
  indexes. Reserve JSONB for genuinely provider-specific extensions.
- Make provenance content-addressed. A source archive/file is stored once and
  referenced by events.
- Keep unmatched historical identities small and resolvable on demand.
- Store full checkpoints only when they protect a bounded operation or named
  release. Represent routine provider evolution as current state plus compact
  changes and verified receipts.
- Every deletion or compaction command supports plan, apply, and verify phases;
  it must be resumable and report preserved invariants.
- Do not partition 149,314 events merely for fashion. Introduce time
  partitioning only when measured query or maintenance behavior warrants it.

## Retention policy

- Current provider inventory: one materialized current state per account.
- Pre-apply provider checkpoints: retain temporarily, initially 30 days.
- Named release/baseline checkpoints: retain selectively in compact form.
- Verified plans, approvals, and apply receipts: retain permanently without
  embedding complete provider inventories.
- Normalized listening events: retain permanently.
- Raw provider archives: retain permanently outside PostgreSQL with hashes.
- Per-event raw archive JSON: remove only after archive verification and a
  successful restore/rebuild rehearsal.
- Derived caches and unreferenced generations: expire automatically.

## Approved migration strategy

Do not mutate or discard the v0.1.2 database in place as the first step.

1. Preserve a custom-format logical dump, schema, restore catalog, and checksum
   outside Neon.
2. Restore it into an isolated rehearsal database and prove that it is usable.
3. Implement v2 schema and compaction as versioned, tested migrations and
   explicit CLI plan/apply/verify commands.
4. Materialize the current Spotify state as the new provider baseline.
5. Migrate durable Chordrift intent and normalized listening evidence.
6. Rebuild statistics and embeddings instead of copying stale cache rows.
7. Compare invariants before cutover: provider surface counts and order,
   canonical assignments, exclusions, queue state, 149,314 listening events,
   listening duration, first/last playback, import hashes, and verified apply
   history.
8. Cut over configuration only after both databases pass comparison.
9. Keep the old dump through an observation period; delete the old Neon data
   only with a separate exact approval.

The initial physical target is approximately 100–150 MB without sacrificing
meaningful history. This is a target to verify, not a guaranteed result.

## Workstream boundaries

The work should be split into sequential Codex tasks, not concurrent branches
editing the same schema:

1. **Safe cleanup foundation:** restore rehearsal, invariant report, storage
   report, and provider-free compaction plan. No production deletion.
2. **Database v2:** schema, retention model, migrations, tests, and compact
   current-state persistence.
3. **Migration and cutover:** rehearse, migrate, compare, switch, observe, and
   eventually retire old storage with approval.
4. **Code refactor:** move provider queries, recipes, caches, and diagnostics to
   v2 APIs; remove compatibility code only after cutover.
5. **Native UI:** build on stable v2 query/command DTOs and background-operation
   contracts rather than coupling UI code to legacy tables.

Tasks 1–4 are causally dependent. UI design may proceed separately, but UI
implementation should not bind to the database until the v2 bridge contracts
are stable.

## Existing backup baseline

The pre-compaction backup is stored at:

`$DROPBOX/Music/Chordrift/Backups/2026-08-26-pre-compaction/`

It contains the custom-format dump, schema-only SQL, `pg_restore` catalog, and
SHA-256 checksum. The catalog has been parsed successfully. A complete restore
rehearsal remains the first task.

## Safe cleanup foundation measurements

Measured on 2026-08-26 from commit `65b4c98`, without changing production:

- The dump SHA-256 is
  `8c5796cba5729931678f825021fe03268b81129352349266d7a68b487b3711ae` and
  matches `SHA256.txt`. The checksum file uses BSD syntax, so verification must
  compare its recorded digest with `shasum -a 256` rather than use macOS
  `shasum -c` directly.
- The custom-format dump restored with `--no-owner --no-acl --exit-on-error`
  into an isolated local PostgreSQL 18.6 database. `_sqlx_migrations` contains
  39 successful migrations, versions 1 through 39, and no failures.
- `pg_amcheck` completed successfully across all 676 relations and 30,156
  pages. The rehearsal database reports 249,657,023 bytes. The sum of ordinary
  table totals is 238,862,336 bytes: 158,220,288 heap bytes, 160,030,720 table
  bytes including auxiliary forks/TOAST, and 78,831,616 index bytes.
- The largest restored relations are `listening_events` at 152,256,512 total
  bytes, `provider_playlist_tracks` at 29,360,128, `sync_operations` at
  7,872,512, `managed_playlist_verified_tracks` at 6,864,896, and
  `account_track_embeddings` at 6,496,256. A fresh logical restore is therefore
  materially smaller than the approximately 391 MB production measurement;
  part of the difference was physical churn/bloat, not live logical content.

The repeatable `chordrift db invariant-report --account personal` rehearsal
baseline is:

- one provider account; latest successful provider snapshot
  `66915ea3-11e8-4e0a-b9f4-930ceab27c5d` captured at
  `2026-08-26T14:53:38.887405Z`;
- 22 current playlists, 1,790 ordered memberships, and 1,765 unique playlist
  tracks; exact SHA-256 order fingerprint
  `9c0ca2a48ed65c5941bc0e53756cc7dc78613332f64c979363b30f283acc0793`;
- zero current saved tracks, saved albums, or saved-album track rows;
- approved generation `f521e707-8e5f-4283-a0bd-d123df3329f1` with 16
  canonical playlists and 1,754 unique ordered assignments; assignment
  SHA-256 fingerprint
  `d32d747874c61b330686f89a050fde15a6ae49c23c467b7ec7036436b4c789df`;
- 107 active exclusions; one active Re-evaluate surface with zero current
  tracks;
- 149,314 active normalized listening events across 15,575 historical Spotify
  identities: 1,720 matched and 13,855 unmatched identities, with 100,926
  matched and 48,388 unmatched events;
- 23,769,184,794 total listening milliseconds (6,602.55 hours), from
  `2014-11-05T05:56:18Z` through `2026-08-26T06:30:27.850Z`;
- two immutable Spotify archive import records, with hashes
  `9a9bd3174ec070d83107a280ed4df6d8a5bf556a6f9a73708845845f9aa5b01f`
  and `840130a929cdbb9858a80009a791953973f9422a4f2031c7a322d9b7873b2202`;
- 19 provider-verified apply runs; the earlier final zero-operation plan is
  `56a0d535-f83e-42ae-898e-8ed627e6f4e9`, while the newest stored plan
  `fac3d2ba-6b6e-47e6-9575-24e10fa4458b` contains one reconcile-phase
  `exclude_track`. This is a pending intent delta, not database corruption, and
  both states must remain visible during v2 comparison.

`chordrift db storage-report` emits exact heap, table, index, and total bytes
for every ordinary table. Both reports are read-only and make no provider
requests.

## Provider snapshot protection and compaction plan

`chordrift db compact plan --account personal` starts a read-only transaction,
classifies effects, and rolls it back. It has no provider adapter path and no
apply surface. On the rehearsal it reports 58 provider snapshots:

- one latest snapshot is the materialized current provider state;
- 41 older snapshots are protected by durable references;
- 16 older snapshots are redundant routine observations with no durable
  reference. Normalizing those 16 would replace 506 playlist headers, 26,490
  ordered playlist membership rows, 14,814 saved-track rows, 138 saved-album
  rows, and 1,314 saved-album membership rows.

The protected set is the union of snapshots referenced by immutable sync plans
(33 distinct snapshots), managed playlist verifications (29), embedding or
signal generations (4), external-bookmark history (5), and cleanup approval or
Re-evaluate audit history (1). The overlap is intentional. PostgreSQL
`RESTRICT` foreign keys directly protect sync plans, managed verifications,
embedding/signal generations, cleanup approvals, and Re-evaluate events.
Bookmark observations use cascading ownership but are durable audit history
and must be migrated before their source snapshots can be normalized.
`provider_import_runs.snapshot_id` is nullable with `ON DELETE SET NULL`; its
58 references do not by themselves require retaining 58 complete inventories.
Current analysis/statistics references are rebuildable caches, and snapshot
child tables cascade with their parent.

The 16 redundant snapshots are planning candidates only. This workstream does
not authorize deleting them. Database-v2 must first detach durable receipts
from complete inventories, preserve compact named checkpoints where required,
and rerun the invariant report before and after any later apply.

## Listening-event recipe contract

Recipes need these normalized typed facts per retained event:

- provider account and historical provider identity;
- playback timestamp and milliseconds played;
- skip evidence and normalized completion evidence (derived today from the
  provider `reason_end = trackdone` value);
- provider context URI/type when present;
- source import, source kind (`archive` or provisional `recent_api`), stable
  source event identity, duplicate occurrence, and supersession time.

Those fields support recency, lifetime play/duration counts, meaningful-play
thresholds, 45-minute listening sessions, skips/completions, context-aware
signals, and honest provisional-versus-archive capability reporting. Track,
artist, and album display names belong once on the historical provider identity,
not on every event.

The verified immutable Spotify archives can recover the original source file
and raw fields that recipes do not query: platform, connection country,
`reason_start`, the original `reason_end`, shuffle, offline state/timestamp,
incognito mode, and the repeated display metadata. The rehearsal contains
45,906,222 bytes of per-event `raw_metadata` JSON. Removing that JSON is allowed
only in a later apply after v2 stores normalized completion/context evidence,
identity metadata is materialized once, archive hashes/import manifests remain
intact, and an archive-to-normalized rebuild comparison passes.

## Additive v2 schema foundation

Migration `0040_database_v2_foundation.sql` implements workstream 2 without
deleting or rewriting any legacy row. It introduces four explicit storage
boundaries:

1. `provider_current_inventories` and `provider_current_playlists` hold one
   transactionally replaceable provider state per account.
2. `provider_playlist_revisions`, saved-track revisions, and saved-album
   revisions retain content-addressed bodies once and reuse them when provider
   content is unchanged. Exact order and duplicates remain part of the SHA-256.
3. `provider_inventory_checkpoints` references those immutable revisions for
   bounded pre-apply and named baselines. New nullable checkpoint references on
   plans and managed verifications permit the later migration away from full
   routine snapshots without weakening audit history.
4. `historical_provider_track_identities`, `listening_evidence_imports`, source
   files, and `normalized_listening_events` establish the typed evidence model.
   They remain empty until the separately measured evidence migration; schema
   creation is not treated as successful event migration.

`materialize_provider_current_state_v2(account, snapshot)` is an internal
transactional database function. Migration 0040 calls it once for each latest
successful account snapshot, and the Spotify importer dual-writes through it
after completing a compatibility snapshot. The legacy write remains until the
later cutover so existing queries continue to work. Repeating identical
provider content updates observation time and the one current pointer but does
not duplicate a playlist or saved-surface revision.

The provider adapter supplies data but does not define v2 retention. The
materializer accepts an account and imported snapshot, uses provider-qualified
identities, and performs no provider request or write.

### Rehearsal result

Migration 0040 was applied only to a clone of the restored PostgreSQL 18.6
rehearsal database. It completed in 153 ms with 40/40 migrations successful.
The complete v1 invariant report was byte-identical before and after, and
`pg_amcheck` passed all 758 relations / 30,301 pages afterward.

The v2 current state contains one inventory, 22 current playlist pointers, 22
content revisions, and 1,790 ordered revision tracks. Current playlist order,
saved tracks, saved albums, and album-track order all compare exactly with
legacy snapshot `66915ea3-11e8-4e0a-b9f4-930ceab27c5d`. The additive schema
and current backfill increased the restored database from 249,657,023 to
251,205,311 bytes; v2 playlist revision tracks use 589,824 total bytes.

The repeatable read-only command is:

```console
chordrift db v2 status --account personal
```

Its rehearsal status deliberately reports `ready_for_cutover: false` because
149,314 normalized events, 15,575 historical identities, two evidence import
manifests, compact checkpoints for 43 plans, and checkpoint references for 420
managed verifications have not yet been migrated. This is the required safe
boundary between schema workstream 2 and migration/cutover workstream 3.
Production migration, connection cutover, and cleanup remain unapproved.
