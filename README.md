# Chordrift

Chordrift is a personal music-library intelligence and synchronization system
for organizing a library across streaming services.

Its product promise is **a clean listening surface backed by lossless musical
memory**. Streaming libraries accumulate old favorites, unexplained tracks,
followed playlists, abandoned utility lists, and provider-generated discovery
without preserving an obvious account of why each item is there. Chordrift
retains provenance and history before simplifying that surface, then turns the
useful material into a small, purposeful set of approved playlists that the
listener actually wants to play.

Chordrift is therefore not merely another playlist generator. It is library
archaeology, curation, and orchestration: explain where music came from,
preserve what mattered, separate external bookmarks from personal intent, and
make the active library enjoyable again without silently losing history.

## Release status

**v0.2.1-alpha.18 is the current daily-driver prerelease.** It preserves the
proven maintenance CLI through one application facade and adds the provider-
neutral product domain, onboarding and audit boundaries, deterministic recipe/
Spin previews, explicit plan origins, and additive schema migrations 0046 and
0047. The personal
cutover pairs the v0.2.0 binary with the verified 47/47 database candidate;
mixing a v0.1.4 binary with that database is not a supported operating mode.
The `v0.1.4` tag remains the exact historical reference and rollback source.

The v0.2.1 alpha line provides official installable checkpoints on the path to
the hosted-authority v0.2.1 final daily driver. The installed CLI and verified
47/47 account database may continue handling ordinary maintenance while later
architecture work is paused; no hosted service or Classification Authority is
required for the current workflow. Alpha.18 makes interrupted direct moves
idempotent: paired plan evidence is recorded once, an editable proposal cannot
turn the full accepted library into intake, and active exclusions win over
historical assignment revisions. Alpha.17 adds the authenticated remote CLI,
OS-keychain product sessions, compatibility negotiation, and explicit local
development transport; hosting and public login remain V021-06. Alpha.16 names an already-managed
destination for a newly liked track and remembers the per-track choice to keep
or clear Liked Songs. An undecided or keep choice cannot plan an Unlike; an
explicit clear choice produces one exact reviewed saved-state effect, and a
later direct Unlike supersedes an older keep decision. This uses the existing
migration-47 virtual-surface/directive schema and raises the thin-client
application contract to 1.3. Alpha.15 adds restart-safe hosted background
operations: typed commands and exact idempotent receipts are durable before
work, workers use expiring exclusive leases, and ordered progress,
cancellation, bounded retry, and abandoned-work recovery survive process
restarts. Alpha.14 adds the encrypted hosted provider-
credential vault: authenticated ciphertext stays server-side, encryption keys
stay outside PostgreSQL, and clients retain only Chordrift sessions. It adds no
provider-token route and does not require the personal database to migrate.
Alpha.13 also makes every exactly converged
provider observation the durable ordinary-maintenance baseline, preserves
provider-authored order while replaying assignment evidence, and adds explicit
inspection/emptying of the reversible exclusion archive. Alpha.12 added
persisted product identities, account ownership, and revocable digest-only Chordrift sessions under
product-session schema 1 while leaving application contract 1.2 unchanged. It
does not select an identity vendor or expose a hosted endpoint. Migration 0048
belongs to the hosted identity deployment; the
local maintenance client remains explicitly compatible with the verified
47-migration music database. Hosted identity and credential work use additive
migrations 0048 through 0050 only when deployed. The rest of the v0.2.1 hosted-authority sequence
remains intact. The separate
Classification Authority project and a later Chordrift refactor follow v0.2.1
final.

Install the locked release with:

```console
$ cargo install chordrift --version 0.2.1-alpha.18 --locked --force
```

Read the [v0.2.1-alpha.18 release notes](docs/releases/V0.2.1-alpha.18.md), the
[provider-first convergence contract](docs/design/PROVIDER_FIRST_CONVERGENCE.md),
the [provider credential vault contract](docs/design/PROVIDER_CREDENTIAL_VAULT_V021_03.md),
the [web workflow capability matrix](docs/design/WEB_WORKFLOW_CAPABILITY_MATRIX.md),
the [durable operation contract](docs/design/DURABLE_BACKGROUND_OPERATIONS_V021_04.md),
the [remote CLI contract](docs/design/REMOTE_CLI_PARITY_V021_05.md),
and the
[recovery procedure](docs/how-to/RECOVERY_AND_ROLLBACK.md) before changing an
existing database-backed installation.

