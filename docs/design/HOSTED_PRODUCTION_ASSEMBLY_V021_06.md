# Hosted production assembly — V021-06

Status: implementation checkpoint; read-only provider observation and durable
maintenance-session persistence are composed, while ordinary maintenance
interpretation and provider writes remain disabled.

## Process boundary

```text
web / remote CLI
        |
        | authenticated typed CommandRequest
        v
Chordrift API ----------------------> PostgreSQL service_operations
  |                                      |
  | typed read queries                   | exclusive expiring lease
  v                                      v
PostgreSQL read models              Chordrift worker
                                         |
                                         | authorized account/provider identity
                                         v
                              encrypted credential vault
                                         |
                                         | short-lived decrypted lease
                                         v
                                Spotify Rust adapter
                                         |
                                         | complete read-only inventory
                                         v
                              atomic PostgreSQL persistence
```

The API never receives a refresh token and does not contact Spotify. It checks
that the authenticated Chordrift account owns the selected provider connection,
then durably accepts `ObserveProvider`. Idempotency and operation history are
stored before a worker starts. The browser follows the same operation DTOs that
the remote CLI uses.

The separately scoped worker is the only process that may decrypt a provider
credential. It rechecks current subject membership and provider ownership,
exchanges the leased refresh token for a short-lived access token, verifies the
stable Spotify account identity, rotates a replacement refresh credential when
Spotify supplies one, and invokes the existing Rust inventory importer
directly. It cannot invoke the Chordrift CLI, a shell, client SQL, or a
client-supplied provider URL.

## Failure and restart behavior

- Commands are accepted into migration-0050 durable storage before provider
  access.
- One worker owns a command through an expiring lease. A heartbeat renews the
  lease during long provider reads.
- Cancellation is account/subject/capability checked. Dropping an in-flight
  observation cancels HTTP work; any active PostgreSQL transaction rolls back.
- Recoverable failures use the persisted bounded retry policy. Another worker
  may recover an abandoned expired lease after process restart.
- Provider inventory is persisted atomically and becomes current only after the
  complete observation succeeds.
- Operation progress and completion remain queryable after API, worker, or
  browser restart.

## Container boundary

One pinned multi-stage build produces two executables in the same minimal,
non-root runtime image:

- `chordrift-server` — HTTPS-behind-proxy API, identity/session boundary,
  static browser client, typed queries, and durable command acceptance;
- `chordrift-worker` — provider credential lease and provider/database work,
  with no listening port.

Vortex Compose runs them as separate read-only services with dropped Linux
capabilities, bounded logs/resources, and the same host-only secret file. Only
the API publishes the private Nexus-facing port.

## Remaining production-assembly gate

Read-only `ObserveProvider` is the first production operation. The branch now
also routes `StartMaintenance`, `RefreshMaintenance`, and
`ResolveMaintenance` through the same durable worker. Start and refresh take a
fresh provider observation, the PostgreSQL adapter converts ordinary reconcile
work into one typed session projection, and web plus remote CLI query that same
session. Paired remove/add plan rows collapse into one logical move.

This is still a staged record-only boundary. The adapter does not yet commit
resolved session intent back into the canonical playlist model, and it rejects
cleanup, publication, retirement, and any unsupported plan phase. Provider
authorization is explicitly unavailable. Production assembly is not complete
until canonical intent projection, saved-intake handling, exact provider-effect
review, separately authorized apply, and verification are proven.

Migration 0051 now supplies the restart-safe task boundary described in
[Durable maintenance sessions](DURABLE_MAINTENANCE_SESSIONS_V021_06.md). The
adapter produces typed projections directly from PostgreSQL provider
observations through the Rust planner; it never delegates to the legacy shell
wizard.
