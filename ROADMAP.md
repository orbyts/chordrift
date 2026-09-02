# Roadmap

Chordrift will be developed in small, auditable milestones. The newest complete
provider observation is the source of truth for ordinary user-authored library
state. Neon is the durable history, intent, exclusion, and publication ledger;
Spotify and later Apple Music remain provider adapters. Chordrift writes a
provider only through a separately originated, exactly reviewed publication.

The Spotify downloadable history archive is an optional, independently
importable enrichment source. No other milestone is blocked on receiving it.

## Release execution map

This section is the authoritative ordered backlog. Each slice is one bounded
Codex task and ends with tests appropriate to its risk, documentation and
`CODEX_HANDOFF.md` updates, a commit, and a push. `CODEX_HANDOFF.md` identifies
the one slice that is currently allowed to start. Later narrative sections
preserve design rationale and completed migration history; they do not override
this order.

The v0.2.1 alpha is now the daily-use CLI, paired with the verified 47/47 Neon
music database. Neon remains authoritative for durable listening evidence,
configured intake, exclusions, historical correction evidence, classifications,
accepted provider baselines, and verified publication history. The newest
complete provider observation is authoritative for ordinary current membership
and order. The
former v0.1.4 binary/config/database pair and final pre-cutover backup are
retained only for controlled recovery. No development slice may dual-write
experimental state or change Spotify unless an exact plan receives separate
approval.

### v0.1.x maintenance line

Runtime behavior is frozen except for correctness, security, compatibility,
and serious usability fixes discovered during normal use.

- [ ] **M01 — Establish the maintenance line when the first fix is needed.**
  Create `release/0.1` from the last compatible main commit; do not interrupt
  the installed v0.1.4 workflow merely to create a release.
- [ ] **M02 — Reproduce and fix one reported defect.** Add a regression test,
  preserve database/provider safety, and carry the fix into both `release/0.1`
  and `main`.
- [ ] **M03 — Publish the next patch only when warranted.** Use v0.1.5, then
  v0.1.6 and so on. Cargo releases do not use four-part versions such as
  v0.1.4.1.

Maintenance slices are event-driven and may interrupt the v0.2 sequence only
for a real reported defect. They are not prerequisites for v0.2.0.

### v0.2.0 — Portable core and CLI-first Spins

Goal: prove the new product model end to end through the CLI, migrate the
latest personal state without interrupting v0.1.x use, and generate the first
deterministic provider-free Spins.

- [x] **V020-01 — Application contract foundation.** Define versioned command,
  query, event, progress, cancellation, structured-error, and compatibility
  types. Add contract tests. Do not change CLI behavior, SQL, configuration,
  Neon, or Spotify.
- [x] **V020-02 — CLI application-facade parity.** Route existing CLI handlers
  through one Rust application facade while preserving commands, redirected
  output, interactive presentation, and provider/database behavior.
- [x] **V020-03 — Provider-neutral domain foundation.** Add typed ownership and
  provider IDs, capability reports, collection membership strength, playlist-
  surface axes, recipe-v1 values, and Spin identities without leaking SQL or
  Spotify payload types.
- [x] **V020-04 — Isolation and fake-provider proof.** Add two-account and two-
  provider-namespace adversarial tests plus deterministic generation,
  idempotency, cancellation, retry, and unsupported-capability coverage.
- [x] **V020-05 — Additive schema plan and local rehearsal.** Reconcile proposed
  ownership, collection, surface, recipe, Spin, onboarding-session, and
  publication-link tables with existing equivalents. Implement and verify an
  additive migration only on isolated PostgreSQL 18.
- [x] **V020-06 — Onboarding session boundary.** Allow a session to read a
  provider inventory and selected evidence while ignoring existing Chordrift
  intent by default. Persist session inputs and output provenance without any
  provider write.
- [x] **V020-07 — Inventory-only new-account audit.** Produce an honest library,
  overlap, uncertainty, and capability report plus a starter organization using
  OAuth/current-inventory evidence alone.
- [x] **V020-08 — Enriched new-account audit.** Run the same acceptance path
  with extended listening evidence and explain exactly which conclusions became
  stronger. The inventory-only path must remain usable.
- [x] **V020-09 — Discovery + Rediscovery recipe v1.** Implement immutable source
  lanes, allocation, familiarity cadence, eligibility, hard boundaries,
  repetition/artist budgets, and simple narrative sections.
- [x] **V020-10 — Deterministic Spin preview.** Persist and display the exact
  ordered tracks, selection and ordering reasons, recipe revision, capability
  snapshot, input fingerprint, and seed. Replaying identical inputs must produce
  identical output.
- [x] **V020-11 — CLI-first product rehearsal.** Add consistent onboarding,
  collection, recipe, and Spin preview commands plus an installed-binary helper
  workflow. Compare inventory-only and enriched results without provider writes.
- [x] **V020-11R — Recovered-intake compatibility reconciliation.** Before
  publication planning, selectively reconcile the v0.1.x 92-track incident
  fixes with current main: enumerated ordinary playlist writes, the complete
  operator intake workflow, a binary capability handshake, fake-binary shell
  compatibility, and explicit separation of maintenance versus future Spin
  publication plans. Preserve safety invariants and operator outcomes through
  adapters; do not force legacy command shapes into the v0.2 Rust architecture
  when they conflict. Do not merge the maintenance branches wholesale.
- [x] **V020-12 — Publication-plan integration.** Convert an approved Spin into
  the existing immutable plan/readiness/apply/verify model. Exercise planning
  and verification with a fake provider; stop before a real Spotify write.
  Persist and expose `spin_publication` as an origin distinct from
  `maintenance`; maintenance intake helpers must reject it. Spin additions
  must use the same enumerated-write invariant, and fake-provider tests must
  prove that publication cannot replace unrelated membership or restore a
  manually removed track.
- [x] **V020-13 — Latest-state migration rehearsal.** Take a new logical backup
  of the then-current live database, migrate a local copy, and compare current
  inventory/order, intake, exclusions, correction history, assignments, listening
  evidence, archives, and durable plan/apply/verification history. Compare
  maintenance plan origins/capability observations as well as Spin publication
  origins, and rerun the complete intake fake-binary compatibility suite before
  presenting any cutover plan.
- [x] **V020-14 — Candidate and personal cutover gate.** Create a fresh candidate
  only when local rehearsal passes and capacity permits. Migrate the newest live
  state, verify runtime and invariants, present exact database and Spotify plans,
  and stop for separate approvals before connection cutover or provider writes.
  The candidate binary must pass the machine-readable capability handshake and
  the complete maintenance/intake and Spin-origin separation suite. Adapters may
  preserve v0.1.x safety outcomes, but must not weaken the provider-neutral v0.2
  ownership, determinism, or publication architecture. Candidate creation,
  migration, exact parity, runtime, capability, compatibility verification,
  and presentation of both plans completed the slice. The user later granted
  the separate approval and V020-15 recorded the successful atomic personal
  cutover. No Spotify write occurred.
- [x] **V020-15 — Release v0.2.0.** Complete formatting, strict Clippy, unit/doc/
  PostgreSQL integration tests, packaging, recovery documentation, GitHub
  release, and crates.io publication after the personal candidate is verified.

Post-release daily-driver hardening now exposes one ordinary maintenance wizard
for Likes, named intake, managed-playlist edits, exclusions, and direct moves
between managed playlists. It infers one unambiguous destination as
reclassification, asks only when destinations are ambiguous, summarizes the
net Spotify effect, and accepts one authorization. Exact proposals, plans,
readiness assessments, ordered publish/reconcile/cleanup phases, retries, and
receipts remain internal. New playlist/artwork design, retirement, and Spin
publication stay separate. The former `Re-evaluate` queue is retired; its Neon
events remain historical correction evidence and current clients must not
recreate it.

### v0.2.1 alpha checkpoints — installable daily-driver testing

Goal: publish official prereleases before and during v0.2.1 so Suhail can test
the exact crates.io artifact through the normal CI/release path. Alpha releases
are checkpoints, not replacements for the hosted-authority goal.

- [x] **A021-01 — Unified ordinary maintenance.** Replace separate operator
  workflows with one capability-checked wizard for Likes, named intake,
  exclusions, managed edits, and direct reclassification moves.
- [x] **A021-02 — Retire the correction holding queue.** Preserve all Neon
  correction history, deactivate the empty `Re-evaluate` surface, remove its
  empty Spotify relationship through one exact retirement operation, and prove
  it stays absent. Direct managed-playlist moves are the replacement gesture.
- [x] **A021-03 — Publish the first alpha.** Publish
  `v0.2.1-alpha.1`, then increment only for real fixes found through daily use.
  Every alpha must pass CI and install from crates.io with the capability
  handshake; never rely on the ambiguous v0.2.0 version string.
- [x] **A021-04 — Direct-move and confirmation repair.** Recover the six tracks
  affected by the alpha.1 Dakshina Pulse → Uttara Glow incident; recognize a
  new-destination drift removal as canonical reassignment before provider
  apply; bind authorization to only the reviewed plan phase; and replace opaque
  track and plan identifiers in the ordinary wizard with human names. Publish
  the regressions as `v0.2.1-alpha.2` through the normal CI/crates.io path.
- [x] **A021-05 — Fast delta maintenance.** Replace the 615-revision replay
  N+1 path with set-based SQL, batch same-destination moves atomically, expose
  visible progress, and prove the result on a representative isolated rehearsal
  copy. Publish the repair as `v0.2.1-alpha.3`.
- [x] **A021-06 — Reviewed Indian-library surface expansion.** Preserve and
  consider reusing the retired Re-evaluate visual asset. Design and preview
  managed surfaces for North Indian classical, South Indian classical,
  Indian film classics through 1979, 1980s-to-recent Indian film music, and
  non-film Indian music while retaining Uttara Glow as the exclusive A. R.
  Rahman surface and Dakshina Pulse. Resolve the 1970s boundary explicitly,
  present poetic names, exact membership changes, and artwork for approval,
  then apply only the separately approved design. Publish the implementation
  and reviewed asset set as `v0.2.1-alpha.4`.
- [x] **A021-07 — Fast bulk maintenance preview.** Replace the two per-track
  inspection passes after provider observation with one set-based plan preview
  containing human labels and direct-move interpretation. Print visible
  analysis progress immediately, preserve the immutable plan and confirmation
  boundary, advertise an exact binary capability, and add fake-binary
  regressions proving an ordinary review makes zero `tracks inspect` calls.
  Rehearse the reported 22-operation shape against Neon and publish the repair
  as `v0.2.1-alpha.5`.
- [x] **A021-08 — Direct managed-playlist intake.** Treat a previously unknown
  track added directly to a managed provider playlist as preserved intake plus
  explicit destination intent, not provider drift to remove. Keep direct moves
  of known tracks unchanged, resolve active exclusions explicitly, batch the
  new assignments through the ordinary wizard, and add fake-binary regressions
  proving destination-only additions cannot be deleted or require Liked Songs.
  Publish the repair as `v0.2.1-alpha.6` before relying on Spotify's per-playlist
  Add action for new tracks.
- [x] **A021-09 — Complete artwork carry-forward and resumable intake.** Carry
  the current 25-cover approved visual system onto model-only proposal
  revisions, keep an assigned direct addition pending until proposal approval,
  and prove an interruption after assignment resumes without repeating the
  Spotify gesture or applying membership work. Publish the repair as
  `v0.2.1-alpha.7`.
