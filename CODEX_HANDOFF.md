# Codex handoff

Read this file at the start of a new Chordrift task. It records durable product
decisions, current operational state, and the safest next action without
requiring the previous conversation. Update it whenever a task changes those
facts. Never add credentials, tokens, database URLs, private keys, or personal
archive contents.

Last updated: 2026-08-31.

## Hosted Spotify connection checkpoint

The active private deployment now runs exact code commit
`386334c67c0ca32d59c573389277f6bee81056f7` as image
`chordrift:386334c` (image
`sha256:3fb5a938fc4103b21fe01416177da9abd96ed88e4d7b2be790431a1440a7aaa9`)
on Vortex. Both API and worker run as UID/GID 65532 with
read-only root filesystems and `unless-stopped`; the 43.2 MB runtime contains
only `chordrift-server`, `chordrift-worker`, and runtime necessities. Before
deployment, a validated 29.4 MB compressed logical backup was written to the
Vortex operator backup directory, then additive migration 0051 completed the
canonical database at 51/51. HTTPS liveness/readiness and security headers
pass through Nexus/Cloudflare. The same authenticated Chordrift browser
session survived an API/worker restart and retained the existing account,
Spotify connection identity, playlists, track views, and 455 active reversible
exclusions. Lightleak Reverie now compares as 501 provider and 501 Chordrift
memberships with order-only drift. The hosted executor can perform only
server-rederived, exact-review maintenance effects; this surface is not a
general publication API.

The live Disconnect → Reconnect lifecycle now succeeds and the encrypted
credential is active. The next daily-driver incident showed that observation
captured a direct managed-playlist addition while the hosted session displayed
zero changes: the shared intake audit classified it correctly, but the hosted
PostgreSQL interpreter appended only Liked Songs intake. Commit `ce6f0e1`
projects direct managed additions into the same Rust maintenance DTO used by
web and remote CLI. One observed destination is pre-resolved as canonical
placement without a provider write; multiple simultaneous destinations remain
one decision; a Like remains a separate saved-state choice. The deployed image
is healthy privately and through HTTPS. Its readiness response still reports
the old read-only capability until the current exact-review repair is deployed.

The next authenticated review exposed a thin-client DTO defect: **Record these
decisions** appeared inert for one destination choice plus one Liked-state
choice. The destination dropdown had submitted the model playlist's stable key
where `MaintenanceSurfaceView` requires a Rust-issued opaque resource ID; the
HTTP rejection was also not rendered. The deployed repair includes the complete
typed `maintenance_surface` in every playlist query, returns that DTO unchanged
from the browser, keeps the server-issued Liked Songs source, and renders a
retryable error for any rejected submission. A Node browser-DTO harness covers
single and composite choice shapes and runs on every CI push. CI, exact-image
deployment, API/worker health, public readiness, and repaired-asset checks pass;
authenticated acceptance then reached execution.

That execution exposed a beta-blocking partial-apply defect. For the Liked-only
track **It Must Have Been Love**, the chosen Neon Affection placement was
recorded as Chordrift intent, but the hosted review contained only the saved-
track removal. Spotify accepted the Unlike; no destination addition occurred;
verification failed; the track is currently absent from both Spotify surfaces.
Do not perform an automatic recovery write. The branch repair emits the exact
destination addition first, observes and verifies it, then offers saved-state
cleanup only as a separate exact review. It also interprets the already-pending
ordinary `publish/add_track` as the one recovery addition the user may review
after deployment. Failed operations no longer claim Spotify was unchanged.
The complete GitHub gate passed for repair commit `727bc8f` and the current
top-placement tip `cc597b0`. Exact image `chordrift:cc597b0` (manifest
`sha256:82e030b42ab56a1cd5bfce4355dee6e9f563fef004971eafc855066ca0cc5289`)
is deployed on Vortex as both API and worker. Private and public readiness are
healthy and report `provider_write_scope=exact_review_only`. Deployment did not
start maintenance or write Spotify. Authenticated recovery acceptance remains:
hard-refresh, Check provider changes, require one exact Neon Affection addition,
then let Suhail separately authorize it.

New Liked-only placements now use a Rust-owned `Top` policy and the exact review
names that position. The Spotify adapter maps it to zero-based position 0; the
stateful fake provider proves both single and composite additions land at the
top without duplication. This does not move an already-present rediscovered
favorite. A later additive contract may expose top, bottom, or an exact
position uniformly to web, CLI, iOS, and Android clients.

The library/exclusion explorer now receives album, meaningful-play count, and
last-heard time in its typed Rust query results. The browser can sort playlist
membership by custom order, plays, recency, album, or title and sort/group the
Excluded archive by plays, recency, album, prior playlist, or last-heard bucket.
This does not authorize restoration or forgetting. A future explicit restore
command must reuse a surviving prior surface only after review or request a new
destination; permanent forget is a separately confirmed destructive intent
change. Provider artwork remains deferred until post-beta UI polish.

Contract 1.5 distinguishes canonical identity from placement evidence. Newly
liked tracks receive a preselected destination only when an active accepted
assignment or the latest prior approved/published generation yields exactly one
destination that still exists. The recommendation includes its reason, is not
consent, and ambiguous or absent evidence stays blank. Commit `386334c` passed
the complete GitHub gate and is deployed as both API and worker. Private and
public liveness/readiness are healthy; deployment did not start maintenance or
write Spotify.

A read-only personal history-only audit was generated outside the repository at
`/Users/suhail/Documents/ChatGPT/Music/outputs/chordrift-history-audit-20260831/Chordrift-History-Only-Listening-Audit-2026-08-31.xlsx`.
It excludes current provider library membership, active exclusions, and current
Chordrift-model placement. The snapshot contains 14,138 identities: 296
canonical matches and 13,842 unmatched archive identities. No database or
provider state was changed.

`tests/provider_behavior_acceptance.rs` is the permanent six-track synthetic
provider harness. It uses the production maintenance DTO/state machine, a
stateful fake provider, and a fake durable database. CI covers single add,
remove, move, reorder, and Like gestures; composite snapshots; delayed
observation; failures before both placement and saved-state cleanup; worker
restart; idempotent replay; exact add-before-unlike order; and no-loss/no-
duplicate invariants. Production interpretation unit regressions cover direct,
ambiguous, direct-plus-Like, duplicate move halves, and pending ordinary
recovery additions. Add isolated and composite failure cases here whenever
daily use finds another edge.

The first hosted Reconnect attempt was rejected by Spotify with
`redirect_uri: Not matching configuration`. Chordrift correctly derives the
server callback as
`https://chordrift.suhail.ink/providers/spotify/callback`; the Spotify
developer application must allowlist that exact string in addition to any
retained local CLI loopback callback. No authorization code, provider
credential, Neon intent change, or Spotify library mutation resulted from the
rejected attempt.

The remote Chordrift CLI no longer uses a custom Chordrift approval form or a
browser-to-localhost callback. Beta.8 moves product login to Auth0's standard
OAuth 2.0 Device Authorization Flow with a separate public Native application.
The CLI opens Auth0's verification URI, follows Auth0's polling cadence, and
exchanges the verified issuer credential once for the existing account-scoped,
revocable Chordrift product session. Spotify authorization remains a separate
server-owned provider grant.

The subsequent first disconnect/revocation rehearsal found two beta blockers.
The web form POST was rejected with HTTP 403 because the route's exact-Origin
guard did not recognize the thin wrapper request after proxying. Separately,
removing Chordrift from Spotify's Apps page invalidated the grant without a
provider webhook; the next observation failed but left the local encrypted
envelope active and the UI stale. The branch repair submits disconnect through
a session-authenticated non-simple same-origin fetch, immediately revokes an
envelope when Spotify terminally rejects its refresh credential, returns the
typed `authentication_required` failure, and reloads connection state so the
web and remote CLI both offer Reconnect. The UI now says **Authorized** and
shows the separate last-verification time because out-of-band revocation cannot
be known before the next provider call. Neon history and Spotify state remain
unchanged by either failure. Exact image `chordrift:7d869ea` containing the
repair is deployed and public liveness/readiness plus repaired-asset checks
pass. One authenticated browser Disconnect or post-revocation provider check
remains to accept the corrected live transition.

That browser acceptance then uncovered a second reconnect defect: Spotify
displayed consent and returned successfully, but the callback could not store
the credential because the vault computed generation from only an active row.
With generation 1 retained but revoked, it attempted generation 1 again and
failed the immutable uniqueness constraint. The branch repair locks the stable
provider account, advances from the greatest active-or-revoked generation, and
activates the new encrypted envelope without deleting audit history. In-memory
and PostgreSQL lifecycle regressions cover Disconnect → Reconnect explicitly.
The exact disposable PostgreSQL 18 proof passed generations 1 → 2 → revoked →
3, one active envelope, stable provider identity, and retained history; its
database container and temporary build/cache volumes were deleted immediately.
Exact image `chordrift:951ce63` is deployed with a matching OCI revision label;
container health and public readiness pass. The corrected authenticated
Reconnect callback is the remaining live acceptance action.

The V021-06 beta branch keeps Chordrift product login and Spotify provider
authorization separate. Server-owned Spotify PKCE routes implement Connect,
Reconnect, Add Account, and Disconnect. Stable Spotify identity selects the
existing account-owned `provider_accounts` row and rotates its encrypted vault
credential without losing Neon history; a different identity creates a new
isolated connection; a mismatched pinned reconnect or cross-tenant identity
fails closed. Disconnect revokes only the encrypted credential and retains all
observations and intent. The web client only launches these routes, renders
status, and selects a connection. A disposable PostgreSQL 18 proof on Vortex
passed same-row history retention, encrypted credential rotation, cross-tenant
rejection, and history-preserving disconnect; its database container, source
copy, and root-owned build output were removed. The implementation is deployed;
browser lifecycle acceptance remains gated on the explicit reconnect above.

Contract v1.4 adds one Rust-owned, set-based provider/model comparison query.
It distinguishes provider-only occurrences, Chordrift-only occurrences,
unresolved identities, order-only drift, unlinked provider surfaces, and
unpublished Chordrift surfaces. The web Library view and typed remote
`service library compare` command consume the same DTO. Unit tests cover
duplicates, unresolved counts, and reorder-only state; HTTP/local transport
parity is covered, and the disposable PostgreSQL intake fixture exercises the
real SQL path. Deployment and authenticated-browser acceptance passed.

Every daily-driver failure and its permanent regression belongs in
`docs/design/DAILY_DRIVER_EDGE_CASE_LEDGER.md`. Treat chaotic cumulative
Spotify use as the normal product environment, not operator error.

## Product-experience rule for every task

People use Spotify normally; they do not operate Chordrift's internal ledger.
Chordrift may automatically observe provider changes and record supported,
reversible intent in Neon. A removal or move already performed by the user in
Spotify does not need duplicate approval merely to be remembered. Correlate a
safe remove/add pair into placement intent and a surface-specific negative
directive; leave ambiguous changes as reviewable suggestions.

This observation-first rule includes provider order. A user reorder with exact
unchanged membership is accepted into Neon as current order and must never
produce a compensating Spotify reorder. Broader meaning—permanent cadence,
classification, or learning evidence—is a separate inference. Chordrift may
write a different order only for an explicitly requested and authorized
Chordrift-authored operation such as Spin publication; that operation has a
separate plan origin and authorization boundary.

Every complete, internally consistent pull becomes the newest baseline for
user-authority state. Previously recorded gestures accumulate across stopped
runs. Any plan or readiness assessment tied to an older snapshot is stale and
must be rebuilt. Record-only proposal revisions may converge automatically to a
bounded fixed point; an unfinished Chordrift-authored operation retains its
explicit intent but requires a newly reviewed plan before provider mutation.

Batch IDs, plan IDs, readiness assessments, idempotency keys, receipts, and
verification remain mandatory Rust-owned safety evidence, but clients should
hide that machinery during ordinary use. Explicit consent is needed when
Chordrift itself will mutate a provider—create, rename, reorder, add, remove,
delete, or change artwork—and destructive onboarding reorganization needs an
exact recoverable preview. One understandable reviewed action may span several
internally gated phases; ask once for that action instead of asking the person
to authorize every internal plan or assessment separately.

Onboarding must support rebuild-and-tidy, preserve-and-enhance, and custom
organization paths. Mixed-authority “collaborative” surfaces protect user-added
and pinned tracks while Spins compute the remaining seats. Ordinary Spin
publication is enumerated: it cannot replace unrelated live membership or
restore an actively excluded track. Compatibility preserves these outcomes;
it never forces v0.1.4 command ceremony into the v0.2 product architecture.

The immediate Chordrift v0.2.1 goal is the hosted Rust authority. Expose the
existing command/query/event application contract through authenticated
transport, then add product identity/authorization, a server-side encrypted
provider credential vault, durable background operations, remote CLI parity,
and a recoverable observable service release. Do not redefine domain behavior
to fit a transport and do not start a native client during this release.

Learned shared classification is now explicitly a separate product/project and
future Chordrift dependency. It owns its repository, roadmap, Storexa-backed
Neon knowledge store, model artifacts, evaluations, releases, and developer
Classification Lab. Chordrift owns private provider/account state, a narrow
query adapter, exact private report caches, account placement/Spin policy,
approval, and provider execution. Its preserved project brief is
`docs/design/CLASSIFICATION_KNOWLEDGE_FOUNDATION.md`.

The dependency is a generalizing classifier, not an exhaustive song-by-song
catalog. It stores representative reviewed examples, taxonomy, lawful facts and
provenance, artist/catalog priors, disagreements, model releases, and
evaluations. It must classify unseen recordings with ranked multidimensional
claims, calibrated confidence, alternatives, evidence, and unknown/conflict
states; weak evidence causes abstention or review. A Like is only Chordrift's
private trigger. A future query may contain recording identity and permitted
catalog metadata, never the Like, listener/account identity, playlists, play
counts, listening history, or private behavior. Do not create that project or
its Neon store from a Chordrift task.

The intended consumer client is a responsive web application, not a macOS-first
product. A private daily-driver web client is now required for
`v0.2.1-beta.1` alongside the CLI; unrestricted public launch still follows
v0.2.1 final, the separate Classification Authority contract, a focused
Chordrift refactor, and exhaustive cumulative-provider testing. The CLI remains
the contract proving ground and recovery tool. Native clients are later work:
iOS and Android are the intended mobile clients, while macOS and Windows may
also follow.

The product will use multiple commercial tiers beginning with a genuinely
useful free plan. Do not invent prices or limits before hosted cost and usage
measurements. Paid entitlements may scale automation, compute, history, and
provider/account capacity, but safety, privacy, tenant isolation, export, and
account deletion never become paid-only guarantees. Public deployment follows
a private web beta and an explicit edge-case/auth/vault/jobs/backup/rate-limit
launch gate.
The durable sequence is `docs/design/WEB_PRODUCT_AND_LAUNCH_STRATEGY.md`.

Web flexibility must not come from porting `chordrift-maintain.sh` to browser
JavaScript. V021-01 now exposes the Rust-owned task-level maintenance workflow
through in-process and authenticated HTTP DTO transports with identical
outcomes and provider-call traces. The acceptance contract is
`docs/design/WEB_SERVICE_CONTRACT.md`; never add a generic “run CLI command”
endpoint.

The wrapper boundary is now stricter than “shared behavior”: every web, CLI,
iOS, and Android client must remain a lightweight adapter that authenticates,
submits typed commands, reads immutable task views/events, captures decisions,
and renders them. Provider observation, cumulative interpretation, ambiguity
rules, authorization preconditions, durable evidence, execution, and
verification belong to the Rust authority. No wrapper may assemble plans,
choose allowed actions, or reproduce the maintenance state machine.

The published v0.2.1 alpha plus the verified 47/47 account database is the
current daily-driver pair; neither hosted authority nor the separate classifier
is required for normal CLI maintenance. Alpha.1 passed CI run `33275624573`.
Daily testing then exposed a direct-move defect: new membership was treated as
drift removal, then a later exclusion plan inherited the earlier confirmation.
The affected six tracks were restored to Uttara Glow, verified present in
Spotify and Neon, and are not excluded. Alpha.2 recognizes that drift shape
before apply, scopes confirmation to the displayed plan phase only, and uses
human track/playlist names in the wizard. Commit `115e41d` passed CI run
`33277340929`; `v0.2.1-alpha.2` is published on crates.io and GitHub and the
exact registry artifact is installed at `~/.cargo/bin/chordrift`. Its version,
capability handshake, and read-only live maintenance review passed. These
alphas are installable checkpoints during the v0.2.1 hosted-authority sequence,
not a replacement for it.

Alpha.3 removes the whole-library maintenance bottleneck: commit `85d2795`
passed CI run `33279291297`, and `v0.2.1-alpha.3` is published on crates.io and
as a GitHub prerelease. The exact registry artifact is installed at
`~/.cargo/bin/chordrift`; it reports `0.2.1-alpha.3`, satisfies
`maintenance.unified-workflow.v1`, and the login environment has
`CHORDRIFT_BIN` unset. The standard wizard therefore resolves the installed
alpha.3 binary instead of the retired temporary v0.1.4 build.

Alpha.4 is the reviewed Indian-library expansion. A current pull captured
snapshot `20972dcb-40fc-4522-9288-47ffa7733b35` with 21 playlists and proved
`reevaluate_surfaces: 0` and `reevaluate_tracks: 0`; A021-02 is complete and
Neon correction history remains. Approved proposal
`322ed115-4eaa-4b22-bacb-0b634f7cc175` adds Raga Meridian, Kaveri Resonance,
Celluloid Mehfil, Cinema Monsoon, and Unscripted Rasa. Indian Film Classics is
through 1979; modern Indian film is 1980 onward. Only four reviewed classic
Hindi tracks move from Rasa Archive to Celluloid Mehfil; the other four new
manual destinations intentionally begin empty.

Approved artwork batch `2a425045-1c7b-40fe-bed0-6ee505923fda` contains 25
artifacts under `artwork/canonical/drift-atlas-v5-indian-surfaces/`. Raga
Meridian reuses the retired Re-evaluate background. Celluloid Mehfil uses the
approved monochrome master; its color study is preserved under `studies/`.
Planner v11 publishes explicitly approved empty manual categories while still
suppressing empty generated clusters. The pre-release exact plan is
`b602e1c0-960d-47a3-b91d-3a5f3e76f0d1`: five creates, five artwork uploads,
four additions, four matching removals, and no exclusions or retirements.

Alpha.4 is complete. Commit `a6beae7f1dbf7e2a94684e4e3dfbcbf3c0cfe92e`
passed CI run `33280954942`; `v0.2.1-alpha.4` is published on crates.io and as
a GitHub prerelease, and the exact registry artifact is installed at
`~/.cargo/bin/chordrift`. It reports `0.2.1-alpha.4`, passes the capability
handshake, and `CHORDRIFT_BIN` is unset. Publication apply run
`70b0cdd6-8ac4-4d77-b3b0-802bd6bd5dbc` completed 14/14 operations. After a
fresh pull, reconcile apply run `1a88b40f-b3f9-4025-a681-094df364a4b9`
completed the exact four reviewed Rasa Archive removals. Spotify's playlist
index exposed the returned removal snapshot after two initially unchanged
observations; snapshot `b8bb2a83-eeac-4399-83ed-89fa9514ecff` then verified the
receipt. Final plan `009ab341-a81f-47c7-8c88-611856fd0faf` contains zero
operations. A021-06 is complete.

Alpha.5 is the post-observation latency repair. The alpha.4 shell started a
fresh Neon-backed `tracks inspect` process for every candidate during move
inference and then repeated the same inspection for display. A review-only run
of the reported 22-removal plan took 108.58 seconds after the Spotify pull was
skipped. Main now appends title, artists, `ordinary`/`direct_move`/
`ambiguous_move`, old destination, and destination to `sync plan-show
--details` through one set-based query while preserving the original first
eight TSV columns. `scripts/chordrift-maintain.sh` requires
`maintenance.bulk-plan-preview.v1`, prints `Analyzing observed changes…`, and
makes zero `tracks inspect` calls. The same stored 22-operation plan rendered in
3.06 seconds; a complete current plan/audit review finished in 8.54 seconds.
Both rehearsals were read-only.

A021-07 is complete. Commit `27b89889cb0f492becce1ab1c75ec172f444cae5`
passed CI run `33282706019`, including disposable PostgreSQL integration,
Spotify persistence round-trip, strict Clippy, all targets, docs, and package
verification. `v0.2.1-alpha.5` is published on crates.io and as a GitHub
prerelease. The exact registry artifact is installed at
`~/.cargo/bin/chordrift`; it reports `0.2.1-alpha.5`, advertises
`maintenance.bulk-plan-preview.v1`, and runs with `CHORDRIFT_BIN` unset. Its
installed-binary review-only check printed analysis progress immediately and
completed in 9.57 seconds with the ordinary library already in sync.

Alpha.6 makes Spotify's destination-native `Add` action a supported intake
gesture. The planner suppresses provider-drift removal when a current managed
membership is absent from the approved model everywhere and is not actively
excluded. The intake audit emits `direct_managed_addition`; the ordinary wizard
batches an unambiguous existing destination into the editable proposal and
rebuilds an empty plan without a Spotify apply. Multiple destinations require
a decision, and an active exclusion cannot be restored automatically. The
binary advertises `maintenance.direct-managed-intake.v1`, and planner plus
fake-binary regressions prove the provider membership cannot be deleted.

