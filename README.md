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
- **Intake surfaces** record how music entered the system or that it needs
  attention. Liked Songs is the lowest-friction intake; named inboxes retain
  richer provenance; `Re-evaluate` holds corrections without teaching a false
  destination.
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
credential store, with Apogee providing approved environment configuration for
development; they never belong in project files, logs, command history, or
shell startup files. Long work reports useful progress, can be cancelled, and
resumes safely after interruption.

Existing immutable plans, readiness assessments, resumable apply history,
post-write pulls, and zero-operation convergence remain the execution
foundation for the future UI and background agent.

Neon PostgreSQL will be the canonical source of truth. Spotify and Apple Music
will be provider adapters rather than competing authorities. Future releases
will use playlist history, personal listening context, and versioned embeddings
to organize tracks, and will support Spatial Audio-aware playlist variants in
Apple Music.

Spotify's downloadable listening-history export will be optional enrichment.
Library import, canonical identity, playlist analysis, matching, organization,
and synchronization will not wait for it; when available, the export will add
signals such as play counts, listening duration, first play, and last play.

> [!WARNING]
> Chordrift is in early development. v0.1.3 permits remote Spotify
> mutation only through exact, audited, resumable phase confirmations. Never
> run an apply command without inspecting its immutable plan and readiness ID.

## Current foundation (v0.1.3)

- Storexa-backed Neon PostgreSQL connection management
- an application-owned canonical music-library schema
- compile-time embedded SQLx migrations
- secret-safe database health and migration status
- Spotify Authorization Code with PKCE for account authorization
- refresh-token storage in the operating system credential store
- atomic snapshots of owned and accessible collaborative playlists
- ordered playlist membership that preserves duplicate entries
- saved-track snapshots kept separate from playlists
- provider metadata and stable Spotify identities for later canonical matching
- a one-command incremental pull that leaves Neon current with Spotify edits
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

Set the canonical Neon connection URL through the application-specific
`CHORDRIFT_DATABASE_URL` environment variable. Chordrift never prints it.

```console
$ chordrift --version
chordrift 0.1.3

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
traffic. Unchanged playlists are detected by Spotify `snapshot_id` and copied
forward inside Neon without requesting their items again. Saved tracks use a
single newest-page probe; when its total and leading signature match, the prior
saved-library snapshot is copied forward without downloading the remaining
pages. A detected change triggers a complete reconciliation so removals are not
silently missed.

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
future Chordrift outputs as `managed`:

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
will not be used to train an ML or AI model. Later personal embeddings will use
Chordrift's canonical and user-supplied signals within provider-policy limits.

The downloadable listening-history archive remains optional. When available,
`chordrift history ingest` enriches the library with account-scoped play counts,
duration, skips, completions, and recency without changing or replacing Web API
inventory snapshots. Neon remains authoritative; unchanged local ZIPs are
retained only for recovery and future reprocessing.

See [docs/HOW_TO_CHORDRIFT.md](docs/HOW_TO_CHORDRIFT.md) for the task-oriented
guide and table of contents, the linked CLI reference for every command,
[ROADMAP.md](ROADMAP.md) for planned milestones, and
[CHANGELOG.md](CHANGELOG.md) for release history. New focused development tasks
should begin with [CODEX_HANDOFF.md](CODEX_HANDOFF.md) for current decisions and
operational state.

## License

Chordrift is licensed under the [MIT License](LICENSE).
