# Provider-first convergence

Status: normative ordinary-maintenance design as of v0.2.1-alpha.14.

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

    alt Provider-authored change is unambiguous
        Core->>Neon: Record additions, moves, order, names, and removals
        Note over Core,Neon: Removal creates an active exclusion; no provider write
    else Destination or meaning is genuinely ambiguous
        Core-->>Client: Return bounded decision request
        User->>Client: Select destination or exclude
        Client->>Core: Resolve exact task revision
        Core->>Neon: Record the decision
    end

    Core->>Core: Prove Neon model equals Sₙ exactly
    Core->>Neon: Commit Sₙ as accepted baseline Aₙ
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

## Convergence rules

1. A pull always starts from a new complete provider observation. If the API is
   temporarily stale, Chordrift records no invented delta and sees the cumulative
   change on a later pull.
2. Interrupted or rejected maintenance does not make an older intermediate plan
   authoritative. The next complete observation is compared with the last
   accepted baseline, so changes accumulate safely.
3. Direct provider additions and moves are placement evidence. Membership-equal
   reordering is provider-authored order. Neither requires a provider write.
4. A track removed from an accepted managed membership becomes actively
   excluded. The exclusion retains identity and history while preventing an
   automatic re-add.
5. Re-adding an excluded track through the provider is an explicit resurrection
   gesture. Emptying the exclusion archive is a separate Neon-only operation;
   it resolves the visible archive entry, retains audit history plus an internal
   forget tombstone so older placement cannot replay, and is refused while an
   excluded track is still in the observed provider library.
6. An observation becomes the next baseline only after exact ordered equality.
   A partial proposal, unresolved decision, or provider-lagged publication can
   never be accepted accidentally.
7. Only a separately originated, exactly reviewed publication task may write to
   a provider. Ordinary maintenance is record-only except for a user-authorized
   intake publication that is explicitly represented in that task.

## Current commands

The temporary CLI wrapper uses the same core boundaries:

```console
$ scripts/chordrift-maintain.sh --account personal
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
