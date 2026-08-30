# Web service contract and transport conformance

Status: A021-12, V021-01, and V021-02 are implemented. The asynchronous Rust
authority is available through typed HTTP routes using persisted, revocable
Chordrift product sessions and is tested over a real loopback TCP server.
Durable execution, credential vaulting, hosting, and deployment remain later
V021 slices. This document does not expose a production endpoint or authorize
deployment.

## The boundary

The hardened CLI must not become the product API. Shell commands, flags,
terminal tables, prompts, plan phases, and internal UUID ceremony are adapter
details. The stable boundary is the Rust-owned, transport-neutral
command/query/event contract in `src/contract.rs`.

All consumer clients are deliberately thin. They may authenticate, submit a
typed command, read task state/events, display the Rust-selected allowed
actions, capture a decision, and render a result. They may not interpret
provider deltas, assemble plans, decide whether authorization is required, or
sequence internal safety phases. Those responsibilities stay in the Rust
application core regardless of the wrapper or visual skin.

That contract already provides useful primitives:

- semantic contract and schema compatibility negotiation;
- typed command and query envelopes;
- request, operation, cancellation, and idempotency identities;
- structured progress, lifecycle events, cancellation, and recovery; and
- fixed client-safe errors and capability reporting.

The task DTO/reducer and asynchronous application service own ordinary workflow
transitions and orchestration behind typed backend ports. The published local
daily-driver still invokes the established shell until remote CLI parity in
V021-05; that compatibility shell is not the service contract. A web client
must never port it to JavaScript or reproduce its sequence with button-specific
endpoints.

## Task-oriented application workflows

Ordinary maintenance now has a Rust-owned, versioned `MaintenanceSession`
contract. It supports task-level operations equivalent to:

- start, reconnect to, or refresh observation and cumulative reconciliation;
- read the current immutable review view;
- resolve one or more genuinely ambiguous placement meanings;
- approve one exact human-readable provider mutation, when any exists;
- cancel long work; and
- read progress, recovery state, receipt, and final convergence.

The concrete 1.2 application contract exposes typed start, refresh, resolve,
authorize, and session-query DTOs. One user action may
create several internal proposals, plans, assessments, and receipts. Those
objects remain Rust-owned safety evidence and appear in advanced diagnostics,
not as mandatory web ceremony.

A review DTO should contain opaque resource identity, a monotonic revision or
precondition token, human track/artist/playlist labels, exact proposed provider
effects, ambiguity choices, warnings, capabilities, and the actions currently
allowed. It must not expose SQL rows, provider credentials, terminal text, or
require the client to infer safety from plan internals.

## Concurrency and cumulative state

Each command carries an idempotency key. Every decision or authorization is
bound to the exact session revision and newest complete provider snapshot. A
new pull invalidates stale execution authorization, rebases record-only user
gestures cumulatively, and returns a structured conflict/review view rather
than applying an old plan.

Authentication establishes the product identity and tenant context. Resource
IDs in a DTO never grant authority by themselves, and a client-supplied account
ID cannot override the authenticated owner.

## Initial web transport

An authenticated HTTP adapter may serialize the same command/query envelopes
and expose operation events through polling or server-sent events. Exact route
design is a V021-01 implementation choice; HTTP status codes, headers, cookies,
and streaming are transport concerns rather than domain behavior.

The web client should be generated from or compile against machine-readable
schema derived from the Rust DTOs. Additive optional fields and explicit
capabilities evolve within a compatible contract minor version. Renaming a
field, changing meaning, or removing a variant requires a new major contract.
Do not build a generic “run CLI command” endpoint.

## Required conformance matrix

The conformance suite runs start/query/decision/authorization, record-only
provider order, stale revision, and cumulative refresh scenarios through both
in-process calls and authenticated HTTP over a real loopback server. It also
proves account isolation, idempotent replay and collision rejection, reconnect,
cancellation, ordered event cursors, request budgeting, incompatible-contract
rejection, capability failure, and secret-free errors. The matrix is:

1. in-process application transport — implemented;
2. real loopback HTTP using serialized DTOs and authenticated test sessions —
   implemented;
3. fake provider plus the existing disposable PostgreSQL integration suite —
   implemented, with durable service persistence deliberately reserved for
   V021-04; and
4. deployed read-only/synthetic smoke tests — required by V021-06.

The HTTP harness is the early web simulation. It tests JSON round trips,
authentication, tenant isolation, idempotent retries, duplicate submission,
stale revisions, reconnect/resume, cancellation, event ordering, pagination,
capability degradation, rate limiting, and secret-free errors. Every scenario
in the daily-driver edge-case ledger—including cumulative intake followed by a
newly exposed reorder—must reach the same final DTO and provider-call trace
through in-process and HTTP transports.

Later browser tests have a different purpose:

- DTO/client tests prove application and transport behavior without a UI;
- component tests prove rendering and user decisions for each review state;
- browser end-to-end tests prove a small number of critical journeys,
  accessibility, navigation, session expiry, reconnect, and mobile layouts.

Do not rely on browser end-to-end tests to discover core workflow divergence;
the transport conformance suite should catch it earlier and more precisely.

## Flexibility without ambiguity

Flexibility comes from stable task-level DTOs, explicit capabilities, versioned
schemas, opaque resources, and server-owned workflow state—not from untyped
JSON blobs or endpoints for every button. A web, iOS, Android, CLI, or future
client may present the same allowed actions differently while receiving the
same decisions and safety outcomes from Rust.