A021-08 is complete. Commit `000fb77f388996685119351d7749c210affb5c8c`
passed CI run `33283841228`, including strict Clippy, every target, docs,
disposable PostgreSQL 18 intake/migration tests, Spotify persistence round-trip,
and package verification. `v0.2.1-alpha.6` is published on crates.io and as a
GitHub prerelease. The exact registry artifact is installed at
`~/.cargo/bin/chordrift`; it reports `0.2.1-alpha.6`, advertises
`maintenance.direct-managed-intake.v1`, and runs with `CHORDRIFT_BIN` unset.
No production Neon or Spotify write was made while implementing or releasing
this checkpoint.

The first live alpha.6 intake recorded “Hai Apna Dil To Aawara (Happy)” for
Celluloid Mehfil, then stopped before proposal/artwork approval because the
wizard still referenced the obsolete 20-cover Drift Atlas v4 manifest while
the proposal requires 25 covers. Alpha.7 switches carry-forward to the approved
Drift Atlas v5 manifest and classifies a current managed membership in a merely
`proposed` generation as `direct_managed_addition` until approval. This makes
the already-recorded live proposal safely resumable without repeating the
Spotify Add. `maintenance.artwork-carry-forward.v1` prevents the repaired
script from running against alpha.6.

A021-09 is complete. Commit `84712eb54c0ae6a4b6b5e9ba3896c2d206cfeaeb`
passed CI run `33284531493`, including the disposable PostgreSQL 18 suite and
package verification. `v0.2.1-alpha.7` is published on crates.io and as a
GitHub prerelease. The exact registry artifact is installed at
`~/.cargo/bin/chordrift`; it reports `0.2.1-alpha.7`, advertises
`maintenance.artwork-carry-forward.v1`, and runs with `CHORDRIFT_BIN` unset.
The interrupted personal proposal has not been altered after the reported
failure; rerun the ordinary wizard once to resume it.

The next alpha.7 run successfully recorded two more Celluloid Mehfil direct
intakes and carried artwork, then stopped because the seven-track provider and
proposal memberships were equal but ordered differently. Alpha.8 adds the
Neon-only `proposals align-provider-order` operation with exact unique
membership equality. The wizard clones the approved proposal, accepts the
observed order, approves/carries artwork, and rebuilds an empty provider plan;
it never executes the planned Spotify reorder. The binary capability is
`maintenance.provider-order-intent.v1`.

The Classification Authority brief now records Spotify's client recommender as
potential affinity evidence only. The exact “based on this playlist” panel has
no public playlist-recommendation endpoint, new/development apps lost general
Recommendations access in 2024, and current Spotify API terms forbid using
Spotify Platform content to train ML/AI. An explicit user Add remains private
placement evidence; Chordrift must not scrape unselected recommendations.

## V021-05 completion checkpoint (historical)

V021-05 is implemented as `v0.2.1-alpha.17`. `RemoteHttpClient` and
`LocalDevelopmentClient` consume one compatibility/command/query trait. The
remote client requires HTTPS outside loopback, zeroizes its bearer, negotiates
before every CLI DTO submission, and maps only structured `ClientError` data.
`chordrift service session save|status|remove` retains the opaque product
session in the OS credential store; command/query files are typed contract
envelopes, not shell/SQL/provider escapes. V021-06 owns hosting, external login,
service URL distribution, observability, backup/restore, and release rehearsal.

Implementation commit `bc847e1cef6f453e599ab9fe9905b6da2b9a48a4`
passed CI run `33355318790`, including formatting, strict Clippy, all targets,
documentation tests, every ignored disposable-PostgreSQL integration, Spotify
persistence round-trip, and package verification. The annotated
`v0.2.1-alpha.17` tag, GitHub prerelease, and crates.io artifact are public.
The exact locked registry artifact is installed, reports
`chordrift 0.2.1-alpha.17`, and satisfies `service.remote-cli.v1` under
application contract 1.3. `CHORDRIFT_BIN` remains unset. V021-05 used no
personal Neon access, provider read/write, or migration.

The future Classification Authority signal inventory is explicit in
`docs/design/LEARNING_SIGNAL_TAXONOMY.md`. Read it with
`CLASSIFICATION_KNOWLEDGE_FOUNDATION.md` before planning that separate project.
Do not collapse private preference/lifecycle evidence, shared classification,
and Chordrift placement policy into one model or store. Raw provider/account
behavior is private by default; shared learning requires a minimized explicit
contribution plus provenance, rights, privacy, review, and evaluation.

Daily-driver UX findings that do not invalidate the current safety contract are
batched in `docs/design/DAILY_DRIVER_EDGE_CASE_LEDGER.md`. The first queued
refinement is rediscovered-favorite context: when a Like is already represented
in a managed playlist, show the destination and canonical occurrence position,
and later consider a separately authorized move-to-top choice. Do not infer a
reorder from the Like or from the keep/clear saved-state answer.

V020-01 through V020-15, A021-01 through A021-13, and V021-01 through V021-04
are complete.
v0.2.0 is released and the separately approved personal binary/database
cutover is complete. `v0.2.1-alpha.16` is the installed daily-driver
checkpoint. It retains alpha.13's two daily-use
defects: assignment replay no longer turns revision chronology into provider
playlist order, and every exactly converged record-only observation receives an
immutable managed verification baseline. A later provider removal is therefore
an exclusion and cannot be restored from an older proposal. The Rust core also
exposes `tracks exclusions`, exact-confirmed `tracks empty-exclusions`, and the
low-level exact-equality `sync accept-current` operation. See
`docs/design/PROVIDER_FIRST_CONVERGENCE.md` and
`docs/design/DAILY_DRIVER_EDGE_CASE_LEDGER.md`.

A previously unknown track added directly to exactly one managed Spotify playlist
is now preserved in place and recorded as canonical destination intent without
a Spotify membership write. Multiple destinations remain ambiguous; active
exclusions require explicit restoration. Known-track direct moves remain
unchanged. Never call or write Spotify without the user-authorized exact
publication, maintenance, or retirement operation.

V021-03 keeps provider OAuth refresh credentials inside the hosted Rust
authority. `ProviderCredentialVault` encrypts plaintext with XChaCha20-Poly1305
before persistence; AEAD metadata binds each immutable revision to its
Chordrift account, provider account, provider namespace, credential kind,
algorithm, revision ID, and external key ID. Keys remain in an external key
ring and never enter PostgreSQL. Plaintext values are non-debuggable,
non-serializable, zeroized leases used only by an internal provider adapter.
There is no command/query or HTTP route that returns a provider token.

Additive migration 0049 stores only encrypted envelopes, generations, key
selectors, and rotation/revocation evidence. Every operation rechecks current
V021-02 subject/membership/account/provider ownership. Active members may lease;
only the active owner may rotate or revoke. Unit and disposable-PostgreSQL
tests prove ciphertext round-trip, no plaintext persistence, key rollover,
one-active-generation rotation, tenant denial, identity-substitution and tamper
failure, and post-revocation denial. Migration 0049 was not applied to the
personal database and no personal Neon or Spotify operation was used. Local
maintenance still requires only migration 0047. See
`docs/design/PROVIDER_CREDENTIAL_VAULT_V021_03.md`.

V021-03 implementation commit `9ce536985ed575d208245177be15d1eaa7043e29`
passed CI run `33328348984`, including formatting, strict Clippy, all targets,
documentation tests, every disposable PostgreSQL integration (including
migration 0049 encryption/rotation/revocation/tenant isolation), Spotify
persistence round-trip, and clean packaging. The annotated
`v0.2.1-alpha.14` tag, GitHub prerelease, and crates.io package are public. The
exact locked registry artifact is installed at `~/.cargo/bin/chordrift`,
reports `chordrift 0.2.1-alpha.14`, satisfies
`service.provider-credential-vault.v1`, retains application contract 1.2 and
product-session schema 1, and has `CHORDRIFT_BIN` unset. No personal Spotify or
Neon operation was used for implementation, testing, release, or installation
verification.

V021-04 adds `DurableOperationQueue` and additive migration 0050. A typed
application command, its account/subject-scoped idempotency key, canonical
fingerprint, exact receipt, retry policy, and queued event commit before worker
execution. Identical replay across fresh service instances returns the original
receipt; key reuse for different intent fails closed. PostgreSQL `SKIP LOCKED`
claims issue one random expiring lease generation, explicit heartbeat supports
long work, and every worker update requires the current unexpired lease.

Lifecycle/progress events are immutable and operation-local ordered. Queued and
recoverable cancellation is immediate; running cancellation persists until a
safe worker acknowledgement. Retryable failure and abandoned leases become
recoverable within a fixed attempt budget, then terminal. Current V021-02
authorization is rechecked for acceptance, cancellation, queries/history, and
worker eligibility. Queue payloads are typed command DTOs, never CLI/shell/SQL,
provider credentials, or provider-write authority. Unit and disposable-
PostgreSQL tests prove restart replay, concurrent single claim, heartbeat,
progress, stale-worker rejection, recovery, retry exhaustion, cancellation,
tenant isolation, history, and cursor-contiguous events. Migration 0050 was not
applied to the personal database. See
`docs/design/DURABLE_BACKGROUND_OPERATIONS_V021_04.md`.

V021-04 implementation commit `b02114b67b2b91cb14eb010f0aa4452aa4feb304`
passed CI run `33329866917`, including formatting, strict Clippy, all targets,
documentation tests, every disposable PostgreSQL integration (including
restart-safe replay, concurrent claiming, lease recovery, bounded retry,
cancellation, progress, and tenant isolation), Spotify persistence round-trip,
and clean packaging. The annotated `v0.2.1-alpha.15` tag, GitHub prerelease,
and crates.io package are public. The exact locked registry artifact is
installed at `~/.cargo/bin/chordrift`, reports `chordrift 0.2.1-alpha.15`,
satisfies `service.durable-operations.v1`, retains application contract 1.2 and
product-session schema 1, and has `CHORDRIFT_BIN` unset. No personal Spotify or
Neon operation was used for implementation, testing, release, or installation
verification.

Alpha.16 adds remembered saved-intake disposition before V021-05. Liked Songs
is represented as a virtual user-authority intake surface using the existing
migration-46 `playlist_surfaces` and revisioned `playlist_track_directives`.
When a current Like is already in a verified managed destination, the ordinary
wizard names every destination and asks whether to keep both. `include` maps to
the client-safe `preserve` answer; `exclude` maps only to
`clear_after_verified_assignment` for this virtual surface and is not a global
track exclusion. Undecided and keep states cannot plan an Unlike. Clear yields
one exact reviewed `remove_saved_track` effect and leaves managed membership
unchanged. A later direct provider Unlike supersedes an older include directive
during exact baseline acceptance. The answer may also be revised with
`chordrift intake liked-disposition`; that command is Neon-only.

Application contract 1.3 adds `MaintenanceResolution::ConsumeIntake` so remote
CLI, web, and mobile clients can render the same decision without owning its
meaning. Capability `maintenance.saved-intake-disposition.v1` gates the current
shell adapter. Fake-binary coverage proves human track/artist/destination
review; disposable PostgreSQL proves undecided safety, remembered keep,
explicit clear, revision supersession, and direct-Unlike convergence. This
feature uses schema 0047 already present in the personal database; it does not
require or apply hosted migrations 0048 through 0050.

Alpha.16 implementation commit `5631b502a78490739c9dcc8ab111eee8c699813e`
passed CI run `33331740305`, including formatting, strict Clippy, all targets,
documentation tests, every disposable PostgreSQL integration, Spotify
persistence round-trip, and clean packaging. The annotated
`v0.2.1-alpha.16` tag, GitHub prerelease, and crates.io package are public. The
exact locked registry artifact is installed at `~/.cargo/bin/chordrift`,
reports `chordrift 0.2.1-alpha.16`, satisfies
`maintenance.saved-intake-disposition.v1`, exposes application contract 1.3,
retains product-session schema 1, and has `CHORDRIFT_BIN` unset. No personal
Spotify or Neon operation was used for implementation, testing, release, or
installation verification.

Historical V021-05 directive: move the installed CLI to the authenticated typed
service while preserving explicit local development parity. This is now
satisfied by alpha.17. The current gate is V021-06; never call or write Spotify
without an exact separately authorized provider operation.

Alpha.13 implementation commit `ff425146e89d177f4bc9828c7784e3322f5fe9a3`
passed CI run `33325908241`, including formatting, strict Clippy, all targets,
documentation tests, fresh/upgrade PostgreSQL integration, the exact provider-
order/baseline/removal/forget regression, Spotify persistence round-trip, and
clean packaging. The annotated `v0.2.1-alpha.13` tag, GitHub prerelease, and
crates.io package are public. The exact locked registry artifact is installed
at `~/.cargo/bin/chordrift`, reports `chordrift 0.2.1-alpha.13`, satisfies
`maintenance.provider-baseline.v1`, retains application contract 1.2, and has
`CHORDRIFT_BIN` unset. No personal Spotify or Neon operation was used for this
repair, its release, or installation verification.

V021-01 adds application contract 1.2 and capability
`service.authenticated-transport.v1`. `MaintenanceApplication` is one
asynchronous Rust command/query authority behind typed async backend ports. The
Axum adapter exposes only `POST /v1/commands` and `POST /v1/queries`; there is
no CLI/shell/SQL/provider escape hatch. A real loopback TCP suite proves
in-process/HTTP parity, identical provider traces, authenticated account
isolation, idempotent replay and collision rejection, reconnect, cancellation,
ordered event cursors, cumulative provider-order refresh, stale-review
rejection, request budgeting, contract rejection, and secret-free errors. No
public listener, product token store, Neon URL, or provider credential is
included. See `docs/design/AUTHENTICATED_SERVICE_TRANSPORT_V021_01.md`.

V021-02 keeps application contract 1.2 unchanged, adds product-session schema 1,
capability `service.product-identity.v1`, and additive migration 0048. A
pluggable external verifier returns stable issuer/subject claims; Chordrift
validates persisted ownership, returns a 256-bit random opaque session token
once, and stores only its SHA-256 digest. Every request rechecks session
expiry/revocation, subject, membership, and account status. The real HTTP and
PostgreSQL matrices cover cross-tenant denial, guessed/expired tokens, session
and membership revocation,
subject/account suspension, idempotent trusted owner provisioning, and owner-
takeover refusal. There is no public provisioning, password, subject-selection,
role-override, SQL, or CLI-command endpoint. Migration 0048 is required by the
hosted identity service; local maintenance explicitly requires only schema
through 0047 and therefore does not force a personal database migration. See
`docs/design/PRODUCT_IDENTITY_AUTHORIZATION_V021_02.md`.

V021-02 implementation commit `e6ff3cae1be0f6bd10a015b4e8a74487ea86d4b8`
passed CI run `33323175289`, including formatting, strict Clippy, all targets,
documentation tests, fresh/upgrade PostgreSQL integration, the product identity
and immediate-revocation persistence matrix, Spotify persistence round-trip,
and clean packaging. The annotated `v0.2.1-alpha.12` tag, GitHub prerelease,
and crates.io package are public. The exact locked registry artifact is
installed at `~/.cargo/bin/chordrift`, reports `chordrift 0.2.1-alpha.12`,
satisfies `service.authenticated-transport.v1` and
`service.product-identity.v1`, exposes application contract 1.2 and session
schema 1, and has `CHORDRIFT_BIN` unset. Migration 0048 was not applied to the
personal database, and no personal Spotify or Neon operation was used for this
slice or its release verification.

Alpha.11 implementation commit `7bf39280b38b68fb658885b70b60fa23cf360373`
passed CI run `33320863679`, including formatting, strict Clippy, all targets,
documentation tests, fresh/upgrade PostgreSQL integration, Spotify persistence
round-trip, and clean packaging. The annotated `v0.2.1-alpha.11` tag, GitHub
prerelease, and crates.io package are public. The exact locked registry artifact
is installed at `~/.cargo/bin/chordrift`, reports `chordrift 0.2.1-alpha.11`,
satisfies `service.authenticated-transport.v1`, exposes application contract
1.2, and has `CHORDRIFT_BIN` unset. No personal Spotify or Neon operation was
used for this slice or its release verification.

A021-12 introduces application contract 1.1, capability
`maintenance.task-session.v1`, typed maintenance start/refresh/resolve/
authorize/query DTOs, and a Rust-owned session reducer/router. The core owns
revision checks, exact review binding, allowed actions, cumulative rebase, and
secret-free workflow errors. Direct in-process and serialized JSON-loopback
tests produce identical outcomes; a provider-authored reorder is record-only,
and a newer complete provider snapshot invalidates an older review. This is the
transport-neutral foundation only: the existing shell remains the daily-driver
adapter. V021-01 now supplies authenticated HTTP plus asynchronous
database/provider ports; V021-05 moves the installed CLI onto that service.

Alpha.10 implementation commit `12f39c26d6f5fc71fb9776370dc005db5cdec11a`
passed CI run `33318686812`, including strict Clippy, all targets,
documentation tests, fresh/upgrade PostgreSQL integration, Spotify persistence
round-trip, and clean packaging. The annotated `v0.2.1-alpha.10` tag, GitHub
prerelease, and crates.io package are public. The exact locked registry artifact
is installed at `~/.cargo/bin/chordrift`, reports `chordrift 0.2.1-alpha.10`,
satisfies `maintenance.task-session.v1`, exposes application contract 1.1, and
has `CHORDRIFT_BIN` unset.

Alpha.8 daily use exposed one more sequencing defect: after direct intake for
`Tum Hi Ho Bandhu` was recorded, the newly approved proposal revealed the
membership-equal Celluloid Mehfil order delta only in the next plan. Alpha.9
rebuilds after every record-only revision and absorbs newly exposed provider
order to a bounded fixed point. The exact fake-binary regression runs intake,
then reorder, then an empty plan and proves no `sync apply` call occurs.

Alpha.9 implementation commit `9787f580e0b5b85c704f1db4444c0ae1301fa9e6`
passed CI run `33286406152`, including strict Clippy, all targets, documentation
tests, fresh/upgrade PostgreSQL integration, Spotify persistence round-trip,
and clean packaging. The annotated `v0.2.1-alpha.9` tag, GitHub prerelease, and
crates.io package are public. The exact registry artifact is installed at
`~/.cargo/bin/chordrift`, reports `chordrift 0.2.1-alpha.9`, satisfies
`maintenance.provider-order-intent.v1`, and has `CHORDRIFT_BIN` unset.

Alpha.8 implementation commit `f788a9be7761ad9e0fe1cffbdb686967c0cda67a`
passed CI run `33285324445`, including strict Clippy, all targets, documentation
tests, fresh/upgrade PostgreSQL integration, Spotify persistence round-trip,
and package verification. The annotated `v0.2.1-alpha.8` tag, GitHub
prerelease, and crates.io package are public. Its exact crates.io artifact was
installed and verified before being superseded by alpha.9.

## Released v0.2.0 foundation and current v0.2.1 alpha deployment

Release commit `a079eba0eb71955969cf29186e9d73cffae1cd82` is annotated as
`v0.2.0`. CI run `33262366722` passed formatting, strict Clippy, all targets,
documentation tests, fresh/upgrade PostgreSQL 18 integration, Spotify
persistence round-trip, and clean packaging. The GitHub release and crates.io
package are public. That v0.2.0 artifact predates the post-release maintenance
capability command and must not be used with the current wizard. Install the
current documented alpha from crates.io; the wizard deliberately trusts the
machine-readable capability handshake rather than a potentially ambiguous
version string.

The user separately approved the personal cutover after a final read-only
Spotify pull, backup, candidate refresh, and exact parity proof. The installed
v0.2.0 binary is paired with database `chordrift_cutover` in verified Neon
project `royal-snow-31539822`; it is healthy at 47/47 migrations. Apogee selects
`~/.config/apogee/secrets.env` through `~/.config/apogee/config.toml`. An
already-running shell can retain the old exported URL, so open a fresh terminal
or unset `CHORDRIFT_DATABASE_URL` before `eval "$(apogee)"` when verifying a
connection change. This is current operator plumbing only. Never make Apogee a
Chordrift dependency, product contract, installer requirement, or GUI setting;
hosted/native clients authenticate to the Rust authority and retain only a
revocable Chordrift session in the OS credential store.

### Retiring the historical Re-evaluate correction surface

One daily-driver correction was used to expose excessive compatibility-wizard
ceremony. Spotify accepted and Chordrift has
verified three enumerated additions: `Dirty Old Town` to `Snowglass Letters`,
plus `Hona Tha Pyar` and `Yeh Jo Des Hai Tera` to `Uttara Glow`. Current provider
inventory is 22 playlists and 1,517 memberships. The verified publish receipt is
`2d1b7e2b-0a60-4fde-928c-d23220bbc045`; those additions must not be replayed.

`scripts/chordrift-maintain.sh --account personal` is now the only user-facing
ordinary workflow. It covers Likes, intake, exclusions, managed edits, and
direct moves between managed playlists; asks only for ambiguous destinations;
summarizes net provider effects; and accepts one authorization. The former
intake and Re-evaluate wizards were removed. Internal plan phases and
verification remain recovery evidence.