- [x] **A021-10 — Provider-order intent without provider reorder.** When direct
  intake leaves a membership-equal ordering difference, clone the approved
  proposal and accept the exact observed order in Neon under an equality guard.
  Never turn the repair into a Spotify reorder, addition, or removal. Publish
  the fix as `v0.2.1-alpha.8`.
- [x] **A021-11 — Cumulative observation convergence.** Treat every complete
  provider pull as the newest baseline for user-authority state, preserve
  previously recorded provider gestures across interrupted runs, and rebuild
  stale plans against the cumulative snapshot. Iterate record-only proposal
  revisions to a bounded fixed point so direct intake can reveal and then
  absorb an order delta without stopping or calling provider apply. Record the
  daily-driver incidents in one edge-case ledger, prove the intake-then-reorder
  sequence with a fake binary, and publish the repair as `v0.2.1-alpha.9`.
- [x] **A021-12 — Wrapper-neutral maintenance task contract.** Move
  task-level maintenance decisions and transitions into Rust-owned DTOs and a
  session reducer; give clients typed start, refresh, resolve, authorize, and
  query operations with revision-bound reviews and server-selected allowed
  actions. Prove that in-process and JSON-loopback transports produce identical
  outcomes, that provider-authored ordering converges without authorization,
  and that a newer complete snapshot invalidates stale authorization. Advertise
  `maintenance.task-session.v1` under application contract 1.1 and publish the
  foundation as `v0.2.1-alpha.10`. The existing shell remains a temporary daily-
  driver adapter; authenticated HTTP and typed database/provider execution
  ports remain V021-01.
- [x] **A021-13 — Provider-first convergence checkpoint.** Repair the
  daily-driver loop in which replaying an already-satisfied assignment changed
  accepted provider order, and record an immutable exact-membership-and-order
  baseline after every successful record-only maintenance run. Prove that a
  later removal becomes a durable exclusion and can never be restored from an
  older proposal. Add explicit list/empty operations for the reversible
  exclusion archive, retain its audit history, document the provider-first
  sequence as a wrapper-neutral contract, and publish the repair as
  `v0.2.1-alpha.13` before resuming V021-03.
- [x] **A021-14 — Remembered saved-intake disposition.** When a newly liked
  track is already present in a verified managed destination, name that
  destination and ask whether Liked Songs should remain. Persist the per-track
  keep/clear answer as a revisioned virtual-surface directive; undecided and
  keep states must never plan an Unlike, while clear produces one exact
  confirmed saved-state effect. Accept a later direct provider Unlike as newer
  intent that supersedes an older keep directive. Extend application contract
  1.3 with an explicit `consume_intake` resolution, update the canonical
  sequence diagram, prove wrapper output plus disposable-PostgreSQL behavior,
  and publish `v0.2.1-alpha.16` before V021-05.
- [x] **A021-15 — Interrupted move convergence.** Collapse paired remove/add
  plan evidence into one logical move before proposal mutation, treat current
  membership represented by an editable copy as already covered, and prevent
  stale assignment revisions from overriding active exclusions during
  proposal extension. Recover the five-track Rasa Archive → Cinema Monsoon
  incident without a Spotify write, prove zero pending maintenance and zero
  phantom intake, and publish the regression as `v0.2.1-alpha.18` while
  V021-06 remains in progress.

**Daily-driver experience refinement queue (non-blocking):** batch the active
items in `docs/design/DAILY_DRIVER_EDGE_CASE_LEDGER.md` into a later coherent
checkpoint. The first recorded case treats a newly liked track that is already
managed as a rediscovered favorite: display its destination and canonical
position, then eventually offer a separately authorized move-to-top action.
This queue does not reopen A021-14 or delay V021-05.

### v0.2.1 — Hosted Rust authority

Goal: make the same application contract safely consumable by shipped clients
without distributing database or provider credentials. v0.2.1 final remains
the expected daily driver while the separate Classification Authority project
and later Chordrift refactor begin.

- [x] **V021-01 — Authenticated service transport.** Expose the existing
  command/query/event contract without redefining domain behavior. Place
  maintenance orchestration behind one asynchronous Rust application service
  and typed database/provider ports; clients never invoke CLI commands. Run the
  same cumulative-observation and provider-safety scenarios through in-process
  and authenticated HTTP
  transports, proving serialized round trips, idempotent retry, reconnect,
  cancellation, stale-revision rejection, event ordering, secret-free errors,
  and identical provider-call traces. Never expose a generic “run CLI command”
  endpoint. The acceptance contract is
  `docs/design/WEB_SERVICE_CONTRACT.md`. The real product-session
  authenticator is V021-02, provider credential storage is V021-03, durable
  operation persistence is V021-04, and the current CLI moves onto this service
  in V021-05. Publish the transport checkpoint as `v0.2.1-alpha.11`.
- [x] **V021-02 — Product identity and authorization.** Persist provider-neutral
  product subjects, verified external-identity bindings, account ownership, and
  revocable digest-only Chordrift sessions in additive migration 0048. Exchange
  an upstream identity credential only through a pluggable verifier, never let
  clients assert a subject or ownership relation, and resolve every request
  through current session, subject, membership, and account state. Prove the
  complete two-tenant denial matrix over real HTTP and PostgreSQL while keeping
  local maintenance explicitly compatible with schema 0047. Publish the
  checkpoint as `v0.2.1-alpha.12` with unchanged application contract 1.2,
  product-session schema 1, and capability `service.product-identity.v1`.
- [x] **V021-03 — Encrypted provider credential vault.** Keep refresh credentials
  server-side with XChaCha20-Poly1305 authenticated envelopes whose keys remain
  outside PostgreSQL. Bind every revision to its account, provider connection,
  provider, kind, algorithm, and external key ID; allow internal leases only
  after current membership authorization; and restrict atomic rotation and
  revocation to the account owner. Prove tenant isolation, tamper failure, key
  rollover, one-active-generation persistence, and revocation on disposable
  PostgreSQL. Clients retain only Chordrift sessions, and no raw-credential
  route exists. Publish the checkpoint as `v0.2.1-alpha.14` with additive
  migration 0049 and capability `service.provider-credential-vault.v1`.
- [x] **V021-04 — Durable background operations.** Persist typed commands and
  exact account/subject-scoped idempotent receipts before execution. Add
  exclusive expiring worker leases with heartbeat, immutable ordered progress
  events, durable cooperative cancellation, bounded retry, abandoned-lease
  recovery, stale-worker rejection, and reconnectable operation/history/event
  queries. Prove exact replay and collision rejection across fresh service
  instances, one winner under concurrent claim, tenant isolation, cancellation,
  recovery, and retry exhaustion on disposable PostgreSQL. Publish the
  checkpoint as `v0.2.1-alpha.15` with additive migration 0050 and capability
  `service.durable-operations.v1`.
- [x] **V021-05 — Remote CLI parity.** Make the installed CLI an authenticated
  service client while retaining an explicit local development transport.
  The installed binary now stores only an opaque Chordrift session in the OS
  credential store, negotiates contract/schema/capabilities before work, and
  submits the same typed command/query DTOs over authenticated HTTPS. Non-TLS
  transport is limited to loopback development. An explicit in-process client
  implements the identical transport trait for deterministic tests; neither
  client can submit shell, SQL, provider URLs, or provider credentials. Real
  HTTP/in-process conformance proves compatible negotiation and response
  parity. Publish as `v0.2.1-alpha.17` with capability
  `service.remote-cli.v1`; hosting and external login selection remain V021-06.