The v0.2.0 development sequence established a
public, transport-neutral Rust application contract for commands, queries,
immutable views, lifecycle events, progress, cancellation, structured errors,
and compatibility/capability negotiation. V020-02 now routes the existing CLI
through one application facade with exact behavioral and output parity.
Both preserve the established command behavior. V020-03 adds
provider-neutral ownership, identity, capability, collection, surface, recipe-
v1, and Spin value types. V020-04 now proves account/provider isolation,
deterministic fake generation, idempotency, cancellation, bounded retry, and
visible capability failure entirely in tests. V020-05 adds migration 0046's
provider-neutral ownership, collection, surface, recipe, onboarding, Spin, and
publication-link schema after fresh and migration-45 PostgreSQL 18 rehearsals;
it is now active in the 47/47 v0.2.0 database. V020-06 adds the provider-read-only
onboarding boundary: one selected immutable inventory and optional extended
evidence are content-addressed through the shared facade, while account
ownership, capability failure, idempotent replay, ignored existing intent, and
zero provider writes are explicit. V020-07 now reads only that immutable current
inventory to return a deterministic library, overlap, uncertainty, capability,
and preserve-first starter-organization report. It performs no provider or
database writes, treats the proposal as unapproved, and explicitly refuses to
infer listening behavior, user intent, or collection membership. The enriched
V020-08 now runs that same inventory baseline with one explicitly selected,
content-addressed extended-history import. It reports only directly supported
gains—observed, repeated, long-term, history-only, completion, and skip
evidence—with exact supporting record/track counts, while still refusing to
infer preference or intent. V020-09 now executes immutable Discovery +
Rediscovery recipe revisions as deterministic, unordered selection drafts. It
allocates source seats, reserves familiarity cadence and narrative-section
capacity, enforces eligibility and repetition/artist budgets, and reports
capability degradation and unfilled seats without provider or database access.
V020-10 now turns that verified draft into an exact ordered Spin with stable
identity, seed, capability snapshot, selection/ordering reasons, narrative
sections, warnings, fingerprints, idempotent migration-0046 persistence, and an
account-scoped query view. It still performs no provider action. V020-11 now
exposes those Rust-owned onboarding, collection, recipe, and Spin views through
one opt-in `product` CLI namespace and an installed-binary comparison/replay
helper. The commands accept fake/captured inputs, require an isolated
migration-0046 database for persistence, and cannot call or write a provider.
V020-11R selectively reconciles the recovered 92-track incident without
merging its maintenance branches: ordinary additions are enumerated rather
than full replacements, a capability-checked complete intake wizard delegates
to a read-only Rust audit, and every maintenance plan exposes an origin that
rejects future Spin publication plans. V020-12 now converts an approved Spin
into a checkpoint-bound immutable plan
with explicit `spin_publication` origin. A provider-neutral fake execution path
proves readiness, enumerated additions, replay, preservation of unrelated live
membership, active-exclusion safety, and verification without wiring Spotify.
Migration 0047 adds only the missing surface-to-recipe and Spin-plan identity
links. V020-13 proves a fresh
45-migration production backup restores locally, advances to 47/47, preserves
the complete invariant report and exact required-domain hashes, passes
`pg_amcheck`, and retains honest absence of newer plan-origin/capability rows.
V020-14 has now created and independently verified a fresh Neon PostgreSQL 18
candidate at 47/47 with exact newest-state invariant and durable-domain parity,
the capability handshake, and the complete compatibility/origin test suite.
V020-14 completed candidate verification and exact cutover planning. V020-15
packaged and published v0.2.0 with an explicit recovery procedure, then paired
the released binary with the parity-checked 47/47 candidate. Hosted transport,
the intended web client, and optional native clients remain later work. The
release and cutover made no Spotify
write; v0.2.0 still has no production Spotify adapter for Spin publication.

Documentation on `main` describes the v0.2 architecture and current v0.2.1
alpha daily driver. Use the `v0.1.4` tag when exact
historical commands or behavior are needed.

The next Chordrift release line is **v0.2.1 hosted Rust authority**. It exposes
the existing application contract through authenticated transport, adds product
identity and tenant authorization, keeps provider credentials server-side,
makes background operations durable, preserves remote/local CLI parity, and
finishes with an observable recoverable service release. See the
[roadmap](ROADMAP.md).

V021-05 adds an authenticated remote CLI over the same DTO contract. It stores
only an opaque Chordrift session in the OS credential store, negotiates service
compatibility before work, requires HTTPS outside loopback testing, and keeps
an explicit in-process development transport. Hosting and product-login
selection remain V021-06; there is no public service URL yet.

