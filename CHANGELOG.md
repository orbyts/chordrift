# Changelog

All notable changes to Chordrift are documented here.

## [Unreleased]

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

[Unreleased]: https://github.com/orbyts/chordrift/compare/v0.0.2...HEAD
[0.0.2]: https://github.com/orbyts/chordrift/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/orbyts/chordrift/compare/v0.0.0...v0.0.1
[0.0.0]: https://github.com/orbyts/chordrift/releases/tag/v0.0.0
