# Changelog

All notable changes to Chordrift are documented here.

## [Unreleased]

## [0.2.1-alpha.18] - 2026-08-31

- Collapse the removal and addition halves of one provider move into one
  canonical maintenance decision before creating or editing a proposal; stop
  safely if one track has conflicting inferred destinations.
- Treat current Spotify membership already represented in an editable copy as
  covered, preventing an interrupted run from relabeling the full library as
  direct intake.
- Keep active exclusions authoritative when replaying historical assignment
  revisions into an editable proposal, and version the corrected extension
  algorithm so an older cached copy cannot be reused.
- Recover the live interrupted five-track Rasa Archive → Cinema Monsoon move
  without writing Spotify, then prove the approved 1,429-track model has zero
  pending maintenance operations and zero phantom direct intake.
- Add fake-binary, unit, and disposable-PostgreSQL regressions for paired plan
  evidence, editable-copy classification, and exclusion precedence.

## [0.2.1-alpha.17] - 2026-08-30

- Add the installed CLI's authenticated remote service client with mandatory
  contract, schema, and capability negotiation before commands or queries.
- Store only opaque Chordrift product sessions in the operating-system
  credential store; session values are accepted through standard input and are
  never printed.
- Require HTTPS outside loopback development and accept only typed application
  DTOs and structured client-safe errors.
- Retain an explicit in-process development client over the identical transport
  trait and prove remote/local compatibility and command/query parity.
- Advertise `service.remote-cli.v1`. Hosting, public login, and deployment
  configuration remain V021-06.

## [0.2.1-alpha.16] - 2026-08-30

- Name the verified managed destination when a newly liked track is already
  placed, then ask whether Liked Songs should remain.
- Persist each per-track keep/clear answer as a revisioned directive on a
  virtual Liked Songs intake surface already present in the migration-47
  product schema; undecided and keep states never plan an Unlike.
- Allow an explicit clear answer to produce only an exact reviewed saved-state
  removal, while a later direct provider Unlike supersedes an older keep
  directive during record-only convergence.
- Add `maintenance.saved-intake-disposition.v1` and application-contract 1.3's
  explicit `consume_intake` resolution for thin CLI/web/mobile clients.
- Extend fake-binary and disposable-PostgreSQL regressions for human labels,
  remembered answers, clear planning, undecided safety, and direct Unlike.

## [0.2.1-alpha.15] - 2026-08-30

- Add a restart-safe typed application command queue with exact durable
  account/subject-scoped idempotent receipts and collision rejection.
- Add exclusive expiring PostgreSQL worker leases, explicit heartbeat, atomic
  concurrent claim, stale-worker rejection, and append-only ordered lifecycle
  and structured-progress events.
- Persist cooperative cancellation through service restarts, cancelling queued
  work immediately and running work only at a safe worker checkpoint.
- Add bounded retry and abandoned-lease recovery with durable recoverable and
  terminal client-safe failure states.
- Add migration 0050 for current operation state plus immutable event streams;
  payloads contain typed application DTOs and never provider credentials.
- Prove process restart, exact replay, concurrent claim, progress, tenant
  isolation, lease expiry, retry exhaustion, cancellation, history, and event
  cursors on disposable PostgreSQL.
- Advertise `service.durable-operations.v1` while retaining application
  contract 1.2 and local maintenance compatibility with migration 0047.

## [0.2.1-alpha.14] - 2026-08-30

- Add a server-side XChaCha20-Poly1305 provider refresh-credential vault whose
  root keys remain outside PostgreSQL and whose plaintext values are zeroized.
- Bind every authenticated ciphertext revision to its Chordrift account,
  provider account, provider namespace, credential kind, algorithm, and
  external key ID so tampering or identity substitution fails closed.
- Recheck current product membership and ownership on every operation; allow
  internal credential leasing to active members while limiting atomic rotation
  and revocation to the active owner.
- Add migration 0049 with one active encrypted generation per provider account,
  retained rotation/revocation history, and no plaintext token or key material.
- Prove round-trip encryption, key rollover, tenant isolation, authorization,
  tamper failure, rotation, and revocation in memory and on disposable
  PostgreSQL without contacting Spotify or the personal Neon database.
- Advertise `service.provider-credential-vault.v1` while retaining application
  contract 1.2 and local maintenance compatibility with migration 0047.

## [0.2.1-alpha.13] - 2026-08-30

- Preserve accepted provider order when replaying an assignment that is already
  satisfied in its requested destination instead of deleting and appending the
  track by revision chronology.
- Checkpoint every exactly converged ordinary-maintenance observation as the
  next immutable managed baseline without contacting or writing Spotify.
