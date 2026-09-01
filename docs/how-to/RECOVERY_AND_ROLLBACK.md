# Recover or roll back Chordrift

Treat the executable and database schema as one deployment. v0.2.0 expects the
47-migration product schema; v0.1.4 expects the former 45-migration deployment.
Never switch only one half and continue normal operation.

## Before an upgrade or cutover

1. Stop all Chordrift writers and scheduled jobs.
2. Record `chordrift --version` and the read-only `chordrift db status` result.
3. Take a logical database backup and verify its checksum.
4. Preserve the current executable and private connection configuration with
   their file modes intact. Never put connection URLs or credentials in the
   repository, logs, or command history.
5. Prove the replacement database has the expected migration count and exact
   current-state invariants before pairing it with the replacement binary.

Provider observation and provider mutation are separate from database cutover.
Do not call Spotify merely to prove a database switch, and never apply a plan
as part of upgrade verification.

## Read-only v0.2.0 verification

After switching the binary and connection together, run only read-only checks:

For the current personal CLI deployment only, Apogee selects
`~/.config/apogee/secrets.env` through
`~/.config/apogee/config.toml`. An already-running shell may still inherit the
former exported URL. Open a fresh terminal, or explicitly run
`unset CHORDRIFT_DATABASE_URL` before reloading `eval "$(apogee)"`; otherwise a
status check can misleadingly reach the former database.

Apogee is not part of Chordrift's product architecture. Hosted and GUI clients
must use authenticated Chordrift sessions and must never load environment files
or connect directly to Neon.

```console
$ chordrift --version
$ chordrift capabilities
$ chordrift db status
$ chordrift db v2 status --account personal
$ chordrift intake audit --account personal
$ chordrift sync plan-show --account personal --details
$ chordrift playlists list --account personal
```

The expected personal cutover state is v0.2.0, 47/47 migrations, the same
current playlist membership and exclusions as the final source backup.
`intake audit` is read-only. The three-track `Re-evaluate` state was historical
cutover evidence; those tracks were subsequently moved to verified canonical
destinations, the queue was emptied, and the surface retired. Recovery must
preserve those correction events without recreating the provider playlist.

`sync plan-show` must identify maintenance plans as `maintenance`. Do not run
readiness or apply during cutover verification. Spin publication remains
provider-free in v0.2.0.

## Immediate rollback

If a read-only gate fails before any new database write occurs:

1. Stop using Chordrift.
2. Restore the preserved v0.1.4 executable and its former private connection
   configuration together.
3. Verify `chordrift --version` and `chordrift db status` against the retained
   45/45 database.
4. Keep the failed v0.2 candidate and logs for diagnosis; do not delete or
   overwrite either database.

This rollback changes no Spotify state because the cutover performs no provider
write.

## After v0.2 has recorded new state

Do not blindly point v0.1.4 at the old database after v0.2 has accepted pulls,
intent, or other writes. The databases have diverged; doing so creates a
split-brain ledger and can lose observations or exclusions. Stop writers, take
fresh backups of both sides, compare the durable domain state, and reconcile or
restore under an explicit recovery plan before resuming.

Spotify remains the current listening surface, but it is not a replacement for
the Neon audit ledger. A database restore never authorizes Chordrift to rewrite
Spotify. Any provider repair requires a separately generated exact plan and
explicit approval.

## Hosted private-beta inspection

The Vortex API and worker emit one JSON object per line using controlled fields
only. API entries contain request ID, method, path, status, and elapsed time;
worker entries contain the same request ID plus operation ID, phase, attempt,
retry budget, and a fixed error code. They never serialize headers, cookies,
request bodies, database URLs, product sessions, OAuth credentials, refresh
tokens, or vault plaintext. Nexus forwards or creates `X-Request-ID`, and the
API returns it to the client for correlation.

Use these read-only checks on Vortex:

```console
$ docker compose --env-file "$XDG_CONFIG_HOME/chordrift-hosted/chordrift.env" \
    -f deploy/vortex/compose.yml ps
$ docker compose --env-file "$XDG_CONFIG_HOME/chordrift-hosted/chordrift.env" \
    -f deploy/vortex/compose.yml logs --since=15m api worker
$ curl --fail --silent --show-error https://chordrift.suhail.ink/health/live
$ curl --fail --silent --show-error https://chordrift.suhail.ink/health/ready
```

Treat repeated `worker_failed`, exhausted attempts, readiness failure, an API
restart loop, or sustained HTTP 5xx responses as an alert. Stop the worker
first to prevent new provider work, preserve logs and the exact image digest,
then redeploy the preceding tagged image for both API and worker together. Do
not downgrade the database or authorize a compensating Spotify write. The
Compose restart policy and bounded local log driver preserve availability and
prevent unbounded host-disk growth.
