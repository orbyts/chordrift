# Durable maintenance sessions — V021-06

Status: persistence, provider-first interpretation, canonical projection, and
the bounded saved-state authorization/apply/verification path are implemented
on the branch; live deployment remains gated.

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
deployment rehearsal. The branch accepts Start, Refresh, Resolve, and exact
Authorize commands into the durable queue. Start and Refresh perform a new
provider read before interpreting the resulting complete snapshot. Authorize
accepts only the displayed revision/review and the worker rederives trusted
effects before any provider call.

Completed branch proof:

1. the PostgreSQL adapter reuses the Rust maintenance planner, rejects every
   non-reconcile phase, and collapses paired move rows by provider track ID;
2. start, refresh, resolve, authorize, apply, observe, and verify run through
   the worker and durable store;
3. web and remote CLI use the same session query DTO; and
4. canonical projection and all seven execution transitions pass disposable
   PostgreSQL proof without contacting Spotify; and
5. unit, transport, fake-provider, shell-compatibility, and full Rust suites
   pass.

Remaining activation gates:

1. run API + worker + browser + remote CLI against disposable PostgreSQL after
   process restart; and
2. deploy the bounded path without enabling broader publication or retirement
   effects, then complete manual private-beta acceptance.
