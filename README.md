# Chordrift

Chordrift is a personal music-library intelligence and synchronization system
for organizing a library across streaming services.

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
> Chordrift is in very early development. Version 0.0.5 adds versioned semantic
> embeddings, independently versioned personal signals, and explicit playlist
> evidence policies. Chordrift still performs no remote Spotify mutations.

## v0.0.5 capabilities

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
- idempotent account-data and extended streaming-history archive ingestion
- cumulative event deduplication across periodic overlapping Spotify exports
- account-scoped play, duration, skip, completion, and recency statistics
- Git-ignored local inbox/archive recovery that preserves original ZIPs
- immutable semantic embedding generations with reproducible parameters
- independently versioned listening and lifecycle signal generations
- explicit semantic, provider-curated, intake, canonical, transport, and
  ignored playlist evidence classes
- nearest-neighbor inspection before any embedding can feed clustering

Set the canonical Neon connection URL through the application-specific
`CHORDRIFT_DATABASE_URL` environment variable. Chordrift never prints it.

```console
$ chordrift --version
chordrift 0.0.5

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

Authorization requests only `playlist-read-private`,
`playlist-read-collaborative`, and `user-library-read`. The refresh token is
stored under an account-scoped entry in macOS Passwords/Keychain; it is never
written to a shell initialization file or the database. `spotify logout`
removes that local credential without revoking access in Spotify.

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

For routine use, `chordrift sync pull` imports the current Spotify state and
then refreshes account-scoped canonical analysis in the same invocation. The
local account label defaults to `personal`, while the stable Spotify user ID is
persisted in Neon; playlist identities and roles are also scoped to that
account. A playlist rename therefore does not create a new identity, and a
playlist that disappears from the latest snapshot remains historically known
but is marked absent.

Imported playlists begin as `observed` with a `provider-wins` drift policy.
Discovery surfaces can be marked as `inbox`, and future Chordrift outputs as
`managed`:

```console
$ chordrift playlists configure --name "Discovery" --role inbox
$ chordrift playlists list
$ chordrift analyze summary
$ chordrift analyze overlap --limit 25
$ chordrift analyze duplicates --limit 25
```

Role and policy configuration is durable, but v0.0.5 remains pull-only. A
future dry-run/apply milestone will use `neon-wins` for managed playlists and
will require explicit, auditable approval before changing Spotify.

Spotify Platform content is retained as provider inventory and provenance. It
will not be used to train an ML or AI model. Later personal embeddings will use
Chordrift's canonical and user-supplied signals within provider-policy limits.

The downloadable listening-history archive remains optional. When available,
`chordrift history ingest` enriches the library with account-scoped play counts,
duration, skips, completions, and recency without changing or replacing Web API
inventory snapshots. Neon remains authoritative; unchanged local ZIPs are
retained only for recovery and future reprocessing.

See [docs/HOW_TO_CHORDRIFT.md](docs/HOW_TO_CHORDRIFT.md) for the complete CLI
guide, [ROADMAP.md](ROADMAP.md) for planned milestones, and
[CHANGELOG.md](CHANGELOG.md) for release history. New focused development tasks
should begin with [CODEX_HANDOFF.md](CODEX_HANDOFF.md) for current decisions and
operational state.

## License

Chordrift is licensed under the [MIT License](LICENSE).
