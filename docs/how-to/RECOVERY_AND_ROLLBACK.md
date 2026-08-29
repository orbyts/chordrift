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

```console
$ chordrift --version
$ chordrift capabilities
$ chordrift db status
$ chordrift db v2 status --account personal
$ chordrift intake audit --account personal
$ chordrift sync plan-show --account personal --details
$ chordrift playlists list --account personal
$ chordrift playlists tracks --account personal --name Re-evaluate
```

The expected personal cutover state is v0.2.0, 47/47 migrations, the same
current playlist membership and exclusions as the final source backup, and the
same three current Re-evaluate tracks. `intake audit` is read-only. A queue
status may report zero pending classification actions even while the provider
inventory correctly contains three Re-evaluate tracks; those are different
views and should not be "cleaned" merely to make the counts match.

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
