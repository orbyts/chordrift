# Hosted production assembly — V021-06

Status: implementation checkpoint; read-only provider observation, durable
maintenance sessions, provider-first interpretation, canonical intent
projection, saved-intake decisions, and exact saved-state
authorization/apply/verification are composed. Live deployment and acceptance
remain gated.

## Three-layer product boundary

```text
CLI / web / later iOS and Android
        |  presentation, authentication, typed DTOs only
        v
Rust application and workflow authority
        |  interpret, decide, orchestrate, review, authorize
        v
Rust domain core + typed infrastructure ports
        |  durable intent, provider/database effects, receipts, verification
        v
PostgreSQL / credential vault / Spotify adapter
```

The middle layer matters: clients do not sequence low-level core calls, and
core/provider adapters do not make product-workflow decisions. CLI and browser
may render different experiences, but both operate the same durable
maintenance session and receive the same allowed decisions and effects.

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

The typed command/query API never receives a refresh token and does not contact
Spotify. It checks that the authenticated Chordrift account owns the selected
provider connection, then durably accepts `ObserveProvider`. Idempotency and
operation history are stored before a worker starts. The separate OAuth
callback exchanges a short-lived authorization code server-side and passes the
result directly to the encrypted connection authority. The browser follows the
same operation DTOs that the remote CLI uses.

## Product login and provider connections

Chordrift login and provider authorization are deliberately separate. Google
through Auth0 establishes the product subject and Chordrift-account session.
Spotify Authorization Code with PKCE then adds a provider connection owned by
that account; the browser never receives the refresh token or a Spotify client
secret.

The hosted Rust authority resolves Spotify's stable account identity before it
changes credential state. Reconnecting a known identity rotates the encrypted
credential on the same `provider_accounts` row, preserving every observation
and intent record. Adding a different identity creates a separate account-owned
connection. A reconnect pinned to one connection fails if Spotify returns a
different identity, and an identity owned by another Chordrift account cannot
be claimed. Disconnect revokes the active encrypted credential without deleting
provider observations, history, intent, or the Chordrift product session. It
performs no music-library mutation.

The browser only launches these routes, renders connection status, and selects
the active connection. Identity matching, ownership checks, encrypted rotation,
and revocation remain Rust behavior. An authorization flow interrupted by an
API restart expires safely and can be started again; no provider credential has
changed before its successful callback.

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

Resolved record-only gestures now project through Rust into canonical playlist
placement, reversible exclusion, provider custom order, and remembered saved-
track disposition. An exact maintenance fork preserves the already-approved
model and artwork and never centroid-assigns unrelated tracks. Replaying an
already-satisfied resolution is a no-op. Saved-track cleanup is ordered after
canonical destination intent and is withheld from review until every required
decision is resolved.

The first bounded provider-write path is saved/Liked cleanup. The API accepts
authorization only for the exact current revision and review. The worker
rederives the effect from trusted observed changes, rejects a mismatched or
newer provider checkpoint, persists Authorized and Applying, performs only the
enumerated idempotent saved-track removal, observes Spotify again, verifies the
track is absent, and persists Verifying and Verified. A crash can resume at
Authorized, Applying, or Verifying without widening the reviewed work; a
completed operation replay is a no-op.

The adapter continues to reject publication, retirement, and unsupported plan
phases. No live provider write is authorized merely because this code exists;
deployment and manual private-beta acceptance remain separate gates.

Migration 0051 now supplies the restart-safe task boundary described in
[Durable maintenance sessions](DURABLE_MAINTENANCE_SESSIONS_V021_06.md). The
adapter produces typed projections directly from PostgreSQL provider
observations through the Rust planner; it never delegates to the legacy shell
wizard.
