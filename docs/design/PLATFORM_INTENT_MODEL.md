# Platform interaction model

The product goal is for people to keep using Spotify or Apple Music while
Chordrift operates as a preservation-first assistant behind the scenes. Neon is
the durable interpretation ledger; provider changes are observations, not
unquestioned commands.

Status: active v0.2 interaction policy, updated 2026-08-27. The lifecycle,
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

## Confidence policy

Chordrift should automate only high-confidence, reversible interpretations.
Ambiguous changes become staged proposals with a concise question or preview.
Destructive or history-erasing interpretations are never inferred silently.

| Observed provider change | Plausible intent | Product behavior |
| --- | --- | --- |
| Add to a named intake | New discovery with explicit provenance | Capture automatically and later propose placement. |
| Add to `Re-evaluate` and remove from the wrong destination | Keep track; reject current destination | Capture automatically as zero-signal corrective intent. |
| Remove from one verified canonical playlist | Exclude, refile, or temporary edit | Stage interpretation; use surrounding actions to disambiguate. |
| Remove from canonical and add to another | Deliberate move or destination preference | Propose a move and learn only after confirmation. |
| Add directly to a canonical playlist | Destination preference or one-off manual choice | Stage a preference; ask whether it should become a lock. |
| Reorder a canonical playlist | Exact-order preference or casual queue editing | Ask whether to lock order; do not silently retrain vibe placement. |
| Remove from Liked Songs | Unsave | Record saved-state change without implying exclusion. |
| Delete or unfollow an external playlist | Remove library relationship | Preserve bookmark/history; never edit the external owner's source. |
| Delete an entire owned playlist | Retirement, replacement, or accident | Require explicit bounded retirement/recovery decision. |

## Context improves inference

Intent should be inferred from a short sequence, not one isolated mutation. For
example, removing a track from `Tidal Hush` and immediately adding it to
`Re-evaluate` is strong reclassification evidence. Removing it with no
other action is ambiguous between exclusion and wrong placement. Removing it
from every active playlist and unsaving it is stronger rejection evidence, but
Chordrift should still retain the historical record.

## Hosted and native-client interaction

The eventual UI should appear only when useful:

- a passive “captured” acknowledgment for high-confidence intake and
  Re-evaluate transitions;
- a compact confirmation for ambiguous delete/move/order changes;
- an exact preview when a new poetic playlist, name, or artwork is proposed;
- a reversible history view explaining why a track moved or disappeared;
- a clear unresolved queue rather than silent drift.

For the personal CLI, the same model is deliberately explicit: pull, inspect,
plan, approve, apply one phase, and pull again. Each edge case proven here
becomes a product rule rather than hidden operator knowledge.
