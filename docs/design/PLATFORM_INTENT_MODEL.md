# Platform interaction model

The product goal is for people to keep using Spotify or Apple Music while
Chordrift operates as a preservation-first assistant behind the scenes. Neon is
the durable interpretation ledger. A user action already completed on the
provider is authoritative evidence of that exact action: Chordrift records it
and does not ask to write it back. The action is not, by itself, authority for
a broader semantic conclusion such as a permanent exclusion, classification
rule, or reusable cadence policy.

Status: active v0.2 interaction policy, updated 2026-08-29. The lifecycle,
progress, cancellation, structured-error, and capability shapes needed to
present this policy are implemented in V020-01; existing v0.1.4 CLI handlers
now pass through the V020-02 application facade without behavioral change.
V020-03 adds typed provider/evidence capabilities and independent playlist-
surface authority, purpose, and refresh values. V020-04 proves cancellation,
retry, idempotency, isolation, and visible capability gaps in a test-only fake
application/provider harness; automated interpretation and learning remain
later work. V020-05 adds the account-owned storage foundation for future
onboarding inputs, collection intent, surface directives, recipes, and Spins;
V020-06 captures selected onboarding inventory/evidence and explicit
no-intent/no-write provenance through application code. V020-07 now reports
observable current-library shape and a conservative starter organization while
explicitly labeling user intent, listening behavior, and collection membership
as not inferred. The proposal is unapproved and writes no intent or recipe.
V020-08 adds directly observed listening, repetition, long-span, completion,
skip, and history-only facts from one selected import, but explicitly does not
translate those observations into preference, favorite status, collection
membership, or approved intent.
V020-09 consumes only explicit recipe inputs and prepared candidate facts. Its
deterministic selection draft enforces explicit exclusion and hard collection
boundaries, reports unavailable evidence instead of guessing, and creates no
approved collection intent or provider change. Exact ordered Spin presentation
is now implemented in V020-10 as a provider-free immutable preview. Every track
has structured selection and ordering reasons, and shortfalls remain warnings;
approval, publication, and provider mutation remain later boundaries.

## Observation is the default; provider mutation is explicit

For user-owned, user-editable Spotify surfaces, ordinary maintenance follows
one directional rule:

1. the user edits membership, placement, metadata, or order in Spotify;
2. Chordrift observes and records the resulting state and exact gesture in
   Neon; and
3. Chordrift performs no Spotify write merely to accept what the user already
   did.

This includes reordering. If exact playlist membership is unchanged, the
observed Spotify order becomes current playlist intent under an exact
membership-equality guard. Chordrift must not generate or apply a compensating
`reorder_playlist` operation.

The opposite direction is a separate product operation. A Spin, approved
playlist design, restoration, or other Chordrift-authored change may propose a
new membership or order. It may write to Spotify only after the person reviews
and authorizes that understandable operation. Spin publication uses its
distinct `spin_publication` plan origin and never borrows ordinary-maintenance
authorization.

### Cumulative pulls and interrupted work

Each complete, internally consistent pull supersedes the previous provider
snapshot as the current baseline for user-authority state. Chordrift folds the
new delta on top of already recorded gestures; it does not require the user to
repeat an earlier edit because a wizard stopped between Neon-only revisions.

Any plan or readiness assessment bound to the older snapshot becomes stale.
Record-only interpretation may be rebased and continued automatically. An
unfinished Chordrift-authored operation keeps its explicit product intent, but
Chordrift must rebuild and reauthorize its provider plan against the newest
snapshot. It may never apply an old plan merely because it was previously
approved.

Convergence may require more than one internal revision: recording direct
intake can expose a membership-equal order delta in the next plan. Ordinary
maintenance absorbs such Neon-only deltas to a bounded fixed point before it
classifies remaining work or considers any provider apply.

## Confidence policy

Chordrift should automate only high-confidence, reversible interpretations.
Ambiguous changes become staged proposals with a concise question or preview.
Destructive or history-erasing interpretations are never inferred silently.

| Observed provider change | Plausible intent | Product behavior |
| --- | --- | --- |
| Add to a named intake | New discovery with explicit provenance | Capture automatically and later propose placement. |
| Remove from one verified canonical playlist | Exclude, refile, or temporary edit | Stage interpretation; use surrounding actions to disambiguate. |
| Remove from canonical and add to exactly one other managed playlist | Deliberate reclassification | Infer the move, show it in the maintenance review, and retain the confirmed correction as evidence. |
| Remove, observe the absence, then later add to exactly one managed playlist | Restoration or delayed reclassification | Supersede the active exclusion with the newest single placement regardless of elapsed time, retain both events, and perform no provider write. Several current destinations remain ambiguous. |
| Add directly to a canonical playlist | Explicit current placement | Preserve and record the destination automatically; treat any permanent lock or classification claim as a separate inference. |
| Reorder a canonical playlist | Explicit current sequence | Accept the observed provider order in Neon when exact membership is equal; do not write a reorder back or silently infer a cadence/classification rule. |
| Remove from Liked Songs | Unsave | Record saved-state change without implying exclusion. |
| Delete or unfollow an external playlist | Remove library relationship | Preserve bookmark/history; never edit the external owner's source. |
| Delete an entire owned playlist | Explicit provider removal; possibly retirement, replacement, or accident | Record it as absent and do not recreate it automatically; offer a bounded recovery/retirement decision only if further Chordrift action is useful. |

## Context improves inference

Intent should be inferred from a short sequence, not one isolated mutation. For
example, removing a track from `Tidal Hush` and adding it to `Dakshina Pulse`
is strong reclassification evidence. Removing it with no other managed
destination is ambiguous between exclusion and wrong placement. Removing it
from every active playlist and unsaving it is stronger rejection evidence, but
Chordrift should still retain the historical record.

## Hosted and native-client interaction

The eventual UI should appear only when useful:

- a passive “captured” acknowledgment for high-confidence intake and direct
  reclassification transitions;
- a compact question only when the broader meaning of an observed action is
  ambiguous, never to duplicate-authorize the provider action itself;
- an exact preview when a new poetic playlist, name, or artwork is proposed;
- a reversible history view explaining why a track moved or disappeared;
- a clear unresolved queue rather than silent drift.

For the personal CLI, the same model is deliberately explicit: pull, inspect,
plan, approve, apply one phase, and pull again. Each edge case proven here
becomes a product rule rather than hidden operator knowledge.

The earlier `Re-evaluate` holding queue is retired in v0.2.0. Its durable events
remain historical evidence, but current clients must not recreate it or require
it for correction handling.
