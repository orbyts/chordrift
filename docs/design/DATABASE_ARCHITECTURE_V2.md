# Chordrift database architecture v2

The zoomable [schema overview](database-v2-schema-overview.svg) maps the
implemented table groups, their main relationships, the clean runtime
staging/recovery boundary, and the lifecycle of a manual Spotify playlist
removal.

The v0.2 product layer now under development is documented separately in
[Playlist product architecture](PLAYLIST_PRODUCT_ARCHITECTURE.md). It builds
account ownership, overlapping collections, versioned recipes, and reproducible
Spins additively on this clean v2 foundation. Development migration 0046 now
implements that product-schema foundation, V020-06 uses its onboarding tables,
V020-07 reads the referenced immutable inventory revisions, and V020-08 reads
one explicitly fingerprinted database-v2 listening import in isolated tests;
V020-09 consumes prepared provider-neutral candidate facts entirely in memory
and adds no schema or database access. V020-10 persists exact ordered previews
in migration 0046's existing `playlist_spins` and `playlist_spin_tracks` tables
without changing database-v2 or the migration. Migration 0046 is not applied to
production Neon.

Status: database-v2 foundation complete in the released v0.1.4 runtime, updated
2026-08-27. The live project uses content-addressed current provider state,
normalized evidence, compact checkpoints, and the clean runtime schema. The
legacy physical tables and former rollback project were retired only after the
recorded exact-confirmed gates passed. This document preserves that chronology;
it is not authorization to replay a migration, delete a database, or write to a
provider.

Sections below are a dated execution record. Statements such as “next gate” or
“remains unapproved” describe the boundary at that historical checkpoint; they
are not current instructions. For routine v0.1.4 operation use the
[task-oriented guide](../HOW_TO_CHORDRIFT.md). V0.2 extends this completed
foundation additively and does not reopen the migration.

## Current state at a glance

| Boundary | Current status |
| --- | --- |
| Released runtime | v0.1.4 reads and writes only the database-v2 current-state and normalized-evidence surfaces. |
| Schema | Production is healthy at 45/45 migrations; migration 0045 repairs the last stored function that referenced removed v1 relations. |
| Legacy storage | Database-v1 physical tables and the former rollback project are gone after independently verified exact-confirmed cleanup. |
| Recovery | The verified pre-compaction logical dump and immutable Spotify archives remain the external recovery sources. |
| Routine operation | Use `sync pull` and the normal plan/readiness/apply workflow; do not replay migration or cleanup applies. |
| v0.2 relationship | Migration 0046 now extends this foundation additively with the provider-neutral product schema. It passed fresh and migration-45 upgrade rehearsals on isolated PostgreSQL 18 and is not applied to production Neon. |

The exact migration-0046 reconciliation and ownership model is documented in
the [V020-05 product schema foundation](PRODUCT_SCHEMA_V020_05.md). It adds
product intent beside database-v2; it does not reopen, rename, or replace the
current-state and normalized-evidence foundation shown below. The first runtime
consumer is the provider-read-only
[V020-06 onboarding boundary](ONBOARDING_SESSION_V020_06.md); its first
read-only result consumer is the
[V020-07 inventory audit](ONBOARDING_AUDIT_V020_07.md).
The [V020-08 enriched audit](ENRICHED_ONBOARDING_AUDIT_V020_08.md) reuses the
existing `listening_evidence_imports`, historical identities, and normalized
events without another schema change. The
[V020-09 recipe executor](DISCOVERY_REDISCOVERY_RECIPE_V020_09.md) is a pure
Rust selection boundary and likewise leaves database-v2 and migration 0046
unchanged. The [V020-10 Spin preview](DETERMINISTIC_SPIN_PREVIEW_V020_10.md)
is the first runtime consumer of the existing Spin tables; it adds no migration
and performs no provider operation.

## Why this redesign existed

The v0.1.2 personal database was healthy and fully migrated, but its physical
representation was unnecessarily expensive. A read-only audit measured about
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

## Executed migration strategy

The migration deliberately did not mutate or discard the v0.1.2 database in
place as its first step. The executed sequence was:

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

## Workstream boundaries and outcome

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

Tasks 1–4 were completed sequentially through v0.1.3 and hardened in v0.1.4.
V0.2 now builds its application/domain contracts above the clean storage
boundary. UI implementation still must not bind directly to SQL.

## Historical recovery baseline

The pre-compaction backup is stored at:

`$DROPBOX/Music/Chordrift/Backups/2026-08-26-pre-compaction/`

