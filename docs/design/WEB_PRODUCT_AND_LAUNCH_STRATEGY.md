# Web product and public-launch strategy

Status: active product direction, recorded 2026-08-29. This document defines
sequence and acceptance boundaries; it does not authorize deployment, billing,
production-provider writes, or creation of a web codebase.

## Product form

Chordrift's intended consumer experience is a responsive web application over
the hosted Rust authority. People continue using Spotify normally. The web app
is the companion surface for:

- first-run connection and complete-library audit;
- understandable review of ambiguous meaning rather than internal UUIDs;
- collection and playlist design, names, and artwork approval;
- explicit Spin preview and publication authorization;
- history, explanation, recovery, account settings, and data export; and
- progress, cancellation, retry, and actionable failure recovery.

The CLI remains a first-class contract client, developer/operator recovery
tool, and aggressive edge-case harness. It is not the expected consumer UI.
iOS and Android applications are intended after the web experience is proven.
macOS, Windows, or Linux applications may also be added later through the same
versioned contract. None is required for the initial web launch.

## Service boundary

The browser holds only a revocable Chordrift session. The hosted Rust authority
owns tenant authorization, the encrypted provider credential vault, Neon
access, durable jobs, idempotency, and provider execution. The browser never
receives a Neon URL or Spotify refresh credential and never reimplements domain
policy.

Every client consumes the same command/query/event contract. Free and paid
plans use the same safety and domain semantics; entitlements control capacity
or service level, not correctness.

Ordinary workflows must be Rust-owned task-level sessions rather than shell or
browser orchestration. V021-01's transport conformance and simulated web-call
requirements are defined in `WEB_SERVICE_CONTRACT.md`.

## Tier principles

The commercial model begins with a genuinely useful free plan. Exact names,
prices, quotas, and packaging remain undecided until hosted operation produces
real cost and usage measurements.

Reasonable entitlement dimensions to evaluate include:

- background-sync and automation frequency;
- Spin and classifier compute budgets;
- recovery/history retention;
- number of connected provider accounts or providers; and
- advanced scheduled or collaborative workflows.

Security, tenant isolation, provider-write confirmation, understandable error
recovery, data export, and account deletion are baseline guarantees. They must
not be weakened or paywalled.

## Sequence

1. Continue using the CLI and fake binaries/providers to exhaust ordinary
   Spotify behavior, recording each failure in the daily-driver edge-case
   ledger.
2. Complete v0.2.1's authenticated hosted Rust authority, identity,
   authorization, credential vault, durable jobs, remote CLI parity, backup,
   observability, and service deployment.
3. Establish the separate Classification Authority contract and perform the
   focused Chordrift client-boundary refactor.
4. Number and build a thin responsive web client without duplicating Rust
   policy.
5. Run an invite-only private beta, add chaos and tenant-isolation cases to the
   same permanent regression corpus, and measure provider/API/compute/storage
   costs.
6. Design and test entitlements and billing from measured economics.
7. Pass the explicit public-launch gate, then open registration progressively.
8. After the web product and contract are proven, number iOS and Android client
   releases without moving domain or provider authority into the mobile apps.

## Public-launch gate

Do not deploy to the world merely because the happy path works. Before public
registration, prove at minimum:

- cumulative provider edits converge across interruption, reordering,
  deletion, restoration, duplication, ambiguity, and eventual consistency;
- cross-tenant reads, commands, events, jobs, caches, credentials, and provider
  writes are impossible under adversarial authorization tests;
- sessions and provider grants can be revoked and rotated;
- jobs survive restarts, retry idempotently, expose progress, and cancel safely;
- stale plans and approvals cannot write against a newer provider snapshot;
- backup/restore, disaster recovery, audit trails, observability, alerting, and
  support diagnostics are rehearsed;
- rate limits, abuse controls, cost ceilings, quotas, and degraded-provider
  behavior fail safely;
- onboarding, ordinary maintenance, Spin publication, recovery, export, and
  account deletion are understandable without CLI knowledge; and
- every private-beta incident becomes a documented rule and automated
  regression before wider rollout.

Public launch is a deliberate product release after these gates, not an
automatic consequence of V021-06.
