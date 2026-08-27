# Account isolation and provider boundaries

This document records the current architecture boundary and the audit required
before Chordrift is presented to additional users or gains another live music
provider. It is not a claim that the current Spotify implementation is already
provider-neutral.

Status: active v0.2 boundary, updated 2026-08-27. V020-01 implemented the
provider-neutral application-contract vocabulary and capability-negotiation
foundation; V020-02 routes every existing CLI handler through the shared
application facade without behavioral change; V020-03 adds account-owned and
provider-qualified domain IDs plus typed capability reports; V020-04 proves the
pure boundary with two-account/two-provider adversarial tests and a deterministic
fake-provider harness. Storage queries and the Spotify adapter remain on the
v0.1.4 path until later roadmap slices.

## Implemented application boundary

The public Rust `contract` module now gives every future client the same
transport-neutral shapes for commands, queries, immutable views, lifecycle
events, progress, cooperative cancellation, structured client-safe errors,
request/operation/idempotency identity, and contract/schema/capability
negotiation. It contains no SQL rows, Spotify payloads, terminal presentation,
platform APIs, or execution engine.

This is a client boundary, not a claim that the current production implementation
is already multi-account or multi-provider. V020-02 routes the existing CLI
through one application facade without behavioral change; V020-03 supplies typed
ownership and provider-neutral domain values; V020-04 proves those boundaries in
an isolated harness without changing the production storage or Spotify paths.

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

This is the right relational shape for multiple accounts. The pure application
boundary now has a purpose-built isolation suite, but production storage,
credentials, and provider mutations still require the later end-to-end audit
before a friend's account is treated as a product trial.

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

## Remaining modularity and isolation work

Before a UI or second live provider, perform a full code and schema audit:

1. Map each provider adapter's authentication, inventory, playback, playlist,
   artwork, and saved-library behavior into the implemented capability-report
   foundation without pretending unsupported operations exist.
2. Keep canonical music identity and account intent in provider-independent
   domain modules. Provider payloads must stop at adapter boundaries.
3. Replace Spotify-named read models with provider-qualified generic views or
   adapter-owned queries without destroying immutable migration history.
4. Make every command construct an explicit account context; defaults are a UI
   convenience, not an ownership boundary.
5. Extend the completed pure two-account proof into storage/provider integration
   tests proving imports, classifications, exclusions, proposals, plans,
   credentials, and applies cannot cross accounts.
6. Populate a provider capability matrix so unsupported Apple/Spotify behavior
   is visible and safely deferred rather than emulated.
7. Keep IDs namespaced by provider and resolve cross-provider recordings through
   canonical identity evidence, never string coincidence.

## v0.2 client implication

Every token, playlist, correction, and review batch belongs to an explicit
account. A future UI may make account switching feel lightweight, but it must
never merge personal cohorts or intent across accounts unless the user creates
an explicit shared construct.