- Ensure a track removed after that baseline becomes an active exclusion and
  cannot be restored from an older approved proposal.
- Add `tracks exclusions` plus exact-confirmed `tracks empty-exclusions`; the
  latter changes only Neon disposition, retains audit history and a hidden
  replay-blocking forget tombstone, and refuses tracks still present in the
  current provider observation.
- Add the normative provider-first sequence and permanent regressions for
  order stability, later removal, interrupted cumulative state, and the
  exclusion lifecycle.

## [0.2.1-alpha.12] - 2026-08-30

- Add provider-neutral external identity verification plus persisted product
  subjects, account ownership, and revocable Chordrift bearer sessions.
- Generate 256-bit opaque session tokens, return plaintext once, and store only
  SHA-256 digests in additive migration 0048.
- Add authenticated session exchange and exact current-session revocation
  without exposing password, SQL, subject-selection, or role-override routes.
- Invalidate access immediately on session revocation/expiry, subject or account
  suspension, membership revocation, or cross-account use.
- Keep local maintenance compatible with the verified migration-0047 music
  database while requiring migration 0048 before hosted identity traffic.
- Keep application contract 1.2 unchanged, version session exchange separately
  as schema 1, and advertise `service.product-identity.v1`.

## [0.2.1-alpha.11] - 2026-08-30

- Add one asynchronous Rust maintenance application authority behind typed
  provider/database ports.
- Add authenticated `/v1/commands` and `/v1/queries` HTTP routes with no CLI,
  shell, SQL, or arbitrary provider execution endpoint.
- Scope sessions, resources, operation history, and idempotency by authenticated
  subject and Chordrift account.
- Add reconnectable operation views and cursor-filtered lifecycle events,
  cooperative cancellation, structured request budgeting, and fixed safe HTTP
  error mapping.
- Prove identical in-process and real loopback HTTP outcomes/provider traces,
  cumulative refresh, stale-review rejection, auth isolation, retry/collision
  handling, event ordering, cancellation, and secret-free failures.
- Advance the additive application contract to 1.2 and advertise
  `service.authenticated-transport.v1`.

## [0.2.1-alpha.10] - 2026-08-30

- Add wrapper-neutral maintenance session DTOs for typed start, refresh,
  ambiguity resolution, exact authorization, and immutable session queries.
- Keep maintenance workflow transitions, allowed actions, revision checks, and
  client-safe error mapping in the Rust core rather than CLI or web adapters.
- Add a Rust session router plus in-process and JSON-loopback conformance tests
  proving identical behavior, record-only provider ordering, and cumulative
  refresh invalidation of stale reviews.
- Advance the additive application contract to 1.1 and advertise
  `maintenance.task-session.v1`; authenticated HTTP and operational adapter
  wiring remain V021-01.

## [0.2.1-alpha.9] - 2026-08-29

- Re-scan a newly approved maintenance plan for record-only provider order
  after direct intake or another intent revision reveals the delta.
- Converge membership-equal provider ordering through bounded Neon-only
  proposal revisions before classifying remaining provider work.
- Preserve the rule that no order convergence path calls `sync apply`; stale
  plans are rebuilt from the newest complete Spotify snapshot.
- Add a fake-binary regression for direct intake followed by a newly exposed
  Celluloid Mehfil reorder and publish a durable daily-driver edge-case ledger.

## [0.2.1-alpha.8] - 2026-08-29

- Interpret a membership-equal managed-playlist order difference as provider
  ordering intent during ordinary maintenance instead of stopping on a
  `reorder_playlist` operation.
- Add a Neon-only `proposals align-provider-order` command guarded by exact,
  duplicate-free provider/proposal membership equality.
- Rebuild the approved proposal and artwork carry-forward without issuing a
  Spotify reorder or any other provider membership write.
- Add `maintenance.provider-order-intent.v1` plus unit and fake-binary
  regressions for the exact seven-track Celluloid Mehfil failure shape.

## [0.2.1-alpha.7] - 2026-08-29

- Carry the complete 25-cover Drift Atlas v5 artwork system onto ordinary
  model-only proposal revisions instead of importing the obsolete 20-cover v4
  manifest.
- Keep a direct managed addition unresolved until its containing proposal is
  approved, allowing a maintenance run interrupted after assignment to resume
  safely on the next invocation.
- Add `maintenance.artwork-carry-forward.v1` and a fake-binary regression that
  exercises the exact interrupted direct-intake recovery path.

## [0.2.1-alpha.6] - 2026-08-29

- Preserve a previously unknown track added directly to a managed Spotify
  playlist instead of planning a provider-drift removal.
