# Candidate and personal cutover gate — V020-14

Status: candidate verification complete on 2026-08-28; database cutover is
awaiting separate explicit approval. No production database mutation,
connection change, installed-binary change, Spotify access, or provider write
has occurred.

## Verified candidate

Neon had capacity for one additional free-plan project. Chordrift created the
fresh isolated PostgreSQL 18 candidate below without resizing or deleting an
existing project:

| Property | Verified value |
| --- | --- |
| Project ID | `royal-snow-31539822` |
| Project name | `chordrift-v020-candidate-20260828` |
| Region | `aws-us-west-2` |
| PostgreSQL | 18.6 |
| Database / role | `chordrift` / `chordrift_owner` |
| Applied migrations | 47 of 47; zero pending or failed |
| Synthetic storage | 184,295,424 bytes of a 536,870,912-byte branch limit |

The candidate credential is kept only in a mode-`0600` temporary file and is
not part of the repository or recovery backup. A failed local `pg_amcheck`
invocation printed the initial candidate-only URI in diagnostic output. The
role password was immediately changed, the replacement credential was
verified, and the control-plane copy of the previous credential was verified
as rejected. No production credential was involved.

Neon's project role cannot install the optional `amcheck` extension, so remote
`pg_amcheck` could not inspect candidate relations. A local PostgreSQL 18.6
restore of this exact V020-14 dump passed the full parent/heap-index check over
751 relations and 19,115 pages. Candidate verification additionally uses an
exit-on-error restore, exact logical comparison, migration replay, application
health, and the complete test suite; the remote extension limitation is not
hidden or treated as a successful check.

## Newest-state source and exact parity

Immediately before candidate creation, the installed v0.1.4 binary reported
production healthy at 45/45 migrations. A new read-only custom-format backup
was preserved at:

`$DROPBOX/Music/Chordrift/Backups/2026-08-28-v020-14-candidate-source/`

The 22,686,266-byte dump has SHA-256
`cc1f53eb8d6740f94b97d39be24a8131164f479ae5a35ca33bcffe3824703225`.
The directory also contains schema, catalog, invariant, and checksum evidence.
The production invariant digest was
`5e0f61a73f0add2f426c28e209d98731f02fe9a2191905d38513f2a7fb1b9622`.

The dump restored into the candidate at 45/47. Only migrations 0046 and 0047
were then applied, and replay completed with no work. The pre-migration and
post-migration candidate invariant reports are byte-identical to production.
Important retained values are:

| Boundary | Candidate value |
| --- | ---: |
| Current playlists / ordered memberships | 22 / 1,514 |
| Playlist-order fingerprint | `486a998a48898351f5a94c40bc4f6665e6616e17b74053fe9c43bca8e053eb95` |
| Canonical playlists / assignments | 16 / 1,718 |
| Canonical fingerprint | `6b769b7b08529777c49276d48cf5210e9ad244c8475f6b0f8510b9b02fbbbaf2` |
| Active exclusions | 387 |
| Re-evaluate surfaces / tracks | 1 / 3 |
| Re-evaluate fingerprint | `5e73ebf725bee0c5f6c55d1ecac1db0f6f448a427a93be434cb5dca1cd849f98` |
| Listening events / historical identities | 149,412 / 15,605 |
| Listening-evidence imports | 2 |
| Verified apply runs | 37 |

Normalized UTC data-only dumps across the 21 V020-13 durable-domain tables are
byte-identical between a pristine local restore and the candidate. Both have
SHA-256
`77a55e441f84c1ea105d857bdb9c033356d2e1826778e0ddd796a473f7cde44b`.
`db v2 status` reports all inventory, order, saved-library, listening,
checkpoint, and history comparisons true with `ready_for_cutover: true`.

The current intake audit has zero unresolved items and performs no provider
write. All 101 historical plans remain honestly `<legacy-unlabeled>`; the
migrations did not fabricate `maintenance` or `spin_publication`. Capability
observations, onboarding sessions, recipe revisions, surfaces, directives,
Spins, and Spin publications all remain zero.

## Candidate binary and regression gate

The binary was built from current `main` in an isolated target directory so an
archived-worktree artifact could not be mistaken for the candidate. Its JSON
handshake advertises the required enumerated-maintenance, intake-audit,
intake-workflow, plan-origin, and Spin-publication capabilities.

The complete all-target/all-feature suite passes: 160 tests pass, five
database-dependent tests are intentionally ignored in the ordinary run, and no
test fails. The three applicable ignored PostgreSQL integration tests also pass
against a disposable PostgreSQL 18 database. This includes:

- both ordinary-addition regressions: unrelated membership is never replaced
  and a manually removed unenumerated track is never restored;
- all four fake-binary intake cases, including capability-first failure and
  maintenance rejection of a `spin_publication` plan;
- fake Spin publication preserving unrelated live membership and active
  exclusions; and
- account/provider isolation, deterministic replay, cancellation, bounded
  retry, CLI parsing, and user-document command coverage.

## Exact database cutover plan — approval required

Database cutover must be atomic with the compatible v0.2 candidate binary.
Changing only the database URL or only the installed binary is not approved.
If the user approves this plan, the cutover task will:

1. Keep normal Spotify use paused only for the short final comparison window;
   do not invoke Spotify or write either database during the gate.
2. Take a final read-only production status, invariant, and logical backup.
3. Compare that final source with this candidate. If production is unchanged,
   continue. If it advanced, refresh the candidate from the final backup,
   reapply only migrations 0046/0047, and repeat every parity/runtime gate.
4. Install the exact verified `main` binary and switch only the private
   `CHORDRIFT_DATABASE_URL` value to the verified candidate as one controlled
   operation. Do not change provider credentials.
5. Start a fresh process and run only read-only capability, migration, invariant,
   database-v2, plan-origin, and intake checks. The expected result is 47/47,
   exact parity, `ready_for_cutover: true`, and zero unresolved intake.
6. Leave the former production project intact as rollback evidence. If any
   check differs, restore the old binary/config pair; do not improvise a data
   merge or delete either project.

Approval of this database plan does **not** authorize Spotify access or a
Spotify write.

## Exact Spotify plan — no write is currently proposed

The database cutover itself needs no Spotify call. After a successful database
cutover, ordinary use begins with a read-only inventory observation so
Chordrift can compare the provider with the migrated ledger. A zero-operation
maintenance plan means no provider action is needed.

If observation later produces maintenance operations, Chordrift must show the
exact enumerated plan, current checkpoint, readiness result, and rollback
evidence before asking for a separate write approval. Ordinary additions may
append only their listed track IDs; they may not replace full membership or
restore an actively excluded/manual removal. Destructive phases remain
separately gated.

No real-provider `SpinPublicationProvider` exists yet, so V020-14 cannot and
will not publish a Spin to Spotify. A future implementation must preserve the
distinct `spin_publication` origin, mixed-authority/user-added tracks, active
exclusions, and enumerated-write rule before it can present an exact write plan.

## Gate result

The candidate itself is verified. V020-14 remains open at the explicit database
cutover approval boundary. V020-15 must not begin, production must not be
migrated, the installed v0.1.4 daily driver must not change, and Spotify must
not be invoked merely because this candidate passed.
