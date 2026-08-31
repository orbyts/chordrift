# Durable maintenance sessions — V021-06

Status: persistence and record-only hosted interpretation are implemented on
the branch; provider effects remain disabled.

Ordinary maintenance is a Rust-owned task, not a terminal script or browser
workflow. Migration 0051 persists its current typed projection and an immutable
event for every accepted revision:

```text
new complete provider observation
              |
              v
PostgreSQL interpretation adapter
              |
              | MaintenanceProjection
              v
Rust MaintenanceWorkflow
              |
              | validated revision transition + compare-and-swap
              v
maintenance_sessions --------> fast current query for web / remote CLI
              |
              +-------------> maintenance_session_events
                               immutable intent, decisions, and reviews
```

The session row is scoped simultaneously to the authenticated Chordrift
subject, Chordrift account, and account-owned provider connection. A caller
from another tenant receives no session. Every replacement must be exactly the
next revision and must match the currently stored revision, so stale browser,
CLI, API, or worker processes cannot overwrite newer intent.

The stored projection contains client-safe track/surface labels, observed
changes, accepted resolutions, exact provider-effect reviews, and allowed
actions. It contains no product session, provider credential, database URL,
provider URL, shell command, or SQL supplied by a client. Rehydration validates
the same workflow invariants used when the projection was first created.

## Proof and activation boundary

The migration and restart/tenant/CAS path passed on disposable PostgreSQL 18 on
Vortex. The proof container, network, source copy, and root-owned build output
were removed immediately afterward. No Neon branch was created and Spotify was
not read or changed.

Migration 0051 is additive and remains staged until this vertical slice passes
deployment rehearsal. The branch accepts Start, Refresh, and Resolve into the
durable queue. It explicitly rejects Authorize. Start and Refresh perform a
new provider read before interpreting the resulting complete snapshot.

Completed branch proof:

1. the PostgreSQL adapter reuses the Rust maintenance planner, rejects every
   non-reconcile phase, and collapses paired move rows by provider track ID;
2. start, refresh, and resolve run through the worker and durable store;
3. web and remote CLI use the same session query DTO; and
4. unit, transport, fake-provider, shell-compatibility, and full Rust suites
   pass without enabling provider effects.

Remaining activation gates:

1. project accepted session resolutions back into canonical playlist intent so
   the next provider observation converges rather than merely preserving a
   parallel session ledger;
2. support saved-intake cleanup and other ordinary non-reconcile phases without
   weakening the explicit provider-write boundary;
3. run API + worker + browser + remote CLI against disposable PostgreSQL after
   process restart; and
4. open authorization/apply only as a separately reviewed and proven gate.