- Extend the read-only intake audit with `direct_managed_addition` and the
  exact observed destination.
- Record an unambiguous existing destination in the approved Chordrift model
  without removing or re-adding the provider track; batch assignments through
  the ordinary maintenance workflow.
- Keep multiple destinations ambiguous and require an explicit decision;
  never restore an actively excluded track automatically.
- Add the `maintenance.direct-managed-intake.v1` capability plus planner,
  database, and fake-binary regressions.

## [0.2.1-alpha.5] - 2026-08-29

- Replace two per-track inspection passes in the ordinary maintenance wizard
  with one set-based plan annotation query.
- Add track titles, artists, and direct-move interpretation to immutable plan
  details while preserving the original eight machine-readable columns.
- Print `Analyzing observed changes…` immediately after provider observation so
  database-backed preparation is never a silent pause.
- Add the `maintenance.bulk-plan-preview.v1` binary capability and fail closed
  when a script is paired with an older binary.
- Reduce the same 22-operation preview observed in daily use from 108.58 seconds
  to 3.06 seconds; a complete current plan/audit review finishes in 8.54 seconds.

## [0.2.1-alpha.4] - 2026-08-29

- Publish explicitly approved empty manual destinations so the listener can
  move the first tracks into them directly in Spotify; generated empty clusters
  remain suppressed.
- Add five reviewed Indian-library surfaces: Raga Meridian, Kaveri Resonance,
  Celluloid Mehfil, Cinema Monsoon, and Unscripted Rasa.
- Define Indian Film Classics as releases through 1979 and modern Indian film
  music as 1980 onward, eliminating the earlier 1970s boundary gap.
- Move only four already-reviewed classic Hindi tracks from Rasa Archive into
  Celluloid Mehfil; leave the other new surfaces empty rather than guessing.
- Reuse the retired Re-evaluate background for Raga Meridian, preserve the
  rejected color Celluloid study, and approve a monochrome Celluloid master.
- Complete Re-evaluate retirement after a fresh Spotify pull proved zero
  provider surfaces and tracks while Neon history remained intact.

## [0.2.1-alpha.3] - 2026-08-29

- Replace per-revision proposal replay with set-based PostgreSQL operations,
  preserving active manual assignments while eliminating hundreds of Neon
  round trips during ordinary direct moves.
- Assign all inferred moves for one destination in one atomic database
  transaction and one CLI session instead of invoking one transaction per
  track.
- Show visible progress while preparing the editable proposal and recording a
  batch of inferred moves.
- Preserve the retired Re-evaluate artwork as reusable approved visual
  inventory even after its empty Spotify container is manually deleted.

## [0.2.1-alpha.2] - 2026-08-29

- Fix direct managed-playlist moves that initially appear as new-destination
  provider drift: Chordrift now records the new canonical destination instead
  of removing the track from it.
- Bind one maintenance confirmation to only the reviewed plan phase. Any work
  discovered after provider verification requires a fresh wizard review and
  can no longer inherit the earlier confirmation.
- Show track titles, artists, and playlist names in the ordinary-maintenance
  summary while keeping Spotify IDs, plan IDs, and assessments as internal
  diagnostic evidence.
- Add fake-binary shell regressions for the recovered Dakshina Pulse → Uttara
  Glow failure shape and for newly generated exclusion work never being
  applied under an earlier confirmation.

## [0.2.1-alpha.1] - 2026-08-29

- Replace separate intake and correction scripts with one capability-checked
  daily maintenance wizard that infers direct moves between managed playlists,
  asks only for ambiguous placement, summarizes the provider-visible change,
  and retains the existing plan/readiness/apply/verify safety boundary.
- Retire the empty provider-native `Re-evaluate` workflow in favor of natural
  direct playlist moves while preserving all correction history in Neon.
- Keep a pending, separately reviewed retirement from blocking ordinary
  maintenance without allowing the daily wizard to execute that retirement.
- Preserve ordinary playlist membership through enumerated-only additions and
  reject Spin publication origins from maintenance helpers.
- Improve incompatible-binary diagnostics by showing the selected executable
  and identifying stale `CHORDRIFT_BIN` overrides.
- Refresh user-facing documentation around the v0.2.1 daily-driver direction,
  direct reclassification, historical queue retirement, and the separate
  future Classification Authority dependency.

## [0.2.0] - 2026-08-29

- Begin the v0.2.0 development line with a public, transport-neutral Rust
  application contract covering versioned commands, queries, immutable views,
  lifecycle events, progress, cooperative cancellation, structured client-safe
  errors, identity, and compatibility/capability negotiation without changing
  the released CLI, SQL, configuration, Neon behavior, or Spotify behavior.
