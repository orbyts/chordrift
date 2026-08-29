# Latest-state migration rehearsal — V020-13

Status: complete on 2026-08-28. This was a backup, restore, migration, and
comparison exercise. It did not migrate production, create a Neon candidate,
change a connection, invoke Spotify, or change the installed v0.1.4 binary.

## Source and recovery artifact

The installed v0.1.4 binary reported the live `personal` database healthy on
PostgreSQL 18.6 with 45/45 migrations. A new custom-format logical backup was
taken with `pg_dump` and preserved at:

`$DROPBOX/Music/Chordrift/Backups/2026-08-28-v020-13-latest-state/`

The directory contains the 22,686,266-byte custom dump, schema-only SQL,
parsed restore catalog, and a portable SHA-256 manifest. Verification in the
preserved directory passed for every artifact. The dump digest is:

`40223ce9898b756438c864bb12899d14d638883a41d87adc6b02a2d500b941c1`

The backup is recovery evidence, not authorization to modify production.

## Isolated rehearsal

The dump restored with `--no-owner --no-acl --exit-on-error` into an isolated
local PostgreSQL 18.6 cluster. The pristine restore reported 45/47 development
migrations. The repository binary then applied only migrations 0046 and 0047
to its local copy and reported 47/47, zero pending, and zero failed. A second
migrator run completed with no work, proving replay is idempotent.

The installed v0.1.4 read-only production invariant report, pristine-restore
report, and post-migration report are byte-identical. Important current values
include:

| Boundary | Latest-state value |
| --- | ---: |
| Current playlists / ordered memberships | 22 / 1,514 |
| Playlist-order fingerprint | `486a998a48898351f5a94c40bc4f6665e6616e17b74053fe9c43bca8e053eb95` |
| Canonical playlists / assignments | 16 / 1,718 |
| Canonical fingerprint | `6b769b7b08529777c49276d48cf5210e9ad244c8475f6b0f8510b9b02fbbbaf2` |
| Active exclusions | 387 |
| Re-evaluate surfaces / tracks | 1 / 3 |
| Re-evaluate fingerprint | `5e73ebf725bee0c5f6c55d1ecac1db0f6f448a427a93be434cb5dca1cd849f98` |
| Listening events / historical identities | 149,412 / 15,605 |
| Matched / unmatched listening events | 101,266 / 48,146 |
| Listening-evidence imports / source files | 2 / 17 |
| Verified apply runs | 37 |

The current intake audit after migration reports the same current inventory
and approved proposal generation with zero unresolved intake items. It is
read-only and reports provider writes disabled. The pre-migration development
binary correctly refused this newer command while migrations were pending;
the installed v0.1.4 binary does not advertise the later intake command.

## Exact durable-data comparison

A second pristine restore was kept beside the migrated copy. Normalized
data-only dumps (with PostgreSQL 18's random `\\restrict` guard removed before
hashing) compared byte-for-byte across 21 tables covering:

- current inventory, playlist revisions, and order;
- proposal/intake generations and membership;
- exclusions, Re-evaluate events, and durable assignments;
- normalized listening events, statistics, imports, and archive source files;
- synchronization plans and operations;
- readiness assessments and checks;
- apply runs and operations; and
- managed-playlist verification headers and memberships.

All row counts and all 21 SHA-256 values match. Representative retained totals
are 13,130 historical revision memberships, 12,908 proposal memberships,
387 exclusions, 47 Re-evaluate events, 655 assignment revisions, 101 plans,
12,118 planned operations, 58 readiness assessments, 630 readiness checks,
38 apply runs, 3,492 apply operations, 548 verification headers, and 56,161
verified-track rows.

Migrations 0046 and 0047 created no synthetic product activity. The migrated
copy has zero capability observations, onboarding sessions, recipes, Spins,
Spin publications, playlist surfaces, and surface directives. Both pristine
and migrated copies contain the same 101 historical plans, all honestly shown
as `<legacy-unlabeled>`: current production predates persisted `maintenance`
and `spin_publication` origin labels. The migration does not relabel history or
fabricate either origin. Future plans must persist their real origin.

`pg_amcheck` passed all 852 relations and 19,238 pages. The pristine database
uses 159,405,759 bytes and the additive 47-migration copy uses 160,585,407
bytes.

## Compatibility result and next gate

The complete fake-binary intake suite passes all four cases: machine-readable
capabilities, capability-first failure, review-only compatible execution, and
rejection of `spin_publication` by maintenance intake. User-document command
coverage also passes.

V020-13 therefore clears the local-rehearsal prerequisite for V020-14. It does
not itself authorize a candidate database, production migration, connection
cutover, or provider write. V020-14 must take a newest-state backup again,
verify capacity and runtime independently, present exact database and Spotify
plans, and stop for separate approval before either cutover or provider action.