Learned shared classification is a separate product and future Chordrift
dependency, not a Chordrift database module or v0.2.1 slice. That project owns
its knowledge store, model artifacts, evaluation, release lifecycle, and
developer Classification Lab. Chordrift will own only the narrow query adapter,
private exact-fingerprint cache, account-specific placement policy, and provider
operations. The complete project brief is preserved in the
[classification knowledge foundation](docs/design/CLASSIFICATION_KNOWLEDGE_FOUNDATION.md).
The accompanying
[learning-signal taxonomy](docs/design/LEARNING_SIGNAL_TAXONOMY.md) separates
private listening and lifecycle evidence, shared lawful classification inputs,
and account-specific placement/recipe decisions.
The classifier is expected to generalize to unseen recordings from compact
reviewed knowledge—not store one final answer for every song—and return ranked
claims, calibrated confidence, alternatives, evidence, and unknown/conflict
states.

Its longer-term form is a personal listening-system designer. Chordrift should
inspect a library, propose a coherent organization, explain every
recommendation, and—only after approval—carry out an exact, auditable plan. It
should serve someone who wants a good automatic result as well as someone who
wants precise control over composition, repetition, ordering, boundaries, and
the evidence used for each decision.

## Product model

Chordrift separates three concerns that streaming applications commonly mix:

- **Canonical collections** are durable musical identities and user-approved
  boundaries. A collection may be a vibe, tradition, region, era, personal
  cohort, or another meaningful grouping.
- **Intake surfaces** record how music entered the system. Liked Songs is the
  lowest-friction intake and named inboxes retain richer provenance. A
  correction is now the natural Spotify action: move the track from the wrong
  managed playlist to the right one, and let maintenance record the confirmed
  reclassification.
- **Generated listening playlists** are renewable experiences produced from
  versioned recipes. A track retains one explainable canonical identity while
  being eligible for multiple mixes.

A recipe can combine recent discoveries, recent rotation, forgotten favorites,
long-term repeats, recommendations, and selected collections. Users may accept
a curated preset or adjust controls such as new versus familiar, focused versus
varied, rediscovery weight, duration, repetition budget, cross-playlist reuse,
artist spacing, ordering strategy, and hard collection boundaries. Every
output should retain the recipe version, source evidence, constraints, ordering
rationale, and random seed needed to explain or reproduce it.

This model makes a Like the ordinary low-friction action: Chordrift records the
provider save time, preserves the discovery, assigns or reviews its canonical
home, and makes it eligible for new-discovery recipes. An account may opt to
clear Liked Songs only after the destination or durable exclusion is published
and provider-verified.

## Capability-aware intelligence

Recipes declare which signals they require or merely benefit from. Provider
adapters report available capabilities instead of leaking Spotify-specific
assumptions into the core. Saved timestamps may support discovery immediately;
bounded recent-play observations can grow rotation evidence over time; an
optional Spotify extended-history export unlocks reliable lifetime play counts,
skips, completions, dormant favorites, and historical rediscovery. When data is
missing, Chordrift must disable the dependent feature or explain its reduced
confidence rather than fabricate precision.

User instructions range from hard policies (for example, keep one cultural
collection separate) through strong preferences, soft ranking signals, and
one-time recipe choices. Approved corrections become account-scoped evidence.
Chordrift may propose a reusable rule after repeated consistent corrections,
but must show the evidence and obtain approval before applying it broadly.

## Agentic and quiet operation

The intended first-run experience is a read-only library audit followed by an
editable plan. Chordrift identifies overlap, duplicates, uncertain tracks,
legacy containers, possible collections, missing data capabilities, and useful
starter recipes. Reading and recommendation are freely repeatable; constructive
publication and destructive cleanup receive separate bounded approvals and an
exact provider diff.

Normal operation should then be quiet and background-friendly. A signed native
application owns scheduling, progress, notifications, and recovery without
opening surprise Terminal windows, spawning visible helper shells, or repeating
browser/OAuth prompts. OAuth uses the system browser only when consent is
actually required. Secrets remain in 1Password and the operating-system
credential store. Apogee is only an optional local CLI/development environment
loader; it is not a Chordrift dependency, application contract, installation
requirement, or GUI configuration mechanism. Shipped clients authenticate to
the hosted Rust authority and keep only a revocable Chordrift session in the OS
credential store; they never receive Neon or provider credentials. Secrets
never belong in project files, logs, command history, or shell startup files.
Long work reports useful progress, can be cancelled, and resumes safely after
interruption.