- Route every existing CLI handler through one Rust application facade while
  preserving command parsing, redirected bytes, interactive presentation,
  errors, database/provider behavior, and the v0.1.4 safety model.
- Add provider-neutral domain values for account ownership, provider-qualified
  IDs, provider/evidence capabilities, overlapping collection membership,
  independent playlist-surface axes, recipe v1, and account-bound Spin identity.
- Prove two-account and two-provider-namespace isolation with a deterministic
  test-only fake-provider harness covering idempotent acceptance, cooperative
  cancellation, bounded retry, and visible unsupported-capability failure.
- Add migration 0046's provider-neutral ownership, capability, collection,
  surface, recipe, onboarding, Spin, and publication-link foundation; verify a
  fresh chain and migration-45 upgrade on isolated PostgreSQL 18 without
  applying it to production Neon.
- Add a provider-read-only onboarding application boundary that captures one
  immutable inventory checkpoint and optional extended-history evidence,
  persists content-addressed inputs and explicit no-intent/no-provider-write
  provenance, returns idempotent retries before another provider read, and
  enforces capability and account ownership through a fake provider plus
  isolated PostgreSQL 18.
- Add a read-only inventory audit query over V020-06's immutable checkpoint. It
  returns deterministic library, overlap, capability, uncertainty, and
  preserve-first starter-organization values; exposes unreadable data and
  inference limits; creates no approved intent; and is proven against fake
  inventory plus fresh and migration-45 PostgreSQL 18 rehearsals.
- Add the enriched form of the same audit over one explicitly fingerprinted
  extended-history import. Preserve the inventory-only findings, report exact
  history coverage and only directly supported strengthened conclusions, keep
  preference and intent visibly uninferred, and prove both modes side by side
  without provider or database mutation.
- Add provider-neutral Discovery + Rediscovery recipe execution. Canonicalized
  inputs deterministically allocate source seats, reserve familiar cadence and
  narrative-section capacity, enforce eligibility plus track/artist budgets,
  expose unavailable or degraded evidence and unfilled seats, and return an
  explicitly unordered fingerprinted draft for the later Spin preview.
- Add exact deterministic Spin previews over verified recipe drafts. Persist
  stable account/recipe identity, full unsigned seed, capability and provenance
  fingerprints, one-based ordered views, per-track selection/ordering reasons,
  narrative sections, and honest constraint warnings in migration 0046's
  existing Spin tables, with idempotent replay and account-isolated reads.
- Add the opt-in `chordrift product` development CLI over the existing
  onboarding, collection-review, recipe-execution, and Spin-preview application
  boundaries. Add an installed-binary helper that compares inventory-only and
  enriched audits and proves persisted Spin replay without any provider command.
- Reconcile the recovered intake/apply incident selectively: ordinary playlist
  additions now append only enumerated operation IDs; the development binary
  exposes a JSON capability handshake and read-only intake audit; maintenance
  plans carry an explicit origin; and the complete guided intake workflow plus
  supporting helpers fail closed on incompatible binaries and non-maintenance
  plans. Fake-binary and Rust regressions cover full-replacement and unintended-
  restoration failures without a provider write.
- Convert an approved account-owned Spin into an immutable checkpoint-bound
  plan with explicit `spin_publication` origin. Add migration 0047's
  surface-to-recipe and Spin-plan identity reconciliation, provider-neutral
  readiness/fake execution/verification, idempotent replay, enumerated-only
  additions, protected unrelated live membership, and active-exclusion proof
  without wiring the production Spotify mutation adapter.
- Rehearse the complete v0.2 schema against a fresh read-only production
  backup restored only into local PostgreSQL 18. Preserve byte-identical
  invariants and exact hashes for inventory, intake, exclusions, Re-evaluate,
  assignments, listening/archive evidence, and durable synchronization
  history; pass migration replay, `pg_amcheck`, and the complete intake
  fake-binary suite without creating a candidate or changing production.
- Create a fresh Neon PostgreSQL 18 candidate from an even newer read-only
  production backup, migrate it from 45/47 to 47/47, and verify byte-identical
  invariants plus exact normalized data parity across the 21 durable-domain
  tables. Pass the current-main capability handshake, maintenance/intake,
  Spin-origin, fake-provider, and disposable PostgreSQL suites; leave the
  installed v0.1.4 binary and production connection unchanged at the explicit
  atomic cutover approval gate, with no Spotify call or provider write.
- Make the everyday product contract explicit: people continue using Spotify
  naturally while Chordrift hides internal plan/assessment/receipt ceremony,
  records supported reversible intent automatically, leaves ambiguity for
  review, and asks for consent only before Chordrift mutates a provider.