- [ ] **V021-06 — Dual-client daily driver and private-beta release.** Ship the
  hosted authority as `v0.2.1-beta.1` only when the web interface and remote CLI
  are both usable daily drivers over the same Rust contract. This is not yet an
  unrestricted public web launch. Acceptance requires all of the following:

  - [x] **Production assembly.** Add explicit API and worker entry points that
    wire the typed application contract, PostgreSQL product sessions,
    encrypted provider-credential vault, durable operation queue, and a real
    Spotify/PostgreSQL maintenance adapter. A deployed route must never invoke
    a CLI command, shell, arbitrary SQL, or a client-supplied provider URL.
  - [x] **Bounded storage lifecycle.** Keep durable listening evidence,
    accepted user intent, current provider anchors, and exact write receipts.
    Define and rehearse a lossless retention/compaction policy for superseded
    playlist generations, verification history, and routine sync audit rows;
    staging must be empty after successful work and disposable Neon rehearsal
    branches must be deleted immediately. The migration-0050 domain map,
    exhaustive object catalog, and 2026-08-31 storage baseline are complete.
  - [x] **Chordrift identity with Google login.** Use Google through a standard
    OIDC broker as an external proof of identity while Chordrift retains its
    own stable subject, account ownership, revocable sessions, and future
    identity-linking boundary. Spotify authorization remains a separate,
    revocable provider connection and is never a Chordrift login method.
  - [x] **Existing-account adoption.** Bind Suhail's first verified Google
    issuer/subject to the existing Chordrift account through the trusted
    bootstrap boundary. Do not create an empty replacement account, rewrite
    music ownership IDs, re-import Spotify, or mutate provider state. Compare
    account, inventory, exclusion, directive, history, and playlist invariants
    before and after the identity cutover.
  - [x] **Thin browser workbench.** Serve a small HTML/CSS/JavaScript client
    that logs in, negotiates compatibility, submits only typed command/query
    DTOs, follows durable progress/events, renders structured results, and
    requires exact review authorization for provider effects. It must not be a
    browser terminal or accept arbitrary commands.
  - [x] **Provider connection lifecycle.** Keep Chordrift login independent
    from provider authorization. Add explicit Connect, Reconnect, Disconnect,
    and connection-status flows. Reconnecting the same stable Spotify identity
    must recover the same account-owned Neon history, intent, exclusions, and
    observations. Connecting a different Spotify identity creates or selects a
    separate isolated provider connection without overwriting either account.
    Multiple provider connections are first-class even though Spotify is the
    only launch adapter. Cross-account/provider transfer remains later work.
  - [x] **Daily-driver web maintenance.** Make observation, cumulative
    provider-first reconciliation, ambiguity decisions, exact provider-effect
    review, progress, cancellation, retry, and verification usable without a
    terminal. The web client may omit expert forensic queries, but it must make
    high-level divergence understandable—for example, `12 provider-only, 4
    Chordrift-only` rather than showing unexplained unequal totals.
    Library and Excluded views must expose personal play count, last-heard time,
    and album through the shared query contract; the browser may sort and group
    those returned facts without owning restoration policy. A canonical track
    identity is not a playlist assignment. For newly liked tracks, Rust must
    recommend a destination only when retained accepted placement evidence
    yields exactly one still-active surface; thin clients preselect that surface
    without treating the recommendation as consent. Missing or ambiguous
    placement evidence remains unselected.
  - [x] **Shared CLI/web investigation.** Expose the same provider connection,
    state timestamps, playlist summaries, directional membership differences,
    track facts, exclusions, operation state, and safe diagnostics through
    typed queries renderable by both clients. The CLI may offer deeper detail;
    neither client receives shell or SQL access through the service.
  - [x] **Interactive remote-CLI parity.** Provide one resumable hosted
    maintenance wizard that follows durable operations, renders the same
    recommendations and exact reviews as the browser, records typed decisions,
    and defaults every provider authorization to no. Keep low-level JSON
    commands and safe diagnostics as optional CLI-only automation tools; they
    must not duplicate domain or provider logic.
  - [x] **Reproducible containers.** Use a pinned multi-stage Docker build. The
    repository may exist in the checked-out build workspace/builder stage, but
    production API and worker images contain only the Rust executable, static
    web assets, certificates/runtime necessities, and provenance labels—not
    Git history, source, compiler caches, or build credentials. Run as a
    non-root user with read-only filesystem, bounded resources, health checks,
    and separately scoped API/worker processes.
  - [x] **Vortex compute deployment.** Deploy API and worker containers on the
    Ubuntu Vortex host. Keep Neon, Spotify, OIDC, session, and vault-key secrets
    outside images and version control. Bind the upstream service only to the
    private LAN/container boundary and prove a restart does not lose accepted
    operations or sessions.
  - [x] **Nexus ingress.** Terminate HTTPS for `chordrift.suhail.ink` through
    the existing Nexus Nginx/wildcard-certificate system and proxy privately to
    Vortex. Preserve the existing Tailscale subnet-router design; do not expose
    a second Vortex public listener. Restrict the Vortex service port to Nexus
    and verify forwarded-origin, request-size, timeout, and security-header
    policy.
  - [x] **Migration, backup, and restore proof.** A restorable pre-cutover Neon
    backup and isolated restore were verified; additive migrations 0048–0050,
    identity adoption, encrypted provider-credential storage, durable operation
    schema and post-cleanup invariants were rehearsed before the intended
    project was promoted. The retained legacy project was not migrated or
    promoted wholesale, and Spotify remained unchanged throughout cutover.
  - [x] **Observability and operations.** Add secret-redacted structured logs,
    request/operation correlation, health/readiness endpoints, worker lease and
    retry visibility, container restart policy, documented rollback, and a
    tested alert/inspection path. Logs must never contain product sessions,
    OIDC credentials, provider credentials, database URLs, or vault plaintext.
  - [x] **Read-only acceptance gate.** After login, prove the existing library
    is visible through the authenticated account, cross-tenant requests fail,
    logout/revocation works, backup restore works, and no Spotify mutation was
    made. Enable provider writes only through a later explicit user-approved
    gate after manual beta testing.
  - [ ] **Release proof.** Pass CI, container smoke and fake-provider browser
    tests, disposable-PostgreSQL identity/tenant tests, migration/restore
    rehearsal, and private deployment checks. Tag and publish the exact commit
    to crates.io; install that published version as the local CLI; build the
    API/worker image from the same tagged source; restart both Vortex services;
    and record the installed CLI version, deployed image digest, source commit,
    and live health result. Finally, run one DTO-conformance smoke through both
    the installed remote CLI and browser against that deployed authority before
    accepting `v0.2.1-beta.1`.
  - [x] **Provider-behavior acceptance matrix.** Run a deterministic synthetic
    provider account through the wrapper-neutral Rust maintenance contract on
    every CI push. Keep single-gesture cases for add, remove, move, reorder,
    and Like, plus composite, delayed-observation, and interrupted-retry
    snapshots. The fixture is small, credential-free, and cannot contact or
    mutate Spotify or production Neon. The real PostgreSQL interpreter also
    has regressions for direct managed intake, ambiguous placement, combined
    direct-add-plus-Like intent, and duplicate move halves.

  Current implementation checkpoint (2026-09-01): the Rust server entry point,
  Auth0/Google authorization-code + PKCE Web boundary, standard Auth0 Device
  Authorization Flow for the installed CLI, verified-email adoption gate,
  HttpOnly Chordrift sessions, same-origin browser bridge, typed
  JavaScript workbench, pinned non-root runtime image, Vortex Compose file, and
  Nexus proxy policy exist on the V021-06 development branch. A clean image
  built and passed an isolated Vortex liveness/readiness smoke test; the
  proposed Nexus configuration passed an isolated `nginx -t`. The intended
  Neon project used temporary backup and rehearsal branches to prove migrations
  0048–0050, the post-cleanup runtime schema, and schema-only restore. After
  proof, the hosted service and local CLI were consolidated onto the single
  canonical `main` branch/database at 50/50; both temporary branches and one
  stale duplicate database were deleted. The live database is approximately
  195 MB, with owner-only logical backups retained outside Neon. Future Neon
  rehearsal branches must have an expiry at creation and be deleted
  immediately after their recorded proof; prefer disposable local PostgreSQL
  when branch semantics are not under test. Disposable PostgreSQL identity, vault,
  durable-operation, tenant-isolation, migration, and normalized-history tests
  pass. The rehearsal also found and repaired cleanup-receipt verification:
  normal append-only listening/provider activity may change the live invariant
  without invalidating the cutover, while legacy-table return, non-empty
  staging, or evidence loss still fails verification. Active Vortex/Nexus
  deployment and provider-aware provider/model library inspection are proven.
  On 2026-08-31, exact commit
  `94f35133af725565c87eceaeda8beaf819dd03b5` was rebuilt as the pinned
  43.2 MB image `chordrift:94f3513` and deployed as separate non-root,
  read-only API and worker containers on Vortex. A fresh 29.4 MB compressed
  logical backup was validated before additive migration 0051 moved the
  canonical database from 50/51 to 51/51. HTTPS readiness, security headers,
  existing Google-account adoption, provider/model comparison, ordered
  playlist inspection, the 455-item exclusion archive, and Chordrift-session
  survival across a full API/worker restart passed in the real browser. The
  comparison resolves the reported Lightleak Reverie mismatch as 501 provider
  and 501 Chordrift memberships with only custom-order drift. The retained
  Spotify connection currently has no active encrypted credential, so provider
  observation remains correctly disabled until one explicit Reconnect OAuth.
  Consolidation deliberately did not copy rehearsal fixture identities,
  revoked fixture credentials, or fixture operations. Auth0/Google first-owner
  adoption and encrypted generation-1 import of the existing Spotify refresh
  credential must be reverified on the canonical database before beta.1. The
  provider selector visibly reports Spotify connection
  and newest-observation state and is designed for future connections. The
  exact web acceptance surface is recorded in
  `docs/design/WEB_WORKFLOW_CAPABILITY_MATRIX.md`. Remaining gates are the
  Spotify reconnect, canonical provider-effect authorization/apply/verification,
  the complete web maintenance journey, operational recovery checks, and beta
  publication. Read-only
  provider observation is now a real durable operation:
  the authenticated API accepts the typed command, the separately containerized
  worker leases the encrypted account-scoped Spotify credential, verifies the
  stable provider identity, calls the Rust inventory importer directly, rotates
  a returned refresh credential, persists the complete snapshot atomically, and
  exposes reconnectable progress/cancellation. No CLI, shell, arbitrary SQL, or
  client-supplied provider URL is in that route. The only enabled provider
  writes are server-rederived enumerated effects behind an immutable exact
  review; readiness reports this bounded scope explicitly.

  Provider-lifecycle hardening checkpoint (2026-08-31): the first live
  disconnect returned HTTP 403 because a proxied native form POST did not
  satisfy the route's exact-Origin guard. The first out-of-band Spotify Apps
  revocation then proved that a locally active credential cannot truthfully
  imply current provider validity before another provider call. Disconnect now
  uses a session-authenticated non-simple same-origin wrapper request. A
  terminal refresh-token rejection revokes the encrypted local envelope during
  the failed operation, returns typed `authentication_required`, and exposes
  Reconnect without removing provider history or Chordrift intent. The UI
  reports **Authorized** separately from its last provider verification. Unit
  regressions, full Rust tests, Clippy, JavaScript syntax, exact-image Vortex
  deployment, HTTPS health, and public repaired-asset checks pass. One
  authenticated browser transition remains before beta.1 acceptance.

  Browser-decision checkpoint (2026-08-31): the first composite destination
  plus Liked-state review exposed that the JavaScript dropdown serialized a
  playlist stable key where the typed Rust contract requires an opaque UUID.
  Rust now supplies the complete typed destination identity; the thin browser
  returns it unchanged, keeps the server-issued Liked Songs identity, submits the complete
  revision-bound decision set, and renders rejected submissions as retryable
  errors. A browser-DTO harness runs on every CI push alongside a Rust
  server-identity assertion. CI, exact-image deployment, public health, and
  repaired-asset checks pass; authenticated browser acceptance remains.

  Composite-effect safety checkpoint (2026-08-31): the first successful
  destination-plus-clear decision exposed that hosted execution projected only
  the saved-state removal. Spotify removed the track from Liked Songs before
  the selected destination membership existed, and later verification failed.
  Beta.1 now requires a durable two-stage boundary: exact destination add,
  observe and verify, then a separate exact intake-cleanup review. The
  production DTO/state-machine harness uses a fake database and fake provider,
  injects failures at both stages, reloads after worker restart, and proves no
  loss, no duplicate, and add-before-unlike ordering. The live damaged track
  must be offered as one recovery addition after the repaired build is deployed;
  no automatic Spotify recovery write is authorized. The full GitHub gate and
  exact-image Vortex deployment pass; authenticated recovery review remains.
  New Liked-only placements default to position zero (the top) through a
  Rust-owned policy named in the exact review; future top/bottom/specific-
  position choices remain a contract extension, not client logic.

  The first post-Disconnect OAuth consent exposed a retained-history generation
  defect: the vault attempted to reuse generation 1 because no envelope was
  active, so the callback failed its immutable uniqueness constraint. Rotation
  now serializes on the stable provider account and advances from the greatest
  active-or-revoked generation. Disconnect → Reconnect retains history and
  activates the next encrypted generation. In-memory and disposable-PostgreSQL
  lifecycle regressions are required before redeployment.

  The exact disposable PostgreSQL 18 lifecycle proof now passes generations
  1 → 2 → revoked → 3 with stable provider identity, one active envelope, and
  retained history; its container and temporary volumes were deleted
  immediately. Exact image `chordrift:951ce63` is deployed with matching
  revision metadata and healthy API/worker/readiness checks. One authenticated
  browser Reconnect remains for live acceptance.

  Record-only maintenance checkpoint: migration 0051 persistence, the real
  PostgreSQL provider-first interpreter, durable Start/Refresh/Resolve worker
  dispatch, and shared web/remote-CLI session access are implemented on the
  branch. Start and Refresh take a fresh provider observation; paired move rows
  collapse into one logical gesture. Resolved placement, exclusion, custom-order,
  and saved-track decisions now project idempotently into canonical intent through
  an exact maintenance fork that preserves approved artwork and never classifies
  unrelated tracks. Saved cleanup is withheld until all decisions are resolved
  and is ordered after destination intent. The disposable-PostgreSQL projection
  proof passes. Authorize now accepts only a server-rederived saved-state review,
  rechecks the provider snapshot, executes only its enumerated saved-track
  removals, observes again, and verifies before completion. Broader provider
  publication remains unavailable.

  Durable-session checkpoint (2026-08-31): additive migration 0051, a
  tenant/provider-scoped current session store, immutable accepted-revision
  events, exact compare-and-swap replacement, restart rehydration validation,
  and a Rust durable transition authority are implemented. A disposable
  PostgreSQL 18 proof on Vortex passed restart, cross-tenant isolation, and
  stale-revision rejection; its container, network, source copy, and build
  output were removed immediately. The migration remains staged, no Neon
  branch was created, Spotify was not contacted during that proof, and the
  hosted maintenance capability is branch-only rather than deployed. See
  `docs/design/DURABLE_MAINTENANCE_SESSIONS_V021_06.md`.

  Private-beta acceptance checkpoint (2026-09-01): Google/Chordrift login,
  independent Spotify Connect/Reconnect/Disconnect, retained-account adoption,
  shared Library/Excluded/Activity inspection, durable browser maintenance,
  exact provider-effect review, and the API/worker operational surface have
  passed authenticated daily-driver use on Vortex. The release candidate also
  exposed and repaired a provider-observation-lag sequence: a removal-only pull
  may activate an exclusion, but any later single placement—back in the same
  playlist or in a new one—supersedes it without a timing window or provider
  write. The exclusion's prior surface remains provenance so the event is
  recorded as restoration/reclassification rather than new intake. Multiple
  later destinations still require an exact decision. The canonical sequence
  and track lifecycle diagrams, CLI wrapper parity, planner annotations, and
  fake-provider single/composite matrix carry the rule. Live proof retained
  **Indiraiyo Ival Sundariyo** in Dakshina Pulse with one accepted observation
  and zero exact provider changes; the final candidate additionally resolves
  Cinema Monsoon as its retained prior source. Only the immutable release proof
  remains before `v0.2.1-beta.1` is accepted.

  Ordered remaining work for `v0.2.1-beta.1`:

  1. [complete] read-only encrypted-vault observation, durable API/worker
     runtime, maintenance-session persistence, PostgreSQL interpretation, and
     Start/Refresh/Resolve routing, canonical record-only projection, and saved-
     intake interpretation are composed and proven on disposable PostgreSQL;
  2. [complete] implement
     Connect/Reconnect/Disconnect and multiple isolated Spotify connections
     without changing Chordrift product identity; the Rust authority now uses
     hosted PKCE, stable-identity matching, encrypted in-place rotation, and
     history-preserving revocation while the browser only launches and renders
     the lifecycle; a disposable PostgreSQL 18 proof passed same-row history
     retention, credential generation rotation, cross-tenant rejection, and
     history-preserving disconnect, after which its container and source copy
     were deleted;
  3. [complete] finish the
     web maintenance journey and the shared provider/model comparison query
     while preserving remote CLI parity; contract v1.4 now returns set-based
     directional membership, unresolved-identity, and order explanations to
     both the web dashboard and `service library compare`;
  4. [complete] exercise
     provider reads first, publish and verify an enumerated destination addition
     before offering a separately reviewed saved-state removal, and never mix
     placement with intake cleanup in one executable review; broader
     publication remains a separate workflow;
  5. [complete] add
     disposable-PostgreSQL, browser, restart/recovery, tenant-isolation,
     rate-limit, and secret-redaction acceptance tests;
  6. [complete for beta.1] profile observation/planning database paths, remove superseded wrappers
     and dead code, tighten dependencies and container contents, and prove that
     no cleanup changes behavior; and
  7. [in progress] publish, install, and deploy the exact
     `v0.2.1-beta.1` artifact through CI.

