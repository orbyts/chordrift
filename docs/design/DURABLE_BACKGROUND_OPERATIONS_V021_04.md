# Durable background operations — V021-04

Status: implemented and verified on 2026-08-30. This slice persists the service
operation boundary; it does not move the installed CLI to remote transport,
deploy a hosted worker, contact Spotify, or apply migration 0050 to the personal
database.

## Result

Chordrift now has a restart-safe queue for the same typed application commands
introduced in V021-01. A command and its account-scoped idempotency identity are
committed before a worker may act. PostgreSQL retains the exact receipt, command
DTO, retry policy, current state, cancellation capability, worker lease, and an
append-only ordered event stream.

A CLI, web, iOS, or Android client still sees only `CommandReceipt`,
`OperationView`, `OperationHistoryView`, and cursor-filtered `OperationEvent`
DTOs. It does not choose workers, retries, leases, database rows, provider
credentials, or internal execution phases.

## Durable sequence

```mermaid
sequenceDiagram
    autonumber
    actor Client
    participant API as Authenticated Rust API
    participant DB as PostgreSQL operation ledger
    participant Worker as Trusted Rust worker
    participant Domain as Domain/provider adapter

    Client->>API: Typed command + idempotency key
    API->>DB: Commit command, receipt, queued event
    DB-->>API: New or exact replayed receipt
    API-->>Client: 202 Accepted + receipt

    Worker->>DB: Recover expired leases; claim next
    DB-->>Worker: Typed command + exclusive expiring lease
    loop Long work
        Worker->>DB: Renew lease / append structured progress
        Client->>API: Query events after sequence N
        API->>DB: Read authorized event cursor
        DB-->>Client: Ordered new events
    end

    alt Success
        Worker->>Domain: Execute typed idempotent domain operation
        Worker->>DB: Complete with immutable result ID
    else Retryable failure or abandoned lease
        Worker->>DB: Record recoverable error
        DB->>DB: Wait until next attempt; enforce maximum attempts
    else Cancellation requested
        Client->>API: Exact operation + cancellation capability
        API->>DB: Persist cancellation request
        Worker->>DB: Check at safe boundary; acknowledge cancellation
    else Retry budget exhausted
        Worker->>DB: Record terminal client-safe failure
    end
```

This diagram is the operational layer beneath the provider-first maintenance
sequence. It does not alter the rule that ordinary provider observations are
record-only and that Create, Restore, Retire, or Spin publication requires a
separately originated exact authorization.

## Acceptance and replay

The idempotency key is scoped by both Chordrift account and product subject.
The persisted SHA-256 fingerprint covers the canonical typed command, not the
transport request ID. Repeating the same key and command after a timeout or
service restart returns the original receipt and creates no second job. Reusing
the key for different intent fails with `state_conflict` before any worker or
provider action.

The command payload contains application DTO JSON only. It is not a generic job
name, shell command, SQL string, provider URL, provider token, or encryption
key. V021-03 credential leases remain a separate internal adapter boundary.

## Worker leases and concurrency

Eligible jobs are claimed with PostgreSQL row locking and `SKIP LOCKED`, so
concurrent workers cannot receive the same lease generation. A lease includes a
random capability, attempt number, and expiry. Progress, completion, failure,
heartbeat, and cancellation acknowledgement all require the current unexpired
lease. A stale worker cannot append an event or complete work after recovery.

Long work explicitly renews its lease. If a process disappears, the next claim
first converts expired work to `recoverable`, or to `failed` after the bounded
attempt budget. The recovery event is persisted before another lease is issued.
Provider-facing handlers must retain their existing domain idempotency and
snapshot/review preconditions; a queue lease is not provider-write authority.

## Cancellation and authorization

Queued or recoverable work can be cancelled immediately. Running work records a
durable cancellation request; the worker checks at safe boundaries and appends
`cancelled` only after it has stopped. Terminal work returns `too_late`.

Every client acceptance, cancellation, operation query, event query, and history
query rechecks the current V021-02 subject, membership, and account state.
Workers claim only work whose subject, membership, and account remain active.
Resource IDs or cancellation IDs from another tenant confer no access.

## Persistence and compatibility

Additive migration 0050 creates:

- `service_operations`, containing typed accepted commands, current lifecycle,
  retry/lease/cancellation state, and the exact idempotent receipt; and
- `service_operation_events`, containing immutable operation-local sequence
  numbers and typed `OperationState` JSON.

The hosted worker must verify migration 0050 before accepting durable work.
The local maintenance client still requires only migration 0047. The personal
database may therefore show `47/50` with three hosted migrations pending; this
is expected and is not a reason to run `db migrate`.

## Verification

The disposable PostgreSQL proof uses several fresh queue/store instances to
simulate process restarts and proves:

- exact idempotent receipt replay and collision rejection;
- cross-tenant denial;
- exactly one winner between concurrent workers;
- lease heartbeat and ordered structured progress;
- expired-lease recovery and stale-worker rejection;
- retryable failure, delay/budget enforcement, and terminal exhaustion;
- durable cooperative cancellation and `too_late` terminal behavior; and
- contiguous reconnectable event cursors and acceptance-ordered history.

V021-05 makes the installed CLI an authenticated remote client of this service
contract while retaining an explicit local development transport. V021-06
selects worker hosting, deployment concurrency, shutdown/drain policy,
observability, alerting, backup/restore, and production recovery drills.