- Record the long-horizon classification knowledge architecture: explicit
  provenance-backed canonical facts and versioned lawful shared vector/model
  generations remain distinct from private account classifications,
  corrections, listening evidence, and request-time context. Treat vector
  indexes as rebuildable retrieval structures behind an authenticated service,
  and require data/model rights, consent, privacy, and multilingual evaluation
  before any shared learning or foundation-model work.
- Reconcile current documentation around one explicit boundary: v0.1.4 remains
  the released daily-use CLI, while `main` documents implemented and planned
  v0.2 work without presenting completed database/legacy gates as current
  operator instructions.
- Let the operator wrapper retry a zero-operation plan when Spotify has not yet
  exposed a just-edited playlist snapshot, with optional bounded polling.
- Add an installed-binary Inbox placement helper that resolves stable proposal
  destinations, rejects active exclusions and non-Inbox/already-resolved
  tracks, records only reviewed Neon intent, and stops before approval or any
  Spotify write.

## [0.1.4] - 2026-08-26

- Run independent saved-track, saved-album, and recent-play Spotify probes
  concurrently after the single playlist-header request.
- Batch unchanged playlist metadata, account links, transient headers, reused
  memberships, historical identities, and recent events to minimize Neon
  round trips and writes.
- Advance a fully unchanged materialized inventory by retaining its existing
  content-addressed revisions instead of copying and deleting transient
  playlist/saved-surface membership rows.
- Reuse unchanged library analysis and incrementally refresh listening
  statistics only for identities affected by new observations or new matches.
- Skip managed-playlist baseline verification when provider membership is
  unchanged and no apply run awaits a pull; batch verification headers and
  ordered tracks when a real verification is required.
- Render `sync pull` as compact library/evidence tables with per-phase elapsed
  times so real-world latency remains visible and actionable.
- Route every interactive command through one shared presentation layer while
  preserving stable plain redirected output; use compact tables, readable JSON
  evidence, and a consistent multi-phase progress bar and event style.
- Replace the last database-v1 library-candidate function body with v2 current
  revisions and verified baselines, and batch readiness-check receipts.
- Add an operator-only installed-binary wrapper for the complete safe
  pull/plan/readiness/confirm/apply/pull/convergence loop; it refuses
  destructive, stale, and ambiguous multi-phase plans.

## [0.1.3] - 2026-08-26

- Prepare v0.1.3 as the database-v2 runtime: all ordinary reads use current
  content-addressed provider state and normalized listening evidence instead of
  duplicated provider snapshots or legacy event rows.
- Write Spotify archive and recent-play evidence directly to normalized imports,
  historical identities, source files, and typed listening events.
- Persist provider pulls through transient v2 import surfaces, reuse unchanged
  playlist and saved-library revisions, and leave import staging empty at commit.
- Add exact-confirmed `db compact cleanup plan/apply/verify` phases that preserve
  durable invariants and receipts while renaming provider staging tables and
  removing superseded database-v1 event/archive tables.
- Prove the clean runtime on fresh PostgreSQL 18 rehearsals, migrate the verified
  replacement Neon project, switch the persistent application connection, and
  retire the former project only after cleanup and observation gates pass.
- Preserve exact invariant parity, 149,314 normalized listening events, both
  archive manifests, 24 compact checkpoints, and all durable audit history while
  reducing the live database from 358,850,560 to 167,788,544 bytes.

## [0.1.2] - 2026-08-25

- Replace destination-specific review routes with one provider-owned,
  zero-signal `Re-evaluate` queue; record immutable entry/exit history, suppress
  accidental exclusion and source restoration while queued, export long queues
  through the standard classification worksheet, and clear an item only after
  a newer explicit assignment selects a different approved destination.
- Add exact-confirmed, coverage-gated legacy-route retirement whose Spotify
  container archival remains an immutable retirement-plan operation.
- Require complete approved proposals to emit an explicit, separately gated
  retirement for every omitted managed canonical playlist, preventing retired
  concepts such as `Monsoon Cinema` from surviving silently.
- Reconcile the approved 454-track South Asian review into `Dakshina Pulse`,
  `Uttara Glow`, and `Rasa Archive`; retire unavailable `Latika's Theme`, move
  global score material back to `Afterlight Score`, and reach a complete
  1,754-track proposal with zero unresolved or conflicting placements.
- Add the approved Drift Atlas v4 cover set for all 16 canonical and four intake
  surfaces, including new artwork for `Dakshina Pulse`, `Uttara Glow`, `Rasa
  Archive`, and the `Re-evaluate` queue.
