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
> Chordrift is in very early development. Version 0.0.1 establishes the
> database and CLI foundation; provider import and synchronization are not yet
> implemented.

## v0.0.1 capabilities

- Storexa-backed Neon PostgreSQL connection management
- an application-owned canonical music-library schema
- compile-time embedded SQLx migrations
- secret-safe database health and migration status
- a minimal command-line interface

Set the canonical Neon connection URL through the application-specific
`CHORDRIFT_DATABASE_URL` environment variable. Chordrift never prints it.

```console
$ chordrift --version
chordrift 0.0.1

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

The initial schema preserves boundaries for canonical and provider identities,
historical playlist membership, independently importable listening events,
versioned embeddings and clusters, proposed playlist generations, and
idempotent synchronization records. Those later workflows are not implemented
in v0.0.1.

See [ROADMAP.md](ROADMAP.md) for the planned milestones and
[CHANGELOG.md](CHANGELOG.md) for release history.

## License

Chordrift is licensed under the [MIT License](LICENSE).
