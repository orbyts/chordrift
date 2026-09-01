# Provider-first convergence

Status: normative ordinary-maintenance design as of the v0.2.1-alpha.18
checkpoint.

Spotify is the first provider implementation, not a special source of domain
behavior. For ordinary maintenance, the newest complete provider observation is
the user's intended library state. Neon retains immutable observations,
accepted comparison baselines, inferred intent, exclusions, and audit history;
it does not silently push an older model back to the provider.

Provider writes have a separate origin. They occur only after an explicit user
request such as publishing a new managed surface, restoring an excluded track,
retiring a playlist, or publishing a Spin. A maintenance pull never inherits
that authority.

```mermaid
sequenceDiagram
    autonumber
    actor User
    participant Provider as Music provider
    participant Client as Thin client (CLI/Web/App)
    participant Core as Chordrift Rust core
    participant Neon as Neon history + intent

    User->>Provider: Add, remove, move, rename, or reorder
    User->>Client: Sync
    Client->>Core: Start maintenance pull
    Core->>Provider: Read complete account snapshot
    Provider-->>Core: Snapshot Sₙ
    Core->>Neon: Persist immutable observation Sₙ
    Core->>Neon: Load accepted baseline Aₙ₋₁ + current intent
    Core->>Core: Classify cumulative delta Aₙ₋₁ → Sₙ

    alt Liked intake is already in a managed destination
        Core->>Neon: Read prior keep/clear directive for virtual Liked Songs surface
        Core->>Neon: Read current destination position and track count
        alt A prior decision exists
            Neon-->>Core: Reuse remembered keep or clear intent
        else No prior decision exists
            Core-->>Client: Rediscovered favorite; already in Destination X at position P of N
            Note over Client,Core: Future refinement may offer Keep position or Move to top
            Core-->>Client: Keep it in Likes too?
            User->>Client: Keep or clear after verified placement
            Client->>Core: Resolve exact saved-intake decision
            Core->>Neon: Persist revisioned surface directive
        end
        alt Keep in Likes
            Core->>Neon: Preserve both memberships as intended
        else Clear Likes
            Core-->>Client: Review exact Remove from Likes effect
            User->>Client: Authorize exact saved-state change
            Client->>Core: Authorize current review
            Core->>Provider: Remove only the saved/liked state
            Provider-->>Core: Updated complete snapshot Sₙ₊₁
            Core->>Neon: Persist and verify destination remains and Like is absent
        end
    else Placement is confidently resolved
        Core->>Neon: Record observed or inferred destination intent
        Note over Core,Neon: Future classifier confidence may auto-resolve placement meaning
        opt Destination membership does not already exist
            Core-->>Client: Review exact suggested destination addition
            User->>Client: Authorize exact placement effect
            Client->>Core: Authorize current review
            Core->>Provider: Add only the enumerated track at the top
            Provider-->>Core: Updated complete snapshot Sₙ₊₁
            Core->>Neon: Persist and verify placement
            opt Track also remains in a temporary intake surface
                Core-->>Client: New exact review to consume intake
                User->>Client: Authorize intake cleanup separately
                Client->>Core: Authorize current cleanup review
                Core->>Provider: Remove only the verified temporary membership
                Provider-->>Core: Updated complete snapshot Sₙ₊₂
                Core->>Neon: Verify destination remains before accepting cleanup
            end
        end
    else Destination or meaning is genuinely ambiguous
        Core-->>Client: Return bounded decision request
        User->>Client: Confirm suggested destination, select another, or exclude
        Client->>Core: Resolve exact task revision
        Core->>Neon: Record the decision
    else Other provider-authored change is unambiguous
        Core->>Neon: Record moves, order, names, and removals
        Note over Core,Neon: Managed removal creates an active exclusion; no provider write
    end

    Core->>Core: Prove Neon model equals newest complete snapshot exactly
    Core->>Neon: Commit newest snapshot as accepted baseline Aₙ
    Core-->>Client: Converged; zero provider writes

    opt User explicitly requests publication
        User->>Client: Create / restore / retire / Spin
        Client->>Core: Authorize exact reviewed operation
        Core->>Neon: Persist immutable publication plan
        Core->>Provider: Apply idempotent authorized writes
        loop Bounded provider-observation lag
            Core->>Provider: Read current state
            Provider-->>Core: Older or updated snapshot
        end
        Core->>Neon: Verify and accept observed result
        Core-->>Client: Publication verified or safely pending
    end
```

