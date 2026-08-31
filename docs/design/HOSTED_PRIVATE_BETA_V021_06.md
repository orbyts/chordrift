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

The existing Spotify refresh authorization has been adopted into the encrypted
provider credential vault without contacting Spotify. The server reports the
connection as credential-ready, but provider-backed observation, durable worker
composition and the maintenance task remain unavailable until the real Rust
adapter passes the read-only and fake-provider gates. Provider writes remain
disabled. The service must fail closed rather than disguising a test backend or
legacy shell workflow as hosted authority.

The complete product/browser acceptance surface is tracked in
[Web workflow capability matrix](WEB_WORKFLOW_CAPABILITY_MATRIX.md).

## Deployment gates

1. Build and test from the exact Git commit; record the image digest.
2. Back up Neon and prove the backup restores into an isolated target.
3. The loaded daily-driver URL points to `chordrift-v014-legacy-retirement` at
   schema 0045. The intended `royal-snow-31539822` project is at schema 0047,
   has the provider-neutral account foundation, and has matching durable-intent
   counts but a slightly older observation. Compare content hashes, reconcile
   only the read-only observation delta, then rehearse migrations 0048–0050.
4. Configure Auth0 as a Regular Web Application with Google login and callback
   `https://chordrift.suhail.ink/auth/callback`.
5. Start the Vortex API with provider writes unavailable.
6. Validate the Nexus configuration before reload, then verify HTTPS headers,
   liveness, readiness, OIDC state/PKCE, first-owner adoption, logout, session
   revocation, and cross-account denial.
7. Import the existing Spotify refresh credential into the encrypted vault only
   after the database restore proof and identity adoption succeed. **Complete:**
   generation 1 was encrypted without contacting Spotify; deployment use still
   requires the read-only adapter gate.
8. Exercise provider reads and ordinary maintenance against fake fixtures and
   then the personal account in read-only mode.
9. Provider writes require a new explicit gate after Suhail reviews the exact
   beta behavior.