- Mirror the provider-owned Re-evaluate queue through exact replacement without
  targeting the generated-membership partial uniqueness index.
- Verify approved canonical destinations independently of separately gated
  legacy containers, avoiding a circular verification/retirement dependency.
- Complete the live v0.1.2 Spotify reconciliation: publish all approved
  destinations and covers, clear 145 consumed Inbox entries, retire Monsoon
  Cinema and the three legacy routes, and reach a zero-operation plan.
- Opt Suhail's account into verified Liked Songs consumption, remove all 346
  supported saved tracks only after proving 345 canonical placements and one
  durable exclusion, verify zero supported saved tracks from Spotify, and
  converge again to a zero-operation plan.
- Derive PostgreSQL integration-test migration expectations from the embedded
  migration set so release CI cannot drift behind new schema migrations.
- Define the canonical/intake/generated-playlist product model, capability-aware
  recipe direction, permission-bounded agentic onboarding, quiet background
  operation, and the staged native-product roadmap toward v1.0.0.

- Add account-scoped `user_cohorts`, a complete classification/CSV glossary,
  Excel-ready A.R. Rahman examples, and stronger selected-account track
  isolation as the `0.1.2-dev.3` checkpoint.

- Add revisioned private collection, region, tradition, and language facts with
  atomic one-track/small-batch commands and an inert CSV review workflow.
- Render `tracks inspect` as a human-oriented interactive report, retain raw
  generation/provenance detail behind `--technical`, and expand interactive
  tables across the detected terminal width.
- Label this checkpoint `0.1.2-dev.2`; route reconciliation and the reviewed
  Monsoon Cinema split remain required before stable `0.1.2`.

- Add durable, zero-signal routing playlists for low-friction correction
  capture while listening. Route consumption into verified canonical
  destinations remains under development for v0.1.2.
- Avoid rewriting unchanged Spotify track metadata after a saved-library
  change, batch immutable saved-track membership in Neon, and expose TTY
  progress bars with plain redirected fallbacks.
- Render interactive playlist and song listings as compact colored tables while
  retaining complete tab-separated output for scripts and logs.

## [0.1.1] - 2026-08-24

- Add cursor-based Recently Played ingestion to normal Spotify pulls while
  retaining the lifetime extended-history archive as the authoritative source
  for durations, completions, and skips. Later archives supersede overlapping
  provisional API observations before statistics are rebuilt.
- Consolidate Spotify consent around one scope set and stop rewriting unchanged
  Keychain credentials during every command.
- Protect newly encountered user-owned playlists as `user_managed` by default.
  Add non-destructive retirement selection for named playlists, all playlists
  with explicit exceptions, or none; actual retirement keeps every existing
  coverage, approval, readiness, and destructive-apply gate.

- Add `tracks inspect` for one-command current placement, canonical assignment,
  source-playlist provenance, listening signals, embedding/cluster rationale,
  model facts, manual overrides, and exclusions.
- Add the `From Prompts` intake with prompted-interest semantics alongside
  `Inbox`, `From Friends`, and `Liked from Radio`.
- Add Drift Atlas v3 artwork for all canonical and intake surfaces,
  deterministic Spotify-scale lower-left Helvetica Neue labels, and preserved
  label-free masters for future Apple Music typography.
- Suppress already-succeeded identical artwork uploads for the same stable
  Spotify playlist, select only the newest approved artwork batch, and add
  `artwork update --playlist` for immutable one-cover update plans.
- Verify approved empty playlists and compare sparse proposal ordering keys as
  ordered track sequences after Spotify densifies provider positions.
- Document Spotify playlist folders as manual client-only presentation state;
  the Web API exposes neither folder structure nor custom folder covers.
- Define an account-scoped complete-library inventory from saved tracks and
  durable semantic, transport, intake, and canonical playlist history. Add
  `proposals inventory` and `proposals unresolved`, and block complete coverage
  unless every preserved track is placed or explicitly excluded.
- Keep listening history and provider-curated playlists as enrichment signals
  without silently importing every casually played track into the library.
- Add listening-session co-occurrence to personal embedding model v4, reducing
  complete-inventory tracks without useful vectors from 173 to 23 while
  retaining exact model/generation provenance.
- Preserve approved playlist identities through extension proposals, then add
  auditable direct-centroid, analytical-group-consensus, and explicit manual
  assignment paths. The personal proposal now represents all 1,711 preserved
  tracks exactly once with zero exclusions, unresolved tracks, or conflicts.
- Make manual assignment and `tracks inspect` understand transport-only and
  proposed-library tracks instead of limiting them to the current provider
  snapshot or last approved proposal.

