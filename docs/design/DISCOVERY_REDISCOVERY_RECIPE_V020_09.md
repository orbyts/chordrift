# Discovery + Rediscovery recipe execution — V020-09

Status: implemented on the v0.2.0 development line. This is a Rust application
boundary, not a released v0.1.4 CLI command. It performs no provider call,
database write, or Spin persistence.

## Outcome

V020-09 turns one immutable `RecipeV1` revision and a captured set of
provider-neutral candidates into a deterministic **unordered selection draft**.
It establishes the selection half of generation while keeping the exact ordered,
persisted Spin in V020-10.

The executor accepts:

- an account-owned immutable recipe revision;
- a nonzero target track count;
- candidates tied to exactly one source lane and immutable collection or
  evidence source;
- the capability snapshot that says whether each evidence source is available,
  degraded, or unavailable;
- required collection boundaries; and
- nonzero per-track and per-artist selection budgets.

Candidate order is normalized before execution. Both the canonical input and
the complete draft payload receive SHA-256 fingerprints, so replay is
independent of the caller's input order. A canonical track may repeat within
its one primary assignment when the explicit track budget permits, but it may
not claim more than one lane/source assignment in the same execution.

## Selection behavior

Positive source weights receive seats through deterministic largest-remainder
allocation. Familiar or high-rotation capacity is then reserved for each
one-based familiarity-cadence position. A degraded evidence source remains
enabled and is labeled degraded; an unavailable evidence source receives no
seats and is reported rather than silently replaced with invented evidence.
Execution fails with a capability error if no positively weighted source is
usable.

Candidates are considered in stable priority and identity order. Selection
requires all of the following:

- presence in the captured current inventory;
- a playable canonical recording identity;
- no explicit exclusion;
- membership in every required hard-boundary collection;
- room in the canonical-track repetition budget; and
- room in every credited canonical artist's budget.

Every eligibility or budget rejection is counted by a stable reason. Seats that
cannot be filled remain visible as `unfilled_seats`; the executor does not
weaken boundaries or synthesize candidates to hit the target.

## Cadence, sections, and guardrails

The draft reserves familiar-anchor positions and reports whether enough
familiar selections exist to satisfy them later. For a sectioned journey it
also divides target capacity among the immutable narrative sections. These are
capacity plans, not track positions.

Guardrail handling is explicit:

- hard boundaries and artist repetition are enforced during selection;
- artist spacing, duration, and cross-output reuse are marked as deferred when
  they require exact order or prior-Spin state.

The selected entries are returned in canonical identity order solely to make
serialization stable. `playback_order_assigned` is always false. Treating this
order as playback order would violate the boundary.

## Proof and non-effects

Provider-free tests exercise the executor through `ApplicationFacade` and prove:

- identical output and fingerprints after candidate input order is reversed;
- deterministic weighted allocation, cadence capacity, and narrative seats;
- visible degraded and disabled evidence behavior;
- explicit current-inventory, playability, exclusion, hard-boundary, track-
  repetition, and artist-budget rejection;
- cross-account ownership rejection and capability-safe failure; and
- rejection of ambiguous multi-lane/source assignment for one canonical track;
- no exact playback ordering.

V020-09 adds no migration and does not access PostgreSQL. Migration 0046 remains
the already rehearsed additive persistence foundation and remains unapplied to
production Neon. The installed v0.1.4 workflow is unchanged.

## V020-10 consumer

`V020-10 — Deterministic Spin preview` now consumes this verified draft, using
the ordering narrative carried in the draft rather than its canonical
serialization order. It assigns and persists exact tracks, selection/ordering
reasons, recipe revision, capability snapshot, input fingerprint, and seed, and
proves identical replay. See the
[focused Spin record](DETERMINISTIC_SPIN_PREVIEW_V020_10.md).