### v0.2.1 beta hardening and final release

`v0.2.1-beta.1` begins deliberate daily use; it does not assert final
stability. Every reproducible defect found through normal Spotify or web use is
added to the edge-case ledger with a fake-provider/transport regression and is
released as the next `v0.2.1-beta.N`. Do not skip numbers or publish a beta for
documentation-only churn.

Every beta and final release uses one immutable source identity across all
daily-driver surfaces. The crates.io artifact is installed locally as the
`chordrift` CLI, and API plus worker containers are rebuilt from the same tag,
deployed to Vortex, and restarted. Release acceptance records the CLI version,
container image digest, commit, service health, and a cross-client smoke. Web
and CLI presentation may differ, but both must serialize the same typed DTOs
and observe identical Rust-owned authorization, state-transition, retry,
verification, and error semantics.

The final `v0.2.1` release requires Suhail's explicit stability approval plus:

- the web interface and installed remote CLI both complete ordinary daily
  maintenance against the hosted authority;
- provider reconnect, disconnect, multiple-account isolation, restart recovery,
  stale-operation rebase, and exact provider authorization are proven;
- `docs/HOW_TO_CHORDRIFT.md` becomes the user-facing web guide, while a concise
  CLI/operator handbook documents supported recovery and diagnostics without
  exposing internal-ID ceremony as normal product UX;
- all temporary deployment helpers, obsolete scripts, unused code and stale
  documentation are audited; performance and dependency checks pass; and
- CI, package, container, deployment, backup/restore and installed-artifact
  verification all pass from the exact final commit.

### Post-deployment private-beta recovery audit

This is a personal, one-time or occasional operator tool and is not a blocker
for `v0.2.1-beta.1` or final `v0.2.1`. After the hosted daily driver is stable,
add a read-only account report for tracks known from listening history but
absent from every current playlist and saved
surface. Rank candidates by lifetime plays, recent plays, last heard time, and
prior placement evidence. Review destinations explicitly, then use ordinary
maintenance to restore selected tracks. An active exclusion must be shown as
`previously_excluded` and explicitly restored; `known_from_history` means no
active exclusion and may be assigned directly. This audit must not infer a
Spotify write merely from play count.

The first read-only audit workbook was generated on 2026-08-31. Its strict
history-only population excludes current provider playlists, Liked Songs,
saved albums, active exclusions, and current Chordrift-model placements. It
contains 14,138 identities: 296 canonical matches suitable for explicit
destination review and 13,842 unmatched archive identities requiring identity
resolution first. The workbook provides filterable album, meaningful-play,
last-heard, retained-playlist-history, and safe-next-step fields. No Neon or
Spotify state changed. Product integration remains: restore to a still-existing
prior destination or an explicitly selected destination through exact review;
permanent forget remains a separately confirmed durable-intent operation.

### Separate dependency — learned Classification Authority

The shared Classification Authority is a different product/project, with its
own repository, roadmap, release versions, Storexa-backed Neon knowledge store,
model artifacts, evaluation gates, and developer Classification Lab. It is not
a Chordrift database module or a v0.2.1 implementation slice. Its complete
architectural brief is preserved in
`docs/design/CLASSIFICATION_KNOWLEDGE_FOUNDATION.md` for the new project task.
The complete learning inventory and evidence-promotion rules are preserved in
`docs/design/LEARNING_SIGNAL_TAXONOMY.md`: private listener behavior and
lifecycle, shared lawful classification evidence, and Chordrift placement and
recipe policy remain three separate planes.

Chordrift will eventually consume its versioned classification contract like
any other external dependency. Chordrift owns private provider/account state,
the narrow query adapter, exact private response caches, placement policy,
Spins, approvals, and provider mutation. The dependency owns shared reviewed
knowledge and inference. A Like is only a private trigger: Chordrift may send
recording identity and permitted catalog metadata, never the Like, account
identity, playlists, play counts, listening history, or private behavior.

The dependency is a generalizing classifier, not an exhaustive song catalog.
It stores representative reviewed examples, taxonomy, lawful facts and
provenance, artist/catalog priors, disagreements, release manifests, and
evaluations. For an unseen recording it returns ranked multidimensional claims,
calibrated confidence, alternatives, evidence, and unknown/conflict states.
Weak evidence causes abstention or review rather than a forced answer.

### Web-first client and public-launch gate

The intended consumer product is a responsive web application over the hosted
Rust authority. The CLI remains the first contract client, daily-driver proving
ground, recovery tool, and automated edge-case harness; it is not the primary
consumer interface. iOS and Android are the intended later mobile clients;
macOS and Windows may also follow as optional thin clients. None is a
prerequisite for the web launch.

The private daily-driver web client is part of V021-06 and v0.2.1. Do not turn
that private beta into an unrestricted public product immediately after the
hosted-authority release. First establish the separate Classification Authority
project and its stable versioned consumer contract, perform a focused Chordrift
refactor that keeps classification, placement, Spin eligibility, and provider
execution separate, and exhaust the cumulative-provider edge-case ledger with
fake, isolated, and daily-driver testing. Then number public-web,
billing/entitlement, and launch slices from measured service behavior.
The durable strategy and launch gate live in
`docs/design/WEB_PRODUCT_AND_LAUNCH_STRATEGY.md`.

The commercial model is multi-tier and starts with a genuinely useful free
plan. Paid tiers may fund higher automation frequency, heavier Spin/classifier
compute, longer recovery/history windows, and additional provider/account
capacity. Exact limits and prices remain undecided until hosted cost and usage
data exist. Safety, tenant isolation, credential protection, understandable
approval, data export, and account deletion are product guarantees rather than
paid features.

### v0.3.0 through v1.0.0

Later releases have stable outcomes but intentionally receive exact numbered
slices at the preceding release boundary, after real use informs their shape:

- **v0.3.0 — Agentic audit and visual recipe authoring:** collection-policy
  editor, simple presets and advanced composition, deterministic visual
  previews, generated name/artwork proposals, and exact provider diffs.
- **v0.4.0 — Learned correction policies:** explicit correction evidence,
  proposed reusable rules, confidence/conflict review, opt-in automatic routing,
  and immediate overrides.
- **v0.5.0 — Rolling listening experiences:** schedules, stable playlist
  targets, atomic refresh, freshness and duplication budgets, narrative ordering,
  notifications, comparison, and recovery.
- **v0.6.0 — Multi-provider orchestration:** Apple Music as the second proven
  adapter, cross-provider identity evidence, per-provider capability degradation,
  and one account with several isolated provider connections.
- **v0.7.0 — Additional native reach:** Linux or another client only when demand
  justifies it, using the unchanged client contract.
- **v0.8.0 — Privacy and portability:** complete account export, deletion,
  retention controls, credential revocation, and restore-on-new-device flows.
- **v0.9.0 — Product hardening:** installation, updates, accessibility,
  performance/load budgets, observability, support diagnostics, failure
  injection, and end-to-end recovery rehearsals.
- **Classification dependency integration:** consume a compatible release from
  the separately developed Classification Authority after its contract and
  evaluation boundary are proven. Chordrift retains only its private query,
  cache, policy, and placement responsibilities. See
  `docs/design/CLASSIFICATION_KNOWLEDGE_FOUNDATION.md`.
- **v1.0.0 — Consumer-ready release:** a secure, installable, recoverable product
  whose supported clients pass the same contract and safety suite.

**Next gate:** complete V021-06 production/worker composition and provider
connection lifecycle, then finish the shared web/CLI daily-driver journey and
publish `v0.2.1-beta.1`. Iterate only real fixes as `beta.N` until Suhail
approves final `v0.2.1`. After final release, prepare a clean Classification
Authority handoff package: updated founding brief, signal taxonomy, explicit
Chordrift boundary, new-project bootstrap checklist, and a ready-to-paste task
prompt. Project naming, Neon creation, namespace reservation, and its roadmap
belong to that new task—not this repository.

## Portable core and thin clients