It contains the custom-format dump, schema-only SQL, `pg_restore` catalog, and
SHA-256 checksum. The catalog was parsed and the complete restore rehearsal
passed; the dump remains the durable recovery artifact.

## Historical safe-cleanup measurements

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
  `d3186b303fa7d7dabe4d45f605d8a0d97a132fe50cd2bc00368491570f83e90b`;
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

## Historical snapshot-protection and compaction plan

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

## Implemented v2 schema foundation

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

### Historical schema rehearsal result

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

## Historical normalized-evidence and checkpoint rehearsal

Migrations `0041_database_v2_rehearsal_migration.sql`,
`0042_database_v2_listening_dual_write.sql`, and
`0043_collation_stable_v2_hashes.sql` complete the rehearsal portion of
workstream 3 without deleting legacy data. Migration 0041 adds exact-confirmed
data-migration receipts, honest archive-member hash status, compact checkpoint
references for durable cleanup/Re-evaluate audit rows, and a checkpoint
materializer that reuses content-addressed revisions. Migration 0042 dual-writes
new local archive-import and listening-event inserts/updates into v2 throughout
the rollback observation window. Migration 0043 makes inventory hashes
independent of the database's default text collation by ordering hash inputs
with PostgreSQL's bytewise `C` collation. None of these migrations calls a
provider.

The provider-free command surface is:

```console
chordrift db v2 migration plan --account personal
chordrift db v2 migration apply --account personal --confirm <PLAN_SHA256>
chordrift db v2 migration verify --account personal
chordrift db v2 cutover-plan --account personal
```

`plan`, `verify`, and `cutover-plan` use read-only transactions. `apply` is the
only data-moving command and rejects anything except the SHA-256 of the current
plan. It acquires an account-scoped transaction lock, is resumable/idempotent,
restores the current inventory pointer before commit, retains every legacy row,
and writes a verified migration receipt. The cutover command has no apply
surface and explicitly excludes legacy deletion, production connection changes,
and Spotify writes.

### Evidence findings

The legacy rehearsal contains 149,314 track events and no unsupported media
events. All are active. Its 149,195 archive events have valid import IDs and 17
distinct event-bearing archive paths; 119 recent-API events have stable source
event IDs. No historical provider identity mixes matched and unmatched state.

Individual archive-member hashes were never retained by v1. The migration does
not fabricate them: each known member is recorded with
`hash_status = archive_manifest_only` and a null member content hash, while the
verified containing ZIP hash remains authoritative. The evidence-import ledger
preserves both archive hashes, declared source-file counts, event counts,
first/last timestamps, legacy counters, and parser provenance. Rebuilding and
directly hashing archive members remains a separate archive-access rehearsal.

Recipes require only the typed event facts already listed in the listening
contract: account/identity, time, duration, skip/completion, context,
source/import identity, duplicate occurrence, and supersession. The measured
legacy JSON keys `platform`, `connection_country`, `reason_start`, `shuffle`,
`offline`, `offline_timestamp`, and `incognito_mode` are not queried by current
recipes and remain recoverable from immutable archives. Display title, artist,
and album are stored once across 15,575 historical identities. The original
`reason_end` becomes typed completion evidence and remains archive-recoverable.

### Measured rehearsal result

The complete migration was run only on a new PostgreSQL 18.6 clone of the
verified 39-migration restore. Additive schema migrations reached 43/43. The
read-only plan was applicable with exact hash
`a850fb15603f82c934daa127cfb768084938bc8ac601b6f30643ebc3a84e2ae8`.
Apply completed in 12.3 seconds; a second identical apply completed with the
same hash and counts, proving idempotence.

Post-apply verification measured exact parity:

- 149,314 total/active events and 23,769,184,794 listening milliseconds;
- first event `2014-11-05T05:56:18Z`, last event
  `2026-08-26T06:30:27.850Z`;
- 100,926 matched events, 1,720 matched identities, and 13,855 unmatched
  identities;
- both archive manifests/hashes and their imported event counts;
- zero plan, verification, cleanup, or Re-evaluate rows awaiting checkpoints.

Forty-one legacy snapshots protected 43 sync plans, 420 managed playlist
verifications, one cleanup approval, and zero current Re-evaluate events. Their
content deduplicated into 24 named checkpoints, 120 playlist revisions, 8,594
revision tracks, and 638 checkpoint playlist pointers. Embedding/signal
generations remain rebuildable cache references. Five bookmark-observation
snapshots remain durable legacy audit history until that provider-external
observation model is normalized; they are not silently mapped to owned-library
checkpoints.