The user explicitly authorized the already-resolved queue cleanup. Plan
`8453ec0f-ed9c-4ae0-b812-1a1b678108fc` contained exactly three removals from
`Re-evaluate`; apply receipt `54063b3b-7196-48f6-b7f9-7bad1d9923a4` succeeded
3/3. Verified snapshot `8a19bf16-34cb-4c3a-9383-bae30ef9d5f0` has an empty
queue, and current maintenance plan `233d9fbb-5993-41e5-9bf7-fe9a9e744366`
has zero operations. Do not replay the earlier additions or cleanup.

The user subsequently chose to remove the correction queue from the product.
The replacement gesture is a direct move from the wrong managed Spotify
playlist to the correct one. The paired change is current reclassification
evidence and may later become reviewed input to the separate Classification
Authority. Neon history must be retained; only the empty provider playlist and
active routing-surface policy are retired.

The user will delete the inactive empty `Re-evaluate` playlist directly in the
Spotify UI. This is safe: Chordrift must observe the missing provider container,
must not recreate it, and must retain Neon correction history. Do not delete its
approved artwork. Preserve `artwork/review/re-evaluate-background.png` and
`artwork/review/re-evaluate-spotify.png` as reusable visual inventory for the
next reviewed playlist expansion.

Alpha.2 daily use exposed a performance defect after a 28-track direct move.
The Spotify pull itself took 17 seconds, but the wizard then spent about 83
seconds cloning the 1,475-track approved proposal and replaying 615 active
assignment revisions through roughly 1,800 sequential Neon round trips; it then
launched 28 more per-track assignment sessions. Alpha.3 replaces replay with
set-based SQL and batches all same-destination moves atomically. A representative
isolated rehearsal with 610 active decisions and 7,918 memberships extended in
1.12 seconds locally; a two-track atomic batch completed in 0.09 seconds. The
disposable benchmark database was removed after verification.

Alpha.4 completed A021-06 with the 1970s assigned to Celluloid Mehfil (Indian
film through 1979) and Cinema Monsoon beginning in 1980. Classification
Authority outputs rich dimensions; Chordrift decides account-specific surface
granularity from library depth and preference. Deep artist catalogs may justify
several artist-specific cadence-managed surfaces, while sparse catalogs remain
combined. Require separate approval for exact membership, poetic names,
artwork, and all provider writes.

Final read-only checks preserved snapshot
`dc96cc26-c917-4bb4-8a7f-4b3c5e836f66`, 22 playlists, 1,514 memberships, 387
active exclusions, 1,718 canonical assignments, 149,419 listening events,
15,606 historical identities, and the exact three-track `Re-evaluate` queue.
The latest plan `060bdf95-3781-431a-b6a4-658e3e57b92b` is current,
zero-operation, and `maintenance`; intake audit has zero items. Canonicalized
data across all 21 durable-domain tables matched the final source exactly at
SHA-256
`589a14de30552589c50a88a9e2bcefc7ace0c63cbf7aa15cc3cc3e061273ef03`.

The final pre-cutover backup is preserved at
`$DROPBOX/Music/Chordrift/Backups/2026-08-29-v020-14-final-cutover/`; its custom
dump is 22,688,076 bytes with SHA-256
`2410b5107eb3d68e13c60bcef8b2ddbfce091a83f2c9182dd5c7b3569611dede`.
Mode-preserving local rollback copies are
`~/.cargo/bin/chordrift-v0.1.4-pre-v020` and
`~/.config/apogee/secrets.env.pre-v020-cutover-20260829`. Retain the former
45/45 Neon project. After any v0.2 database write, never point v0.1.4 at the old
database without an explicit reconciliation; see
`docs/how-to/RECOVERY_AND_ROLLBACK.md`.

No Spotify write occurred. The final pull before backup was read-only and made
six Spotify requests; the release and database cutover invoked no Spotify
command. Spin publication still has no production Spotify mutation adapter.

## V020-14 completed candidate evidence

V020-14 completed candidate creation, verification, and presentation of the
exact plans. See `docs/design/CANDIDATE_CUTOVER_GATE_V020_14.md`.

## Historical database cutover plan — completed 2026-08-29

The following plan is retained as the approval record. Its gate has been
executed successfully; the current state is the v0.2.0 deployment above.

If approved, take another final read-only production status, invariant, and
logical backup before changing anything. If production advanced after the
candidate source backup, refresh only the candidate, reapply migrations
0046/0047, and repeat exact parity and runtime gates. Cut over the verified
current-main binary and private database connection as one controlled unit,
then perform only read-only post-cutover checks. Retain the old production
project as rollback evidence. If any gate differs, restore the old v0.1.4
binary/config pair and stop. Never split the binary and database cutover.

Do not invoke Spotify during database cutover. After a successful cutover, a
read-only provider observation may be proposed separately. Any resulting
non-zero maintenance write needs its own exact enumerated plan and explicit
approval. There is still no production `SpinPublicationProvider`, so no Spin
write can be approved or performed in V020-14.

The verified candidate is Neon project `royal-snow-31539822`, named
`chordrift-v020-candidate-20260828`, in `aws-us-west-2` on PostgreSQL 18.6. It
is healthy at 47/47 migrations and uses 184,295,424 synthetic bytes against a
536,870,912-byte branch limit. At the V020-14 gate, the installed v0.1.4 binary
and production connection were still unchanged at 45/45; the later completed
V020-15 state is recorded above.

The newest read-only source backup is preserved under
`$DROPBOX/Music/Chordrift/Backups/2026-08-28-v020-14-candidate-source/`; its
22,686,266-byte dump SHA-256 is
`cc1f53eb8d6740f94b97d39be24a8131164f479ae5a35ca33bcffe3824703225`.
Production, pristine restore, and candidate invariants are byte-identical.
Normalized UTC dumps of all 21 required durable tables are byte-identical with
SHA-256
`77a55e441f84c1ea105d857bdb9c033356d2e1826778e0ddd796a473f7cde44b`.
All 101 historical plans remain `<legacy-unlabeled>` and every new product table
remains empty. The capability handshake, complete all-target suite, fake-binary
intake suite, Spin-origin regressions, and disposable PostgreSQL 18 tests pass.

A candidate-only URI appeared in local `pg_amcheck` diagnostic output. Its role
password was immediately changed, the replacement was verified, and the old
credential was verified rejected. No production credential was involved.
Neon's project role cannot install `amcheck`, so remote relation inspection was
unavailable. A local PostgreSQL 18.6 restore of the exact V020-14 dump passed
the parent/heap-index check over all 751 relations and 19,115 pages, and every
exact restore/data/application gate passed.

V020-01 through V020-14 are complete. V020-13 took a fresh read-only logical
backup of the healthy production PostgreSQL 18.6 database at 45/45 migrations
and preserved it under
`$DROPBOX/Music/Chordrift/Backups/2026-08-28-v020-13-latest-state/`. The custom
dump SHA-256 is
`40223ce9898b756438c864bb12899d14d638883a41d87adc6b02a2d500b941c1`.
It restored with exit-on-error into isolated local PostgreSQL 18.6 and advanced
from 45/47 to 47/47; migration replay was idempotent.

The installed-binary production invariant, pristine restore, and migrated
restore reports are byte-identical. Normalized data-only hashes match across
21 tables covering current inventory/order, proposals/intake, exclusions,
Re-evaluate, assignments, listening/archive evidence, plans, operations,
readiness, apply, and verification history. `pg_amcheck` passed all 852
relations / 19,238 pages. The current intake audit has zero unresolved items.
The migrated copy contains zero capability observations or product activity;
all 101 historical plans remain honestly `<legacy-unlabeled>`, so neither
`maintenance` nor `spin_publication` was fabricated. All four intake fake-
binary compatibility cases pass. Production remains unchanged at 45/45; no
candidate, cutover, Spotify access, or provider write occurred. See
`docs/design/LATEST_STATE_MIGRATION_REHEARSAL_V020_13.md`.

Repository cleanup on 2026-08-28 left `main` as the only local and GitHub
branch. Two annotated recovery tags preserve the only intentionally retained
unmerged references: `archive/v0.1.4-intake-wizard` points to tested commit
`9a078f3`, and `archive/apple-music-pre-v0.2` points to prototype commit
`21217a6`. Treat both as historical source, not current architecture. The
intake tag can recreate a migration-45 fallback worktree if absolutely needed;
the safer capability-checked wizard is already on `main` but intentionally
cannot run with the released v0.1.4 binary or against the unmigrated production
database. Prefer the v0.2 path after the V020-14 candidate/cutover gates pass.
The Apple provider must be redesigned against the provider-neutral boundary
rather than merged from its archive tag.

V020-12 adds
`SpinPublicationBoundary`: one explicitly approved account-owned Spin becomes
an immutable, checkpoint-bound synchronization plan with
`plan_origin: spin_publication`. The surface must be active, renewable,
Chordrift/mixed-authority, bound to the same recipe, and linked to the selected
account-owned provider connection. Active surface exclusions are omitted and
the plan contains only optional target creation plus enumerated additions—no
implicit full membership, removal, replacement, or reorder.

The provider-neutral `SpinPublicationProvider` port has no production Spotify
implementation. Readiness binds the exact plan/checkpoint/baseline; fake apply
and verification prove stale-state rejection, preservation of unrelated live
membership, exact enumerated additions, idempotent replay, and post-write
presence. General plan inspection exposes `spin_publication` with no fabricated
legacy proposal generation, while maintenance helpers continue to reject that
origin. Migration 0047 adds only the missing account-safe surface-to-recipe
link and permits checkpoint-bound Spin dry-run identity in the existing sync
ledger. Neither migration 0046 nor 0047 was applied to production. See
`docs/design/SPIN_PUBLICATION_PLAN_V020_12.md`.

V020-11R reviewed `9a078f3` and
`4b7d876` but did not merge either recovered branch wholesale. Apply v4 now
uses exact replacement only for a membership-identical `reorder_playlist`;
ordinary additions append only enumerated IDs. Pure regressions prove they
cannot replace unrelated membership or restore an unenumerated manual removal.

The public contract now includes a serializable installed-binary capability
manifest. `chordrift capabilities --require …` is the exact compatibility
handshake; version strings are informational only. `chordrift intake audit`
provides the recovered read-only current-intake join. Maintenance plans persist
and print `plan_origin: maintenance`; unknown and future Spin origins fail
closed. The unified maintenance wizard and its internal phase helper require
capabilities before doing work and reject every non-maintenance plan.
Fake-binary tests prove capability-first execution, review-only compatibility,
and `spin_publication` rejection before audit/apply. See
`docs/design/RECOVERED_INTAKE_COMPATIBILITY_V020_11R.md`.

Compatibility remains subordinate to the v0.2 architecture: preserve safety
invariants and understandable operator outcomes, not legacy command spelling
or internal paths.

V020-11 adds the opt-in development-line
`chordrift product` namespace for fixture-backed onboarding capture/audit,
account-scoped collection and immutable recipe review, provider-neutral recipe
execution, and exact Spin preview creation/display. Every leaf passes through
`ApplicationFacade` and emits the same stable contract/provider-write-disabled
envelope plus the complete serialized Rust value. Database-backed commands
require `CHORDRIFT_PRODUCT_REHEARSAL=1` and an already migrated isolated
database; no command applies migration 0046. The installed-binary
`scripts/chordrift-product-rehearsal.sh` compares inventory-only with enriched
results and proves exact Spin replay. A fake-binary test proves that it invokes
only `product` commands and never invokes Spotify, apply, migration, approval,
or publication. See `docs/design/CLI_FIRST_PRODUCT_REHEARSAL_V020_11.md`.

V020-10 adds the public `spin_preview`
boundary behind `ApplicationFacade`. It verifies V020-09's unordered draft and
capability snapshot, derives a stable account/input/seed Spin identity, assigns
exact one-based playback order from cadence, lifecycle narrative, section
capacity, artist spacing, and SHA-256 seed ranks, and emits structured selection
and ordering reasons for every track. The complete ordered value has a stable
fingerprint and retains honest unfilled-seat, cadence, spacing, duration-policy,
and cross-output-policy warnings. `SpinPreviewBoundary` persists and reloads the
exact value through migration 0046's existing `playlist_spins` and
`playlist_spin_tracks` tables, preserves the full `u64` seed, returns identical
command replay, and exposes account-scoped `Query::SpinPreview` display. Its
provider-free and isolated PostgreSQL 18 proofs cover deterministic replay,
seed variation, reasons, sections, degradation, mutated inputs, cross-account
create/read, and zero publication rows. It adds no migration, CLI command,
provider port, production access, or provider write. See
`docs/design/DETERMINISTIC_SPIN_PREVIEW_V020_10.md`.

V020-09 adds the public
`recipe_execution` boundary behind `ApplicationFacade`. It canonicalizes one
account-owned immutable recipe revision and its prepared candidates, allocates
positive source weights deterministically, reserves familiar-anchor and
narrative-section capacity, and enforces current-inventory/playability/explicit-
exclusion eligibility, all required collection boundaries, per-track repetition,
and per-artist budgets. Unavailable evidence sources are disabled visibly,
degraded sources remain labeled and usable, all-unavailable execution fails with
a capability error, ambiguous multi-lane/source assignment of one track is
rejected, and unfillable seats remain explicit. Both inputs and the result
receive stable fingerprints. The output is canonically serialized but
explicitly unordered (`playback_order_assigned: false`); it performs no provider
or database access and adds no migration or CLI command. Provider-free tests
prove shuffled-input replay, capability degradation, every eligibility/budget
boundary, ownership rejection, cadence/section capacity, and guardrail
enforcement/deferment. See
`docs/design/DISCOVERY_REDISCOVERY_RECIPE_V020_09.md`.

V020-08 adds
`EnrichedAuditBoundary` beside the unchanged inventory-only boundary. It runs
the same inventory baseline, then resolves exactly one selected
`extended_streaming_history` import by provider account, archive SHA-256, and
record count. The deterministic report exposes usable/superseded records,
history coverage, current/history-only identities, repetition, 180-day
long-span observations, maximum plays, and explicit completion/skip facts.
Only supported categories appear in `strengthened_conclusions`, with exact
record and track counts; preference, collection membership, and intent remain
explicitly uninferred. Side-by-side fake-input/PostgreSQL tests prove unchanged
inventory findings, replay, mode separation, account isolation, no extra
provider read, no intent write, and unchanged session state. See
`docs/design/ENRICHED_ONBOARDING_AUDIT_V020_08.md`.

V020-07 adds the public
`onboarding_audit` query boundary behind `ApplicationFacade`. It validates the
V020-06 owner, provider connection, current-inventory-only manifest, capability
snapshot, and content fingerprints, then reads only immutable checkpoint
playlist/saved-track/saved-album revisions. Its deterministic value reports
library shape, playlist duplicates and unreadable positions, cross-surface
overlap, capability gaps, explicit inference limitations, and an unapproved
preserve-first starter organization. It performs no database write or provider
call; replay keeps the same audit fingerprint and leaves the session in
`created` status. The fake-provider/PostgreSQL proof rejects enriched sessions
on this path and cross-account reads, observes no additional provider access or
collection-intent rows, and passes both fresh and migration-45 PostgreSQL 18
rehearsals. See `docs/design/ONBOARDING_AUDIT_V020_07.md`.

V020-06 adds the public `onboarding`
module and routes its invocation through `ApplicationFacade`. Its read-only
provider port returns one immutable inventory checkpoint and only explicitly
selected extended evidence. The PostgreSQL boundary validates the stored
Chordrift owner, provider namespace/account identity, capability availability,
and checkpoint fingerprint; persists the capability observation, exact input
manifest, content fingerprint, and output provenance through migration 0046;
and fixes `ignore_existing_intent` to true. Idempotent command replay returns
the existing session before another provider read, while the same key with a
different evidence selection fails visibly. The fake-provider/PostgreSQL 18
proof changes collection intent between capture and replay without changing the
session, rejects unavailable capability without a provider call, and rejects a
cross-account connection before provider access. See
`docs/design/ONBOARDING_SESSION_V020_06.md` for the exact boundary.

V020-05's one additive migration, `0046_product_domain_foundation.sql`, remains
unchanged and unapplied to production Neon. V020-12 adds the separate additive
`0047_spin_publication_plans.sql`; the complete 47-file fresh chain and
idempotent migrator replay pass on isolated PostgreSQL 18, while the original
migration-45-to-46 rehearsal remains intact. Production remains healthy at
45/45. See
`docs/design/PRODUCT_SCHEMA_V020_05.md` for the exact reconciliation.

V020-04 adds a test-only deterministic
fake-provider/application harness built on the public `contract`, `application`,
and `domain` modules. Six adversarial tests prove that two Chordrift accounts
and two provider namespaces cannot cross; equal opaque IDs remain distinct;
equal idempotency keys are account-scoped; accepted work is not duplicated by
replay or retry; cancellation stops at the next checkpoint; transient retries
stop at their configured bound; and an unsupported inventory capability fails
visibly without making a provider call. This proof does not make the production
Spotify adapter, storage path, or CLI multi-account/multi-provider.

The public `domain` module provides
validated account-owned UUID identities; provider-qualified account, track, and
playlist IDs; provider and evidence capability reports; collection membership
strength, provenance, and bounded confidence; independent surface authority,
purpose, and refresh axes; recipe-v1 lanes, allocations, cadence, ordering,
sections, and guardrail categories; and account-bound recipe revision and Spin
identities. Zero allocation weight is valid for an individual source, while an
all-zero recipe is rejected. Validated deserialization cannot bypass provider-
namespace, confidence, recipe, membership, or Spin ownership invariants. The
module contains no SQL, provider payload, terminal, platform, or transport type.

Documentation on `main` now describes released v0.2.0 and the architecture
ahead. The `v0.1.4` tag is the exact authority for that historical release.
Completed slice documents retain chronological evidence but must label old
approval gates as historical rather than current operator instructions.

The user now performs normal cleanup and listening with released v0.2.0. Treat
v0.1.4 only as a controlled recovery source; do not retrofit v0.2 features into
that line or run the two database generations as concurrent authorities.

The repository also includes `scripts/chordrift-intake-move.sh` for the current
daily-driver transition. It uses the installed binary to record one or more
reviewed, unresolved Inbox tracks in an editable proposal destination. It
resolves display names to stable playlist keys and refuses active exclusions,
non-Inbox tracks, and already-resolved tracks before changing Neon. It never
approves the proposal, creates or applies a plan, removes Inbox membership, or
writes to Spotify; those remain separate whole-proposal and synchronization
decisions. When the latest proposal is approved, explicit `--prepare` may clone
its structure through the strict 1.0 extension path only if the supplied IDs
cover the entire pre-extension unresolved set. The extension still replays all
durable manual decisions and may therefore expose older needs-review items; the
helper reports rather than classifies them. V020-11R retains this narrow helper
for exact Inbox-only placement and adds the complete capability-checked wizard
for mixed intake; neither replaces the other's stricter scope.

On 2026-08-27 the helper was exercised against the live `personal` account for
one explicitly reviewed Inbox discovery destined for `Dakshina Pulse`. The
previous proposal was approved, so the guarded preparation created editable
proposal `61ce404d-83bf-47ce-bee0-84663db72fd6` and recorded the manual
placement. Read-only inspection proves the track remains in Inbox until a later
apply, has one manual `Dakshina Pulse` placement, and has no active exclusion.
No proposal approval, synchronization plan, cleanup, or Spotify write occurred.
Durable-decision replay exposed 10 older unresolved canonical/transport items;
the proposal is therefore intentionally incomplete at 1,628 of 1,638 required
tracks. Review those items rather than approving or applying the proposal as-is.

## Released v0.1.4 operational state

Chordrift v0.1.4 is released from commit
`657c85a995bdff92559bbd819f2244c9ee54ca71`. GitHub CI run
`33018011273` passed formatting, strict Clippy, all ordinary and documentation
tests, both PostgreSQL 18 integration surfaces, and `cargo package`. The
annotated `v0.1.4` tag resolves to that exact commit, crates.io published
`chordrift 0.1.4`, and the public GitHub release is `Chordrift v0.1.4`; it is
neither a draft nor a prerelease. The first post-merge CI run exposed that the
0045 relation-rename regression left its deliberately partial cleanup
simulation in the shared test database. Commit `657c85a` now runs that
simulation transactionally and rolls it back; a fresh local PostgreSQL 18
reproduction and the complete replacement CI both pass.

The v0.1.4 implementation makes a routine pull proportional to change: saved
tracks, saved albums, and recent plays are fetched concurrently; unchanged
playlist persistence is set-based; historical identities and recent events are
batched; unchanged analysis reuses its checkpoint; and listening statistics
update only identities receiving new events. A fully unchanged provider
inventory retains its existing content-addressed revisions, so it no longer
copies and deletes all 1,790 playlist memberships through transient staging.
The routine history summary/relink path now scans the compact per-identity
statistics cache rather than all normalized events. Explicit history refresh
and summary commands remain event-level verification surfaces.

`sync pull` now emits compact tables, actual Spotify Web API request count, and
provider/analysis/history/publication/total elapsed times. Formatting, all
ordinary targets, strict all-target/all-feature Clippy, PostgreSQL 18 schema
migration, post-clean provider persistence, and optimized recent-history tests
pass. Real personal pulls and one operator-approved reconciliation exercised the
complete workflow. Continue normal use and record any newly observed issue as a
later patch rather than reopening the completed database-v2 migration.