## [0.1.0] - 2026-08-24

- Add a gated, resumable Spotify apply engine with exact assessment
  confirmation, separately acknowledged destructive phases, durable
  per-operation execution history, and post-pull convergence verification.
- Batch playlist additions and removals at Spotify's current limits, reconcile
  live membership before interrupted resumes, and use the February 2026
  `/items` and `/me/library` endpoints.
- Add approved cover uploads with deterministic local PNG-to-JPEG conversion,
  strict 256 KB payload enforcement, and explicit image-upload authorization.
- Add exact-plan retirement approval and require every canonical destination to
  be verified in the current snapshot before cleanup or retirement.
- Add a provider-free publish preflight that validates approved cover hashes and
  JPEG payload limits while estimating the request budget.

## [0.0.9] - 2026-08-24

- Add immutable, idempotent Spotify dry-run plans bound to one approved
  proposal and one imported snapshot, with exact operation inspection and no
  provider requests or mutations.
- Separate publication, managed-drift reconciliation, verified inbox cleanup,
  and explicitly approved legacy retirement into ordered safety phases with
  track-preservation gates.
- Include missing stable `Inbox`, `From Friends`, and `Liked from Radio`
  containers in the plan without duplicating existing intake surfaces.
- Schedule deterministic, original, explicitly approved cover artwork for
  every canonical playlist in v0.0.9, before any v0.1.0 publication.
- Add the original Drift Atlas v1 cover set for all 14 approved canonical
  playlists, including a darker user-selected Open-Sky Anthems revision and a
  local contact-sheet preview.
- Add strict artwork-manifest validation, content hashes, immutable Neon review
  batches, and explicit `artwork import|status|list|approve` commands without
  requesting Spotify image-upload scope or performing provider writes.
- Add immutable v0.0.9 apply-readiness assessments for snapshot freshness,
  approvals, operation ordering, destructive gates, interruption recovery,
  bounded Spotify 429 retries, idempotent replay, and explicit read-only OAuth
  scope probes through `sync readiness|readiness-show`.
- Define External Playlist Bookmarks for followed and externally collaborative
  Spotify/Apple Music playlists so their metadata and accessible contents can
  be retained in Neon before separately approved provider-library cleanup.
- Persist Spotify external bookmarks and immutable observations in Neon,
  separate them from the active library, detect provider snapshot changes,
  reuse readable collaborative contents, and add read-only bookmark list and
  track inspection commands.
- Add explicit one-bookmark refresh with immutable complete, inaccessible, and
  not-found attempt history; retain readable ordered tracks when Spotify
  permits access without adding requests to normal sync.
- Add immutable all-present-bookmark cleanup review batches, explicit approval,
  exact candidate inspection, and separately counted relationship-only cleanup
  operations in Spotify dry-run planner v5; provider writes remain disabled.
- Add a provider-neutral reversible Excluded Tracks ledger and report restores
  separately from ordinary additions and provider drift.
- Add immutable managed-playlist verification baselines so an expected user
  removal proposes an exclusion while an unexpected extra remains ordinary
  drift; allow cleanup gates to recognize durable exclusions as resolved track
  dispositions.

- Add model-neutral pretrained-audio inference artifacts and cache-first
  MusicBrainz recording, tag, release, and artist-area enrichment with strict
  provenance and no provider-audio acquisition.
- Add deterministic semantic-seeded vibe clusters with explicit unassigned
  tracks, immutable inputs, reproducibility hashes, and inspection commands.
- Add non-destructive proposed playlist generations with stable lineage-backed
  identities, per-source retirement coverage, and no Spotify writes.
- Add strict naming-context export and naming-result import with generator
  provenance, revision history, reserved-name validation, and explicit
  generation approval gated by complete naming and track coverage.
- Add a database-level current Spotify playlist projection so active queries
  expose only latest-snapshot names while immutable snapshots retain history.
- Add per-track inspection of proposal coverage gaps with Spotify identities
  and all contributing legacy/intake source playlists.
- Add stable manual semantic categories and reversible, audited track
  assignment/review decisions that replay into future proposal generations.
- Define the stable intake names `Inbox`, `From Friends`, and
  `Liked from Radio`, including their distinct user-facing meanings and safe
  retirement rules for obsolete utility playlists.

## [0.0.5] - 2026-08-24

- Add immutable, account-scoped semantic embedding generations with audit,
  status, and nearest-neighbor inspection commands.
- Keep listening, saved, rotation, discovery, intake, recommendation, and
  prompted-interest signals in a separately versioned behavioral generation.
