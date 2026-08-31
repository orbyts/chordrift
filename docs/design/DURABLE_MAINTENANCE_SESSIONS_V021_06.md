# Durable maintenance sessions — V021-06

Status: persistence foundation proven; hosted interpretation and provider
effects remain disabled.

Ordinary maintenance is a Rust-owned task, not a terminal script or browser
workflow. Migration 0051 persists its current typed projection and an immutable
event for every accepted revision:

```text
new complete provider observation
              |
              v
PostgreSQL interpretation adapter (next gate)
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

Migration 0051 is additive but remains staged until the hosted maintenance
vertical slice is ready. The service must not advertise hosted maintenance or
accept `StartMaintenance`, `RefreshMaintenance`, `ResolveMaintenance`, or
`AuthorizeMaintenance` into the durable queue until:

1. the PostgreSQL adapter turns the newest complete provider observation and
   durable intent into the exact cumulative `MaintenanceProjection`;
2. start, refresh, and resolve execute through the worker and this store;
3. web and remote CLI render the same session after process restart;
4. fake-provider regressions prove record-only gestures never become provider
   writes; and
5. authorization/apply remains a separately opened, exact-review gate.