The first instrumented personal pull completed in 70.3 seconds and isolated a
57.5-second publication-check phase. The cause was unconditional recreation of
16 managed-playlist verification headers and roughly 1,754 verification-track
rows, issued one row per Neon round trip, despite no pending apply. The
implementation now returns after one pending-work probe when playlist
membership is unchanged, and batches headers plus ordered tracks when a real
verification is required.

The rerun completed in 4.6 seconds (provider 3.7 s, analysis 473 ms, history
313 ms, publication checks 98 ms), proving the latency fix. A subsequent track
removal completed in 10.0 seconds and produced the expected one-operation stale
plan display. Readiness then failed with SQLSTATE 42P01 because the stored
`account_track_is_library_candidate` function—missed by the v0.1.3 runtime
refactor—still named cleanup-removed v1 tables. Additive migration 0045 replaces
that body with v2 current revisions, latest managed verification baselines, and
active exclusions. A PostgreSQL 18 regression physically renames the legacy
relations as cleanup does and proves the function still executes. The user
subsequently approved exactly additive production migration 0045. It applied in
722 ms; production is healthy at 45/45 with zero pending or failed migrations.
Read-only inspection of removed Spotify identity
`3A2o7x7zA3MwgQlcPgVboZ` successfully exercised the repaired v2 candidate path,
retaining canonical placement and exclusion context. A readiness attempt
against stale plan `fac3d2ba-6b6e-47e6-9575-24e10fa4458b` now returns the
intended actionable stale-plan error instead of SQLSTATE 42P01. A subsequent
fresh plan/readiness/apply/pull workflow successfully recorded the requested
exclusion, verified apply run `c4d0084c-d66b-478f-a1fe-877427f2bea7`, and
completed its follow-up pull in 5.4 seconds (provider 3.9 s, publication checks
686 ms). No cleanup occurred.

All interactive command output now passes through `src/presentation.rs`.
Key/value and TSV reports become compact titled tables, JSON evidence is
flattened, existing bespoke reports pass through, and redirected output remains
byte-stable for scripts. `src/terminal.rs` owns the shared table language,
workflow progress bar, and provider/database event rendering. Readiness check
receipts are batched into one Neon insert.

The repository now includes executable `scripts/chordrift-workflow.sh` for the
operator convenience loop. It uses the installed `chordrift` command by
default (or exact `CHORDRIFT_BIN` override), never `cargo run`. It performs an
initial pull, creates/shows the plan, optionally preflights publish, runs probed
readiness, requires the operator to type the exact assessment UUID, applies one
publish or reconcile phase, pulls/verifies the receipt, and creates/shows the
final convergence plan. It refuses cleanup, retirement, stale plans, and plans
spanning multiple phases. `--skip-initial-pull` is available for a just-pulled
state. Shell syntax, ShellCheck when available, and help output pass.

A post-release manual deletion exposed Spotify snapshot propagation delay, not
a binary mismatch: the wrapper's first pull reused all 22 playlists and planned
zero operations at 1,789 entries; roughly one minute later the same installed
v0.1.4 binary observed one changed playlist and 1,788 entries. The wrapper now
offers an interactive retry for a zero-operation plan and supports bounded
`--wait-for-change SECONDS` polling at ten-second intervals. Apply run
`a19d260a-0c11-432c-8597-222e6b361778` succeeded its one reconcile operation
and is awaiting the required verification pull. The next safe operator action
is a normal `chordrift sync pull --account personal`, followed by receipt and
convergence inspection; do not repeat the apply.

The prior v0.1.3 release state follows.

Chordrift v0.1.3 is released. Release commit
`2355aec3512006ec65a95fb623e5b073b005cdfd` passed GitHub CI run
`33008595427`, including formatting, strict Clippy, all ordinary tests,
PostgreSQL 18 integration tests, clean-schema provider/history round trips, and
`cargo package`. The annotated `v0.1.3` tag resolves to that exact commit.
crates.io published `chordrift 0.1.3`, and a registry-only lookup from outside
the workspace downloaded and identified that version successfully. The public
GitHub release is `Chordrift v0.1.3`; it is neither a draft nor a prerelease.

Work is on `main` (the implementation is also retained on
`codex/v0.1.3-database-v2-runtime`). The v0.1.3 code refactor is
implemented: ordinary provider reads use current/revision/checkpoint v2
surfaces, archive and recent-play writes go directly to normalized evidence,
and provider pulls use transient v2 import staging that is empty at commit.
Migration 0044 supplies these runtime surfaces and cleanup receipts. It is now
installed on the live `chordrift` project, which is healthy at 44/44.

The exact provider-free cleanup engine is implemented as `chordrift db compact
cleanup plan/apply/verify`. A second fresh PostgreSQL 18 restore reached 44/44,
reapplied approved data plan
`a850fb15603f82c934daa127cfb768084938bc8ac601b6f30643ebc3a84e2ae8`, and
successfully applied rehearsal-only cleanup plan
`0688bf0984ea6f6b26cf65ca7ab1c9fcb762601c6a512b204e7a79312830f964`.
The invariant hash remained
`24f5da45845bb48b3cfeb49cbd09fe371043c7f9544ea38993d3016beaf0d6a3`.
The clean copy retains 149,314 normalized events, 15,575 historical identities,
both evidence manifests, 58 observation headers, 24 checkpoints, 22 current
playlists, 1,790 ordered memberships, and every durable plan/verification/audit
reference. Its measured size is 167,974,591 bytes. Provider inventory and
archive-history write round trips both pass against the post-clean schema.

Production cleanup was explicitly approved and applied to project
`damp-hall-40280714`, display name `chordrift`, using exact plan
`0688bf0984ea6f6b26cf65ca7ab1c9fcb762601c6a512b204e7a79312830f964`,
preserving invariant
`24f5da45845bb48b3cfeb49cbd09fe371043c7f9544ea38993d3016beaf0d6a3`,
at `2026-08-26T19:52:34.237614+00:00`. Independent verification proved legacy
tables absent, provider-import staging empty, all 149,314 normalized events and
both evidence imports retained, 44/44 migrations, every parity gate true, and
`ready_for_cutover: true`. History, signals, embeddings, albums, playlists, and
database-v2 status all passed against the post-clean schema. A fresh child
process using the owner-only persistent secret also passed database status,
cleanup verification, and a runtime playlist read without exposing the value.
Database size fell from 358,850,560 bytes to 167,788,544 bytes; ordinary-table
total is 156,459,008 bytes.

After the post-clean observation gate passed, deletion of former project
`mute-recipe-86719846`, named `chordrift-legacy-rollback`, was separately and
explicitly approved. Neon deleted that exact project and a subsequent project
listing proved it absent while `damp-hall-40280714` remained present. The live
project then passed database health and cleanup verification again. The
pre-compaction dump was rehashed after deletion and still matches SHA-256
`8c5796cba5729931678f825021fe03268b81129352349266d7a68b487b3711ae`;
it is now the durable rollback artifact. No Spotify request or write occurred.

Operational warning: this Codex desktop task inherited a stale
`CHORDRIFT_DATABASE_URL` targeting the former project even though the persistent
`/Users/suhail/.config/apogee/secrets.env` value targets `chordrift` and remains
mode `0600`. The first approved 0044 invocation therefore added the same
additive migration to former project `mute-recipe-86719846`. No row was deleted,
normalized, compacted, or rewritten there; before its later approved retirement
it reported 44/44 but zero normalized events and zero checkpoints. After
discovery, every operation
used explicit project ID `damp-hall-40280714`; 0044 was then applied and verified
on the intended live project. Future production commands in this still-running
desktop process must not trust the inherited variable: obtain a short-lived
connection for the explicit project ID or restart Codex so it loads the
verified persistent secret.
Do not shell-source `secrets.env`; URL query characters are not shell syntax.
No Spotify request or write occurred.

## Historical: database-v2 project cutover

The no-cost replacement candidate is complete and verified. Neon project
`damp-hall-40280714`, now named `chordrift` (formerly
`chordrift-v2-candidate-20260826`), is an isolated
PostgreSQL 18 project in `aws-us-west-2`. It is now Chordrift's configured
database through the private Apogee `CHORDRIFT_DATABASE_URL` value. Do not print
or persist its connection URL elsewhere. The former production project
`mute-recipe-86719846` was later renamed `chordrift-legacy-rollback` and, after
cleanup and observation passed, deleted under separate exact-ID approval.

The pre-compaction dump hash was reverified as
`8c5796cba5729931678f825021fe03268b81129352349266d7a68b487b3711ae`.
It restored into the candidate with 39/43 migrations, byte-identical invariants,
and a compact 249,331,712-byte database. Migrations 0040-0043 then reached
43/43, and exact plan
`a850fb15603f82c934daa127cfb768084938bc8ac601b6f30643ebc3a84e2ae8`
completed successfully on the candidate.

Independent verification is exact: 149,314 legacy and normalized events;
23,769,184,794 ms; identical first/last timestamps; 100,926 matched events;
1,720 matched and 13,855 unmatched identities; both archive manifests; 24
checkpoints; and zero plan, verification, cleanup, or Re-evaluate references
awaiting checkpoints. Current provider state matches all 22 playlists / 1,790
ordered memberships and both saved surfaces. `db v2 status` reports
`ready_for_cutover: true`. The candidate is 358,686,720 database bytes and
347,504,640 ordinary-table bytes, within the free project's 0.5 GB allowance.
Its read-only cutover-plan hash is
`32f1e7f3e9899c72a822a5faf588c29dc905d62ead3b3b17313d165d6e4640b8`.

One candidate-only connection URI was echoed by `pg_amcheck` when the tool
rejected URI syntax. Treating that credential as compromised, its role password
was immediately reset through the Neon API and the replacement credential was
verified before cutover. No then-active production credential was involved;
never copy the old value into docs or logs. Managed Neon does not allow the
project owner to install the
superuser-only `amcheck` extension, so remote `pg_amcheck` cannot run. The
structurally equivalent local PostgreSQL 18 rehearsal already passed
`pg_amcheck`; all candidate application/migration checks pass independently.

The approved connection cutover is complete. Apogee's private secret file
remains mode `0600`; a fresh process loaded through Apogee reached the candidate
and reported 43/43 migrations, byte-identical invariants, verified normalized
evidence, 24 checkpoints, `ready_for_cutover: true`, cutover hash
`32f1e7f3e9899c72a822a5faf588c29dc905d62ead3b3b17313d165d6e4640b8`,
and 358,686,720 database bytes. This switches the database project, not the
application's individual legacy-table read paths; those were then a later code
refactor. At this historical gate no cleanup, old-project deletion, or Spotify
operation had occurred, and the former project was retained throughout
observation pending separate approvals.

The first exact-confirmed production data-migration attempt using plan
`a850fb15603f82c934daa127cfb768084938bc8ac601b6f30643ebc3a84e2ae8`
was rejected by PostgreSQL with SQLSTATE `53100` (insufficient storage). Do not
retry blindly. The transaction rolled back logically: normalized events,
historical identities, evidence imports, checkpoints, and migration receipts
all remain zero; all 464 durable references still await checkpoints; the plan
hash is unchanged and applicable; and the full legacy invariant is unchanged.

The aborted inserts nevertheless allocated dead physical pages. Production now
reports 514,457,600 database bytes and 503,087,104 ordinary-table bytes;
`normalized_listening_events` has zero visible rows but 98,500,608 total bytes,
and `historical_provider_track_identities` has zero visible rows but 4,128,768
total bytes. That former production project remained healthy at 43/43
migrations. No vacuum,
compaction, quota change, retry, read cutover, deletion, connection change, or
Spotify operation was performed.

This failed in-place path is superseded by the verified replacement-project
cutover. The former project was retained only for rollback and has now been
retired under the separate exact-ID approval recorded above.

The prior cutover task ended on the pushed
`codex/database-v2-migration-rehearsal` branch. Do not resume the completed South Asian,
legacy-route, Inbox, or Liked Songs cleanup. First read `README.md`, the
v0.2.0-and-later sections of `ROADMAP.md`, this section, and
`docs/HOW_TO_CHORDRIFT.md`.

The safe cleanup foundation, additive database-v2 schema, local migration
rehearsal, replacement candidate migration, and database-project connection
cutover are complete through `codex/database-v2-migration-rehearsal`. The live
configured candidate was then 43/43 with exact legacy/current/evidence parity
and inside the free storage allowance. At that historical point, application
read-path refactoring, legacy cleanup, rollback, and old-project deletion were
still later gates requiring separate approvals.

The current v0.1.2 database is healthy but its production physical footprint
was about 391 MB because raw metadata is repeated across 149,314 listening
events and complete playlist membership is copied across routine snapshots. A
fresh logical restore occupies 249,657,023 bytes, proving that some production
storage is churn/bloat while substantial logical duplication remains. The
13,855 unmatched tracks are lightweight historical identities from the Spotify
archive, not current library members; preserve and resolve them lazily.

A verified pre-compaction backup exists at
`$DROPBOX/Music/Chordrift/Backups/2026-08-26-pre-compaction/`. It contains a
25 MB custom-format dump, schema SQL, parsed `pg_restore` catalog, and SHA-256
checksum. The dump hash
`8c5796cba5729931678f825021fe03268b81129352349266d7a68b487b3711ae`
was verified, then restored with exit-on-error into isolated local PostgreSQL
18.6. The restore has all 39 successful migrations and zero failed migrations;
`pg_amcheck` passed all 676 relations / 30,156 pages. Production was not
mutated, its connection was not changed, and Spotify was not contacted.

Reusable read-only commands are now:

```console
chordrift db invariant-report --account personal
chordrift db storage-report
chordrift db compact plan --account personal
chordrift db v2 status --account personal
chordrift db v2 migration plan --account personal
chordrift db v2 migration verify --account personal
chordrift db v2 cutover-plan --account personal
```

The compaction planner starts a read-only transaction and rolls it back. The
rehearsal contains 58 provider snapshots: keep one current, 41 older snapshots
are protected by durable plan/verification/generation/bookmark/intent history,
and 16 older routine snapshots have no such durable reference. Those 16 contain
26,490 repeated playlist membership rows plus repeated saved-surface rows.
This is measured planning evidence, not authorization to delete anything.

The invariant baseline is 22 playlists / 1,790 exact ordered memberships /
1,765 unique playlist tracks; 16 canonical playlists / 1,754 unique canonical
assignments; 107 active exclusions; one empty active Re-evaluate surface; zero
saved tracks or albums; 149,314 active listening events across 15,575
historical identities (1,720 matched / 13,855 unmatched); 23,769,184,794 ms
from 2014-11-05 through 2026-08-26; two archive import hashes; and 19 verified
apply runs. Exact order/hash values and full physical sizes are recorded in the
database-v2 design. The earlier final zero-operation sync plan remains
`56a0d535-f83e-42ae-898e-8ed627e6f4e9`; the newest stored plan
`fac3d2ba-6b6e-47e6-9575-24e10fa4458b` contains one reconcile
`exclude_track`, which must remain visible as a pending intent delta during v2
comparison.

Migration `0040_database_v2_foundation.sql` is additive. It adds one current
provider inventory per account, content-addressed playlist/saved-surface
revisions, compact checkpoint tables, historical provider identities, typed
normalized listening evidence, and nullable checkpoint references for plans
and managed verifications. The Spotify importer dual-writes the compact current
state while retaining legacy snapshots until cutover. Repeating an identical
fixture creates no duplicate v2 playlist or saved-surface revision.

The migration was applied only to a clone of the restored PostgreSQL 18.6
database. It completed in 153 ms with 40/40 migrations successful. The complete
v1 invariant report was byte-identical before and after; `pg_amcheck` passed
758 relations / 30,301 pages. The v2 current state
has one inventory, 22 current playlist pointers, 22 immutable revisions, and
1,790 ordered revision tracks; current playlist order and both saved surfaces
match exactly. Database size increased only from 249,657,023 to 251,205,311
bytes for the additive schema/backfill.

Migrations 0041 and 0042 add the exact-confirmed migration/receipt surface and
local listening-evidence dual-write compatibility. Migration 0043 makes report
and current-inventory hashes stable across Neon `C.UTF-8` and local
`en_US.UTF-8` collations. The new local rehearsal clone reached 43/43
migrations. Exact plan
`a850fb15603f82c934daa127cfb768084938bc8ac601b6f30643ebc3a84e2ae8`
migrated all 149,314 events and 15,575 identities, both archive manifests, 43
plans, 420 managed verifications, and one cleanup approval. Forty-one referenced
snapshots deduplicated to 24 compact checkpoints. A second apply with the same
hash was idempotent.

Independent verification matches 23,769,184,794 ms, first/last timestamps,
100,926 matched events, 1,720 matched identities, and 13,855 unmatched
identities exactly. The original invariant report is byte-identical before and
after. `db v2 status` now reports `ready_for_cutover: true` on this rehearsal,
and `pg_amcheck --parent-check --heapallindexed` passes. The fresh dual-storage
rehearsal is 358,815,423 bytes; normalized events plus identity metadata are
smaller than the legacy event relation, but no legacy deletion is approved.

Individual archive-member hashes were not stored by v1. The migration records
17 known event-bearing paths as `archive_manifest_only` with null member hashes
instead of fabricating digests; the verified containing ZIP hashes remain
authoritative. Migration 0042 dual-writes future local archive/recent-event
changes into normalized evidence during the rollback observation window.

The read-only rehearsal cutover plan hash is
`32f1e7f3e9899c72a822a5faf588c29dc905d62ead3b3b17313d165d6e4640b8`.
Never apply that hash blindly to production.

The read-only production preflight and additive schema gate are complete.
Backup hash
`8c5796cba5729931678f825021fe03268b81129352349266d7a68b487b3711ae`
still verifies; Neon is healthy on PostgreSQL 18.6 with 43/43 migrations; and
production/base invariant reports are byte-identical with stable playlist
fingerprint
`d3186b303fa7d7dabe4d45f605d8a0d97a132fe50cd2bc00368491570f83e90b`.
All 17 non-empty playlist hashes match individually. A read-only prospective
production current-state calculation matches the rehearsal hash
`f12ef35e6ac961c99819be5d667eb60273435c25f0dd5b6f9182b369ba8e0ff3`.
After the additive schema gate, production measures 411,852,800 database bytes
and 400,482,304 ordinary-table bytes. The compaction classification remains
one current, 41 protected, and 16 redundant snapshots. No production write or
Spotify request occurred.

Production emitted migration data-plan hash
`a850fb15603f82c934daa127cfb768084938bc8ac601b6f30643ebc3a84e2ae8`,
with `applicable: true`: 149,314 events, 15,575 identities, two archive
manifests, 41 checkpoint source snapshots, 43 plan references, 420 verification
references, and one cleanup reference. Current v2 state is exactly 22 playlists
and 1,790 ordered memberships; normalized evidence and checkpoints remain
empty, proving the data apply has not run. The next bounded gate is only
`chordrift db v2 migration apply --account personal --confirm <exact hash>`,
followed immediately by read-only invariant/status/verify/storage reports and a
stop. Read cutover, observation window, and legacy cleanup remain distinct
approval gates.

The approved direction is broader than static playlist organization. Chordrift
becomes a personal listening-system designer with three distinct layers:

1. **Canonical collections** retain durable identity, membership rationale,
   user-approved boundaries, and artwork.
2. **Intake surfaces** retain discovery/review provenance. Like is the least-
   friction intake; named inboxes add context; `Re-evaluate` means keep but
   reconsider the current destination.
3. **Generated listening playlists** are renewable, versioned recipe outputs.
   A track may appear in several outputs without duplicating its canonical
   identity.

Recipes support both curated automatic presets and advanced controls. Initial
dimensions are discovery recency, recent rotation, old favorites, forgotten
favorites, recommendations, collection eligibility and diversity,
new-versus-familiar balance, artist spacing, target duration, repetition and
cross-output budgets, ordering philosophy, and user-defined sections. Every
generation needs immutable inputs, recipe version, evidence capabilities,
constraints, random seed, per-track reasons, and final order.

Data availability is first-class. Provider adapters report capabilities;
recipes declare required and optional evidence. Spotify save timestamps can
drive New Discoveries, Recently Played supplies bounded short-term evidence,
and the optional extended-history archive unlocks trustworthy lifetime repeats,
skips/completions, forgotten favorites, and deep rediscovery. Missing evidence
disables a feature or reduces confidence with a visible explanation.

User intent has strength: hard collection boundaries, strong preferences, soft
ranking facts, and one-generation choices are not interchangeable. Corrections
become account-scoped evidence. Chordrift may propose a broader reusable rule
after repeated consistent corrections, but cannot silently activate it. A
future classic-Hindi collection remains a v0.2+ review candidate; use
`Re-evaluate` until the evidence supports a coherent collection, name, and
artwork.

The intended onboarding is agentic but permission-bounded: read-only inventory
and audit; explain findings and missing capabilities; propose collections,
intakes, recipes, cleanup, names, and artwork; let the user edit or approve;
show an exact diff; obtain separate publication and destructive-cleanup
approval; execute through immutable readiness/apply gates; pull and prove
zero-operation convergence.