Chordrift is one portable Rust product with several thin clients. The CLI is
the first client; a responsive web app is the intended consumer client. Native
clients—including later iOS and Android apps—remain intentionally deferred
until the hosted-authority work, separate Classification Authority contract,
web experience, and
post-hosting client-boundary refactor are proven. Client code owns presentation
and platform integration only. Accounts, provider inventory,
evidence, collections, recipes, Spins, publication safety, persistence,
background work, and diagnostics remain Rust-owned.

The Rust-owned portion is itself split deliberately. A task-oriented
application/workflow layer interprets gestures, exposes decisions, coordinates
durable operations, and binds review/authorization. Beneath it, the
provider-neutral domain core and typed infrastructure ports enforce durable
intent, playlist/exclusion invariants, persistence, provider effects, receipts,
and verification. Client skins do not sequence core calls, and provider or
database adapters do not decide the user workflow. This three-layer boundary is
the portability contract for CLI, web, iOS, Android, and any later client.

The shippable authority is a hosted Rust service. The web app, optional native
clients, and the CLI consume one versioned command/query/event contract and
never connect directly to Neon or hold provider refresh credentials. A
development CLI may use an
in-process transport for the same application service, but it must not gain a
separate business path. Protocol negotiation reports API/schema compatibility,
provider capabilities, evidence capabilities, and feature availability.

This direction borrows Photara's portable-core, typed-bridge, native-client,
and cross-cutting-contract layers. Chordrift does not need Photara's node
packages, proxy graph, or general runtime registry. The detailed boundary and
zoomable overview live in
[`docs/design/PLAYLIST_PRODUCT_ARCHITECTURE.md`](docs/design/PLAYLIST_PRODUCT_ARCHITECTURE.md).

## Discovery and orchestration model

The proposed playlist-product foundation is documented in
[`docs/design/PLAYLIST_PRODUCT_ARCHITECTURE.md`](docs/design/PLAYLIST_PRODUCT_ARCHITECTURE.md),
with a zoomable overview of the first-run journey, database boundaries, and
matching Rust domain types. It treats collections as an overlapping dynamic
library map and Spins as ordered, reproducible projections from versioned
recipes. This design is not yet an applied schema migration.

The guiding product principle is **a clean listening surface backed by lossless
musical memory**. Every active playlist should have an intentional purpose;
every retained track should have inspectable provenance explaining whether it
was a personal favorite, an old playlist member, a provider discovery, a friend
recommendation, or part of an external followed playlist. Cleanup must preserve
the best available history before removing clutter and explicitly identify any
provider data that could not be recovered.

The resulting canonical playlists should combine current high-rotation music,
forgotten favorites, new discoveries, and explicit recommendations according
to an inspectable composition policy. Their approved names and artwork should
make the final provider surface inviting enough to replace defaulting to radio,
while native radio and discovery continue supplying new intake.

Chordrift will become the canonical orchestrator of playlists, while song
discovery remains native to each streaming platform. New provider playlists can
act as discovery inboxes: Chordrift ingests their tracks, identifies songs
already represented in the canonical library, and incorporates only the new
material into its analysis. Repeated runs should leave those inbox surfaces
clean and ready for further discovery without losing any tracks.

Existing provider playlists and their names remain intact until Chordrift has
clustered every track into an inspectable proposed structure and the user has
approved the LLM-suggested names and organization. Retiring an old or inbox
playlist must be an explicit, auditable synchronization operation performed
only after all of its tracks are accounted for in approved canonical playlists.

Beyond canonical organization, Chordrift will generate renewable listening
playlists from versioned recipes. Canonical collections answer “what is this and
where does it belong?”; intake surfaces answer “how did it arrive or what needs
review?”; generated playlists answer “what would be rewarding to hear now?” A
track may participate in several generated playlists without duplicating or
weakening its canonical identity.

Recipes combine eligible source sets with composition weights, hard
constraints, ordering policies, and a repetition budget. Initial dimensions
include recent discovery, recent rotation, long-term favorites, forgotten
favorites, explicit recommendations, canonical diversity, artist spacing,
duration, energy flow, cross-output reuse, and user-defined sections. Curated
presets and advanced controls compile to the same versioned recipe model. Each
generation records its inputs, evidence capabilities, recipe version,
constraints, random seed, selection reasons, and final order.

Provider adapters expose capabilities rather than forcing provider-specific
logic into recipes. A feature may be available immediately, improve as normal
syncs accumulate observations, require an optional archive, or remain
unsupported. The UI must say which case applies. Spotify saved timestamps
support new-discovery recipes; Recently Played provides bounded short-term
evidence; the optional extended-history export enables trustworthy lifetime
rotation, skip/completion behavior, and deep rediscovery.

User intent has explicit strength. Hard boundaries cannot be crossed by a
normal recipe; strong preferences affect eligibility; soft facts affect rank;
one-time choices affect only one generation. Approved corrections are
account-scoped learning evidence. Chordrift may propose broader rules from
repeated corrections, but must explain and obtain approval before activation.

For every account except an explicitly selected migration, the default is
**retire none**. User-created playlists are protected, remain provider-owned,
and retain their custom names and exact ordering in Neon. Users may opt named
playlists into retirement, select all with explicit exclusions, or reset to
none; those policy commands never mutate a provider and do not bypass coverage,
review, readiness, or destructive-apply gates.

Stable user-managed intake names are `Inbox` for direct personal discoveries,
`From Friends` for explicit recommendations, `Liked from Radio` for
radio/autoplay discoveries, and `From Prompts` for tracks intentionally carried
forward from Spotify prompt-generated playlists. Spotify manages `On Repeat`,
`Daily Mix`, and the source prompted playlists. Chordrift-managed outputs receive approved generated vibe
names and must not be edited as intake surfaces. The temporary Atmos workaround
uses `Chordrift Spatial Audio`.

Suhail's explicitly approved one-time cleanup targets a final Spotify surface
containing only those four user-managed
intakes, Spotify-managed sources, Chordrift-managed canonical playlists, and
the temporary `Chordrift Spatial Audio` companion. All other user-created
legacy vibe and utility playlists are retirement candidates once their
semantic evidence has been consumed and every track has a published, verified
canonical destination. This explicitly includes `Melodi(es)` and
`Ambient Music Therapy – Indian Lounge - Relaxing Music for your Six Senses`.
Retirement removes the old playlist container, not its tracks from the library.
Spotify Liked Songs is a provider library surface rather than a playlist and is
not part of legacy-playlist retirement. It is also the primary low-friction
intake action: Like means “keep and classify.” The safe account default keeps
Liked Songs intact. An explicit account policy may instead consume each saved
track only after its canonical playlist placement or durable exclusion is
published and verified; Neon retains the original saved timestamp and history.

## v0.0.0 — Namespace reservation

Reserve the crate and repository names with a minimal, dependency-free package.

## v0.0.1 — Project skeleton and Storexa

Add configuration, CLI boundaries, Storexa-backed Neon access, migrations, and
the canonical schema. Provide version and database-status commands.

Status: complete.

The post-v0.1.1 listening path combines one lifetime extended-history baseline
with cursor-based Recently Played ingestion during every normal pull. Annual
cumulative exports supersede overlapping provisional observations and repair
any gaps without duplicating events.

## v0.0.2 — Spotify read-only inventory

Authenticate with Spotify and snapshot playlists, ordered memberships, saved
tracks, and provider metadata without remote mutations.

Status: complete.

## v0.0.3 — Canonical model and playlist analysis

Normalize tracks into provider-independent identities and report playlist
overlap, duplicate memberships, and library statistics. Add an incremental
Spotify-to-Neon pull that reconciles removals, keeps account identity dynamic,
and refreshes derived state. Persist account-scoped observed, discovery-inbox,
and managed playlist roles plus explicit drift authority, without remote
provider mutations.

Status: complete.

## v0.0.4 — Optional listening-history enrichment

Import Spotify history exports independently, preserve matched and unmatched
events, and derive play-count, duration, recency, and skip statistics. The rest
of Chordrift remains usable before or without this data.

Add a basic read-only query surface for accounts and playlists, ordered songs
within a playlist, canonical analysis, listening-history summaries, and
per-track listening statistics. These commands should make it possible to
verify that Neon is clean and current without writing SQL. Preserve duplicate
entries and expose stable provider IDs whenever a mutable name is ambiguous.

Archive ingestion uses a Git-ignored per-account inbox and collision-safe local
archive. Imports are content-addressed and event-deduplicated so periodic
overlapping exports can be added safely. Raw IP addresses and unrelated account
PII are excluded from the canonical database. Neon remains authoritative; the
unchanged local ZIP archive is a disaster-recovery and future reprocessing
source that can rebuild enrichment without requesting another Spotify export.

Status: complete.

## Deferred provider track — Apple Music

Apple Music is not on the critical path to the canonical library. Its provider
foundation is isolated on the `codex/apple-music` branch until paid Apple
Developer Program access is independently worthwhile and the integration can
be tested with real credentials. When resumed, add MusicKit authorization,
ISRC-first catalog matching with scored metadata fallbacks, unresolved-match
reporting, and optional privacy-export history ingestion. Keep all operations
read-only until the normal synchronization approval milestone.

Spotify and Neon remain sufficient to develop and validate embeddings,
clustering, naming, playlist proposals, and dry-run synchronization. Apple work
will be rebased onto the then-current main line rather than reserving or
blocking a release number.

Neon remains the durable identity, provenance, history, and orchestration
ledger; Spotify is the only active live provider and intake surface until
native Apple support resumes. Bootstrap the existing Apple library once from
SongShift JSON rather than creating temporary Spotify playlists. Retain the
original exports in the Git-ignored content-addressed archive, automatically
link only unambiguous identities, and stage uncertain metadata matches for
review. After consolidation, SongShift can mirror multiple Chordrift-managed
Spotify playlists to Apple Music using the same approved canonical names. No
aggregate "two way sync" or transfer-relay playlist is required.

The normal Neon library surface is a live projection of the latest successful
provider snapshot: only current Spotify playlists and their current names are
active. Older names and removed playlists remain solely in immutable sync/audit
history. Proposed Chordrift playlists remain separate until approved and
published.

Provider membership and playlist ownership are separate facts. A playlist
owned by another person or organization is an **External Playlist Bookmark**
when it is followed, added to the library, or externally collaborative. It is
not part of the active canonical library, contributes no clustering or
behavioral signal by default, and is never a legacy-retirement source. Before
account cleanup, Chordrift should retain its provider, stable ID, owner, public
link, relationship, last-known metadata, and an immutable content snapshot when
the provider permits access. If contents are inaccessible, record that fact
rather than pretending the bookmark is complete.

Account cleanup may then propose removing the external playlist from the
user's provider library without deleting or modifying the original owner's
playlist. This is always an explicitly approved operation. Neon retains the
bookmark and last-known contents afterward so the user can inspect or revisit
it even though the provider account surface stays clean. The same
provider-neutral distinction applies to Spotify and Apple Music.

When both native providers are active, each platform is authoritative evidence
for user actions on that platform. A user removal creates a provider-scoped
tombstone/override; it does not erase the canonical track, history, or
provenance from Neon. Reconciliation policy decides whether an intentional
removal propagates to other providers and must prevent deletion/re-addition
loops.

