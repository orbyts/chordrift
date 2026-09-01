# Web service contract and transport conformance

Status: A021-12 and V021-01 through V021-04 are implemented. The asynchronous Rust
authority is available through typed HTTP routes using persisted, revocable
Chordrift product sessions and is tested over a real loopback TCP server.
Provider refresh credentials remain behind an encrypted internal vault; there
is no raw-token route. Typed command acceptance, operation state, ordered
events, cancellation, retry, and recovery now have restart-safe PostgreSQL
persistence. Remote CLI parity, hosting, and deployment remain later V021
slices. This document does not expose a production endpoint or authorize
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

The implementation boundary is deliberately three layers, even when all three
Rust layers ship in one service process:

1. **Client skin:** CLI, browser, and later iOS/Android code performs product or
   provider login, compatibility negotiation, DTO submission, progress display,
   and accessible rendering. It owns no maintenance rules.
2. **Rust application/workflow layer:** task services interpret user gestures,
   expose genuine decisions, coordinate durable operations, bind exact review
   and authorization, and sequence core capabilities. This is the reusable
   machinery behind every skin.
3. **Rust domain/core and infrastructure ports:** provider-neutral intent,
   playlist and exclusion invariants, persistence, provider adapters, vaults,
   receipts, and verification perform the actual work. Provider-specific and
   PostgreSQL details enter only through typed ports.

A client action such as “keep this Like” therefore selects a Rust-provided
decision; it does not implement the meaning. A workflow such as maintenance
may compose multiple core operations, but the provider adapter cannot invent
the workflow. This separation allows another skin without another product
implementation.

That contract already provides useful primitives:

- semantic contract and schema compatibility negotiation;
- typed command and query envelopes;
- request, operation, cancellation, and idempotency identities;
- structured progress, lifecycle events, cancellation, and recovery; and
- fixed client-safe errors and capability reporting.

The task DTO/reducer and asynchronous application service own ordinary workflow
transitions and orchestration behind typed backend ports. The local development
CLI and remote CLI are adapters to that application boundary; the historical
shell remains only a compatibility/operator tool and is not the service
contract. A web client must never port it to JavaScript or reproduce its
sequence with button-specific endpoints.

## Task-oriented application workflows

Ordinary maintenance now has a Rust-owned, versioned `MaintenanceSession`
contract. It supports task-level operations equivalent to:

- start, reconnect to, or refresh observation and cumulative reconciliation;
- read the current immutable review view;
- resolve one or more genuinely ambiguous placement meanings;
- approve one exact human-readable provider mutation, when any exists;
- cancel long work; and
- read progress, recovery state, receipt, and final convergence.

The concrete 1.4 application contract exposes typed start, refresh, resolve,
authorize, session-query, and provider/model comparison DTOs. The set-based
comparison reports provider-only, Chordrift-only, unresolved-identity, and
custom-order differences to web and remote CLI; clients never infer a reason
from unequal totals alone. One user action may
create several internal proposals, plans, assessments, and receipts. Those
objects remain Rust-owned safety evidence and appear in advanced diagnostics,
not as mandatory web ceremony.

A review DTO should contain opaque resource identity, a monotonic revision or
precondition token, human track/artist/playlist labels, exact proposed provider
effects, ambiguity choices, warnings, capabilities, and the actions currently
allowed. It must not expose SQL rows, provider credentials, terminal text, or
require the client to infer safety from plan internals.

Saved/liked intake uses the same maintenance DTOs. A `saved_state` change names
the track, the virtual intake surface, and every verified managed destination.
`keep_observed` means retain the Like; `consume_intake` means clear only that
temporary source after verified placement. The Rust authority persists the
decision and returns an `update_saved_state` provider effect for exact review.
Clients must not infer this effect from a checkbox or duplicate the rule.

## Concurrency and cumulative state

Each command carries an idempotency key. Every decision or authorization is
bound to the exact session revision and newest complete provider snapshot. A
new pull invalidates stale execution authorization, rebases record-only user
gestures cumulatively, and returns a structured conflict/review view rather
than applying an old plan.

Authentication establishes the product identity and tenant context. Resource
IDs in a DTO never grant authority by themselves, and a client-supplied account
ID cannot override the authenticated owner.

Provider adapters obtain a short-lived plaintext credential lease only inside
the Rust authority after current tenant authorization. Thin clients never send,
receive, persist, or inspect provider refresh credentials. The vault contract is
documented in
[Provider credential vault](PROVIDER_CREDENTIAL_VAULT_V021_03.md).

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
   implemented, including restart-safe durable operation persistence, exact
   replay, concurrent worker claim, recovery, retry, and cancellation; and
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