Existing immutable plans, readiness assessments, resumable apply history,
post-write pulls, and zero-operation convergence remain the execution
foundation for the v0.2 application facade and later hosted web/native clients.

Neon PostgreSQL is the canonical ledger for the released CLI. In the shippable
architecture, a hosted Rust service owns that connection and exposes the same
versioned application contract to every client; Spotify is the first provider
adapter, with Apple Music and other providers added only through explicit
capability boundaries. Later releases will use playlist history, personal
listening context, and versioned embeddings to organize tracks, and may support
Spatial Audio-aware playlist variants in Apple Music.

Spotify's downloadable listening-history export will be optional enrichment.
Library import, canonical identity, playlist analysis, matching, organization,
and synchronization will not wait for it; when available, the export will add
signals such as play counts, listening duration, first play, and last play.

> [!WARNING]
> Chordrift is in early development. v0.2.0 permits remote Spotify
> mutation only through exact, audited, resumable phase confirmations. Never
> run an apply command without inspecting its immutable plan and readiness ID.

## Released CLI foundation (v0.2.0)

- Storexa-backed Neon PostgreSQL connection management
- an application-owned canonical music-library schema
- compile-time embedded SQLx migrations
- secret-safe database health and migration status
- Spotify Authorization Code with PKCE for account authorization
- refresh-token storage in the operating system credential store
- one materialized current provider inventory per account with
  content-addressed playlist and saved-library revisions
- ordered playlist membership that preserves duplicate entries without copying
  complete unchanged membership on every pull
- normalized permanent listening evidence with display metadata stored once per
  historical provider identity
- provider metadata and stable Spotify identities linked to canonical records
- a one-command incremental pull that leaves Neon current with Spotify edits
- concurrent Spotify probes, batched Neon persistence, incremental statistics,
  API request counts, and per-phase timings for routine pulls
- account-scoped observed, inbox, and managed playlist roles
- explicit provider-wins, Neon-wins, and manual drift policies
- current overlap, duplicate-membership, and aggregate library reports
- ordered track queries for individual current playlists
- one-command track inspection across current placement, retained playlist
  provenance, listening signals, embedding/cluster rationale, and semantic facts
- idempotent account-data and extended streaming-history archive ingestion
- cumulative event deduplication across periodic overlapping Spotify exports
- account-scoped play, duration, skip, completion, and recency statistics
- Git-ignored local inbox/archive recovery that preserves original ZIPs
- immutable semantic embedding generations with reproducible parameters
- independently versioned listening and lifecycle signal generations
- explicit semantic, provider-curated, intake, canonical, transport, and
  ignored playlist evidence classes
- nearest-neighbor inspection before any embedding can feed clustering
- immutable, exact Spotify publication, cleanup, and retirement plans
- resumable per-operation apply history with post-pull convergence proof
- approved labeled canonical and intake cover uploads, reusable label-free
  masters, convergent upload receipts, and provider-free payload preflight
- preserved external-playlist bookmarks separated from the active library
- one shared formatted interactive presentation with stable redirected output
- an operator-only installed-binary wrapper for the complete safe
  pull/plan/readiness/confirm/apply/pull/convergence loop

Set the canonical Neon connection URL through the application-specific
`CHORDRIFT_DATABASE_URL` environment variable. Chordrift never prints it.

```console
$ chordrift --version
chordrift 0.1.4

$ chordrift db status
database: chordrift-primary
provider: neon
status: healthy
...

$ chordrift db migrate
...
```

`db status` is read-only. `db migrate` applies Chordrift's embedded,
application-owned migrations through Storexa.

## Spotify setup