An already-placed track entering Liked Songs is not merely redundant intake. It
is also a **rediscovery signal**. Human-facing clients should therefore name the
managed destination and report the track's current one-based position and the
playlist length, for example `Neon Affection — position 37 of 92`. The position
is the provider's canonical playlist occurrence, not a temporary title, artist,
album, or date-added sort selected in a Spotify client.

The current contract remembers only whether to keep or clear the saved state.
A later, batched experience refinement may additionally offer **keep current
position** or **move this occurrence to the top**. Moving it is a distinct,
explicitly reviewed provider reorder; the Like itself is useful evidence but
does not authorize Chordrift to reorder anything automatically.

A genuinely new Liked-only track that receives a new canonical destination is
different: its beta placement default is the top of that destination. This
policy is owned by the Rust application layer and is named in the exact review.
It does not reorder an already-present rediscovered favorite. A future contract
may expose `top`, `bottom`, or an exact position without moving that policy into
the web, CLI, iOS, Android, or provider adapter.

## Convergence rules

1. A pull always starts from a new complete provider observation. If the API is
   temporarily stale, Chordrift records no invented delta and sees the cumulative
   change on a later pull.
2. Interrupted or rejected maintenance does not make an older intermediate plan
   authoritative. The next complete observation is compared with the last
   accepted baseline, so changes accumulate safely.
3. Direct provider additions and moves are placement evidence. Membership-equal
   reordering is provider-authored order. Neither requires a provider write.
4. Liked/Saved is an intake surface, not canonical placement. When a liked track
   is already in a verified managed destination, Chordrift names that destination
   and asks whether the saved state should remain. The answer is a revisioned
   surface directive: `keep` suppresses future cleanup; `clear` permits one exact
   confirmed saved-state removal. No answer means no removal.
5. A later direct provider-side Unlike is a newer user decision. The next exact
   accepted observation supersedes an older keep directive without restoring the
   Like. The user may instead change the directive through Chordrift and authorize
   the same exact removal.
6. A track removed from an accepted managed membership becomes actively
   excluded. The exclusion retains identity and history while preventing an
   automatic re-add.
7. Re-adding an excluded track through the provider is an explicit resurrection
   gesture. Emptying the exclusion archive is a separate Neon-only operation;
   it resolves the visible archive entry, retains audit history plus an internal
   forget tombstone so older placement cannot replay, and is refused while an
   excluded track is still in the observed provider library.
8. An observation becomes the next baseline only after exact ordered equality.
   A partial proposal, unresolved decision, or provider-lagged publication can
   never be accepted accidentally.
9. Only a separately originated, exactly reviewed publication task may write to
   a provider. Ordinary maintenance is record-only except for a user-authorized
   intake publication that is explicitly represented in that task.

## Current commands

The temporary CLI wrapper uses the same core boundaries:

```console
$ scripts/chordrift-maintain.sh --account personal
$ chordrift intake audit --account personal
$ chordrift intake liked-disposition --account personal \
    --spotify-id SPOTIFY_TRACK_ID --disposition preserve \
    --reason "Keep this track in Liked Songs"
$ chordrift tracks exclusions --account personal
$ chordrift tracks empty-exclusions --account personal --confirm personal
```

`chordrift sync accept-current` is the low-level adapter used by the maintenance
wrapper after exact convergence. Web and mobile clients must invoke the typed
application operation rather than reproduce this orchestration.

## Diagram evolution

This provider-first sequence remains the canonical ordinary-maintenance flow.
Future diagrams may expand the optional publication branch into named Create,
Restore, Retire, and Spin lifecycles, but those branches must retain a separate
operation origin, exact review, idempotent apply, bounded observation lag, and
verified result. They must never turn an ordinary pull into implicit provider
write authority.
