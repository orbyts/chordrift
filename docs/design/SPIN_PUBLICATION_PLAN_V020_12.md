# Spin publication-plan integration — V020-12

Status: implemented on the v0.2 development line. No production provider
adapter is wired to this boundary, no Spotify request is made, and migrations
0046/0047 remain unapplied to production Neon.

## Outcome

An explicitly approved, account-owned immutable Spin can now become an
immutable synchronization-ledger plan with `plan_origin: spin_publication`.
The plan links the Spin, its matching renewable surface, one provider
connection, and one immutable provider inventory checkpoint through
`playlist_spin_publications`. Identical planning input reuses the same plan.

This is internal safety machinery. A future client should present the human
effect—publish this Spin to this surface—and may keep the plan ID, readiness
identity, and verification receipt in progress/details views. The user does not
need to operate those IDs during ordinary use.

## Boundaries

`SpinPublicationBoundary` validates all of the following before persistence:

- the `ApprovePublication` command and negotiated contract version;
- one account owns the Spin, surface, and provider connection;
- the immutable recipe revision is approved;
- the surface is active, renewable, Chordrift or mixed-authority, manual or
  scheduled, and bound to the Spin's recipe;
- a current immutable provider checkpoint exists;
- every non-excluded Spin track has an identity in the selected provider;
- active surface-specific exclusions are omitted.

The resulting publish phase contains only an optional target creation and
individual enumerated `add_track` operations. It contains no remove, replace,
or reorder operation and has no implicit “full desired membership.” Existing
user-added or otherwise unrelated live membership is therefore outside the
write set. An active exclusion cannot be restored merely because the track is
still present in the approved Spin.

## Readiness, fake apply, and verification

The public `SpinPublicationProvider` port is intentionally not implemented by
Spotify in this slice. A readiness value binds one plan hash to one observed
checkpoint and baseline membership. The fake execution path:

1. re-observes and rejects stale checkpoint or membership state;
2. creates a missing target only when enumerated by the plan;
3. appends only the exact planned provider track IDs;
4. treats already-present planned additions as idempotent replay;
5. re-observes and proves every baseline track survived and every planned
   addition is present.

Tests prove unrelated live membership survives, fake execution/replay does not
duplicate work, stale readiness fails, and a manually removed track represented
by an active exclusion cannot become an implicit addition.

## Additive schema evolution

Migration 0047 adds an optional account-safe recipe link to
`playlist_surfaces` and lets the existing `sync_runs` dry-run identity support
either a legacy maintenance proposal or a checkpoint-bound
`spin_publication` plan. It adds no replacement product table. Migration 0046's
Spin/publication link and the existing plan/operation ledger remain the
foundation.

## Deliberate limits

- No production Spotify mutation path implements the new provider port.
- No released v0.1.4 command changes.
- No automatic provider-change inference is implemented here; the architecture
  records that product rule for later observation/intent slices.
- No destructive replacement, cleanup, retirement, artwork, scheduling,
  hosted transport, or native-client work is included.