Create an application in the
[Spotify developer dashboard](https://developer.spotify.com/dashboard) with
Web API access and register this exact redirect URI:

```text
http://127.0.0.1:8888/callback
```

Expose its public Client ID as `CHORDRIFT_SPOTIFY_CLIENT_ID`. Chordrift does
not require or store a Spotify client secret. An alternate loopback callback
can be set with `CHORDRIFT_SPOTIFY_REDIRECT_URI`, but it must use an explicit
loopback IP address and port and must exactly match the dashboard entry.

```console
$ chordrift spotify auth --account personal
$ chordrift spotify status --account personal
$ chordrift db migrate
$ chordrift spotify import --account personal
$ chordrift sync pull --account personal
```

Authorization requests `playlist-read-private`, `playlist-read-collaborative`,
`user-library-read`, `user-read-recently-played`, `user-top-read`,
`playlist-modify-private`, `playlist-modify-public`, `user-library-modify`, and
`ugc-image-upload`. The refresh token is
stored under an account-scoped entry in macOS Passwords/Keychain; it is never
written to a shell initialization file or the database. `spotify logout`
removes that local credential without revoking access in Spotify.

OAuth consent is consolidated into that one authorization. Normal sessions
read the credential once and no longer rewrite an unchanged refresh token. A
signed distribution is still required for a non-technical user's stable,
one-time macOS Keychain trust experience; frequently rebuilt unsigned debug
binaries may otherwise be treated as different applications by macOS.

Spotify imports materialize a complete inventory from remote reads and reusable
Neon state before opening one database transaction. A failed fetch or
persistence operation therefore cannot leave a partial snapshot. Imports never
create, edit, reorder, or delete Spotify content. Under Spotify's current
Development Mode, Chordrift snapshots owned playlists and collaborative
playlists it can access; followed playlists that Spotify will not expose
through the playlist-items endpoint are reported as skipped.

After the first complete baseline, imports use Neon to minimize Spotify API
traffic. Unchanged playlists are detected by Spotify `snapshot_id` and retain
their existing content-addressed Neon revisions without requesting their items
again or copying their memberships. Saved tracks use a single newest-page
probe; when its total and leading signature match, the existing saved-library
revision is reused without downloading the remaining pages. A detected change
triggers a complete reconciliation so removals are not silently missed.

Beginning with v0.1.4, the independent saved-track, saved-album, and recent-play
probes run concurrently. Unchanged playlist bookkeeping and memberships are
persisted with set-based statements, recent observations are inserted in
batches, and only affected listening-statistic rows are refreshed. Routine
pulls report Spotify API request counts and per-phase elapsed time so provider
latency and Neon work can be distinguished without enabling debug logs.

For routine use, `chordrift sync pull` imports the current Spotify state and
incrementally retains new Recently Played observations before refreshing
account-scoped canonical analysis in the same invocation. The lifetime privacy
export remains the authoritative duration/skip baseline; later exports
supersede overlapping API observations so plays are not counted twice. The
local account label defaults to `personal`, while the stable Spotify user ID is
persisted in Neon; playlist identities and roles are also scoped to that
account. A playlist rename therefore does not create a new identity, and a
playlist that disappears from the latest snapshot remains historically known
but is marked absent.

Imported user playlists begin protected as `user_managed`, `observed`, and
`provider-wins`. Chordrift does not retire them unless the user explicitly
changes retirement intent. Discovery surfaces can be marked as `inbox`, and
Chordrift outputs as `managed`:

```console
$ chordrift playlists configure --name "Discovery" --role inbox
$ chordrift playlists list
$ chordrift analyze summary
$ chordrift analyze overlap --limit 25
$ chordrift analyze duplicates --limit 25
```

Role and policy configuration is durable. Chordrift can publish managed
playlists and artwork, consume verified intake, and retire approved legacy
containers only through immutable plans, readiness proofs, exact phase
confirmation, resumable operation receipts, a post-write pull, and convergence
verification.

Spotify Platform content is retained as provider inventory and provenance. It
is not used to train an ML or AI model. Personal embeddings use Chordrift's
canonical and user-supplied signals within provider-policy limits.

The downloadable listening-history archive remains optional. When available,
`chordrift history ingest` enriches the library with account-scoped play counts,
duration, skips, completions, and recency without changing or replacing Web API
inventory snapshots. Neon remains authoritative; unchanged local ZIPs are
retained only for recovery and future reprocessing.

See [docs/HOW_TO_CHORDRIFT.md](docs/HOW_TO_CHORDRIFT.md) for the current
task-oriented v0.1.4 guide and table of contents, the linked CLI reference for
every released command,
[scripts/README.md](scripts/README.md) for operator and development helper
scripts,
[docs/design/PLAYLIST_PRODUCT_ARCHITECTURE.md](docs/design/PLAYLIST_PRODUCT_ARCHITECTURE.md)
for the v0.2 product and client direction,
[docs/design/ONBOARDING_SESSION_V020_06.md](docs/design/ONBOARDING_SESSION_V020_06.md)
for the implemented provider-read-only onboarding boundary,
[ROADMAP.md](ROADMAP.md) for planned milestones, and
[CHANGELOG.md](CHANGELOG.md) for release history. New focused development tasks
should begin with [CODEX_HANDOFF.md](CODEX_HANDOFF.md) for current decisions and
operational state.

## License

Chordrift is licensed under the [MIT License](LICENSE).