- Add explicit playlist signal classes and clearing policies so provider-owned,
  intake, transport, ignored, semantic legacy, and canonical playlists cannot
  accidentally teach or clear one another.
- Generate a deterministic 1,024-dimensional semantic fallback from approved
  playlist, artist, album, and historical-name relationships while reserving
  canonical acoustic embeddings for lawful locally owned audio.
- Record the deferred Apple Music bootstrap, Spatial Audio workaround,
  provider tombstones, and reversible Excluded Tracks policy without enabling
  provider mutations.
- Document provenance-aware language, release-country, artist-region, mood,
  and sound enrichment as the prerequisite to clustering.

## [0.0.4] - 2026-08-24

- Add `playlists tracks` to inspect the ordered contents of a playlist's latest
  imported Neon snapshot by unambiguous name or stable Spotify ID.
- Add and link a canonical user-facing CLI guide with everyday sync and
  verification examples.
- Add privacy-conscious inspection and idempotent import of Spotify account-data
  and extended streaming-history ZIP archives.
- Preserve exact music playback timestamps, durations, skips, interaction
  reasons, platform/context flags, and Spotify track IDs while excluding IP
  addresses and account-profile PII.
- Add a Git-ignored, collision-safe local inbox/archive workflow that keeps
  Spotify's original `my_spotify_data.zip` filename.
- Add replay of retained local archives for Neon disaster recovery while
  keeping Neon authoritative during normal operation.
- Deduplicate cumulative future exports by stable core playback identity rather
  than archive or source-file boundaries.
- Derive account-scoped per-track event, meaningful-play, duration, skip,
  completion, and recency statistics; relink history after normal provider syncs.

## [0.0.3] - 2026-08-18

- Add `sync pull` to incrementally reconcile Spotify edits into Neon and refresh
  canonical analysis in one command.
- Preserve stable Spotify account and playlist identities independently from
  local account labels and mutable playlist names.
- Track account-scoped observed, discovery-inbox, and managed playlist roles,
  including provider-wins, Neon-wins, and manual drift policies.
- Mark historically known playlists absent when they disappear from the latest
  imported snapshot without deleting their history.
- Add aggregate library summaries, cross-playlist overlap reports, and
  within-playlist canonical duplicate reports.
- Keep v0.0.3 pull-only: role and drift policy are durable preparation for a
  later auditable dry-run/apply workflow and do not mutate Spotify.

## [0.0.2] - 2026-08-18

- Add Spotify Authorization Code with PKCE using read-only playlist and library
  scopes.
- Store account-scoped refresh tokens in macOS Passwords/Keychain and keep
  OAuth secrets out of Neon and shell initialization files.
- Add Spotify authorization, status, logout, and inventory-import commands.
- Snapshot owned and accessible collaborative playlists, ordered membership,
  duplicate entries, saved tracks, and provider metadata atomically in Neon.
- Preserve unavailable and unsupported item counts and report playlists skipped
  because of Spotify Development Mode access restrictions.
- Reuse unchanged playlist membership and saved-track inventories from Neon,
  reducing steady-state imports to the playlist index, changed playlists, and a
  one-page saved-library probe.
- Keep Spotify listening-history downloads optional for later play-count and
  listening-statistics enrichment.

## [0.0.1] - 2026-08-18

- Add the initial command-line application and `--version` support.
- Integrate Storexa 0.1.0 for Neon/PostgreSQL connections, health checks, and
  application-owned migration execution.
- Add `db status` for read-only health and migration diagnostics.
- Add `db migrate` for the embedded canonical schema.
- Establish canonical tracks, provider identities, immutable provider-library
  snapshots, listening events, embeddings, clusters, playlist generations,
  and synchronization audit records.
- Keep Spotify listening-history exports optional and independently importable.

## [0.0.0] - 2026-08-18

- Reserve the Chordrift crate and repository namespaces.

[Unreleased]: https://github.com/orbyts/chordrift/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/orbyts/chordrift/compare/v0.1.4...v0.2.0
[0.1.4]: https://github.com/orbyts/chordrift/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/orbyts/chordrift/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/orbyts/chordrift/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/orbyts/chordrift/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/orbyts/chordrift/compare/v0.0.9...v0.1.0
[0.0.9]: https://github.com/orbyts/chordrift/compare/v0.0.5...v0.0.9
[0.0.5]: https://github.com/orbyts/chordrift/compare/v0.0.4...v0.0.5
[0.0.4]: https://github.com/orbyts/chordrift/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/orbyts/chordrift/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/orbyts/chordrift/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/orbyts/chordrift/compare/v0.0.0...v0.0.1
[0.0.0]: https://github.com/orbyts/chordrift/releases/tag/v0.0.0