Recommended first slice: design the provider-neutral recipe and capability
model plus deterministic, provider-free preview for one `New Discoveries +
Rediscovery` recipe. Do not start with scheduling or a large UI. Reuse canonical
identity, classifications, complete inventory, listening signals, immutable
proposals, readiness, apply receipts, and verification.

## Operator and development requirements

- Normal product work runs quietly in the background. Do not design workflows
  that open Terminal windows, visible helper shells, or repeated browser
  pop-ups. OAuth may open the system browser only when consent is required and
  must return to a clear success/failure state in the app.
- The future signed native app owns scheduling, progress, cancellation,
  notifications, retry, and interruption recovery. Long work must never look
  stalled and must be safe to resume.
- Personal secrets are managed through 1Password and OS Passwords/Keychain.
  Apogee may load approved development environment variables, but Chordrift
  must not require Apogee for end users. Never print secret values or place
  them in config files, logs, shell history, source control, launch agents, or
  shell startup files. Use secret references or already-loaded variables.
- Keep user-editable non-secret configuration under
  `$XDG_CONFIG_HOME/chordrift`. A future preferences UI reads and writes the
  same schema so advanced users retain direct control.
- During Codex operations, inspect first, generate immutable/provider-free
  plans, report exact effects, request the smallest necessary approval, then
  apply and verify. Consolidate predictable permissions before long unattended
  work, but stop for artwork approval and final provider mutation approval.
- Do not launch GUI applications merely to show that a file exists. Give paths
  and instructions unless interactive OAuth, visual approval, or explicit user
  direction requires a launch.
- Prefer background-friendly, non-interactive commands and concise progress
  updates. Never expose tokens, database URLs, or 1Password values in command
  output.

## v0.1.2 release closure

- The live personal Spotify reconciliation is complete. Four approved
  destinations and their covers were published, 598 placements applied, 145
  consumed Inbox entries cleared, and Monsoon Cinema plus three legacy Route
  playlists retired.
- Liked Songs uses the account-scoped
  `clear-after-verified-assignment` policy. Of 346 supported saved tracks, 345
  had verified canonical placements and one had a durable exclusion. Cleanup
  apply run `39bc8d9b-1d05-4e13-aa5e-2d667ac4eaf0` removed all 346 with zero
  failures and was provider-verified against snapshot
  `7bca3d63-48d3-4193-afca-fc9cb634360c`.
- Final immutable plan `56a0d535-f83e-42ae-898e-8ed627e6f4e9` contains zero
  operations. The current Spotify surface has 22 playlists, 1,791 entries,
  1,766 unique playlist tracks, zero supported saved tracks, and no duplicate
  entries.
- The provider briefly reported one unsupported/unavailable saved item; it was
  never treated as an addressable supported track. The final stable pull
  reported zero unsupported items as well.
- A possible dedicated classic Hindi cinema destination is deferred to v0.2.0.
  Until reviewed as a coherent collection, move such edge cases into
  `Re-evaluate`, export a CSV if the queue grows, and classify them explicitly.

## Private classification sidecar (completed v0.1.2 foundation)

- Migrations `0036_user_track_classifications.sql` and
  `0037_classification_decisions.sql` add set/clear revision history and
  exact-approval CSV batches. User facts do not overwrite external/model facts.
- CLI: `classify set`, `clear`, `history`, `export`, `import`, and `approve`.
- CSV rows are inert unless `action` is `set` or `clear`; imports are drafts;
  approval must repeat the exact batch UUID.
- Embedding model v5 adds active collection/region/tradition/language facts in a
  `user-classification@v1` namespace at weight 1.25. Notes are explainability
  only. The base acoustic model vector is unchanged.
- The Monsoon Cinema and North/South review was completed in v0.1.2. Its
  resulting South Asian destinations were published and the legacy containers
  retired. Do not repeat that migration.
- Interactive `tracks inspect` now presents human sections and tables; raw
  provenance is opt-in with `--technical`. Interactive tables use the full
  detected terminal width. A later configuration layer belongs at
  `$XDG_CONFIG_HOME/chordrift/config.toml` and should cover width/layout, color,
  inspection detail, and date formatting.
- Migration `0038_user_classification_cohorts.sql` adds account-scoped,
  multi-valued personal cohorts. `user_cohorts` is backward-compatible in CSV
  schema v1 and is deliberately excluded from the sound embedding; it is future
  composition intent. The dedicated glossary and copy/paste templates are in
  `docs/how-to/CLASSIFICATION_DIMENSIONS.md`.
- The current schema is shaped for multiple accounts, and classification lookup
  now proves selected-account library membership. A complete two-account and
  provider-neutrality audit remains mandatory before a friend trial; scope is
  recorded in `docs/design/ACCOUNT_AND_PROVIDER_BOUNDARIES.md`.

## Project and repositories

- Project: Chordrift, a personal music-library intelligence and synchronization
  CLI written in Rust.
- GitHub: <https://github.com/orbyts/chordrift>
- Primary Codex workspace: `/Users/suhail/Documents/ChatGPT/Music`
- User's normal clone: `$CRATES/chordrift`, currently
  `/Users/suhail/Library/CloudStorage/Dropbox/matrix/crates/chordrift`
- Local Storexa clone, if its source is needed: `$CRATES/storexa`
- Current release line: `v0.2.1` prereleases; alpha.16 is the remembered saved-
  intake checkpoint after V021-04. Historical branches are recovery references, not pending
  merge sources.

Before editing, inspect `git status --short`, the current branch, this file,
`ROADMAP.md`, and `docs/HOW_TO_CHORDRIFT.md`. Preserve unrelated user changes.

## Durable architecture decisions

- The newest complete provider observation is authoritative for ordinary
  user-authored current membership and order.
- Neon PostgreSQL is the canonical durable ledger for observations, exact
  accepted baselines, history, exclusions, intent, and authorized publication.
- Provider APIs are adapters. Current Spotify state is pulled into immutable
  snapshots; provider-authored changes update the Neon model after bounded
  interpretation rather than being reversed from an older model.
- Chordrift can write to Spotify only through immutable inspected plans, exact
  readiness assessments, phase-scoped confirmation, resumable operation
  receipts, a post-write pull, and provider convergence verification. Preserve
  those gates for the native UI and every future provider adapter.
- Spotify downloadable archives are optional enrichment and immutable local
  recovery inputs. They are not the operational database.
- A routine `chordrift sync pull` reads live Spotify state and reconciles
  history already stored in Neon. It never scans local ZIP files.
- `chordrift history ingest` is the explicit command for newly downloaded ZIPs.
  It deduplicates cumulative exports at archive and event level, then archives
  the original ZIP locally.
- `chordrift history restore` replays retained archives for database recovery.
- Secrets belong in macOS Passwords/Keychain. Environment exposure is managed
  by Apogee. Do not modify `.zshrc`, `.bashrc`, or other shell initialization
  files.
- Minimize provider requests. Reuse Spotify playlist snapshots and saved-track
  baselines from Neon whenever their remote signatures are unchanged.
- The normal Neon/CLI playlist surface contains only playlists present in the
  latest successful Spotify snapshot and uses that snapshot's current names.
  Older names and removed playlists remain only in immutable sync/audit history;
  proposed Chordrift names remain separate until published.

## Product intent

The concise product thesis is **a clean listening surface backed by lossless
musical memory**. The problem is accumulated playlist/library entropy: old
favorites, unexplained songs, followed or shared playlists, provider discovery,
and abandoned user lists become indistinguishable, so the user stops exploring
their own rich history and defaults to song radio. Chordrift must first explain
where each playlist and track came from, retain the best available provenance
and history, explicitly mark gaps, and only then simplify the provider surface.
The result should be a small set of purposeful, approved, artwork-backed
playlists the user genuinely listens to—not merely more generated playlists.

Chordrift will become the canonical playlist orchestrator while discovery stays
native to each streaming platform. A small number of provider playlists can be
marked as discovery inboxes. Chordrift will consume new tracks, recognize
existing canonical tracks, and eventually clear or retire inbox/legacy
playlists only after every track is represented in an approved replacement.

Clustering and LLM-proposed playlist names must remain inspectable and require
explicit user approval. No track or playlist deletion may be implicit. Managed
playlist application must be idempotent, auditable, interruption-safe, and
converge to zero changes on repeated runs.

If this personal workflow proves valuable, a future UI should expose the same
audited model: active library, external bookmarks, immutable history,
“why is this here?” provenance, bounded cleanup approvals, artwork review, and
manual vibe corrections. Do not assume a commercial multi-user product before
validating that the problem and workflow generalize.

## Current Spotify implementation

Completed releases:

- v0.0.1: Storexa/Neon project skeleton and migrations.
- v0.0.2: Spotify PKCE authorization and read-only inventory.
- v0.0.3: canonical analysis, incremental pull, removals, playlist roles, and
  drift policies.
- v0.0.4: cumulative Spotify history ingestion/recovery and read-only query
  commands.

Useful commands:

```console
chordrift sync pull
chordrift playlists list
chordrift playlists tracks --name "Playlist name"
chordrift analyze summary
chordrift analyze overlap --limit 25
chordrift analyze duplicates --limit 25
chordrift history summary
chordrift history top --limit 25
```

The last verified history state contained 2 archives, 149,195 music events,
15,553 unique Spotify track IDs, 6,602.55 listening hours, and dates from
2014-11-05 through 2026-08-20. Treat these as a checkpoint, not hard-coded
expectations; query Neon for current values.

Local Spotify recovery inputs are Git-ignored beneath:

```text
data/spotify/personal/
├── inbox/
│   ├── account-data/my_spotify_data.zip
│   └── extended-streaming-history/my_spotify_data.zip
└── archive/<kind>/<date>/<sha256>/my_spotify_data.zip
```

Keep Spotify's original ZIP filename. Folder kind, date, and SHA-256 prevent
collisions. Do not commit anything beneath `data/`.

## Deferred Apple Music provider

The user plans to enroll in the Apple Developer Program eventually, partly for
another Swift project named Photara, but intentionally deferred the annual fee.
Apple Music must not block Chordrift's Spotify milestones.

The `codex/apple-music` branch contains an offline-tested foundation for:

- ES256 Apple developer-token signing;
- Keychain-backed Media Services and per-user credentials;
- loopback MusicKit user authorization;
- read-only catalog access;
- batches of up to 25 ISRC lookups;
- metadata fallback searches; and
- extended `audioVariants` detection, where `dolby-atmos` indicates Spatial
  Audio availability.

It has no user-facing CLI or persisted matching decisions and has not been
tested against Apple. When resuming, first create a MusicKit-enabled Media ID
and `.p8` Media Services key, then rebase the branch onto current `main` and
integration-test before designing migrations or merging.

Temporary Spatial Audio workaround:

1. Create a dedicated public Spotify playlist of candidates.
2. Scan it at <https://helloatmos.app/spotify/>.
3. Export the Atmos subset to a specially named Apple Music Spatial Audio
   playlist, or make a temporary filtered Spotify playlist and mirror it with
   SongShift.

Hello Atmos is a third party. Its matches are temporary convenience results,
not verified Neon provider state. Native Chordrift matching must retain exact
recording, storefront, timestamp, and evidence provenance.

Apple privacy exports do not require developer membership. Do not implement a
history parser from assumed examples; inspect the user's actual archive first,
then apply the same immutable, cumulative, PII-excluding principles used for
Spotify.

Neon remains the durable identity, provenance, history, and orchestration
ledger, while Spotify is the only active live provider for now. SongShift can
mirror every canonical Spotify playlist individually, so do not replace the
obsolete `Two Way Sync` playlist. Bootstrap the old Apple library once from the
two SongShift JSON exports rather than transferring it through temporary
Spotify playlists. SongShift remains a temporary publishing workaround, not an
authoritative native provider adapter.

The two 2026-08-24 exports are preserved under the Git-ignored
`data/apple/personal/bootstrap/` content-addressed archive. They contain 952
and 309 entries, 73 exact overlaps, and 1,185 unique Apple service IDs. Only 173
unique ISRCs are present, so do not create canonical tracks from loose fuzzy
matches. Automatically link unambiguous identities and stage ambiguous or
unmatched metadata for review.

Future native platforms are authoritative evidence for live user actions on
their respective surfaces. A deletion should create a provider-scoped
tombstone/override in Neon, not hard-delete the canonical track, history, or
provenance. Reconciliation must distinguish intentional user removal from
provider drift and prevent delete/re-add loops before propagating an action to
another platform.

The user approved the name **Excluded Tracks** for the provider-neutral view of
intentional removals. Only removal from a Chordrift-managed playlist after its
published state has been verified creates the account-level exclusion. Preserve
provider, time, prior canonical assignment, and restore history. Removals from
provider-curated, intake, transport, and legacy playlists are drift, not global
exclusions. Do not create a Spotify playlist for this internal view.

## Current roadmap and next task

Apple was removed from the critical release path. v0.0.5 is active on
`codex/embeddings`. The target representation is hybrid: a reusable pretrained
music-audio foundation vector plus an independently versioned account-specific
component. MERT v1 95M is the preferred first acoustic candidate; evaluate MuQ
as an alternative. Both published weight sets are currently non-commercial, so
revisit licensing before any commercial Chordrift use.

The acoustic models require waveforms. Do not download, scrape, or record
Spotify audio. Populate canonical acoustic embeddings only from lawful,
locally owned DRM-free audio. Spotify-only tracks receive a deterministic
semantic fallback from explicitly semantic playlist co-occurrence, artists,
albums, and historical playlist-name tokens. Keep listening behavior separate:
plays, recency, completion, skips, `On Repeat`, inbox state, and recommendation
provenance are preference/lifecycle signals for composition and ordering, not
musical-similarity dimensions.

Language and region are desired semantic dimensions, but Spotify does not
provide authoritative track language or origin. Do not equate availability
markets with origin and do not guess from titles. Plan provenance-aware
MusicBrainz enrichment for recording/release language, release country, and
artist area, retaining unknown values and confidence. Re-check Spotify's
current Platform policy before clustering ships. The intended operation is not
model training: independently resolve artist/title/ISRC, run a pretrained model
or import external semantic tags, and cache the inference with provenance,
model/version, confidence, and retrieval time. Spotify remains the sync and
user-action adapter.

Playlist policy has three distinct classes: provider-curated signal sources
(`On Repeat`, Daily Mix, prompted playlists), user-owned intake surfaces
(exact names: `Inbox`, `From Friends`, `Liked from Radio`, and `From Prompts`), and
Chordrift-managed canonical playlists. `Inbox` means a direct strong personal
discovery; `From Friends` means an explicit recommendation; `Liked from Radio`
means radio/autoplay discovery; `From Prompts` means a track intentionally
carried forward from a Spotify prompt-generated playlist. Canonical outputs use approved generated vibe
names and are never intake. The temporary Atmos companion is `Chordrift Spatial
Audio`.
Never clear provider-curated sources. Clear intake entries only after Neon
retains provenance and a published canonical Spotify destination is verified.
Do not feed Chordrift-managed output back into semantic training; use previous
assignments only as stability constraints.

The intended final Spotify surface contains the four intake playlists,
Spotify-managed sources, multiple Chordrift-managed canonical playlists with
approved generated names, and the temporary `Chordrift Spatial Audio`
companion. All other user-created legacy vibe and utility playlists are to be
retired only after their semantic evidence is consumed and every track has a
published, verified canonical destination. The user explicitly added
`Melodi(es)` and `Ambient Music Therapy – Indian Lounge - Relaxing Music for
your Six Senses` to that retirement set; both currently remain
`semantic_legacy` with weight 1.0. Retirement removes playlist containers, not
tracks. Spotify Liked Songs remains a provider library surface.

The user also wants followed/shared playlists owned by other people removed
from the visible Spotify and future Apple Music library surfaces. Treat these
as provider-neutral **External Playlist Bookmarks**, including externally owned
collaborative playlists. Before cleanup, retain provider ID, owner, link,
relationship, metadata, and an immutable last-known content snapshot when
accessible; explicitly mark inaccessible content. Bookmarks contribute no
semantic or behavioral signal, do not count as active canonical library
playlists, and are never legacy-retirement sources. A separately approved
cleanup removes only the user's provider-library relationship, never edits or
deletes the source owner's playlist. Neon keeps the bookmark for later
inspection. The first v0.0.9 slice adds `external_playlist_bookmarks`,
immutable pull-bound bookmark observations and track snapshots, plus
`chordrift bookmarks list|tracks`. The importer routes externally owned
collaborative and public followed playlists away from the active library,
copies unchanged readable collaborative contents from Neon, and retains
metadata-only public followed records under Development Mode. Private
Spotify-owned personalized surfaces remain active provider-curated signals.
Migration 0022 and the bookmark cleanup commands now provide immutable
all-present candidate review, explicit batch approval, and relationship-only
dry-run operations. Migration 0025 adds the targeted refresh described below.

Migrations 0009-0011 and the CLI keep canonical `track_embeddings`
separate from immutable account-scoped `embedding_generations` and
`account_track_embeddings`. New commands cover input audit, playlist semantic
weights, deterministic generation/status, and nearest-neighbor inspection.
The live Neon database is current at 11/11. `Collaboration Jessica ` is ignored;
`Liked from Radio` is discovery intake. Signal generation v2
`4fa57f0d-fce1-4c95-8d85-bba9d206afe2` covers 2,005 tracks: 1,554 history,
927 saved, 30 rotation, 102 discovery, 65 intake, and zero recommendation or
prompted tracks. Semantic audit finds 666 playlist-connected, 1,469
artist-related, and 1,015 album-related tracks.

The 128-dimensional diagnostic generation exposed obvious hash collisions. A
1,024-dimensional generation (`a33ef4ef-bd70-4375-9cc5-ca2f2ef59eb7`) embeds
1,733 of 2,005 tracks and produced materially cleaner inspected neighbors:
Nine Inch Nails remained with Nine Inch Nails/Trent Reznor, and the spurious
A. R. Rahman collision disappeared. The code default is now 1,024. Treat this
as an inspectable semantic fallback, not the final acoustic representation or
authorization to publish/modify playlists.

## Verification and release discipline

Before committing a code change, run checks proportional to the change. The
normal full baseline is:

```console
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo test --doc
cargo package
```

Postgres integration tests require the disposable test database environment
documented in the repository. Apply migrations to live Neon only after local
and disposable-database verification. Keep the task-oriented
`docs/HOW_TO_CHORDRIFT.md`, focused `docs/how-to/` pages, and
`docs/reference/CLI_COMMANDS.md` synchronized with CLI changes;
`tests/user_docs.rs` enforces aggregate leaf-command coverage.

For a release: confirm CI is green, tag the exact tested commit, create the
GitHub release, publish to crates.io, install the released version, fast-forward
`$CRATES/chordrift`, and verify the installed CLI against Neon. Never expose
credentials in command output, commits, release notes, or this handoff.

## Handoff maintenance

At the end of each focused task:

1. Update the date and any changed decisions, versions, branches, migrations,
   operational checkpoints, and next action.
2. Remove stale claims rather than appending a chronological transcript.
3. Link detailed permanent documentation instead of duplicating it here.
4. Confirm the handoff contains no secrets or unnecessary personal data.
5. Leave the active branch and working tree state explicit for the next task.

v0.0.5 was merged at `ffbcc40`, tagged, released on GitHub, published to
crates.io, installed locally, verified against healthy Neon, and fast-forwarded
into `$CRATES/chordrift`. Active development is on
`codex/semantic-enrichment` for v0.0.6; PR #2 is open. Migration 0012 is live
and CI run `32765541591` passed. The bounded MusicBrainz adapter caches raw ISRC
and recording-detail responses separately, respects the one-request-per-second
limit, and persists conservative match/fact provenance. A live high-priority
probe matched M83 and retained 35 useful genre/tag/release facts. Pending
requests are prioritized by intake, rotation, saved state, then meaningful
plays; these values affect scheduling only, never similarity. The separate
cache-first `enrich artists` operation retains MusicBrainz
primary-associated-area evidence without claiming birthplace, nationality, or
track language. CI run `32767604600` passed, including migration 0013 on
disposable PostgreSQL 18; migration 0013 is live and Neon is healthy at 13/13.
A three-artist probe considered M83, Reinoud Ford, and Keaton Henson: two
primary areas resolved, one transient request was cached for later retry, and
two track-level facts were written. An immediate repeat converged with zero
artists, requests, or writes. Current live coverage is 2 matched tracks, 39
MusicBrainz facts, and 2 tracks with artist-area facts. Pretrained mood/sound
inference is next; Excluded Tracks remains future work.

The pretrained-audio boundary is model-neutral and requires authorized local
audio; Chordrift will not download or infer from Spotify audio. Migration 0014,
`enrich model-import`, `enrich model-status`, and the strict path-free
`docs/model-inference-v1.schema.json` are implemented. Artifacts pin model
name/version/revision/license, input hashes, inference time, aggregation,
embeddings, and mood/sound facts. MERT and MuQ-MuLan are candidate foundation
spaces and Essentia provides explicit classifiers, but the evaluated weights
carry non-commercial terms and all require real audio. Tracks without lawful
audio remain unembedded rather than receiving invented acoustic evidence. CI
run `32769042155` passed; migration 0014 is live and Neon is healthy at 14/14.
The live model status correctly reports 2,005 eligible tracks and zero imported
inferences, embeddings, facts, or models because no authorized local audio has
been supplied.

