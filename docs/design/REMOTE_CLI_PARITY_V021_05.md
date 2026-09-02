# Remote CLI parity — V021-05

Status: implemented 2026-08-30. This slice does not select hosting, create a
public listener, exchange an external login, contact Spotify, or apply hosted
migrations to the personal database.

## Result

The installed CLI can now act as a thin authenticated client of the same typed
application contract intended for web and mobile wrappers. It performs an
authenticated compatibility negotiation, then sends an exact `CommandRequest`
or `QueryRequest` to the Rust authority. It never receives Neon credentials,
provider refresh credentials, SQL access, or an endpoint for shell commands.

V021-06 completes the user-facing sign-in with Auth0's standard OAuth 2.0
Device Authorization Flow. A separate public Native application gives the CLI
an Auth0-hosted verification URI and short user code. The CLI polls Auth0 at
the prescribed interval, then exchanges the verified identity credential once
for a separate revocable `chd_session_…` credential. Only that opaque product
session is stored under a named profile in the operating-system credential
store. Neither identity nor product tokens enter browser history or terminal
output. Status reports only whether a value exists; removal never prints it.

```text
chordrift service session login --url https://chordrift.suhail.ink
chordrift service compatibility --url https://service.example
chordrift service command --url https://service.example --file command.json
chordrift service query --url https://service.example --file query.json
chordrift service session remove --profile default
```

JSON files must deserialize as the public typed envelopes. This is temporary
developer presentation, not an arbitrary request escape hatch. Friendly CLI,
web, iOS, and Android screens compile user gestures into the same DTOs.

## Security and compatibility

- HTTPS is mandatory outside the in-process fake-provider acceptance harness.
- Auth0 owns device-code expiry, polling cadence, authentication, and consent.
  Chordrift accepts identity credentials only from its configured issuer,
  derives the account server-side, and returns a distinct revocable session.
- OIDC discovery, device authorization, verification, and token endpoints must
  all use the configured issuer's exact HTTPS origin. The Native client has no
  secret, and Chordrift never accepts a client-supplied identity-provider URL.
- Authentication precedes compatibility, command, and query dispatch.
- The client negotiates the highest common contract, accepted hosted schema,
  and required service capabilities before every command/query invocation.
- Error bodies must deserialize as fixed `ClientError`; arbitrary server text
  is never displayed.
- Bearer memory is zeroized on client drop and never appears in debug output.
- Retry/idempotency, progress, reconnect, cancellation, and authorization stay
  owned by the application contract and durable service, not CLI branching.

## Local development transport

`LocalDevelopmentClient` implements exactly the same compatibility/command/
query trait over an explicitly supplied in-process application and authenticated
subject. It is dependency-injected test/development machinery, not an implicit
fallback when the network fails. A failed remote call never opens Neon or loads
provider credentials locally.

Real loopback HTTP tests run the shipped client against Axum and compare it with
the explicit local client. A separate fake Auth0/service acceptance proves
device discovery, pending polling, identity-token exchange, and issuance of a
distinct Chordrift session without a browser-to-localhost callback.

## Release identity

For every beta and final release, the locally installed CLI and the deployed
API/worker containers must come from the same tagged source commit. The CLI is
installed from the published crates.io version; Vortex rebuilds and restarts
both services from that tag and records the image digest. A cross-client smoke
then sends equivalent typed requests through the installed CLI and browser.
Their wording and controls may differ, but authorization, durable revisions,
reviews, provider effects, verification, and structured errors must be
contract-identical because those behaviors belong to the Rust authority.

## Next boundary

V021-06 selects hosting and external product authentication, configures the
service compatibility declaration, verifies backup/restore and observability,
and performs release rehearsal. It must not weaken this client boundary.
