# Onboarding session boundary — V020-06

Status: implemented on the v0.2.0 development line and verified with a fake
provider plus disposable PostgreSQL 18. It is not a released CLI workflow and
has not been connected to live Spotify or production Neon.

## Boundary delivered

V020-06 adds the public Rust `onboarding` module behind the shared
`ApplicationFacade`. One `CreateOnboardingSession` command captures:

- the explicit Chordrift account and selected provider connection;
- one immutable `provider_inventory_checkpoints` row and its state fingerprint;
- the provider and evidence capability snapshots used for the read;
- current inventory in every session;
- exactly one extended-playback-history evidence source only when the command
  explicitly selects it; and
- a deterministic SHA-256 fingerprint over the complete provider-neutral input
  manifest.

The provider port is `OnboardingProviderReader`. It has one read method and no
provider mutation method. V020-06 does not adapt the production Spotify client
to this port; the first implementation is exercised only by the deterministic
fake reader.

## Execution order and safety

The boundary performs the following checks in order:

1. Validate the contract version, command shape, and requested account.
2. Resolve the selected connection through `provider_accounts` and require the
   stored Chordrift owner, provider namespace, and provider-owned account ID to
   match the typed `AccountContext`.
3. Return an already persisted session for an identical account-scoped
   idempotency key before another provider read. Reusing that key with a
   different extended-history selection returns `state_conflict`.
4. Require readable current-inventory capabilities and, when selected, readable
   extended-history evidence. An unavailable capability fails before the fake
   provider is called.
5. Read the selected inputs through the mutation-free provider port.
6. Require the returned checkpoint and fingerprint to match the selected
   provider account in PostgreSQL.
7. Insert or reuse the capability observation and content-addressed onboarding
   session in one transaction.

The existing contract remains responsible for operation lifecycle and
cooperative cancellation around this invocation; V020-06 does not create a
second cancellation or job model. The V020-04 suite continues to prove that
cancellation stops work at the next checkpoint without an extra provider call.

## Durable record

The migration-0046 `onboarding_sessions` row remains in `created` status because
V020-07 has not produced an audit yet. It records the immutable checkpoint,
capability observation, extended-history selection, input fingerprint, input
manifest, and output provenance.

`ignore_existing_intent` is always true. The implementation does not query
`library_collections`, collection membership, playlist surfaces, recipes,
Spins, or publication history. The PostgreSQL proof adds collection intent
between an initial capture and its replay; the replay returns the same session
and fingerprint. Provenance also records:

```json
{
  "boundary": "onboarding_input_capture",
  "chordrift_intent_read": false,
  "provider_write_requested": false,
  "next_boundary": "inventory_only_audit"
}
```

No provider credential, provider payload, SQL row type, or terminal concept is
part of the provider-neutral input types returned to clients.

## Verification and exclusions

The isolated PostgreSQL 18 proof covers inventory-only capture, explicitly
selected extended evidence, exact manifest/provenance persistence,
content-addressed reuse, command-key replay without another provider read,
same-key conflict, visible capability failure, and cross-account rejection
before provider access. The existing fresh 46-migration path and `45 → 46`
upgrade rehearsal remain green.

V020-06 intentionally adds no audit findings, starter collections, recipes,
Spins, publication plans, CLI commands, production configuration, credential
handling, live Spotify request, provider write, or production Neon migration.
Those boundaries remain separately reviewable.

## Next slice

`V020-07 — Inventory-only new-account audit` may consume only the captured
current-inventory inputs and capability snapshot to produce an honest library,
overlap, uncertainty, and starter-organization proposal. It must remain
read-only and must not begin the enriched-history comparison assigned to
V020-08.
