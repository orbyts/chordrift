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
> Chordrift is in very early development. Version 0.0.0 reserves the crate
> namespace only and contains no application functionality.

See [ROADMAP.md](ROADMAP.md) for the planned milestones.

## License

Chordrift is licensed under the [MIT License](LICENSE).