Manual correction is explicitly post-generation: after proposed playlists
exist, the user must be able to reject a track's current vibe and optionally
choose or lock another destination. The next generation moves it and retains
that account-scoped decision as an auditable stability constraint while
preserving the original model score and assignment history. Do not implement
this as a free-floating pre-clustering mood tag; wait for stable cluster and
playlist identities.

The semantic fallback model is now `semantic-feature-hash@3`; its immutable
input includes MusicBrainz facts, imported model facts, and
deterministically projected lawful acoustic vectors in addition to legacy
playlist, artist, album, and historical-name evidence. Source/parser and
model/version identities are recorded in generation parameters; behavior
remains excluded. Migration 0015 and the `clusters generate/status/list/tracks`
commands are implemented using deterministic spherical k-means, an exact
embedding-generation input, explicit low-similarity/undersized unassignment,
idempotent generation hashes, and temporary content-derived machine labels.
Cluster output is diagnostic and cannot create or modify Spotify playlists.

CI runs `32770150613` and `32770941996` passed. Migration 0015 is live and Neon
is healthy at 15/15. Live embedding generation
`f0c8eda3-ad34-41b9-a362-2fb56354bb95` is model v3, 1,024 dimensions, and covers
1,733 tracks. The first all-track centroid fit exposed bad 2–3-track groups and
a 650-track catch-all, so it was superseded by semantic-seeded algorithm v2.
The current diagnostic generation `8ec8512f-66fc-4f59-a50e-65d5b7ac8d13`
contains 12 clusters of 30–251 tracks and leaves 895 weakly supported tracks
unassigned. An identical command reused the generation. Samples show coherent
M83 and A. R. Rahman groups, while a generic legacy-playlist cluster still has
many equal scores; do not mistake this sparse-evidence fallback for final
acoustic classification or publish its machine labels. More independent
semantic/acoustic coverage is still needed before these clusters should be
published.

v0.0.7 proposal work is implemented on `codex/semantic-enrichment`. Migrations
0016 through 0018 are live and Neon is healthy at 18/18. Migration 0017 provides
the latest-snapshot-only Spotify playlist surface. The `proposals` commands
provide a non-destructive workflow with stable `playlist-*` concepts, overlap-based
lineage, strict naming artifacts, complete generator/hash provenance, and
explicit approval. The first live proposal is
`ca81d1b2-e56b-41e6-8846-cdb379cb039b`, derived from cluster generation
`8ec8512f-66fc-4f59-a50e-65d5b7ac8d13`. It contains 12 playlists and 838
assigned tracks. All 12 candidate names were imported as an OpenAI Codex GPT-5
naming revision. Two additional manual categories, `Open-Sky Anthems` and
`Weightless Horizons`, were created and 46 initially uncovered tracks were
reviewed into stable destinations. The proposal now contains 14 named playlists
and represents all 699 of 699 legacy/intake tracks; `proposals missing` is
empty. The account owner explicitly approved generation
`ca81d1b2-e56b-41e6-8846-cdb379cb039b`; Neon reports `approved`, 14/14 named,
and complete coverage. No Spotify state was changed.
Migration 0018 and `proposals category-create/assign/review` add
stable manual semantic destinations, reversible active decisions, complete
revision history, a non-provider needs-review state, and replay into later
proposal generations.

v0.0.8 adds migration 0019 and `sync plan` / `sync plan-show`. A plan is an
immutable Neon audit record bound to an exact approved proposal and exact
imported Spotify snapshot. Identical inputs reuse the same plan. Operations are
ordered into publish, reconcile, cleanup, and retirement phases. Inbox cleanup
and legacy retirement are deferred behind publication/verification gates, and
retirement additionally requires separate future approval. The planner makes
no Spotify request and Spotify write scopes remain disabled. Migration 0019
also introduces stable concept mappings for future published provider
playlists and the provider-neutral reversible `excluded_tracks` ledger.
Migrations 0019 through 0025 are live and Neon is healthy at 25/25. Migration 0020
adds immutable successful managed-playlist baselines so a later missing
expected track becomes an internal `exclude_track` proposal rather than an
automatic re-add; an unexpected extra remains ordinary provider drift. The
current planner is `spotify-dry-run-v5`; earlier development plans remain
immutable audit artifacts and must not be applied. The verified v4 plan is
`cda2639d-da67-4b23-9492-a9274c71088c`, bound to approved proposal
`ca81d1b2-e56b-41e6-8846-cdb379cb039b` and Spotify snapshot
`622a94b4-b60e-4f26-8da2-20e540e160c1`. It contains 1,007 exact operations:
16 creates (14 canonical plus missing `Inbox` and `From Friends`), 884 ordered
track additions, 65 deferred intake removals, and 42 separately approved
legacy retirements; no renames, restorations, or exclusions before initial
publication. `Liked from Radio` already exists and is reused. Every inspected
retirement has complete preservation. The current v5 plan is
`68ee490c-f5f4-4e23-9a48-7f4933cd6511`, bound to Spotify snapshot
`c187fc99-5e7c-42f7-a694-86bcb9d1930b`. It contains the same 1,007 canonical
operations plus 13 separately approved `remove_external_playlist` operations,
for 1,020 total. Cleanup batch `016defcd-f46b-4070-991d-73cb4c89f00a`
captures and approves all 13 present external bookmarks with input hash
`8528685a4f488784acd5a9381d183a7795485547714981cc3d5eb25006cfaa12`.
Repeated v5 planning reuses the plan exactly. v0.0.9 apply-readiness validation
is now implemented and remains read-only against Spotify.
That milestone also generates one simple original deterministic cover per
canonical playlist from its approved name/description/tags, stores generator
version and SHA-256, produces a contact-sheet-style preview, and requires
explicit artwork approval. Do not request Spotify image-upload scope or upload
covers until v0.1.0.

Artwork implementation is now complete in source as migration 0023,
`src/artwork.rs`, and `chordrift artwork import|status|list|approve`. The
approved Drift Atlas v1 set contains 14 original 1254×1254 PNGs in
`artwork/canonical/drift-atlas-v1`, with strict `manifest.json` provenance and
`contact-sheet.png`. The user approved the 13 original candidates, requested a
darker replacement for #8 Open-Sky Anthems, then explicitly approved that
replacement and the complete set. Migration 0023 is live and Neon is healthy
at 23/23. Approved artwork batch
`450e2e83-37d5-4100-99b7-cef4a56240f5` is bound to proposal
`ca81d1b2-e56b-41e6-8846-cdb379cb039b`, contains 14 verified covers, and has
input hash `c5e295d0914f1ee8d386fcf4f7ca297e2811449cb84acbe30287afddd8d7714a`.
Re-importing the unchanged manifest reuses that exact approved batch. Artwork
approval is local/Neon-only and must not request Spotify image-upload scope or
upload covers before v0.1.0.

Migration 0024, `src/apply_readiness.rs`, and
`chordrift sync readiness|readiness-show` now persist an immutable safety
assessment for one exact plan. Live assessment
`7cedca9e-ed2b-4ddb-baca-f2a701db531c` is bound to plan
`68ee490c-f5f4-4e23-9a48-7f4933cd6511` and input hash
`575c5971219bbfc8bb3f1f8471833fadc8e19abdb16997a4d5d3d5feed0f8e91`.
It is `ready`: 10/10 checks passed across 1,020 operations, five simulated
restart checkpoints recovered all operations, and replay produced zero
changes. The one-request live probe confirmed only
`playlist-read-private`, `playlist-read-collaborative`, and
`user-library-read`; no modify or image-upload scope is granted. All 120
destructive operations remain deferred, and `spotify_writes: disabled`.

Migration 0021 established the bookmark inventory. Two consecutive
read-only pulls produced snapshots `6544a59b-c6e7-4ec0-92d8-3129132bb449`
and `c187fc99-5e7c-42f7-a694-86bcb9d1930b`: both saw 62 Spotify playlists,
kept 49 active, and retained 13 external bookmarks with no external item
requests. All 49 active playlists and 927 saved tracks reused Neon on the
second pull. `alone in the car` is bookmark Spotify ID
`1128mckrHSNSNt3PzyE4Bp`, owner `trinwoodward`, 52 reported items, status
`metadata_only`; its `last_changed_at` remained stable across the repeated
pull. It is absent from `chordrift playlists list`. The 13 followed public
bookmarks are metadata-only because Spotify Development Mode does not expose
their contents to this app; `bookmarks tracks` reports that honestly.

Migration 0025 and `chordrift bookmarks refresh` add explicit, targeted
refresh for exactly one present or archived bookmark. Refresh attempts and any
readable ordered track metadata are immutable and separate from provider
library snapshots, so they neither stale the normal sync baseline nor increase
its request budget. Spotify's February 2026 API permits playlist items only for
owned/collaborative playlists; followed public shared lists will usually record
a 403 `inaccessible` attempt while retaining their bookmark metadata and any
older readable contents. The intended workflow is: follow/save the shared list
in Spotify, pull once to bookmark it, selectively listen in Spotify, add chosen
songs to `Inbox` or `From Friends`, then run Chordrift normally. Bookmark tracks
never become semantic inputs automatically.

v0.0.9 was released on 2026-08-24 from merged `main` commit
`6580ce8f5874f1c607f0e759484d6acb80979b8d`: crates.io publication, annotated
Git tag `v0.0.9`, and the GitHub release all succeeded. Hosted CI run
`32788740035` passed formatting, clippy, all ordinary and documentation tests,
both disposable-PostgreSQL integration suites, and package verification.
Repository artwork is intentionally excluded from the crates.io archive
because the 14 approved full-resolution PNGs are review/publication assets
rather than runtime data; they remain in Git.

v0.1.0 work is on `codex/v010-spotify-apply`. Migration 0026, planner v6,
readiness v2, and `src/apply.rs` introduce the first provider-write path. Apply
is phase-scoped, requires exact assessment confirmation, persists every
operation and resolved Spotify target, batches at current API limits, resumes
against live membership, uploads only approved artwork, and stops at
`awaiting_pull`. The next pull verifies exact ordered canonical membership and
records immutable managed baselines. Cleanup/retirement additionally require
`--allow-destructive`; retirement also requires exact-plan durable approval.
No v0.1.0 live Spotify mutation has occurred yet. The existing stored OAuth
credential is read-only and must be explicitly reauthorized for the seven
documented v0.1.0 scopes before a new v6 readiness assessment can pass.
Migration 0026 is live and Neon is healthy at 26/26. Current v6 plan
`e89854e8-c1dc-42fc-b469-b7e113fcd831` is bound to snapshot
`c187fc99-5e7c-42f7-a694-86bcb9d1930b` and input hash
`520fa5b82c70fccfa3024927ad568ced0594732cecec2c0c415f2689780e7793`.
It is current and contains 1,034 operations: 16 creates, 884 additions, 14
approved artwork uploads, 65 deferred intake removals, 13 deferred external
relationship removals, and 42 deferred legacy retirements. No Spotify request
was made while creating or inspecting it.

`chordrift sync apply-preflight` now validates an exact current v6 publish plan
without contacting Spotify. The live preflight for plan
`e89854e8-c1dc-42fc-b469-b7e113fcd831` passed: all 14 approved source hashes,
PNG decodes, and JPEG conversions are valid; the largest base64 JPEG is 221,788
bytes, below Spotify's 256 KB limit. Publishing will create 16 containers,
populate 14 canonical playlists with 884 ordered memberships through 17 item
writes, and upload 14 covers. The estimated publish budget is 15 Spotify reads
and 47 writes. The preflight made zero Spotify requests. Local verification is
clean at 72 passing library tests plus the CLI/docs tests and warning-free
clippy. Hosted CI run `32791199258` completed every job step successfully,
including both disposable-PostgreSQL suites and package verification; confirm
the final GitHub status after the follow-up preflight commit is pushed.

The user completed v0.1.0 Spotify reauthorization for account `personal` on
2026-08-24. Account identity `5DPKF9q1Xm` (`suhails`) matches Neon, and the
system-keychain credential now has all seven required read, playlist-modify,
library-modify, and image-upload scopes. Read-only readiness assessment
`16c6c402-9f82-4179-8f18-f9cc24912dc9` is `ready` for exact plan
`e89854e8-c1dc-42fc-b469-b7e113fcd831`: 10/10 gates passed, all 1,034
operations recovered across five simulated restart checkpoints, and replay
produced zero changes. No Spotify write has occurred. Publishing now requires
the user to explicitly confirm that exact assessment ID before running
`sync apply --phase publish`; do not infer authorization for cleanup or
retirement from publication approval.

The user explicitly approved publication. Apply run
`35db437b-f348-434d-8402-ddde1ecb3eb8` executed all 914 publish operations
(16 creates, 884 memberships, 14 covers) with zero failures, then the pull
committed snapshot `98ec0798-c946-4b0b-bd9d-5dbf2fe64679` and reported
`verified_apply_runs: 1`. The first verification pull exposed and safely rolled
back a canonical semantic-weight constraint mismatch; no snapshot was partially
committed. The corrected importer sets canonical semantic weight to zero.
A second pre-cleanup audit caught newly created empty `Inbox` and `From Friends`
being imported with the default legacy policy. The importer now recovers intake
policy from the succeeded create operation. Read-only snapshot
`2ce8e24f-e88b-4051-8927-3501c65edc34` confirms both are protected `inbox /
provider_wins / intake / after_verified_assignment` surfaces. Current plan
`74caa6d4-8cee-40d1-a507-f8141dff5799` contains zero creates/additions, 65
deferred `Liked from Radio` removals, 13 deferred external relationship removals,
and the original 42 legacy retirements. No cleanup or retirement approval has
been inferred from publication.

The user explicitly approved cleanup. The first attempt was safely blocked
before writes because a repeated pull had a newer snapshot without a carried
verification baseline. `verify_pending_publications` now recomputes canonical
proof on every pull rather than trusting or blindly copying an older baseline.
Cleanup apply run `20f5f69c-f74a-464e-a9af-fd9643556718` then completed all 78
operations: 65 `Liked from Radio` removals plus 13 external relationship
removals, with zero failures. Spotify's playlist index briefly returned the old
snapshot marker for `Liked from Radio`; the following read-only pull observed
the new marker and exact 65-entry decrease. Destructive apply runs are now also
marked succeeded only when a later imported snapshot proves every planned track
and relationship absent. Snapshot `b9e8d29e-b409-4de4-802b-7e77f78c1d85`
reports 65 active playlists, 2,309 entries, zero followed/external playlists,
and `verified_apply_runs: 1`. All 13 external bookmark records remain in Neon as
not-present history. No legacy retirement has occurred.

The user then explicitly approved exact retirement plan
`f7c926c3-26f7-4adc-ad69-4e40d62fbf0f`. Apply run
`f767f050-ddff-4758-9a1c-6085eb9cff27` removed all 42 legacy relationships with
zero failures; snapshot `e38b7c81-9513-4d98-9d9d-9ecc73575d69` proves the live
playlist count fell from 65 to 23 and reports `verified_apply_runs: 1`. The
post-retirement audit found four documented obsolete utilities still present
because ignored/transport classes were unintentionally omitted from planner
retirement. Planner cleanup now includes `ignored` and `transport` sources in
the separately approved retirement phase while intake remains protected. Exact
plan `fa8289fc-d636-448c-8203-a8bd1ca67ae6` contains only four retirements:
`All my saved songs`, `Collaboration Jessica`, `My top tracks playlist`, and
`Two Way Sync` (plus 14 non-destructive artwork operations in publish). These
four have not been approved or removed yet.

The user explicitly approved the four utility retirements. Apply run
`ffe42fb9-7fe9-40c9-bd97-ef9e468bb9ca` completed 4/4 with zero failures, and
snapshot `5280abe1-7220-4cbb-8e9c-9acf7ef72121` verified the final live surface:
19 playlists, 951 entries, 902 unique playlist tracks, zero duplicate entries,
zero followed/external playlists, and `verified_apply_runs: 1`. The 19 are
exactly 14 canonical Chordrift playlists, `Inbox`, `From Friends`, `Liked from
Radio`, `Daily Mix`, and `On Repeat`. Current plan
`c64d615a-1fd8-4c80-afc9-08d82a42b58d` has zero creates, additions, removals,
retirements, external cleanups, or deferred destructive operations; its only 14
operations are idempotent approved artwork uploads. v0.1.0 release metadata is
being prepared; do not claim the crate/tag/release exists until publication is
verified.

v0.1.0 was released on 2026-08-24 from merged `main` commit
`2968af5`: PR #3 merged with full audited history, hosted CI run `32795061524`
passed formatting, warning-free clippy, all ordinary and documentation tests,
both disposable-PostgreSQL integration suites, credential persistence, and
package verification. `chordrift 0.1.0` is published on crates.io. Annotated Git
tag `v0.1.0` was pushed, and the non-draft, non-prerelease GitHub release is
available at `https://github.com/orbyts/chordrift/releases/tag/v0.1.0`.

Apogee configures a machine-wide shared Cargo target. Because the released
`$CRATES/chordrift` clone and this development workspace currently share the
same package name/version, a plain `cargo run` reused an older final executable
that lacked `proposals`. For unreleased development commands, use a repository-
specific target such as `cargo run --target-dir target -- ...`; do not modify
shell initialization files.

## v0.1.1 task state

Work is on `codex/v0.1.1-track-inspection-artwork`. The task adds
`chordrift tracks inspect --name TITLE [--artist ARTIST]` and
`--spotify-id ID` as a single explainability report spanning current Spotify
placement, approved canonical destination and provenance, all retained source
playlist observations, listening/lifecycle signals, embedding generation and
dimensions, cluster similarity/rank, independent semantic model facts, manual
assignment reasons, and active exclusions.

Migration 0027 allows approved artwork artifacts to target canonical or intake
surfaces, including an intake that has not been created yet. Planner v7 adds
`From Prompts` with prompted-interest semantics and suppresses an artwork upload
when the exact content-addressed operation already succeeded against the same
stable Spotify playlist ID. Apply target resolution can bind a newly created
intake cover by its unique planned name.

Drift Atlas v3 lives at `artwork/canonical/drift-atlas-v3`: 14 previously
approved backgrounds remain visually unchanged and four intake backgrounds
cover `Inbox`, `From Friends`, `Liked from Radio`, and `From Prompts`. All 18
pristine label-free masters remain in the `backgrounds/` child directory for a
future Apple Music typography pass. `scripts/render_artwork_label.swift`
performs the exact 1254×1254 CoreText overlay; AI-generated text is never used.
At user review, Helvetica Neue Bold increased to Spotify-like 116/132-point
labels measured and anchored 42 pixels from the lower edge. The schema-2 v3
manifest contains exact hashes and provenance summaries.

The v0.1.0 preservation gate was discovered to be too narrow: it counted only
699 current semantic-legacy/intake tracks, while the durable account inventory
contains 1,711 distinct tracks. Migration 0028 is live in Neon (28/28 healthy)
and defines the account-scoped preservation universe as latest saved tracks plus
all historical semantic-legacy, transport, intake, and canonical membership,
with active reversible exclusions as the only alternate disposition. Raw
listening history and provider-curated playlists remain enrichment signals and
do not enlarge the library. Readiness v4 dynamically requires exactly one
disposition per inventory track, so an older stored coverage flag cannot bypass
the corrected invariant for this or any future account.

The current editable proposal is `fcfaccc7-e17d-4dee-a54c-65a73000fcc2`.
It preserves all 14 approved concepts, names, descriptions, tags, artwork
identities, and the original 884 placements, then adds the missing inventory
through direct centroid fit, analytical-cluster group consensus, listening-
session context, and explicit account-scoped manual decisions. Exact live Neon
audit: 1,711 inventory, 1,711 placed, 0 excluded, 0 unresolved, 0 conflicting.
Source-class audits are also complete: 927/927 saved, 674/674 semantic legacy,
143/143 transport, 65/65 intake, and 884/884 prior canonical tracks. The
proposal is fully named and `coverage_complete=true`. The user approved and
published this exact generation; its 1,711-track managed baseline is verified
against Spotify.

Embedding generation `baf6d7af-0333-461b-a72d-7392e315357f` is model
`semantic-feature-hash@4`, 1,024 dimensions, and embeds 1,688/1,711 inventory
tracks. v4 adds normalized 45-minute meaningful-listening-session
co-occurrence; 1,159 tracks share session context and the unembedded tail fell
from 173 to 23. Analytical cluster generation
`180b4b87-fbff-4c42-90ce-76b853550f2a` has 18 groups and zero cluster-level
unassigned embedded tracks. Group consensus required at least 10 known members
and 55% destination dominance; every automated membership records its exact
embedding/cluster generation, score, counts, and threshold.

`tracks inspect` now reads the newest proposal as well as approved/published
state. `Do Your Best` by John Maus is explicitly assigned to `Neon Affection`
with the reason that its lo-fi synth-pop/nocturnal post-punk character is a
better fit than its borderline 0.0504 Open-Sky centroid result. The report also
shows 64 meaningful plays, 72 events, 12 skips, 2.79 hours, the v4 embedding,
analytical cluster, retired `Two Way Sync` provenance, and the manual override.

