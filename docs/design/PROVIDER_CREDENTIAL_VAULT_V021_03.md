# Provider credential vault — V021-03

Status: implemented and verified on 2026-08-30. This slice does not choose a
hosting platform, expose a provider-token HTTP route, apply migration 0049 to
the personal database, or contact Spotify.

## Result

Chordrift can now keep a provider OAuth refresh credential inside the hosted
Rust authority without distributing it to a CLI, browser, or future mobile
client. A trusted server-side OAuth adapter gives plaintext directly to the
vault. The vault encrypts it before persistence and returns plaintext later
only as a short-lived internal lease for one authorized provider operation.

Clients continue to retain only the opaque, revocable Chordrift product session
introduced in V021-02. There is deliberately no command/query DTO or HTTP route
that returns a provider refresh token.

## Encryption envelope

The vault uses XChaCha20-Poly1305 authenticated encryption. Every immutable
credential revision has a fresh random 24-byte nonce and authenticated metadata
binding it to:

- its credential revision ID;
- the owning Chordrift account;
- the provider-account connection and provider namespace;
- the `oauth_refresh` credential kind;
- the envelope schema and algorithm; and
- the external key ID.

Changing ciphertext or substituting any of those identities makes decryption
fail closed with a fixed client-safe dependency error. Plaintext values and
key material are not serializable or debuggable and are zeroized when their
temporary Rust values are dropped.

## Key ownership and rotation

Encryption keys belong to deployment configuration or a later managed KMS,
not PostgreSQL. The process receives a key ring containing one active write key
and, during rotation, retained decrypt-only keys. PostgreSQL stores only the
key selector. New credential revisions use the active key; older revisions can
remain decryptable while the old key is retained.

This separates two rotations:

1. provider credential rotation creates a new encrypted generation and
   atomically revokes the previous active generation; and
2. encryption-key rotation changes the active external key and may later
   re-encrypt active envelopes through a separately controlled operation.

The database enforces at most one active refresh credential for a provider
account. History remains auditable after rotation or revocation.

## Authorization

Every store operation rechecks the current V021-02 subject, membership,
Chordrift-account status, and provider-account ownership edge. An active account
member may lease the credential for internal provider work. Only the active
account owner may provision, rotate, or revoke it. A caller-provided resource ID
never grants access, and cross-tenant or cross-provider substitution is denied.

Revocation changes only Chordrift's encrypted credential state. Calling the
provider's own token-revocation endpoint is a distinct adapter operation and
must be separately modeled when deployment wiring is added.

## Persistence and compatibility

Additive migration 0049 creates `provider_credential_vault`. It stores account
and provider identity, generation, algorithm, external key ID, nonce,
ciphertext, creator, timestamps, and revocation evidence. It contains no
plaintext token and no encryption key.

The hosted authority must verify migration 0049 before enabling provider-backed
work. Local daily-driver maintenance still requires only migration 0047, so the
personal 47-migration music database does not need migrations 0048 or 0049.
Seeing `47/50` in local status is therefore expected now that the later durable-
operation migration is also present but hosted slices remain undeployed.

## Verification

The permanent test matrix proves:

- plaintext round-trip through the Rust lease while ciphertext contains no
  plaintext token;
- active-member lease but owner-only rotation and revocation;
- tenant mismatch and provider-identity substitution denial;
- monotonic rotation with one active database revision;
- retained-old-key and active-new-key behavior;
- ciphertext tampering and missing-key failure without secret-bearing errors;
- immediate post-revocation denial; and
- a complete migration-0049 rehearsal on disposable PostgreSQL.

V021-04 now persists jobs and resumable operation state around this boundary.
V021-05 may make the CLI a remote Chordrift-session client. V021-06 selects the
deployment key/KMS source, provider OAuth redirect policy, backup/restore,
observability, and production rotation runbook.