The original invariant report is byte-identical before and after migration.
`db v2 status` reports every current-state comparison true and
`ready_for_cutover: true`. `pg_amcheck --parent-check --heapallindexed` passed;
the application scope contains 571 relations / 42,295 pages.

During the fresh dual-storage observation state, the database is 358,815,423
bytes versus 249,657,023 bytes for the fresh legacy restore. This temporary
increase is expected: legacy and v2 evidence coexist. Normalized events occupy
98,451,456 total bytes and historical identities 8,069,120 bytes, compared
with 152,256,512 bytes for legacy events. Content-addressed provider playlist
revisions occupy about 2.82 MB versus 29.36 MB for legacy playlist memberships.
No reclaim estimate is authorization to delete either representation.

The rehearsal-only production cutover plan hash is
`32f1e7f3e9899c72a822a5faf588c29dc905d62ead3b3b17313d165d6e4640b8`.
It requires a fresh production invariant/plan comparison because production
may advance and therefore emit different hashes. Production migrations,
data apply, read cutover, observation-window start, legacy cleanup, and any
connection change remain unapproved and require separate explicit authority.

### Historical checkpoint: read-only production preflight

The production preflight on 2026-08-26 used only read-only reports. The backup
checksum was reverified, Neon reported PostgreSQL 18.6 and 39/43 migrations,
and no production migration, row movement, connection change, deletion, or
Spotify operation occurred.

Production and the pristine restore have byte-identical invariant reports
after making report ordering explicitly `COLLATE "C"`. Before that correction,
the combined playlist fingerprint differed solely because production uses
`C.UTF-8` while the local restore uses `en_US.UTF-8`; every one of the 17
non-empty playlists already had identical counts and per-playlist hashes. The
stable combined fingerprint is the value recorded above. A read-only
calculation over production's legacy rows also produced the same prospective
v2 current-state hash as the 43-migration rehearsal:
`f12ef35e6ac961c99819be5d667eb60273435c25f0dd5b6f9182b369ba8e0ff3`.

Production measured 410,181,632 database bytes and 399,286,272 ordinary-table
bytes. `listening_events` accounts for 288,555,008 total bytes and
`provider_playlist_tracks` for 33,300,480. The compaction classification is
unchanged: 58 snapshots, one current, 41 protected, and 16 redundant routine
snapshots. These figures are planning evidence, not deletion authority.

The fresh 43-migration rehearsal retained exact invariant parity, emitted data
plan hash
`a850fb15603f82c934daa127cfb768084938bc8ac601b6f30643ebc3a84e2ae8`,
completed its exact-confirmed apply in 12.3 seconds, and passed `pg_amcheck`
over 571 application relations / 42,295 pages. Its database size is
358,815,423 bytes.

The next production gate must remain deliberately narrow: apply additive
schema/current-state migrations 0040 through 0043, run the read-only invariant,
status, storage, and migration-plan checks, then stop and report the actual
production data-plan hash. Normalized-evidence migration, read cutover,
observation-window start, and legacy cleanup each remain separate approval
gates.

### Historical checkpoint: additive production schema gate

With explicit approval, migrations 0040 through 0043 were applied to production
on 2026-08-26. They completed in 3.964 seconds; Neon is healthy at 43/43
migrations with zero pending or failed migrations. No normalized-evidence apply,
read cutover, deletion, connection change, or Spotify operation occurred.

The post-migration invariant is unchanged. The v2 current inventory points to
the same source snapshot and matches all 22 playlist headers, 1,790 ordered
memberships, and both empty saved surfaces exactly. Production emitted the
applicable normalized-evidence/checkpoint plan hash
`a850fb15603f82c934daa127cfb768084938bc8ac601b6f30643ebc3a84e2ae8`.
It covers 149,314 events, 15,575 historical identities, two archive manifests,
41 checkpoint source snapshots, 43 plan references, 420 verification
references, and one cleanup reference, with zero unsupported events or missing
required identities/imports.

The stopped boundary is independently visible: normalized events, historical
identities, evidence imports, and checkpoints are all zero; 43 plans, 420
verifications, and one cleanup remain awaiting checkpoints; and
`ready_for_cutover` is false. Post-schema production storage is 411,852,800
database bytes and 400,482,304 ordinary-table bytes. The next gate is the
exact-confirmed data apply using the production-emitted hash above, followed by
read-only verification and another stop. It does not authorize read cutover or
legacy cleanup.

### Historical checkpoint: first production data-migration attempt

The exact-confirmed apply for production plan
`a850fb15603f82c934daa127cfb768084938bc8ac601b6f30643ebc3a84e2ae8`
was attempted under explicit approval on 2026-08-26. PostgreSQL rejected the
transaction with SQLSTATE `53100`, indicating insufficient storage. The apply
was not retried.