New inventory/repair commands are documented and enforced by the CLI-doc test:
`proposals inventory`, `unresolved`, `placement-audit`, `extend`,
`group-tracks`, `consensus-assign`, and `centroid-assign`. The pending Drift
Atlas v2 batch `f1430424-8c71-4210-86cb-07adf4eb17ff` targets the old proposal
`ca81d1b2-e56b-41e6-8846-cdb379cb039b`; do not approve it. The local manifest
is rebound to approved complete proposal
`fcfaccc7-e17d-4dee-a54c-65a73000fcc2` without changing any of the 18 image
bytes or hashes. Its replacement immutable artwork batch is
`6587f24a-999d-4b88-a97b-2a1bfe49c425`, is `approved`, contains 18 artifacts,
and has input hash
`f9151ca22c887456abfdc4fe02720f1ca6db2ba88dd388b2cbd63017c033a1c9`.
The user approved it at `2026-08-25T02:46:01.285564+00:00`; none of its image
bytes changed after approval. It was published as the initial v2 surface
and was later superseded by the approved v3 typography pass described below.

Historical complete-library v7 dry-run plan
`67e0b557-126e-4a60-ba11-676caffe85ff` targets approved proposal
`fcfaccc7-e17d-4dee-a54c-65a73000fcc2` and current source snapshot
`cf361d5b-1f9c-4ca8-8ec3-d716d8351283`. It contains 846 wholly
non-destructive publish operations: one intake create, 827 additions, and 18
artwork uploads, with zero removals, retirements, exclusions, external cleanup,
or deferred operations. Offline preflight passed: 13 populated playlists, 18
batched item writes, all 18 covers valid, largest converted cover 224,456
bytes, estimated 14 Spotify reads and 37 writes, and zero Spotify requests
made. Plan input hash is
`6d089c22cec40a674c249e0a9c48c9c53217457c369188dac4af431292a101d9`.

The user reauthorized the unreleased binary for account `5DPKF9q1Xm`
(`suhails`) with all seven required scopes. Membership apply run
`af9e3265-a0ce-486f-94df-4a0cc3256414` executed 846/846 operations with zero
failures and is verified `succeeded`: one `From Prompts` intake create, 827
membership additions, and 18 v2 cover uploads. Import snapshot
`9a9a4fb6-097a-4315-9938-385605a46dc8` established 20 owned playlists, 1,778
entries, 1,727 unique playlist tracks, zero duplicate entries, and the exact
1,711-track canonical baseline.

Two post-publication verifier defects were fixed in `src/apply.rs`: approved
empty canonical playlists must participate in desired-state comparison, and
sparse proposal ordering keys must be compared as ordered track-ID sequences
rather than raw positions because Spotify densifies positions from zero. The
first complete verification exposed both safely; no membership was lost or
rewritten while diagnosing them.

Drift Atlas v3 batch `776ae100-f16b-477d-838d-8b90cfda9e6e` is approved with
input hash `06de03171089f7c3dba0116709a275090850a082af01d29d31e9722420862ae0`.
Cover-only plan `d4b78b32-9f82-4717-9891-0d93b4855879` passed preflight and
11/11 readiness gates. Apply run `7031e284-a9e9-4b01-963c-84a735d36d46`
uploaded 18/18 larger lower-anchored covers with zero failures and is verified
`succeeded` against snapshot `3cc2d1c0-6f1b-42dc-8cff-2b6e9b952567`.
Planner artwork selection now uses only the newest approved batch, preventing
stale approved revisions from entering a later plan.

`chordrift artwork update --account personal --playlist NAME_OR_STABLE_KEY`
now builds a focused immutable one-cover plan from the newest approved batch.
It refuses missing, ambiguous, unresolved, or already-uploaded artifacts; the
existing preflight/readiness/apply flow remains the provider-write boundary.
Spotify playlist folders and folder covers are unavailable through the Web API
and remain manual, provider-controlled presentation state.

The earlier repair verification was clean and the live Spotify membership and
v3 artwork publications described above completed successfully. The final
v0.1.1 release verification is recorded below.

## v0.1.1 final working model

Normal `sync pull` now incrementally retains Spotify Recently Played events
after a durable Neon cursor. These API observations are provisional because
Spotify does not supply playback duration, completion, or skip facts; a later
cumulative Extended Streaming History import supersedes overlapping API rows
before rebuilding listening statistics. Migrations `0029` through `0031` add
the incremental-history cursor/source model, protected `user_managed` playlist
defaults, and exact playlist-order replacement support.

Spotify OAuth uses one consolidated PKCE consent. Chordrift no longer rewrites
an unchanged Keychain credential on every command; the earlier repeated prompts
were caused by an unconditional credential write compounded by changing unsigned
debug binaries. A stable signed build remains the correct friend-test delivery.

User-owned playlists now default to protected and retirement defaults to none.
`chordrift playlists retirement` can include exact names, select all with
explicit exceptions, or reset to none. This changes Neon intent only; complete
coverage, immutable-plan inspection, exact approval, readiness, and
`--allow-destructive` remain mandatory before a provider write.

Live v0.1.1 convergence completed on 2026-08-24. Publish apply
`2496d685-eb6f-485f-a6ec-0b7d19705290` repaired the exact order of three
canonical playlists. Retirement plan `558a6cc7-2be8-4519-9208-f048a759430b`
and assessment `72f7ed23-88d2-4afa-ba55-bd276baac506` passed 11/11 checks and
retired only the explicitly approved user-owned duplicate `On Repeat`
(`0z02mUNjp2VHfZIjt7Iuhm`). The post-retirement snapshot
`08469f65-6095-430c-84b7-281d8725aa02` contains 19 playlists, 1,752 entries,
1,727 unique playlist tracks, 927 saved tracks, and zero duplicate entries.
The unique-track count stayed at 1,727 while 30 duplicate playlist entries
disappeared, proving no song was lost.

Listening history is current through `2026-08-25T04:46:07.125Z`: 149,249
retained events, 15,563 unique historical tracks, and 6,602.55 listening hours.
The complete-library readiness gate covers all 1,715 preserved inventory tracks
with zero unresolved or conflicting dispositions.

Final local release verification: 77 ordinary library tests pass, one
PostgreSQL-only library test is expectedly ignored locally, the user-doc
coverage test passes, formatting is clean, clippy is warning-free across all
targets and features with `-D warnings`, and `cargo package --allow-dirty`
successfully verifies 80 packaged files. The live disposable-PostgreSQL suites
remain hosted-CI responsibilities.

## v0.1.2 listening-review decision

The next milestone is an ongoing listening-review loop, not a fresh global
recluster. The user has already noticed placements that may be acoustically
plausible but personally wrong—for example, some A. R. Rahman or other South
Asian recordings inside `Tidal Hush`. Preserve four distinct intents: reject a
destination, prefer/lock another destination, hold for review, or exclude from
active Chordrift playlists. Prior assignments and scores remain auditable.

Build a first-class review session over a cloned approved generation, with
ranked alternatives, batch correction, coherent new-playlist promotion, exact
diff approval, normal sync readiness/apply, and post-pull convergence. Keep
track-specific feedback as a hard constraint. Region, culture, language,
soundtrack context, instrumentation, and mood are separate facets; never infer
a universal placement rule from an artist name or one correction. Repeated
consistent corrections may produce an evidence-backed rule suggestion that
still requires approval.

Direct edits to Chordrift-managed Spotify playlists should be staged as
possible feedback by comparing them with the last verified baseline. A remove
plus add suggests a move; remove-only is ambiguous among wrong vibe, review,
and exclusion; an addition suggests destination preference; reorder is an
ordering-policy question. Never silently learn or reverse ambiguous edits.
This allows Spotify to remain the familiar consumer editing surface while
Chordrift acts as the preservation-first assistant and Neon remains the durable
ledger. The complete design is in the v0.1.2 section of `ROADMAP.md`.

The user clarified that correction capture happens during ordinary listening,
not in a dedicated review session. v0.1.2 therefore separates capture from
reconciliation. The stable action intents are Refile (keep, wrong destination),
Review (keep, undecided), and Exclude (proposed reversible removal). Optional
Spotify routing queues such as `Route — South Indian`, `Route — North Indian`,
and `Route — Decide Later` provide the lowest-friction mobile action. Adding a
track is sufficient; Chordrift records it on pull, publishes a verified
destination before removing the rejected membership, and clears the queue only
after convergence. Queue descriptions are semantic data, not commands.

Concrete motivating example: `Tidal Hush` positions 28–33. The working North
Indian/Hindi cohort is `Chhodo More Baiyyan`, `Ni Main Samajh Gayee`, and
`Kahin Aag Lage`; the working South Indian cohort is Tamil tracks `Kandukondain
Kandukondain` and `Senkathay`, plus Telugu `Thongi Choose`. These are routing
labels; user-approved poetic playlist names and artwork can follow later.

## v0.1.2 routing-playlist subslice

Branch `codex/v0.1.2-listening-review` now contains migrations `0032`–`0033`, a generic
`chordrift routes` CLI, zero-signal route capture during Spotify import, v9
publish planning, route artwork operations, and exact route verification after
pull. A route is a transient corrective inbox: non-empty means pending work;
it must be cleared only after an existing or newly created canonical destination
has been published and verified. Route descriptions are retained semantic
policy data, not executable instructions.

Approved initial routes are `Route — South Indian`, `Route — North Indian`,
and `Route — Decide Later`. Their label-free masters and deterministic Spotify
overlays live under `artwork/routing/route-signals-v1/`. The two regional covers
are intentionally minimal representational instrument studies (veena and
sarod); Decide Later is a minimal junction. This is not a fixed visual template:
future routes need artwork designed for their own meaning and should retain
both the pristine master and provider-specific label render.

Initial desired memberships from `Tidal Hush` are North/Hindi Spotify IDs
`4izSWLwW0wQohWBuEUKL5J`, `7wJphnipgpptcsRk6Aur3w`, and
`3AemMBXKJWFd87svnFyrHy`; South Indian IDs are
`42lDp1YYCiy50UtXUO9FNp`, `1VdBV90HgsUkjdKo95qnLf`, and
`2m7cVrIHAfJmZhCCOZ91qT`. They remain in their current canonical playlist until
the later review/reconciliation step establishes verified poetic destinations.

The first post-publication pull exposed and atomically rolled back an older
clear-policy constraint; migration `0033` now permits verified clearing for
`routing` as well as `intake`. The route snapshots then imported exactly. A
second verification refinement scopes each apply run to the surfaces it
actually touched, so a route-only publication proves route convergence without
being coupled to unrelated canonical-baseline verification.

Live routing publication completed on 2026-08-24 through plan
`3c0268b2-02a5-4148-b1ae-c74f82f6fa0e`, readiness assessment
`a3441c60-bfc5-4b45-8ab8-4c99b371087c` (11/11 checks), and apply run
`e50996f4-a25c-4b42-b12b-b96a73c91447` (12/12 operations succeeded and
post-pull verified). Spotify IDs are `0PrQn0SnXQ7azykCR0Y6PW` for Decide Later,
`11qUBVVOuiKeR4uD6RdO46` for North Indian, and
`04DgVypEVHs0fkmoMBLkgB` for South Indian. Snapshot
`75c91908-b155-49ec-87e4-76a677fe9ca3` reuses all 22 playlist bodies and proves
1,758 entries, 1,727 unique tracks, and zero duplicate entries. The unique
universe is unchanged; the six route entries are preserved review duplicates.
The subsequent v9 plan `88b31b13-f1c8-4a24-960b-6e68ca64a350` contains zero
operations, proving route creation, exact order, and artwork publication are
idempotently converged.

The user documentation is now split by intent. `docs/HOW_TO_CHORDRIFT.md` is an
80-line entry point and table of contents; the former comprehensive guide is
preserved as `docs/reference/CLI_COMMANDS.md`. Focused pages under
`docs/how-to/` cover adding/discovery, deleting/excluding,
routing/reclassification, and sync/convergence. The product-facing inference
rules live in `docs/design/PLATFORM_INTENT_MODEL.md`: high-confidence provider
actions may be captured automatically, ambiguous actions must be staged, and
destructive interpretations are never silently inferred.

## v0.1.2 incremental-sync and terminal subslice

A live pull after one saved-track deletion changed Spotify's reported total
from 928 to 927. Spotify retrieval completed, but persistence remained silent
for roughly six minutes because the importer sequentially rewrote existing
track, album, and artist metadata and inserted each saved membership through a
remote Neon round trip. The pull eventually succeeded as snapshot
`c3c4cbba-7c33-4c52-9573-cbfa9b5cbfac`: 22 playlists, 1,766 playlist entries,
927 provider saved items, 926 supported saved tracks, one unsupported item, and
two new Recently Played observations.

The branch now preloads known Spotify track rows once, compares the retained
JSON payload, skips metadata writes for identical records, batches saved-track
snapshot membership in groups of 1,000, and updates `last_seen_at` for observed
provider rows with one set-based statement. The full persistence transaction
and unsupported-item accounting remain intact. `indicatif` provides TTY-only
progress bars with plain redirected fallbacks. `comfy-table` provides compact
colored interactive tables for `playlists list` and `playlists tracks`; the
existing complete TSV remains unchanged when stdout is redirected. An actual
80-column Neon-backed rendering check led to a four-column stacked playlist
layout instead of an unreadable ten-column grid.

The optimized branch was labeled and locally installed on 2026-08-25 as
`chordrift 0.1.2-dev.1`. This is an intentionally truthful prerelease: deletion
to staged exclusion, intake capture, routing capture, progress bars, and compact
playlist/song tables work, but route reconciliation is not release-complete.
Until it is, a wrong-destination track must be added to a route without also
being removed from its canonical playlist; canonical removal currently means
exclusion. Different tracks may carry deletion, discovery, and routing intent
in the same pull, but one track must not express conflicting intents.

The next named checkpoint is `0.1.2-dev.2`. It adds revisioned private track
classification, safe direct/CSV review paths, a sectioned interactive track
inspection report, opt-in `--technical` provenance, and full-width interactive
tables. Stable `0.1.2` remains gated on route reconciliation and the verified
Monsoon Cinema regional split.

Remaining v0.1.2 gates are: clone the approved library into a focused review
draft without rebuilding unaffected playlists; turn route captures into durable
negative/current-destination constraints; assign route or intake tracks to an
existing canonical concept or promote a coherent cohort into a new poetic
concept; integrate name and artwork approval for that new concept; publish and
verify the destination before removing the old membership; consume both the
Spotify route entry and its durable Neon desired-route membership so it cannot
be restored; and test mixed deletion/routing/discovery cycles end to end. A
route is not necessarily one future playlist: the regional route is a review
facet and may feed several sound-coherent poetic destinations.

## v0.1.2 saved-library reliability subslice

The user paused cleanup until Neon, Spotify, and code convergence are proven.
Two defects were identified and repaired: active exclusions now supersede stale
approved-proposal memberships in readiness accounting, and awaiting reconcile
runs verify both the durable exclusion ledger and provider absence against the
latest pull before succeeding.

Migration `0034_saved_album_inventory.sql` adds immutable saved-album and
ordered album-track snapshots plus account-scoped policies. Normal Spotify
inventory now probes `/me/albums`, copies an unchanged snapshot from Neon, and
otherwise persists complete album track membership. Album-only tracks remain a
separate review surface and do not block playlist readiness. New read-only
commands are `albums list`, `albums audit`, and `albums tracks`; `albums policy`
defaults to preserve and can opt the personal account into review-then-unsave.

Spotify Liked Songs (`/me/tracks`) is now defined as the primary easy intake:
Like means keep and classify. `spotify library-policy --liked-songs
clear-after-verified-assignment` opts an account into gated cleanup only after
verified canonical placement or durable exclusion. The planner emits explicit
deferred `remove_saved_track` cleanup operations; execution requires readiness,
exact confirmation, and `--allow-destructive`, and post-pull verification
requires absence from the newest saved-track snapshot. The default remains
preserve for every account.

Live validation on 2026-08-25 exposed and repaired two additional shared-state
bugs. Post-publish verification had incorrectly recognized only internal
playlist kind `canonical`; approved Chordrift outputs are `generated` or
`manual`. It now recognizes proposal-backed playlists and compares effective
membership after active exclusions. The publish executor also previously read
raw proposal membership during an exact reorder, temporarily restoring 48
excluded Monsoon Cinema tracks. It now uses the same account exclusion filter
as planning/readiness/verification. The 48 entries were removed through exact
plan `a13897d0-4d2b-4247-8323-7ea1c145fe8e`, readiness assessment
`da486e32-b670-4e8c-a7e9-c03d59ddf9ba` (11/11), and verified reconcile run
`5a6e03a8-e45e-4b76-a32e-4902ba940996`.

Changed playlist and saved-album memberships are now batched into Neon. Album
track persistence reuses known full provider identities instead of overwriting
them with simplified album-track payloads, and album-only response fields are
excluded from embedded track metadata serialization to keep comparisons
stable. A second pull proved all 69 albums and 657 album tracks copy forward
from Neon after a one-page signature probe.

The live converged snapshot is `3fac64c9-6b08-47fe-b9a1-0a02ebfe43b4`: 22
playlists, 1,685 entries, 1,622 unique playlist tracks, 923 supported saved
tracks, 69 saved albums, 657 album tracks, and zero duplicate playlist entries.
Fresh plan `9d74e245-180c-4834-8965-720b484d9ac9` contains zero operations. The
album audit reports 485 distinct tracks already preserved in Liked Songs or a
current playlist, 69 explicitly excluded, and 103 awaiting review across six
albums. Both personal policies remain `preserve`; do not enable cleanup until
the user reviews those 103 tracks.

The user subsequently chose an archive-only personal album policy: retire all
saved-album containers while retaining immutable album and ordered-track
history, without forcing album-only tracks into playlists. Migration 0035 and
planner v10 implement exact retirement operations, separately approved apply,
and post-pull verification. The product default remains `preserve`.

Live album retirement completed on 2026-08-25. Exact plan
`89173e61-35a5-43a1-a837-8479a310094a` contained 69 container-only operations;
assessment `84f94a04-77b9-4845-8189-81b72592ebc8` passed 11/11 checks. Apply run
`4958307f-8df1-40fb-85ee-2b7672115ec0` removed all 69 Spotify album containers
with zero failures and verified against snapshot
`63f13a92-434f-4c90-b5f6-e432221f0da3`. That snapshot has zero saved albums,
while `albums history` retains all 69 as retired. Fresh plan
`6114972d-86d1-437f-877e-084c777d5a1a` has zero operations. Next work is route
reconciliation, followed by verified Liked Songs consumption.

## v0.1.2 Monsoon/regional reconciliation audit

The next requested cleanup retires mixed destination `Monsoon Cinema` only
after lossless reassignment. Snapshot `63f13a92-434f-4c90-b5f6-e432221f0da3`
contains 410 Monsoon tracks across 112 albums. The three route surfaces contain
75 entries / 73 unique tracks, but only 29 unique routed tracks are in Monsoon;
44 routed tracks currently live in other managed destinations. Two tracks are
present in both North and South routes. Therefore reconciliation must cover the
whole approved proposal, not merely split Monsoon.

Existing provider metadata is identity-rich but region-poor: the enrichment
ledger has 39 semantic facts and only two tracks with artist-area facts across
1,715 eligible tracks. Spotify supplies title, artist, album, ISRC, duration,
and release metadata but no reliable track-language field. Use explicit routing
as high-priority evidence, album/version evidence where unambiguous, and a
reviewable semantic classification artifact for the remainder. Indian
Classical is a style destination and requires positive classical evidence; do
not infer it from artist nationality or an instrumental title. Non-South-Asian
tracks return to existing sound-based poetic destinations, with a new poetic
destination created only when their sound warrants it. Route cleanup requires
a newer durable assignment and verified destination; current mere placement in
the rejected destination is insufficient.

## v0.1.2 final reconciliation and Re-evaluate handoff (2026-08-25)

Work is on `codex/v0.1.2-listening-review`. Reviewed CSV
`/Users/suhail/Downloads/south-asian-classification-completed.csv` activated as
classification batch `01256e29-40d3-4ade-a9a5-74e79dad252b`; its exact backup
is `south-asian-classification-completed.before-chordrift-v0.1.2.csv`.
Embedding generation `6b726259-9027-411c-84a2-baedebeebcd9` feeds editable
proposal `f521e707-8e5f-4283-a0bd-d123df3329f1`.

The proposal currently represents 1,754 required tracks exactly once, with 106
durable exclusions, zero unresolved, zero conflicts, and no missing retirement
coverage. `Latika's Theme` (`1Nnrj856MMGbtVRzJBMTFE`) is reversibly excluded as
provider-unavailable. `Monsoon Cinema` is absent from the draft. New concepts:

- `Dakshina Pulse` (`playlist-a101297467c8`): reviewed South Indian cinema plus
  the new Tamil Inbox tracks;
- `Uttara Glow` (`playlist-52a3435dea63`): reviewed North Indian cinema plus
  `Rihaayi De`;
- `Rasa Archive` (`playlist-93b60e96e38d`): the intentionally personal,
  nostalgic South Asian cross-section.

A final CSV-versus-proposal audit found and corrected centroid spillover. All 17
globally classified A. R. Rahman/Rachel Portman film-score cues plus `Unborn
Children` now belong to `Afterlight Score`; LCD Soundsystem's `Someone Great`
belongs to `Midnight Niagara`. These are explicit assignment revisions, not
new cultural claims.

