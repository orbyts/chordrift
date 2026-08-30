# Authenticated service transport — V021-01

Status: implemented and verified on 2026-08-30. This slice creates no public
deployment and contains no production bearer token, Neon URL, or provider
credential.

## Result

Chordrift now has one asynchronous Rust application authority consumed through
the same typed contract in process or over HTTP. The HTTP surface is exactly:

- `POST /v1/commands` for `CommandRequest`;
- `POST /v1/queries` for `QueryRequest`, including operation-event polling.

There is deliberately no endpoint for a CLI command, shell script, SQL string,
provider URL, or arbitrary job name. Web, CLI, iOS, and Android clients may
authenticate, submit typed intent, query immutable views/events, and render the
Rust-selected allowed actions. They do not assemble plans or choose workflow
transitions.

## Rust authority

`MaintenanceApplication` owns session routing, account-scoped idempotency,
review/revision checks, lifecycle receipts and ordered events, reconnectable
queries, cancellation, cumulative refresh, provider authorization, apply, and
verification sequencing. Its asynchronous `MaintenanceBackend` owns only the
database/provider implementation seam. This permits future Neon and provider
I/O without blocking the HTTP runtime or moving policy into a client.

Ambiguity decisions are validated first, then the backend returns a newly
computed provider-effect/review projection which the reducer validates and
installs. Authorization can therefore never reuse an effect assembled before a
different decision such as exclude versus restore.

Every resource lookup and idempotency key is scoped by authenticated subject
and selected Chordrift account. V021-02 now supplies product sessions,
ownership persistence, revocation, and comprehensive tenant authorization over
this unchanged seam; see `PRODUCT_IDENTITY_AUTHORIZATION_V021_02.md`.

## HTTP safety

The adapter authenticates before parsing or dispatching a body, limits contract
bodies to one MiB, maps failures to fixed `ClientError` JSON and stable HTTP
statuses, and supports a deployment-supplied authenticated request-budget gate.
The authenticator itself is an asynchronous trait; V021-01 supplies no insecure
default token store. TLS termination, origin policy, cookies, CSRF policy, and
the real product authenticator are deployment/identity decisions in later
slices.

## Conformance proof

The test suite starts an actual Axum server on an ephemeral loopback TCP port
and calls it with Reqwest. The same scenario also runs directly against the
in-process Rust service. Acceptance proves:

- identical maintenance outcomes and provider-call traces;
- idempotent retry returns the exact receipt and does not repeat provider work;
- reuse of an idempotency key for different intent fails closed;
- missing authentication and cross-account access fail before resource data is
  returned;
- sessions reconnect through immutable query DTOs;
- stale revisions and reviews cannot authorize provider work;
- a newer complete provider snapshot accepts record-only ordering and
  invalidates an older review;
- cancellation and operation-event cursors remain ordered and reconnectable;
- request budgeting produces a structured retryable `429` without application
  work; and
- malformed or incompatible requests return secret-free structured errors.

The fake backend is the deliberate V021-01 proof adapter. V021-03 supplies the
server-side credential boundary, and V021-04 now persists typed operations,
events, cancellation, retry, recovery, and replay state. V021-05 moves the
installed CLI onto the service while retaining an explicit
local development transport, and V021-06 selects and rehearses deployment.
