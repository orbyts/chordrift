# Hosted private beta — V021-06

Status: in progress. Nothing in this document marks the hosted service ready
for provider writes or unrestricted public registration.

## Selected topology

```text
browser / remote CLI
        |
        | HTTPS · chordrift.suhail.ink
        v
Nexus · existing Nginx + wildcard TLS + Tailscale subnet router
        |
        | private LAN · 10.214.90.10 -> 10.214.90.35:8787
        v
Vortex · non-root read-only Chordrift API container
        |
        +---- Neon PostgreSQL
        +---- Auth0 / Google OIDC
        `---- Spotify through a short-lived encrypted-vault credential lease
```

Nexus is ingress. Vortex is compute. Neon remains the durable application
database. Vortex does not receive a second public listener and does not need a
second Tailscale installation because Nexus already routes the private subnet.

## Identity and existing-account adoption

Google proves an external identity through Auth0. Chordrift owns the durable
product subject, account membership, revocable product session, provider
connection, and data. Spotify is never a login method.

The first private-beta login may match one deployment-configured, Google-
verified email address. That match is only a bootstrap gate: Chordrift binds
the verified OIDC issuer/subject to the already-existing Chordrift account
through `provision_account_owner`. Later sessions resolve the stable
issuer/subject binding. The bootstrap email setting can then be removed.

The adoption operation must not create a new music account, rewrite the
existing `chordrift_account_id`, re-import Spotify, or modify Spotify. It adds
identity/session rows around the existing ownership boundary.

## Browser boundary

The Rust service serves a deliberately small HTML/CSS/JavaScript contract
workbench. JavaScript submits only the same typed compatibility, command, and
query DTOs used by other clients. It cannot submit a CLI string, shell command,
SQL, provider URL, database credential, or provider credential.

The OIDC authorization-code exchange and Chordrift session stay server-side.
The browser receives an `HttpOnly`, `Secure`, `SameSite=Lax` Chordrift cookie.
Cookie-authenticated DTO calls additionally require the exact public Origin and
an application-specific request header. CLI clients continue using an
`Authorization: Bearer` product session and never rely on browser cookies.

## Container boundary

The repository is checked out on the build host and copied into a disposable
multi-stage builder. The runtime image contains the release server binary and
CA roots only; static assets are compiled into the binary. It contains no Git
history, Rust compiler, source tree, Cargo cache, `.env`, or build credential.

The Vortex container runs as UID/GID 65532, drops every Linux capability, uses
`no-new-privileges`, has a read-only root filesystem and bounded tmpfs, binds
only the Vortex private IPv4 address, has bounded log rotation/resources, and
uses the Rust binary itself for liveness checks.

## Current deployed checkpoint

The authenticated private-beta service is deployed at
`https://chordrift.suhail.ink`. Google login adopts the existing Chordrift
account, browser sessions survive container replacement, and the provider-aware
explorer presents Spotify observation state separately from the Chordrift
model. Playlist order, track detail, personal listening statistics and active
exclusions are available through tenant-scoped typed queries.

The service and local CLI now target the same canonical `chordrift_cutover`
database on the single Neon `main` branch. It is at migration 50/50. The
temporary pre-cutover and rehearsal branches, plus the stale duplicate
`chordrift` database, were deleted after external logical backups and client
health checks passed. Rehearsal fixture identities, revoked fixture
credentials, and fixture operations were not promoted. First-owner login and
encrypted import of the existing Spotify refresh authorization must be
reverified on this canonical database before beta.1. Provider-backed
observation, durable worker composition and the maintenance task are now wired
through the real Rust adapter. Provider writes are limited to server-rederived,
enumerated maintenance effects behind an immutable exact review. A newly
selected placement must be added, freshly observed, and verified before a
separate review may consume Liked Songs or another intake surface. The service
must fail closed rather than disguising a test backend or legacy shell workflow
as hosted authority.

## Storage and rehearsal policy

The durable topology is one Neon project, one `main` branch, and one canonical
application database. At the consolidation gate the PostgreSQL database was
approximately 195 MB. Its largest durable components are normalized listening
history (about 81 MB), revisioned playlist membership (about 35 MB), verified
playlist baselines (about 15 MB), and synchronization receipts (about 9 MB).
Those are product evidence, not branch duplication.

Prefer disposable local PostgreSQL for migration and restore rehearsal. When
Neon branch semantics are specifically being tested, set an expiry at branch
creation, record the proof, and delete the branch immediately afterward. A
temporary Neon branch must never become an undocumented service dependency.
Logical backups live outside Neon and are not a reason to retain a permanent
backup branch on the free plan.

The complete product/browser acceptance surface is tracked in
[Web workflow capability matrix](WEB_WORKFLOW_CAPABILITY_MATRIX.md).

## Deployment gates

1. Build and test from the exact Git commit; record the image digest.
2. Back up Neon and prove the backup restores into an isolated target.
3. **Complete:** project `royal-snow-31539822` is the only Chordrift Neon
   project. Its canonical database is at schema 0050 and is shared by local and
   hosted transports. The former legacy project, temporary rehearsal branches,
   and stale duplicate database are deleted.
4. Configure Auth0 as a Regular Web Application with Google login and callback
   `https://chordrift.suhail.ink/auth/callback`.
5. Start the Vortex API with provider writes limited to server-rederived,
   enumerated effects behind an immutable exact review. Readiness must report
   `exact_review_only`; no general publication or client-supplied write exists.
6. Validate the Nexus configuration before reload, then verify HTTPS headers,
   liveness, readiness, OIDC state/PKCE, first-owner adoption, logout, session
   revocation, and cross-account denial.
7. Import the existing Spotify refresh credential into the encrypted vault only
   after database restore proof and identity adoption succeed. **Pending
   canonical re-verification:** the prior rehearsal proof is retained, but its
   fixture rows were deliberately not copied into the canonical database.
8. Exercise provider reads and ordinary maintenance against fake fixtures and
   then the personal account in read-only mode.
9. Provider writes require a new explicit gate after Suhail reviews the exact
   beta behavior.
