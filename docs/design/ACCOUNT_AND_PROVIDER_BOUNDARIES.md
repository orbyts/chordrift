# Account isolation and provider boundaries

This document records the current architecture boundary and the audit required
before Chordrift is presented to additional users or gains another live music
provider. It is not a claim that the current Spotify implementation is already
provider-neutral.

## Current account model

- `provider_accounts` is the root of account-specific operational state.
- Provider snapshots, playlist policy, listening statistics, signals,
  proposals, plans, apply runs, exclusions, classifications, and review batches
  carry a `provider_account_id` directly or inherit it through an immutable
  parent.
- Canonical recordings, artists, albums, and externally sourced metadata may be
  shared across accounts; personal membership, preference, intent, and user
  classification may not.
- Credentials are isolated by provider and local account label.
- Classification commands resolve tracks inside the selected account's
  preserved-library universe. A track imported only for another account is not
  sufficient authorization to annotate it.

This is the right relational shape for multiple accounts. It still needs a
purpose-built isolation test suite before a friend's account is treated as a
product trial.

## Spotify-specific boundaries that remain

The domain is not yet fully platform-neutral:

- OAuth, library import, Recently Played, artwork upload, playlist mutation,
  saved-track and saved-album cleanup are Spotify adapters.
- Some views and fields use Spotify-specific names such as
  `current_spotify_playlists`, `spotify_id`, and Spotify OAuth scopes.
- Sync planning stores provider-neutral operation names, but apply execution and
  capability checks currently assume Spotify behavior.
- CLI selection and CSV review use Spotify track IDs as the available stable
  external identity.
- Apple Music exists as a deferred branch/foundation and has not been validated
  with live credentials.

## Required modularity audit

Before a UI or second live provider, perform a full code and schema audit:

1. Define a provider capability contract for authentication, inventory,
   playback observations, playlist writes, artwork, saved-library mutation, and
   unsupported operations.
2. Keep canonical music identity and account intent in provider-independent
   domain modules. Provider payloads must stop at adapter boundaries.
3. Replace Spotify-named read models with provider-qualified generic views or
   adapter-owned queries without destroying immutable migration history.
4. Make every command construct an explicit account context; defaults are a UI
   convenience, not an ownership boundary.
5. Add two-account adversarial tests proving that imports, classifications,
   exclusions, proposals, plans, credentials, and applies cannot cross accounts.
6. Add a provider capability matrix so unsupported Apple/Spotify behavior is
   visible and safely deferred rather than emulated.
7. Keep IDs namespaced by provider and resolve cross-provider recordings through
   canonical identity evidence, never string coincidence.

## Future UI implication

Every token, playlist, correction, and review batch belongs to an explicit
account. A future UI may make account switching feel lightweight, but it must
never merge personal cohorts or intent across accounts unless the user creates
an explicit shared construct.