Chordrift also maintains a durable, provider-neutral **Excluded Tracks** view.
Removing a track from a published and subsequently verified Chordrift-managed
playlist records a reversible account-level exclusion with its provider,
timestamp, and previous canonical assignment. It must not hard-delete the track
or listening history, and it prevents the track from silently reappearing in a
future generated playlist. Removal from provider-curated, intake, transport, or
legacy playlists is ordinary source drift and must not create a global
exclusion. Restore is always explicit and audited.

Until native Apple matching resumes, Spatial Audio curation uses an explicit
manual workaround:

1. Chordrift or the user creates a dedicated Spotify playlist of candidate
   tracks.
2. The user scans its public Spotify URL with
   [Hello Atmos](https://helloatmos.app/spotify/) to identify Apple Music Dolby
   Atmos matches.
3. The filtered set is exported to a specially named Apple Music Spatial Audio
   playlist directly, or copied into a temporary filtered Spotify playlist and
   mirrored with SongShift.

This third-party result is a convenience, not authoritative Chordrift provider
state. It must not silently populate verified Apple identifiers or Spatial
Audio flags in Neon. A future native adapter will match exact recordings,
retain storefront and evidence provenance, and cache Apple's extended
`audioVariants` value `dolby-atmos` so repeated queries are unnecessary.

## v0.0.5 — Embeddings

Build versioned hybrid representations. Use a pretrained music-audio foundation
model (initially MERT, with MuQ evaluated as an alternative) as the reusable
acoustic base whenever Chordrift has lawful access to locally owned, DRM-free
audio. Add semantic context from explicitly semantic legacy playlists, artist
and album relationships, and historical names. Add independently sourced
recording/release language, release country, and artist-area evidence with
source, confidence, and retrieval provenance; never infer origin from Spotify
availability markets. Prefer MusicBrainz for this enrichment and keep unknown
values unknown rather than guessing from titles. Store behavioral preference and
lifecycle signals—plays, recency, completion, skips, provider-curated rotation,
inbox status, and recommendation provenance—separately so unrelated favorites
do not become acoustically similar merely because both are frequently played.
Spotify-only tracks must retain a deterministic personal/metadata fallback
rather than downloading or scraping provider audio.

For the completed v0.0.5 scope, Chordrift does not train a music foundation
model. It performs inference with pretrained models or imports independently
sourced semantic tags for an identified recording, then caches the result with
model/source, version, confidence, and retrieval time. The later shared
classification direction may evaluate trained adapters or eventually a
foundation model only after lawful-data, license, consent, privacy, and
multilingual evaluation gates are designed and accepted. Before clustering
ships, review the then-current Spotify Platform policy and keep Spotify as the
live synchronization and user-action provider; resolve the artist/title/ISRC
identity independently for model inference and portable enrichment.

Status: complete. Provenance-aware external semantic enrichment is the first
input task for v0.0.6 before cluster generation begins.

## v0.0.6 — Vibe clustering

First enrich canonical recording identities independently from Spotify. Use a
rate-limited, cache-first MusicBrainz adapter with ISRC-first resolution,
conservative ambiguity handling, and raw-response retention. Persist genres,
folksonomy tags, release countries, and release-title language/script with
source, parser version, confidence, and entity provenance. Resolve credited
artists to separately versioned primary-associated-area evidence, retaining
unknown outcomes and never treating the area as birthplace, nationality, or
track language. Add pretrained mood/sound inference without confusing
release-title language with vocal language.

Then create reproducible cluster generations with stable identities,
representatives, statistics, and support for unassigned tracks. Cluster inputs
must identify the exact enrichment and embedding generations they consumed.
After proposed playlists have stable identities, add account-scoped assignment
feedback: reject a track's current vibe, prefer or lock another destination,
and make the next generation honor that decision as an auditable stability
constraint. Preserve the original model score and prior assignment rather than
rewriting history. This is post-generation correction, not a prerequisite for
the initial classification.

The same feedback surface handles initially unassigned tracks: create a stable
manual semantic destination, assign or move a track using its provider ID, or
leave it in an internal needs-review queue. Active decisions replay into later
proposals; changing a decision supersedes rather than erases its audit record.
Needs Review is never published as a provider playlist and does not satisfy
retirement coverage.

## v0.0.7 — Naming and proposed library

Generate names, descriptions, and semantic tags, then expose a complete,
inspectable, non-destructive proposed playlist structure. Require user approval
of generated names and organization, and prove that every track from each
legacy or discovery playlist is represented before proposing its retirement.

Playlist identity must be independent of both temporary cluster labels and
generated display names, and should carry forward through later generations by
membership lineage. Naming uses a strict model-neutral export/import artifact
with provider/model/version/hash provenance. Approval is generation-specific
and is blocked by stale or incomplete naming, unassigned retirement-source
tracks, or incomplete legacy/intake coverage. This milestone never writes to
Spotify.

The preservation universe is account-scoped and durable: latest saved tracks
plus tracks ever retained through semantic-legacy, transport, intake, or
Chordrift-managed playlist membership. Listening history influences ranking
and classification but is not library membership by itself. A proposal is
publishable only when every preserved track has exactly one acceptable
disposition: canonical placement or an explicit reversible exclusion. This
invariant must hold independently for every future connected account.

## v0.0.8 — Full dry-run synchronization

Plan idempotent Spotify diffs without mutating the service. Include discovery
inbox ingestion, cross-playlist duplicate removal, and explicit retirement
plans for legacy and consumed inbox playlists, with track-preservation checks
and no implicit deletions. Show proposed additions to and restorations from
Excluded Tracks separately from provider drift. Provider-neutral plan
structures must allow Apple Music diffs and Spatial Audio variants to be added
later without changing the canonical model.

## v0.0.9 — Spotify apply readiness

Validate approval records, operation ordering, interruption recovery,
rate-limit handling, and convergence checks against Spotify fixtures and
read-only probes. Continue to prohibit remote mutations while proving that an
approved plan can be executed safely and audited completely.

Apply readiness is recorded as an immutable assessment of one exact dry-run.
It validates snapshot freshness, proposal and artwork approval, operation
integrity and destructive gates, the external-cleanup approval, five simulated
resume checkpoints, bounded 429 retry behavior, zero-change replay, and one
explicit read-only Spotify identity/scope probe. A missing probe or stale gate
produces an inspectable blocked assessment; neither state enables writes.

Inventory owned, provider-curated, collaborative-external, and followed-
external playlists as distinct relationships. Add read-only bookmark list and
content inspection, preserve accessible external contents before cleanup, and
show provider-library removal as a separate explicitly approved plan category.
External bookmarks must never be mistaken for owned legacy playlists or
canonical inputs.

The bookmark foundation persists stable records and immutable pull-bound
observations in Neon. Normal pulls detect snapshot-signature changes for
relationships still visible to Spotify; public followed contents remain
metadata-only under Development Mode, while readable collaborative contents
are copied forward without redundant requests. Explicit on-demand refresh now
targets exactly one present or archived bookmark, stores complete and denied
attempts separately from provider-library snapshots, preserves the last
readable contents, and does not increase normal-sync requests. Under Spotify's
current Development Mode, item refresh succeeds only for owned/collaborative
playlists; ordinary followed public playlists remain metadata bookmarks.

External cleanup review is represented by an immutable candidate batch. The
user approves its exact ID after inspecting all owners, playlist IDs,
preservation states, and Spotify signatures. Only a still-current approved
batch may add relationship-only `remove_external_playlist` operations to the
dry-run; changed membership or signatures require new review. v0.0.9 continues
to prohibit execution of those operations.

Generate one simple original cover-art artifact for every approved canonical
playlist before publication. Artwork should be derived from the approved name,
description, and semantic tags; use a restrained shared visual system while
remaining distinct per playlist. Store generator/prompt or template version,
dimensions, media type, and content SHA-256, provide a local contact sheet or
equivalent preview, and require explicit approval. Identical inputs must reuse
identical artifacts. Do not use provider artwork, copyrighted source images, or
personal photos, and do not request Spotify image-upload scope or upload a
cover during this read-only milestone.

The first implementation uses the original **Drift Atlas v1** visual system:
14 local 1254×1254 PNGs, one per stable canonical playlist key, plus a contact
sheet and strict manifest. Import validates complete proposal coverage, names,
dimensions, media types, and SHA-256 values before Neon records an immutable
review batch; approval remains a separate local-only command.

## v0.1.0 — Canonical music library

Synchronize approved canonical playlists from Neon to Spotify, retain
provenance and operation history, and converge to zero changes on repeated
runs. Upload only explicitly approved cover artifacts after the corresponding
playlist exists, with the same interruption and convergence protections as
track operations. Remove legacy or consumed discovery playlists only as
separately approved operations after their replacement playlists are published
and verified. Remove followed/shared external playlists from the user's
provider library only after their bookmark snapshots are retained and the
separate cleanup operations are approved; never mutate the source owner's
playlist. Apple Music publishing and Spatial Audio companions remain a
subsequent provider
milestone unless the deferred provider track is completed earlier.

Status: complete. The v6 planner and v2 readiness probe gate durable phase
executions, per-operation retry history, provider target resolution, exact
retirement approval, post-pull convergence proof, and batched Spotify requests.
The first live migration published 14 canonical playlists with approved covers
and 884 ordered memberships, preserved three intake surfaces, archived external
bookmarks before cleanup, and retired every approved legacy and utility
container. The final imported Spotify surface contains 19 purposeful playlists
with zero duplicate entries and no pending destructive operations.

## v0.1.1 — Explainability and complete visual surfaces

Add one fast track-inspection command that answers whether a song is already in
the current Spotify surface, its approved canonical destination, retained
source-playlist history, listening/lifecycle signals, embedding generation,
cluster score, independent mood/sound facts, and any manual assignment or
exclusion rationale. Title lookup supports artist disambiguation and stable
Spotify IDs.

Extend Drift Atlas artwork to every Chordrift-owned intake, retain pristine
label-free masters for future provider-specific typography, and render exact
labels locally with an approved platform font rather than generated text. Add
`From Prompts` as a fourth intake carrying prompted-discovery provenance. Cover
planning is convergent: an identical content hash already uploaded to the same
stable Spotify playlist is not requested again. A focused
`chordrift artwork update --playlist NAME` command builds an immutable
one-cover plan for a newer approved artifact without admitting membership or
cleanup operations. Spotify folders remain manual presentation state because
the Web API exposes neither folder structure nor folder artwork.

Status: complete and released as v0.1.1.

The complete-library repair is implemented and proven in Neon for the personal
account: 1,715 distinct preserved tracks, 1,715 canonical placements, zero
active exclusions, zero unresolved tracks, and zero conflicting dispositions.
Embedding v4 adds normalized meaningful listening-session co-occurrence so
personal listening context helps singleton tracks without turning every played
track into library membership. Stable extension, centroid, group-consensus,
and reversible manual-assignment paths preserve the 14 approved playlist
identities; evidence did not warrant another canonical playlist in this repair.
The repaired proposal and its 1,715-track Spotify publication are verified.
Drift Atlas v3's 18 larger, lower-anchored covers are also approved, uploaded,
and verified without changing playlist membership.

## v0.1.2 — Listening review and preference learning

- [x] Add a revisioned private user-classification sidecar (`collection`,
  multi-valued `region`, `tradition`, `cohort`, and `language`, plus non-vector
  `notes`). Cohorts express personal composition intent without claiming that
  their tracks sound alike.
- [x] Support immediate one-track corrections and inert CSV export → draft
  import → exact-ID approval for larger regional review passes.
- [x] Feed active explicit facts into a separate higher-weight personalized
  feature namespace without altering pretrained acoustic vectors or public
  metadata.
- [x] Use the approved South Asian classification batch to retire Monsoon
  Cinema into `Dakshina Pulse`, `Uttara Glow`, and the personal `Rasa Archive`,
  while returning globally classified score and pop material to existing
  sound-based destinations. The complete proposal has zero unresolved or
  conflicting tracks; distinct artwork remains the final user-review gate.
- [ ] Add display preferences under
  `$XDG_CONFIG_HOME/chordrift/config.toml` (table width/layout, color policy,
  compact versus detailed inspection, and date formatting). Keep automatic
  terminal-width detection and sensible interactive defaults when unset.
  Deferred to v0.2.0 so it does not block the personal v0.1.2 reconciliation.
- [x] Publish a user-dimension glossary with literal CSV/Excel templates and a
  future account-scoped drag-and-drop token interaction model.
- [ ] Before testing a friend's account, run the two-account isolation and
  provider-boundary audit in
  `docs/design/ACCOUNT_AND_PROVIDER_BOUNDARIES.md`; fix Spotify-specific domain
  leakage before treating the CLI as a reusable product foundation. Deferred
  to v0.2.0 before any second-account or product claim.

Turn ongoing listening corrections into durable, explainable account knowledge
rather than ad hoc Spotify edits. A correction distinguishes four separate
intentions: reject the current destination, prefer or lock an existing
destination, hold the track for review, or exclude the track from active
Chordrift playlists. Every decision retains the prior assignment, model score,
reason, actor, timestamp, and affected provider state; no correction erases the
track, its provenance, or listening history.

Inventory Spotify saved albums as a distinct immutable provider surface before
personal cleanup. Preserve ordered album tracks and account-scoped policy in
Neon, but do not force album-only tracks into normal playlist readiness. Album
cleanup is opt-in and review-gated: each track must already be preserved in
Liked Songs/current playlists or explicitly excluded before the album may be
proposed for unsaving. Product default is preserve; Suhail's personal target is
review-then-unsave and ultimately a playlist-only active library.

Ongoing listening must not require a dedicated review session. Split the
workflow into **capture now** and **reconcile later**. While listening, one
low-friction action records intent and playback continues; naming, cohort
review, artwork, and publication can happen asynchronously. Add three stable
action intents:

- **Refile** — keep the track, but reject its current destination;
- **Review** — keep the track and defer both rejection and destination;
- **Exclude** — propose removing the track from active Chordrift playlists,
  still subject to explicit confirmation and reversible history.

Use one provider-native `Re-evaluate` holding queue rather than parallel
destination routes. While listening, the user adds a misplaced track to
Re-evaluate and removes it from the wrong destination. Chordrift preserves the
entry and rejected source, gives the queue zero preference weight, suppresses
both exclusion and source restoration while it is present, and removes it only
after a newer explicit assignment targets a different approved destination.
Long queues export through the normal classification CSV workflow.

Legacy multi-route surfaces remain immutable history. Exact-confirmed
`reevaluate retire-legacy` requires the replacement queue plus complete
proposal-or-exclusion coverage, changes only Neon, and lets the next reviewed
plan archive the obsolete Spotify containers after publication.

A later review session starts from the currently approved library without
rebuilding or renaming unaffected playlists. The CLI and future UI should
support:

1. Identify a track by provider ID or unambiguous title and artist.
2. Reject its current playlist and record why it feels wrong to the listener.
3. Rank existing destinations with both model evidence and the user's prior
   corrections visible.
4. Move or lock the track to an existing destination, or place it in an
   internal review cohort.
5. Promote a coherent review cohort into a newly named and illustrated
   Chordrift playlist only when it genuinely warrants a new identity.
6. Preview the exact membership/order delta, approve it, publish through the
   existing immutable-plan/readiness gates, pull, and prove convergence.

Track-specific corrections are hard account constraints in later generations:
“not Tidal Hush” must prevent the track from drifting back there. Cultural,
regional, linguistic, soundtrack, instrumentation, and mood facts remain
separate semantic facets. Artist identity alone must not force placement—for
example, an A. R. Rahman recording may be South Asian cinematic, Western
orchestral, ambient, pop, or a mixture. One correction stays local; after
multiple consistent corrections Chordrift may propose a broader account rule
or destination concept, but it must show the evidence and obtain approval
before applying that rule to other tracks.

Treat edits made directly in Spotify as authoritative observations of the
exact user action. Every complete pull becomes the newest baseline for
user-authority state; Neon preserves history and interpretation but does not
compete with that current state. Compare each pull with the last verified
baseline and fold exact gestures cumulatively:

- removal from one managed playlist plus addition to another is a proposed
  move;
- removal alone asks whether the user meant wrong vibe, temporary review, or
  exclusion;
- addition to a managed playlist is current placement intent, while any
  permanent lock or classification generalization remains separate;
- reordering is current order intent when exact membership is unchanged; it
  does not silently become a reusable cadence rule.

No provider edit is silently reversed. Ambiguous broader meaning may remain
unresolved without disputing the observed action. Once inferred or confirmed,
that meaning is written to Neon and becomes part of future orchestration. This
lets a consumer use Spotify as the familiar editing surface while Chordrift
acts as a preservation-first assistant on top; the dedicated Chordrift review
surface remains the faster, more explainable path for batch corrections.

Maintain the provider-change interpretation matrix in
`docs/design/PLATFORM_INTENT_MODEL.md`. Each personal CLI edge case should
graduate into an explicit product rule: the observed mutation, plausible
intents, confidence boundary, reversible automatic action, and point at which
the user must confirm. Task-oriented personal workflows live under
`docs/how-to/`; the large command catalog is reference material rather than the
primary user journey.

For immediate provider-native capture, adding the playing track to a routing
queue must be enough; the user need not also remove it from the current
canonical playlist. A normal pull records the queue event in Neon. Publication
later adds the verified destination before removing the rejected membership,
so interruption cannot lose the track. A lightweight command or future mobile
shortcut may offer the equivalent `mark current` action, but it must use the
same durable queue model and avoid requiring unsafe interaction while driving
or cycling.

For the personal CLI workflow, add concise `review` commands that clone the
approved generation into a draft, show captured routes and suggestions, record
single or batch corrections, display an exact diff, and approve the draft.
Reuse the existing stable playlist concepts, assignment-revision ledger,
complete-inventory invariant, artwork approval, sync planning, readiness, and
apply machinery rather than creating a second source of truth.

Keep provider and database traffic proportional to observed change. Neon is a
durable cache and change ledger, not a target for wholesale rewrites: unchanged
playlist bodies copy forward by provider snapshot ID; unchanged saved-library
state copies forward after a bounded probe; a changed saved-library snapshot
resolves known provider records in one lookup, rewrites metadata only when its
payload changed, batches snapshot membership, and updates observation times as
one set. Long phases expose progress. Interactive list commands use compact,
colored, width-aware tables; redirected output remains stable plain text for
scripts. Future saved-library pagination should safely reuse an aligned stored
tail when a prefix diff proves that the remaining ordered membership is
unchanged, reducing Spotify requests without weakening complete-inventory
proofs.

Status: complete. The approved South Asian reconciliation, canonical artwork,
Re-evaluate replacement queue, legacy retirement, consumed Inbox cleanup, and
opt-in Liked Songs cleanup have been published and provider-verified. The final
v0.1.2 snapshot converges to a zero-operation plan. Configurable terminal
presentation and the two-account/provider-boundary audit remain intentionally
deferred to v0.2.0.

Saved-album inventory, opt-in Liked Songs consumption, exclusion-aware
readiness/execution/verification, batched changed-surface persistence, and live
zero-plan convergence are implemented. Exact archive-only album-container
retirement retains immutable album and ordered-track history without forcing
album tracks into playlists; review-then-unsave remains the stricter
alternative. The default policies for both albums and Liked Songs remain
preserve.

## v0.2.0 — Portable core and CLI-first Spins

The first acceptance target is CLI-first rather than UI-first. Treat the
existing personal Spotify inventory and enrichment history as newly supplied
account evidence, while ignoring prior Chordrift intent inside an isolated
onboarding session. Produce a read-only library audit, capability report,
starter collection proposal, and deterministic provider-free Spin previews.
Run the same flow with inventory only and with extended history. Neither path
may write to Spotify; accepted publication still uses plan, approval, apply,
and verification.

Before that rehearsal, route the CLI through a versioned Rust application
facade; establish command, query, progress/event, cancellation, idempotency,
and compatibility contracts; and prove account/provider isolation with a fake
adapter. Then add only the ownership, collection, playlist-surface, recipe,
Spin, onboarding-session, and publication-link schema required by the typed
domain. Introduce the hosted service transport after the same core workflow is
proven locally; native UI follows the stable contract rather than driving its
shape.

Before recipe or UI implementation, complete the database-v2 foundation in
`docs/design/DATABASE_ARCHITECTURE_V2.md`. The v0.1.2 database is logically
healthy but stores raw listening metadata per event and complete provider
membership per routine snapshot. Preserve its verified backup, rehearse a full
restore, separate current provider state from durable intent, normalized
evidence, and rebuildable caches, then migrate through measured invariants.
Database cleanup, schema restructuring, migration/cutover, and code refactoring
are sequential gates; native UI implementation must target the stable v2
bridge rather than legacy tables.

Safe cleanup foundation status: complete. The backup checksum and PostgreSQL
18 restore were verified; logical invariants, physical storage, and protected
versus redundant snapshot classes are repeatable through read-only `db`
reports; and compaction planning cannot mutate a database or contact a
provider. The next sequential gate is implementing and rehearsing the v2 schema
and migrations. Production cutover and deletion remain separately approval-
gated.

Database-v2 schema status: additive foundation complete. Migration 0040 adds
content-addressed provider revisions, one current inventory per account,
compact checkpoint structures, historical provider identities, normalized
listening evidence, and provider-neutral cutover diagnostics. Current-state
backfill and repeated-import revision reuse are proven on PostgreSQL 18, and a
full restored-copy rehearsal preserved the v1 invariant report byte-for-byte.
The next gate migrates normalized evidence and durable snapshot references on a
rehearsal copy, verifies parity, and only then requests separate production
cutover authority.

Database-v2 migration rehearsal status: complete. Migrations 0041 and 0042 add
exact-confirmed normalized-evidence/checkpoint migration plus local dual-write
compatibility. A fresh PostgreSQL 18 clone migrated all 149,314 events and 463
durable audit references with exact invariant parity; 41 referenced snapshots
deduplicated into 24 checkpoints. Independent verification, idempotent replay,
PostgreSQL integration tests, and `pg_amcheck` pass. The read-only cutover plan
is now available, but production apply/read cutover and every legacy deletion
remain separate approval gates. After an approved production observation
window, refactor recipes and provider queries onto v2 before beginning the
native review UI.

Database-v2 production preflight status: complete and read-only. Migration 0043
makes v2 hashes stable across production and rehearsal collations. Production
is healthy on PostgreSQL 18.6 with 39/43 migrations; its invariant report is
byte-identical to the pristine restore, all 17 non-empty playlist hashes match,
and a prospective current-state hash matches the fresh 43-migration rehearsal.
No production write occurred. The next separately approved gate is additive
migrations 0040-0043 only, followed immediately by read-only reports and a stop
to present the actual production data-plan hash. Normalized-evidence apply,
read cutover, observation, and cleanup remain later approval gates.

Database-v2 additive production schema gate status: complete. The explicitly
approved migrations 0040-0043 reached 43/43 on Neon in 3.964 seconds with zero
failures. Post-migration read-only reports preserved the complete legacy
invariant, proved exact current provider parity, and emitted applicable data
plan hash
`a850fb15603f82c934daa127cfb768084938bc8ac601b6f30643ebc3a84e2ae8`.
Normalized evidence and checkpoints remain empty; no data apply, read cutover,
deletion, connection change, or Spotify operation occurred. Exact-confirmed
production data migration is the next separately approved gate.

Database-v2 production data migration status: storage-blocked with a clean
logical rollback. The exact-confirmed plan began under explicit approval but
PostgreSQL returned SQLSTATE `53100`. No v2 evidence, identity, checkpoint, or
receipt row is visible; the complete legacy invariant and plan hash remain
unchanged. Aborted tuples increased physical storage to 514,457,600 database
bytes, including 98,500,608 bytes in the empty normalized-event relation and
4,128,768 bytes in the empty identity relation. Do not retry, vacuum, compact,
or cut over until additional Neon headroom or a separately approved bounded
maintenance strategy is available.

Database-v2 no-cost candidate status: complete and verified. A new isolated
free Neon PostgreSQL 18 project in the same region restored the trusted dump at
249,331,712 bytes, reached 43/43 migrations, and successfully applied exact
data plan
`a850fb15603f82c934daa127cfb768084938bc8ac601b6f30643ebc3a84e2ae8`.
All current-state, event, duration, timestamp, identity, archive, and durable
checkpoint invariants pass; `ready_for_cutover` is true. The dual-storage
candidate is 358,686,720 bytes, below the free 0.5 GB allowance. Its role
password was rotated immediately after a candidate-only URI appeared in a
failed checker diagnostic; no production credential was exposed. Managed Neon
cannot install the superuser-only `amcheck` extension, while the equivalent
local rehearsal already passed it. Production remains configured and logically
unchanged. Connection cutover, observation, cleanup, and old-project deletion
remain separate approval gates.

Database-v2 project connection cutover status: complete and verified. The
private Apogee `CHORDRIFT_DATABASE_URL` value now targets the verified candidate
and retains owner-only file permissions. A fresh Apogee-loaded process proved
43/43 migrations, exact invariant and normalized-evidence parity, 24 resolved
checkpoints, `ready_for_cutover: true`, unchanged cutover hash, and 358,686,720
database bytes. At that gate the former project remained intact as rollback
protection; individual application-query refactoring, observation, cleanup,
rollback, and old-project deletion were still separately controlled gates.

Database-v2 v0.1.3 runtime status: implemented and locally verified. Migration
0044 adds stable v2 runtime read surfaces, transient provider-import surfaces,
and exact cleanup receipts. Ordinary application reads no longer depend on
duplicated snapshot bodies; Spotify archives and recent observations write
directly to normalized listening evidence; provider pulls materialize reusable
content revisions and clear import staging before commit. A second fresh
PostgreSQL 18 restore migrated all 149,314 events, applied rehearsal-only cleanup
plan `0688bf0984ea6f6b26cf65ca7ab1c9fcb762601c6a512b204e7a79312830f964`,
and preserved invariant fingerprint
`24f5da45845bb48b3cfeb49cbd09fe371043c7f9544ea38993d3016beaf0d6a3`.
The clean database retains 58 lightweight observations, 22 current playlists,
1,790 ordered memberships, both archive manifests, 24 checkpoints, and every
durable audit reference while shrinking locally to 167,974,591 bytes. Both a
provider-inventory round trip and a normalized archive-import round trip pass
against the post-clean schema. Migration 0044 is installed on the live
`chordrift` project at 44/44. Production cleanup was applied with exact plan
`0688bf0984ea6f6b26cf65ca7ab1c9fcb762601c6a512b204e7a79312830f964`,
preserving the rehearsal invariant hash. Immediate verification and all major
runtime reads passed; legacy tables are absent, transient import staging is
empty, and the database now occupies 167,788,544 bytes.

The verified live Neon project keeps stable ID `damp-hall-40280714` and has
the final display name `chordrift`; renaming changed neither its connection nor
database contents. Cleanup, verification, persistent-connection validation, and
the bounded observation gate passed. Former project `mute-recipe-86719846`,
named `chordrift-legacy-rollback`, was then deleted under separate exact-ID
approval. The live project remained healthy and the preserved dump checksum was
reverified afterward.

## v0.1.4 — Cheap incremental provider synchronization

Make the normal Spotify-to-Neon interaction proportional to observed change on
the database-v2 schema. Fetch independent saved-track, saved-album, and recent-
play surfaces concurrently after playlist discovery. Count Spotify API requests
and show phase timings so latency regressions are visible to the operator.

For unchanged playlists, batch current metadata, account links, policy refresh,
transient headers, and revision-membership reuse into set-based Neon statements
instead of issuing the same sequence once per playlist. When every provider
surface is unchanged, retain the current content-addressed revision pointers
without copying 1,790 playlist memberships through transient staging. Batch historical-
identity and normalized recent-event writes. Maintain listening statistics only
for identities receiving new events, relink against the compact per-track cache,
and summarize routine syncs from that cache. When the active playlist/saved-
track library is unchanged, advance the analysis checkpoint without deleting
and rebuilding the complete derived statistics table. Keep explicit history
refresh and summary commands event-level and authoritative.

Status: complete. Local unit, strict Clippy, schema migration, clean provider
round-trip, and incremental-history tests pass on PostgreSQL 18. Real personal
pulls reduced the unchanged workflow from 70.3 seconds to 4.6 seconds after the
publication-check fix; a later reconciliation workflow completed its follow-up
pull in 5.4 seconds and verified the apply receipt. Production is healthy at
45/45 migrations. No cleanup is part of this milestone.

Interactive presentation is centralized for every command: existing stable
key/value and tabular reports remain unchanged when redirected, while terminal
sessions receive compact titled tables, flattened JSON evidence, consistent
colors, and one workflow progress bar that safely hosts provider/database
events. Migration 0045 replaces the final stored SQL function that still named
cleanup-removed database-v1 tables; its post-clean relation-rename regression
passes locally. Production application was explicitly approved and completed
in 722 ms; Chordrift is healthy at 45/45, the repaired candidate path passes a
live read-only track inspection, and stale readiness emits an actionable
plan-refresh message. No cleanup or Spotify write accompanied the migration.

Build a provider-neutral review UI around the same audited model rather than
moving policy out of the Rust core. It should answer “why is this here?” for
every playlist and track, distinguish canonical collections, intake, generated
experiences, bookmarks, and immutable history, preview organization and
artwork, approve cleanup in bounded batches, explain unknown provenance, and
capture corrections when a track belongs elsewhere.

Implement the provider-neutral recipe domain before building elaborate UI:

- versioned recipe definitions and immutable generation records;
- canonical, intake, and generated-surface roles as distinct concepts;
- provider capability and evidence-availability reporting;
- eligibility, weights, constraints, repetition budgets, and ordering policies;
- per-track inclusion and ordering explanations;
- deterministic preview with no provider writes;
- an initial `New Discoveries + Rediscovery` recipe that can use Like/save time,
  recent observations, and optional extended history;
- a thin web client that can inspect a proposal, adjust a small set of
  meaningful controls, open a track in its provider, and approve through the
  existing immutable execution gates.

The consumer web client presents provider artwork, canonical title and artist, current
destination, listening evidence, and a provider deep link. Double-click opens
the installed provider client; playback and catalog ownership remain with the
provider. The Rust core remains authoritative for identity, recipes,
classification, proposals, history, commands, and diagnostics. The authenticated
web transport exposes typed query/command DTOs; provider adapters own OAuth, inventory,
publication, capability reporting, and deep-link construction.

This milestone also owns configurable terminal presentation, the complete
two-account isolation audit, provider-neutral identifiers, and a first-class
correction review surface based on direct managed-playlist moves. Do not claim
reusable multi-account product support until that audit passes.

The native app establishes the production operating model. It runs scheduled
work through a quiet background helper or system service without opening
Terminal windows or visible helper shells. It surfaces progress, completion,
actionable failures, cancellation, and recovery in the app or normal OS
notifications. OAuth opens the system browser only for initial or renewed
consent and returns to the app cleanly. Release credentials and tokens live in
1Password and the OS credential store; Apogee may expose approved development
configuration, but the shipped product must not require Apogee. Never place
secrets in config files, logs, shell history, source control, or launch scripts.

Evaluate a dedicated classic Hindi cinema destination after v0.1.2. For now,
misplaced older Hindi songs move directly to an existing managed destination
and retain their source and classification history. A later CSV review should distinguish era, language,
cinema tradition, and listening intent before proposing a poetic Sanskrit-
inspired identity and approved artwork; do not create the playlist merely from
artist identity or a few edge cases.

Regional reconciliation operates over the complete approved library, not only
the playlist where mistakes were noticed. Treat explicit user decisions as
stronger than embedding similarity, require positive evidence before assigning
a tradition, and return globally classified tracks to sound-based destinations
rather than a generic international bucket.

## v0.3.0 — Agentic audit and visual recipe authoring

Turn first-run setup into a read-only audit and editable recommendation plan.
Explain overlap, duplicates, uncertain placement, legacy containers, collection
candidates, available evidence, missing capabilities, and starter recipes.
Present simple recipe philosophies first and reveal detailed controls on demand.
The agent may inspect and propose freely; it obtains separate bounded approval
for publication and destructive cleanup and shows the exact provider diff
before either.

Add visual collection-policy controls, hard versus soft boundaries, generated
playlist schedules, and reproducible previews. Keep the configuration format
authoritative and editable outside the UI.

## v0.4.0 — Learned correction policies

Promote confirmed direct reclassification evidence into a complete review
experience. Learn only from explicit approved corrections, distinguish a
one-track exception from a reusable account rule, quantify confidence, and send
ambiguous or conflicting tracks to review. Allow high-confidence automatic
routing only under an explicit user policy with inspectable history and an
immediate override path. Do not recreate a provider holding queue.

## v0.5.0 — Rolling listening experiences

Add scheduled daily and periodic generation, stable provider playlist identity,
atomic replacement, historical generation comparison, freshness windows,
cross-playlist duplication budgets, and ordering strategies such as energy
arcs, smooth transitions, intentional contrast, and user-defined sections.
Recipes degrade honestly when a provider or account lacks required data.

## Toward v1.0.0 — Shippable product

Use the remaining 0.x releases for additional providers, multi-account proof,
recovery and migration, performance, accessibility, signed installation,
background scheduling, privacy controls, polished onboarding, documentation,
and end-to-end product testing. v1.0.0 means a fully working, installable,
recoverable application—not merely a stable internal schema or architectural
preview.
