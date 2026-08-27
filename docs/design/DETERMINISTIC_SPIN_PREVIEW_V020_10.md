# Deterministic Spin preview — V020-10

Status: implemented on the v0.2.0 development line. This is a Rust application
and migration-0046 persistence boundary, not a released v0.1.4 CLI command. It
does not call, mutate, or publish to a music provider.

## Outcome

V020-10 consumes V020-09's verified unordered `RecipeExecutionDraft` and
produces one exact immutable `SpinPreview`. The preview includes:

- a deterministic account-owned Spin identity and immutable recipe revision;
- canonical input, unordered-draft, and ordered-preview SHA-256 fingerprints;
- the complete unsigned 64-bit seed;
- the exact evidence capability snapshot;
- the requested target and honest unfilled-seat count;
- exact one-based playback positions;
- structured selection and ordering reasons for every track;
- planned and occupied narrative-section capacity; and
- explicit cadence, spacing, or unavailable-policy warnings.

The V020-09 draft now carries its recipe ordering narrative and can verify its
own payload fingerprint. V020-10 rejects a mutated draft, a capability snapshot
that disagrees with its evidence-source reports, duplicate canonical tracks,
the wrong recipe revision command, or any cross-account track/recipe value
before persistence.

## Deterministic ordering

Every selected track receives a SHA-256 ordering rank derived from the seed,
verified draft fingerprint, and canonical track identity. The orderer then
walks one-based positions and applies these constraints in order:

1. Fill a reserved familiar-anchor position from the familiar or high-rotation
   lanes when possible.
2. Preserve enough remaining familiar entries for later reserved positions.
3. Avoid an artist credited on the immediately preceding track when the recipe
   requests artist spacing and an alternative exists.
4. Apply the declared narrative:
   - `shuffle`: seed rank;
   - `smooth_transitions`: prefer nearby lifecycle lanes;
   - `intentional_contrast`: prefer a more distant lifecycle lane;
   - `sectioned_journey`: use deterministic lane preferences for warm-up,
     focus, and landing capacity.
5. Resolve every remaining tie by seed rank and canonical identity.

This is intentionally provider-neutral. “Smooth” and “contrast” refer only to
the lifecycle evidence present in recipe v1; the orderer does not invent audio
features. Familiarity shortfalls, unavoidable adjacent-artist repetition, and
unfilled source seats remain visible. Recipe-v1 guardrail categories that lack
an executable numeric policy—currently duration and cross-output reuse—also
remain explicit warnings rather than false enforcement claims.

## Explanations

Each `SpinTrackPreview` persists two schema-versioned reason objects:

- `TrackSelectionReason` records lane, exact collection/evidence source,
  candidate priority, source capability status, allocated source seats, artists,
  and a concise client-facing summary.
- `TrackOrderingReason` records narrative, optional section, familiar-anchor
  use, artist-spacing outcome, deterministic seed rank, and a concise summary.

The reasons are values owned by the Rust application boundary. Thin clients do
not reconstruct decisions from SQL rows or terminal text.

## Identity, persistence, and replay

The Spin UUID is deterministically derived from the owning Chordrift account,
canonical recipe input fingerprint, and seed. The ordered preview fingerprint
covers that identity, recipe/draft inputs, capabilities, exact ordered tracks
and reasons, sections, warnings, and the fact that playback order is assigned.

`SpinPreviewBoundary` persists the preview in the existing `playlist_spins` and
`playlist_spin_tracks` tables. The flexible `capability_snapshot` object retains
the exact evidence snapshot plus the versioned preview manifest; track rows
retain zero-based physical positions while the application view displays
one-based positions. No migration changed.

The existing uniqueness boundary on account, input fingerprint, and seed makes
creation idempotent. Replay loads and compares the complete stored value; any
conflict fails rather than replacing immutable history. The existing
`Query::SpinPreview` reads the account-scoped value through `ApplicationFacade`.

## Proof and non-effects

Provider-free tests prove identical order, identity, fingerprints, reasons,
cadence, sections, artist spacing, seed variation, capability mismatch,
cross-account rejection, and honest degraded output. The retained fresh-schema
PostgreSQL 18 rehearsal additionally proves:

- `u64::MAX` survives the schema's numeric seed representation;
- exact reason objects and positions are persisted;
- command replay and query display return the same preview;
- another account can neither create nor read the Spin; and
- no `playlist_spin_publications` row is created.

V020-10 adds no CLI command, migration, production configuration, credential
handling, live-provider request, publication approval, or provider write.
Migration 0046 remains unapplied to production Neon, and installed v0.1.4 use
is unchanged.

## Boundary for V020-11

`V020-11 — CLI-first product rehearsal` may expose consistent development-line
commands for onboarding, collections, recipe review, and Spin preview, plus an
installed-binary helper workflow. It must use these Rust-owned values rather
than reimplement ordering in the CLI, compare inventory-only and enriched
inputs honestly, and remain provider-write-free.
