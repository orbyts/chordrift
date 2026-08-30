# Product identity and authorization — V021-02

Status: implemented and verified on 2026-08-30. This slice does not choose an
identity vendor, deploy a public service, store provider credentials, or access
the personal Spotify/Neon deployment.

## Result

Chordrift now owns its product sessions and account authorization instead of
trusting a client-supplied account ID. A pluggable external verifier validates
an upstream credential and returns only stable issuer/subject claims. Chordrift
then checks the persisted account ownership binding and returns a random opaque
Chordrift bearer token.

The session token contains no account data. It is generated from 256 random
bits, returned once, and represented in PostgreSQL only by its SHA-256 digest.
Every authenticated command/query resolves that digest through current session
expiry, session revocation, product-subject status, account membership, and
account status. Suspending the session, subject, membership, or account
therefore invalidates existing access immediately without waiting for token
expiry. Revoking an external identity prevents new session exchange; existing
Chordrift sessions remain independently revocable.

## HTTP boundary

- `POST /v1/sessions` exchanges an upstream `Authorization: Bearer …`
  credential plus session `schema_version: 1` and an existing `account_id` for
  a Chordrift session.
- `DELETE /v1/sessions/current` revokes the exact supplied Chordrift bearer.
- `POST /v1/commands` and `POST /v1/queries` continue using the unchanged typed
  V021-01 application contract.

There is no password endpoint, raw SQL provisioning endpoint, client-selected
subject, generic role override, CLI-command endpoint, or provider-credential
route. `provision_account_owner` is a trusted server bootstrap boundary rather
than a public registration API. Identity-vendor selection and public signup
policy remain deployment decisions.

Session exchange has its own schema because it is not an application command
or query. V021-02 therefore leaves the application contract at 1.2, avoiding
an unnecessary compatibility break for existing thin clients.

## Persistence

Additive migration 0048 creates:

- `product_subjects`;
- `product_external_identities`;
- `chordrift_account_memberships`; and
- `product_sessions`.

An account has at most one active owner. An issuer/subject pair has one stable
product subject. Session creation is one `INSERT … SELECT` that succeeds only
while the external identity, subject, membership, and account are all active.
Authentication is read-only; it does not write a “last seen” row for every API
request. Revocation is an exact digest update.

Migration 0048 is required by the hosted identity service but not by the local
maintenance CLI. Local music operations explicitly require schema through 0047
and can therefore keep using the already verified personal database while the
hosted service is developed separately.

## Tenant-safety proof

The real HTTP and repository matrix covers:

- valid exchange, authenticated API use, exact logout, and post-logout denial;
- an identity requesting another account;
- an authenticated session querying another account;
- unknown and guessed bearer tokens;
- expiry, explicit session revocation, subject suspension, account suspension,
  membership revocation, and external-identity revocation;
- idempotent trusted owner provisioning and active-owner takeover refusal; and
- PostgreSQL persistence of a digest rather than plaintext session material.

V021-03 has added the encrypted provider credential vault without changing this
session contract or exposing provider secrets. V021-04 owns durable
background operations; V021-05 moves the CLI to remote sessions; V021-06 picks
the identity provider, hosting, TLS/origin policy, and public deployment.