Read-only verification proves a clean logical rollback: zero normalized events,
zero historical identities, zero evidence imports, zero checkpoints, and no
migration receipt are visible. All 43 plan, 420 verification, and one cleanup
references still await checkpoints. The plan remains applicable with the same
hash, `ready_for_cutover` remains false, and the complete legacy invariant is
unchanged.

PostgreSQL retained physical pages allocated by the aborted transaction.
Database size increased from 411,852,800 to 514,457,600 bytes and the sum of
ordinary-table totals from 400,482,304 to 503,087,104 bytes. Although it has no
visible rows, `normalized_listening_events` occupies 98,500,608 total bytes;
the empty `historical_provider_track_identities` relation occupies 4,128,768.
These are dead physical tuples/pages, not partially committed evidence.

No vacuum, compaction, quota change, read cutover, deletion, connection change,
or Spotify operation followed the failure. The next safe gate requires added
Neon storage headroom or a separately reviewed maintenance/reuse plan, followed
by fresh read-only checks and explicit retry authority.

### Historical checkpoint: no-cost replacement candidate

Rather than buying storage or destructively compacting the current project, an
isolated free Neon PostgreSQL 18 candidate was created in the same region. The
existing production project and application connection were not changed. The
trusted custom-format dump hash was reverified, then restored with
`--no-owner --no-acl --exit-on-error`.

At the restored boundary the candidate was healthy with 39/43 migrations,
byte-identical invariants, and 249,331,712 database bytes. Additive migrations
0040-0043 completed in 2.450 seconds. The candidate independently emitted and
successfully applied exact data plan
`a850fb15603f82c934daa127cfb768084938bc8ac601b6f30643ebc3a84e2ae8`.

Post-apply verification proves exact parity for 149,314 events,
23,769,184,794 total milliseconds, first/last timestamps, 100,926 matched
events, 1,720 matched identities, 13,855 unmatched identities, and both archive
manifests. Forty-one protected source snapshots deduplicate into 24 checkpoints;
all 43 plan, 420 verification, and one cleanup references are resolved. Current
provider parity remains exact for 22 playlists and 1,790 ordered memberships.
`ready_for_cutover` is true and cutover-plan hash is
`32f1e7f3e9899c72a822a5faf588c29dc905d62ead3b3b17313d165d6e4640b8`.

The verified candidate occupies 358,686,720 database bytes and 347,504,640
ordinary-table bytes, comfortably below the free project's 0.5 GB allowance.
The original production project still has zero committed normalized evidence
and remains `ready_for_cutover: false`.

A candidate-only connection URI appeared in a failed `pg_amcheck` diagnostic.
The candidate role password was immediately reset through the Neon API and the
new credential verified; no production credential was involved. Managed Neon
does not permit the project owner to install the superuser-only `amcheck`
extension, so remote `pg_amcheck` is unavailable. The equivalent local
PostgreSQL 18 rehearsal passed `pg_amcheck`, and the candidate passed all
application-level, migration, invariant, and storage checks.

No connection change, deletion, cleanup, or Spotify operation is authorized by
the candidate rehearsal. Connection cutover requires a separate approval and
must be followed immediately by read-only verification and an observation
window. Old-project deletion remains a later destructive gate.

### Historical checkpoint: replacement-project connection cutover

With separate explicit approval, Chordrift's private Apogee
`CHORDRIFT_DATABASE_URL` value was switched to the verified candidate. The
secret file retained owner-only `0600` permissions, and neither old nor new URL
was printed. A fresh process explicitly discarded its inherited environment,
loaded Apogee again, and proved that the durable configuration reached the
candidate by reporting `ready_for_cutover: true`.

Immediate read-only verification reproduced the complete invariant report,
exact normalized-evidence parity, 24 checkpoints, zero unresolved durable
references, 43/43 migrations, cutover hash
`32f1e7f3e9899c72a822a5faf588c29dc905d62ead3b3b17313d165d6e4640b8`,
and 358,686,720 database bytes. No Spotify request or write occurred.

At this historical gate the former project remained intact as the bounded
connection-level rollback target during observation. The project switch did not
itself change application queries from legacy tables to v2 tables; that code
refactor, legacy cleanup, rollback, and project deletion still required their
own later approvals.

After the verified connection cutover, project `damp-hall-40280714` was renamed
from its temporary candidate label to `chordrift`. To free that lowercase name,
the former project `mute-recipe-86719846` was relabeled
`chordrift-legacy-rollback`. These were display-name-only Neon operations:
project identities, connection configuration, and database contents were
unchanged.