The old `Route — South Indian`, `Route — North Indian`, and `Route — Decide
Later` workflow is superseded by one `Re-evaluate` queue. Migration 0039 adds
the surface purpose and immutable entry/exit ledger. Pulls capture the rejected
source concept. Planning suppresses exclusion/restoration while queued and
clears a track only when an explicit assignment revision newer than queue entry
targets a different concept. `reevaluate retire-legacy --confirm "RETIRE LEGACY
ROUTES"` is Neon-only, requires replacement-queue existence and complete
coverage, and makes the later plan archive the old Spotify containers.

The user approved all four new covers. The complete Drift Atlas v4 batch lives
in `artwork/canonical/drift-atlas-v4/`: 16 canonical and four intake artifacts,
plus preserved label-free backgrounds and `contact-sheet.png`. Artwork batch
`e1e7697a-def7-414d-8835-981af018b059` is imported and approved with input hash
`6bde7f1743b9ee06c1d526cd44e1de58d7f956483cb176a957acf07880ff0abc`.
The approved Re-evaluate cover is also retained under `artwork/review/`.

The Neon-only Re-evaluate surface is `0f81d8ea-3b63-4b23-bf0e-c6d8c52d02dd`.
All three legacy route records are inactive and retain coverage history. The
proposal is approved. Planner v10 now explicitly retires any current managed
canonical concept absent from a complete approved proposal; a regression test
protects this invariant so `Monsoon Cinema` cannot survive silently.

The final immutable plan is `9d3fdc18-1ae5-48ef-a9bd-00d1e0a4b3a9`, sourced
from current snapshot `69b80f1a-0c20-4834-9f51-b7a02cfd1c18`. Its 799 exact
operations are: four playlist creates, 598 additions, four artwork uploads, 189
removals, and four container retirements. The removals are exactly 145 consumed
Inbox entries and 44 managed-provider-drift corrections; there are no
exclusions, external cleanups, unexplained removal reasons, renames, reorders,
or restorations. Retirements are `Monsoon Cinema`, `Route — Decide Later`,
`Route — North Indian`, and `Route — South Indian`. Exact retirement approval
is recorded.

Provider-free preflight passed: 1,754 playlist entries, 26 batched item writes,
four artwork uploads, and an estimated 34 Spotify writes. Final readiness
assessment `78dcee8c-055e-4508-937c-d9b4fbd22af2` passed 11/11 checks, including
the authenticated read-only identity/scope probe, interruption recovery, and
idempotent replay. No Spotify write has been made. Stop here until the user
explicitly approves applying this exact plan; do not regenerate it or pull a
new snapshot first, because that would invalidate the audited readiness.

## v0.1.2 live Spotify convergence (2026-08-25)

The user approved execution. Publish run
`a6d8b47d-1b1d-4220-9b64-4fb026971f30` completed all 606 operations with zero
failures. Its exact-order publication batches also removed the 44 managed-drift
memberships while placing all 598 approved additions. Verification exposed two
edge bugs, both fixed and covered by the full test/lint gate before proceeding:

- Re-evaluate mirroring named an unconditional `(playlist_id, track_id)`
  conflict target even though only generated memberships have that partial
  uniqueness rule. The queue is provider-owned and already cleared before
  mirroring, so the replacement insert now correctly has no conflict clause.
- Canonical verification included separately approved legacy concepts, creating
  a cycle where Monsoon had to disappear before the retirement phase was
  allowed. Verification now compares exactly the concepts in the approved
  proposal while retaining legacy containers until their gated retirement.

Canonical publication verified against snapshot
`e41b7e51-f5d0-4f37-ad1e-8253c348749c`. Cleanup run
`6e853243-8c5b-44d1-a990-abbc257a2a3f` removed exactly 145 consumed Inbox
entries with zero failures; Spotify required one propagation refresh before
snapshot `068aeb95-becb-46bb-b21e-5fa72ff5834a` verified the absences.
Retirement-only plan `fa017618-90b8-4b6b-a62a-fec86c276cc9` and readiness
assessment `d6ead94b-728c-4a8a-b896-cdd4861f649c` passed 11/11 checks.
Retirement run `99d95649-3b99-4138-9bba-db5c2312a224` removed the Spotify
library relationships for Monsoon Cinema and all three legacy routes, retaining
their Neon inventory and history, with zero failures.

Final snapshot `ca779284-b161-48e2-bc9a-bde5f0be640b` has 22 playlists, 1,791
playlist entries, 1,766 unique playlist tracks, no duplicate entries, 346
supported saved tracks plus one unavailable/unsupported saved item, and zero
saved albums. Final convergence plan
`66994c42-5f5c-4f31-8b2d-85b17ac81dd3` has zero operations in every phase.

## Playlist-product foundation proposal (2026-08-26)

The next product direction is now recorded in
`docs/design/PLAYLIST_PRODUCT_ARCHITECTURE.md` and its zoomable
`playlist-product-architecture.svg`. The proposed boundary supports Spotify-
first onboarding while keeping provider payloads behind a capability-based
adapter; optional listening archives enrich but never gate basic use.
It adds a product-level `chordrift_accounts` ownership root above the existing
provider-account boundary so one user can eventually connect several providers
without merging their inventories or actions.

Collections are overlapping, unordered musical worlds with navigational
hierarchy, revisioned rules, provenance, and user-authoritative boundaries.
Playlist surfaces separately model authority, purpose, and refresh behavior.
Versioned recipes allocate lifecycle lanes and define cadence, guardrails, and
ordering narrative. A reproducible Spin stores its evidence capabilities,
input fingerprint, seed, selected tracks, explanations, and exact order before
the existing plan/apply/verify publication boundary.

No migration or runtime refactor has been performed for this proposal. The next
implementation gate is to freeze the provider-neutral IDs, capability contract,
surface axes, collection membership strengths, and recipe-v1 Rust types with
unit tests; then prove isolation with a fake provider before designing an
additive migration.

## Historical portable-core client decision (2026-08-26; web-first update 2026-08-29)

The user confirmed that the CLI is only the first client. The later decision is
web-first: a responsive web application is the intended consumer product, with
iOS and Android intended afterward and macOS, Windows, or Linux optional later.
Every client remains a thin presentation over one Rust-owned product. The useful layered ideas from
Photara are adopted—portable core, typed client boundary, thin clients,
explicit capabilities, and cross-cutting contracts—without importing its node
packages, proxy graph, or general runtime registry.

`docs/design/client-core-platform-architecture.svg` records the approved high-
level shape. The shippable authority is a hosted Rust service. It owns the Neon
connection and encrypted provider authorization; shipped clients hold only a
Chordrift session credential and never access SQL or provider refresh tokens.
The CLI may use an in-process development transport, but all clients consume
the same versioned command/query/event application contract. That contract
includes progress, cancellation, structured failures, recovery, idempotency,
and API/schema/provider/evidence capability negotiation. It—not UniFFI, HTTP,
or another transport—is the stable boundary.

The earliest acceptance goal remains UI-free: create an isolated onboarding
session that treats the current personal inventory and optional history as
new-account evidence, ignores existing Chordrift intent by default, emits an
honest audit and starter organization, and produces deterministic provider-free
Spin previews. Run it with inventory-only and enriched evidence. It must make
no Spotify write; later publication still crosses plan/apply/verify.

Before schema changes, route the existing CLI through an application facade,
freeze provider-neutral/domain/contract types, and add fake-provider account-
isolation, idempotency, and cancellation tests. Then design one additive
ownership/collection/surface/recipe/Spin/onboarding migration and rehearse it.

## V021-06 hosted private-beta deployment checkpoint (2026-08-31)

Work continues on branch `codex/v021-06-private-beta`. The authenticated service
is live at `https://chordrift.suhail.ink` through Nexus to a non-root,
read-only Vortex container. Auth0/Google login is active, the verified identity
owns the existing Chordrift account, and its HttpOnly session survives container
replacement. The deployed web client lists the selected Spotify connection,
credential readiness, newest observation time, provider playlists/tracks,
Chordrift-model playlists/tracks, listening detail and active exclusions.

Chordrift login and provider authorization are separate. The existing Spotify
refresh authorization was adopted as encrypted vault generation 1 without
contacting Spotify. New users still need explicit Connect Spotify; reconnecting
the same stable Spotify identity must retain its Neon data, while a different
Spotify identity is a separate provider connection. Add Connect, Reconnect and
Disconnect plus multiple-connection isolation before beta.1.

The latest deployed checkpoint is commit `716f847`; branch tip may be newer
when documentation advances. The container reports healthy. Browser validation
proved the signed-in provider context, 26 observed Spotify playlists, separate
Chordrift-model state, ordered playlist contents, track history/placements and
the exclusion archive. The provider vault capability is available; maintenance
remains unavailable and the Observe button is intentionally disabled until the
real encrypted-vault Spotify session, PostgreSQL maintenance adapter and durable
worker are composed. Never replace that boundary with a CLI or shell call.
Provider writes remain disabled until a later exact user-approved gate.

`v0.2.1-beta.1` now means a usable dual-client daily driver, not merely a
bootable service. Complete ordinary observation, cumulative provider-first
convergence, ambiguity decisions, exact effect review, progress, cancellation,
retry and verification in both web and remote CLI through shared typed DTOs.
The web need not expose every forensic query, but unequal provider/model totals
must have a high-level directional explanation. A read-only Lightleak Reverie
audit found 12 provider-only and four model-only tracks, explaining 501 versus
493; this is pending observed intent, not authorization to overwrite either
side. The shared comparison requirement is in
`docs/design/WEB_WORKFLOW_CAPABILITY_MATRIX.md` and the edge-case ledger.

After beta.1, publish only real fixes as sequential `v0.2.1-beta.N` releases
until Suhail explicitly approves stability. Final v0.2.1 also requires a web
guide, concise CLI/operator handbook, dead-code/script/dependency/performance
cleanup, full CI/container/deployment/backup proof, and exact installed-artifact
verification. The history-known-but-unplaced recovery audit is a personal
one-time tool and not a release blocker.

After final v0.2.1, prepare the separate Classification Authority task: refresh
the founding brief and learning-signal taxonomy, preserve the strict shared-
classification/private-listener/placement boundary, add a bootstrap checklist,
and deliver a ready-to-paste task prompt. The new task—not Chordrift—chooses the
project name, creates its repository and Neon project, reserves its namespace,
and authors its independent roadmap.

### Neon storage consolidation (2026-08-31)

The durable Chordrift topology is now project `royal-snow-31539822`, branch
`br-cool-haze-aflxxqep` (`main`), database `chordrift_cutover`. Local alpha.17
maintenance and the Vortex hosted service both use that database; it is healthy
at 50/50 migrations. The Vortex container and public HTTPS endpoint passed
post-cutover health checks, and a local read-only playlist query passed. No
Spotify request or mutation occurred during consolidation.

Temporary branches `br-red-frost-afeshi7s`
(`rehearse-v02106-20260830`) and `br-bold-haze-afrt5en4`
(`pre-v02106-20260830`) were deleted after cutover. The unused main-branch
database `chordrift`, last synchronized on 2026-08-28 and with no client
connections, was also deleted. The canonical PostgreSQL database is about
195 MB. Neon control-plane/dashboard storage can lag and may retain deleted
pages within the six-hour restore window before falling from the earlier
roughly 419 MB value.

The dashboard's later 228.24 MB reading is consistent with the measured
logical footprint rather than a second application copy: `chordrift_cutover`
is 204,226,560 bytes (about 195 MiB), while PostgreSQL's `postgres`,
`template0`, and `template1` databases contribute about 23.4 MB. The complete
object-by-object glossary is
[`docs/reference/DATABASE_OBJECT_CATALOG.md`](docs/reference/DATABASE_OBJECT_CATALOG.md),
and the grouped storage/dataflow overview is
[`docs/design/chordrift-database-domain-map.svg`](docs/design/chordrift-database-domain-map.svg).
The seven SQL views store no rows. Listening evidence is about 85 MiB and
playlist intent/verified history about 53 MiB; those two domains, not the
number of tables, explain most storage. Do not delete evidence or audit rows
ad hoc. A future bounded compaction policy may retire superseded playlist,
verification, and sync generations only after durable current anchors and
restore invariants are proven.

Owner-only external backups are under
`$DROPBOX/Music/Chordrift/Backups/2026-08-31-pre-main-consolidation/`:

- `chordrift_cutover-47.dump` — SHA-256
  `4a00b29b74659f01a69ad15b577fd6d1d14295f0911f9d36aff9f1105c6a348d`;
- `hosted-rehearsal-50.dump` — SHA-256
  `43a7b048303739d511458224528266be2c8772b26106337af228c332980af6a9`;
- `stale-main-chordrift-47.dump` — SHA-256
  `97c151e5f301342b8b0ca97c4a9c1b82a74ce700cad170a1be37f57ea04d3844`.

The deleted rehearsal database contained only disposable identity/vault/job
fixtures; none were promoted. The canonical hosted tables are empty after
consolidation. Reverify first-owner Google adoption and import the existing
Spotify refresh authorization as encrypted generation 1 before beta.1. Until
then provider-backed observation/maintenance stays unavailable and provider
writes stay disabled.

Future migration/restore rehearsals use disposable local PostgreSQL by default.
If Neon branch behavior itself must be tested, create the branch with an expiry
and delete it immediately after the recorded proof. Never leave a rehearsal
branch as an implicit runtime dependency.

### Durable hosted observation checkpoint (2026-08-31)

The V021-06 branch now has separate `chordrift-server` and `chordrift-worker`
entry points. `ObserveProvider` is accepted into migration-0050 durable storage
only after tenant/provider ownership validation. The worker exclusively claims
it, decrypts the active refresh credential through the account-scoped vault,
verifies the stable Spotify identity, rotates a returned refresh credential,
and calls the Rust inventory importer directly. It emits durable progress,
renews its lease, honors cooperative cancellation, and atomically publishes a
complete provider observation. It never invokes a CLI or shell. The browser's
Observe button submits and follows this typed operation; provider writes remain
disabled.

The pinned runtime image contains both binaries, while Vortex Compose runs them
as separate read-only, non-root API and worker services. Only the API publishes
the Nexus-facing private port. See
`docs/design/HOSTED_PRODUCTION_ASSEMBLY_V021_06.md`.
An isolated Vortex build succeeded as
`sha256:dbbb3dce2df73504791e0691092eaf23810469a531164c8456746033549c8854`;
inspection proved UID/GID 65532, no `/build` source tree, and only
`chordrift-server` plus `chordrift-worker` under `/usr/local/bin`. Compose
configuration validation also passed. This is a disposable development proof,
not the eventual beta.1 release digest and not a live deployment.

Do not mark V021-06 Production assembly complete yet. The next code boundary is
durable maintenance-session persistence plus cumulative provider-first
interpretation/decision recording. Only after that is proven with fake-provider
and disposable-PostgreSQL tests should the hosted maintenance capability become
available. No live Spotify observation was executed by this checkpoint.

### Alpha.18 interrupted-move recovery (2026-08-31)

Daily use exposed one compound record-only failure while five tracks were moved
from Rasa Archive to Cinema Monsoon. The detailed plan represented both halves
of each move, the shell submitted ten IDs, and assignment rejected duplicates.
The interrupted run left a complete editable proposal; the prior classifier
then treated current membership plus a proposed destination as direct intake,
expanding the retry to 1,439 tracks. No provider apply was reached.

Alpha.18 canonicalizes automatic move evidence before proposal mutation,
rejects conflicting destinations, and classifies current membership represented
in either an approved or editable proposal as covered. Proposal extension model
version 3 also prevents active exclusions from being re-added by stale manual
assignment revisions. The changed extension hash prevents reuse of the bad
cached copy.

The live recovery used the fixed development binary against the already-pulled
snapshot only. It recorded exactly five unique moves, omitted the two excluded
tracks that a stale revision had attempted to replay, accepted exact provider
order for membership-equal playlists, and finished with approved generation
`5e35ca8a-279a-4092-99af-a7530c24f58d`: 1,429/1,429 represented, zero pending
maintenance operations, and 1,429 `already_covered` intake rows with no direct
intake. Spotify was neither pulled nor written during recovery.

Implementation commit `2f0757c` was merged to `main` as `1e12c79`. Pull-request
CI run `33438947590` and post-merge main CI run `33439431684` passed formatting,
strict Clippy, all targets, documentation, every ignored PostgreSQL integration,
Spotify persistence, and package verification. The annotated
`v0.2.1-alpha.18` tag, GitHub prerelease, and crates.io artifact are public.
The exact locked registry artifact is installed at
`/Users/suhail/.cargo/bin/chordrift`, reports `chordrift 0.2.1-alpha.18`, and
passes the maintenance/provider-baseline/remote-CLI capability gate with
`CHORDRIFT_BIN` unset. A final installed-artifact `--skip-pull --review-only`
run reported `Everything is already in sync.`

Continue V021-06. Permanent regressions live in the daily-driver edge-case
ledger, the fake-binary suite, the intake classifier unit test, and the
disposable-PostgreSQL intake/extension test.

### Durable maintenance-session checkpoint (2026-08-31)

V021-06 now stages additive migration 0051 with `maintenance_sessions` for the
current typed task projection and `maintenance_session_events` for immutable
accepted revisions. The rows are bound to the authenticated product subject,
Chordrift account, and owned provider connection. Replacement is exact-next-
revision compare-and-swap, so stale web, CLI, API, or worker processes cannot
overwrite newer user intent. Rehydration passes through the Rust
`MaintenanceWorkflow` invariant validator; no provider credential, session
token, shell, client SQL, or client provider URL is stored.

`PostgresMaintenanceSessionStore` supplies create/load/replace and
`DurableMaintenanceAuthority` owns start, refresh, resolve, and exact-review
authorization transitions without executing provider effects. A disposable
PostgreSQL 18 run on Vortex proved migration, restart reload, cross-tenant
non-disclosure, immutable event history, and stale revision rejection. The
container, network, temporary source copy, and root-owned Cargo output were
removed immediately. No Neon branch was created, the canonical database
remains at the verified migration-50 live baseline, and Spotify was not read or
changed.

The branch now contains the next record-only vertical slice. Start and Refresh
perform a fresh Spotify read through the encrypted-vault worker, then the
PostgreSQL adapter reuses the Rust maintenance planner to build the durable
typed projection. Paired plan rows for one move are collapsed by Spotify track
ID. Resolve records exact-revision decisions. Web and remote CLI start, query,
refresh, and resolve the same session; authorization is explicitly rejected.

The following branch checkpoint supersedes the earlier warning about session-
only resolutions. Keep authorization/apply disabled until its separately
approved exact-write gate. See
`docs/design/DURABLE_MAINTENANCE_SESSIONS_V021_06.md`.

### Canonical maintenance projection checkpoint (2026-08-31)

The hosted maintenance path now has the intended three product layers: thin
CLI/web skins submit typed DTOs; a Rust application/workflow layer interprets
gestures and coordinates durable sessions; the Rust domain core plus typed
PostgreSQL/provider ports performs canonical intent and effects. Later iOS and
Android clients must reuse this boundary rather than port workflow logic.

`CanonicalMaintenanceProjector` idempotently records resolved provider-first
gestures as canonical placement, reversible exclusion, accepted custom order,
or remembered saved-track disposition. It creates an exact editable fork of
the approved model, omits active exclusions, preserves selected names and
already-approved artwork, and never centroid-assigns unrelated tracks. A retry
of an already-satisfied resolution creates no generation. Provider-side direct
intake and reclassification become placement evidence; removal becomes an
exclusion; reorder becomes canonical custom order.

Saved/Liked intake is part of the same Rust session. Placement and whether to
retain the Like are separate decisions. The authority remembers preserve or
clear intent; an exact `update_saved_state` effect is withheld until every
required decision is resolved, and clear intent is projected only after the
canonical destination/exclusion work. The browser only renders the server-
provided decisions and sends selected DTO variants.

The next exact-write checkpoint is also implemented on the branch. Web and
remote CLI render immutable provider effects and submit only session,
revision, and review identity. The API rejects stale or mismatched review
authorization before queueing. The worker independently rederives the trusted
effect from the saved-state gesture, refuses unsupported effects, checks that
Chordrift has not observed a newer provider checkpoint, and persists the exact
Authorized → Applying → Verifying → Verified sequence. Saved-track removal is
idempotent; retries resume the persisted stage, a fresh complete observation
verifies absence, and newly observed unrelated gestures become a new session
projection rather than being covered by the older authorization.

The decision reducer validates resolution shape before mutating anything: a
client cannot submit `consume_intake` for a playlist removal or swap the
server-provided Liked Songs source. Provider execution requires the exact
server-rederived effects and review rather than trusting human summary text.
A disposable PostgreSQL 18 proof on Vortex passed the seven immutable session
events and was removed with its source/build directory. This proof did not
contact Spotify. The code exists but remains undeployed; no live provider
write is authorized until private-beta deployment and manual acceptance.

Strict Clippy and focused Rust tests pass. A disposable PostgreSQL 18 container
on Vortex proved canonical removal, idempotent retry, active exclusion, and
approved-artwork inheritance on the newest generation. The database container,
temporary source, and root-owned Cargo output were removed after the proof. No
Neon branch was created and Spotify was not contacted. Next implement the
independent Spotify Connect/Reconnect/Disconnect lifecycle and complete
web/remote-CLI acceptance journey. Provider writes remain disabled in the
deployed checkpoint.