### Implemented clean-runtime rehearsal and production completion

Migration 0044 establishes the application boundary used by v0.1.3. Current
provider reads are reconstructed from `provider_current_inventories`, current
playlist pointers, content-addressed playlist/saved revisions, and compact
checkpoints. Listening reads are reconstructed from
`normalized_listening_events`, `historical_provider_track_identities`, evidence
imports, and source-file manifests. Display metadata is stored once per
historical provider identity. No ordinary runtime module reads a duplicated
provider snapshot body or the database-v1 listening-event table.

Provider pulls now write through five explicitly transient
`provider_inventory_import_*` surfaces. The materializer reuses unchanged
content revisions, atomically replaces current pointers, and deletes every
staged membership before commit. The lightweight
`provider_inventory_observations` row remains as the pull receipt and stable
foreign-key target. Spotify archive imports and recent-play observations write
directly to normalized identities and typed evidence; database-v1 dual-write
triggers are no longer part of runtime correctness.

Cleanup is not an automatic schema migration. `db compact cleanup plan` first
requires every database-v2 parity gate and hashes the durable invariant report
with exact source counts. Only `cleanup apply --confirm <PLAN_SHA256>` may:

1. remove the temporary dual-write triggers;
2. truncate the duplicated provider bodies and rename their physical tables to
   the v2 import-staging names;
3. rename the lightweight snapshot header to
   `provider_inventory_observations`;
4. remove `listening_events` and `spotify_archive_imports` after exact normalized
   parity; and
5. record a durable cleanup receipt before commit.

`cleanup verify` requires the same logical invariant fingerprint, retained event
and archive counts, absent database-v1 table names, and empty provider-import
staging. Plans, approvals, apply receipts, managed verifications, exclusions,
canonical assignments, archive hashes, historical identities, normalized
events, and compact checkpoints remain intact.

The second fresh PostgreSQL 18 rehearsal measured:

- 58 provider observations retained;
- 157,193 duplicated provider-body rows removed;
- 149,314 legacy events removed and 149,314 normalized events retained;
- two legacy archive rows removed and two evidence manifests retained;
- invariant SHA-256
  `24f5da45845bb48b3cfeb49cbd09fe371043c7f9544ea38993d3016beaf0d6a3` before
  and after cleanup;
- exact rehearsal cleanup plan
  `0688bf0984ea6f6b26cf65ca7ab1c9fcb762601c6a512b204e7a79312830f964`;
- 167,974,591 database bytes after cleanup, versus 358,686,720 bytes for the
  verified dual-storage Neon candidate; and
- successful ordinary read commands, provider-inventory persistence/reuse, and
  normalized archive import on the post-clean schema.

Migration 0044 is installed on the live `chordrift` project and immediate
read-only verification reproduced every rehearsal invariant and runtime read.
Production cleanup was explicitly approved and applied using the rehearsal hash
`0688bf0984ea6f6b26cf65ca7ab1c9fcb762601c6a512b204e7a79312830f964`,
and invariant hash
`24f5da45845bb48b3cfeb49cbd09fe371043c7f9544ea38993d3016beaf0d6a3`
was unchanged. Immediate independent verification found legacy table names
absent, transient provider-import staging empty, 149,314 normalized events and
both evidence imports retained, and all database-v2 parity gates true. History,
signals, embeddings, albums, playlists, and database-v2 status all passed on
the clean runtime schema. The measured database size is 167,788,544 bytes and
ordinary-table total is 156,459,008 bytes.

A fresh child process using the owner-only persistent connection configuration
also passed database health, cleanup verification, and a runtime playlist read.
After this gate passed, deletion of former project `mute-recipe-86719846` was
separately approved by immutable ID. Neon deleted it, a subsequent listing
proved it absent, and the live project passed health and cleanup verification
again. The preserved dump was rehashed afterward and still matches SHA-256
`8c5796cba5729931678f825021fe03268b81129352349266d7a68b487b3711ae`;
it is now the durable recovery artifact.

During the 0044 gate, the desktop process was found to carry a stale inherited
database URL for the former project even though the owner-only persistent secret
targets `chordrift`. The first migration invocation therefore installed the
same additive 0044 schema on the former project. It did not delete or rewrite
rows; before its later approved retirement the former project reported zero
normalized events/checkpoints and 44/44 migrations. The intended project was then
addressed by explicit ID and verified at 44/44. Operational commands must use an
explicit project target or a fresh correctly loaded secret, never the stale
desktop environment.
