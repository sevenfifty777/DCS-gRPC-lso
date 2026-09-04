# Deployment and rollback runbook

This runbook is documentation only. It was not executed during development. Phase 1 does not deploy
or modify DCS-gRPC, protobuf, DLL or Lua.

## Prepare a release candidate

1. Run the mandatory validation commands from the repository root.
2. Build with `cargo build --release --locked`.
3. Record SHA-256 of `lso.exe`, Git revision/diff identifier and the manifest from
   [LIVE_VALIDATION.md](LIVE_VALIDATION.md).
4. Copy the candidate to a versioned directory, never over the active binary.
5. Back up `lso.db` using SQLite backup or a stopped-process file copy.
6. Keep the prior binary and its configuration next to the candidate.

Suggested layout:

```text
C:\LSO\releases\<revision>\lso.exe
C:\LSO\releases\previous\lso.exe
C:\LSO\data\lso.db
C:\LSO\active.txt
```

The output directory remains shared because migrations are additive. Test the old binary against a
copy of the migrated database before production change; if it cannot read additive columns, point it
to the pre-change database backup during rollback.

## Switch

1. Stop only the LSO process/service and wait for exit.
2. Update the service executable path or version pointer to the candidate.
3. Start LSO and verify within two minutes: connection, reported server version/session, no migration
   error, expected strict pair count, dashboard on `127.0.0.1`, and ten-second metrics log.
4. Run one non-production smoke recovery when authorized.

For an immediate behavioral rollback without changing binaries, restart the candidate with
`--legacy-inline-hook-sampling`. This restores the former blocking hook path only; it does not undo
schema additions or other reliability fixes. Preserve the independent-mode log before switching so
the A/B percentiles remain comparable.

Do not restart or reconfigure DCS/DCS-gRPC for this module-only release.

## Roll back in under five minutes

1. Stop LSO.
2. Point the service/version pointer to the preserved previous binary.
3. If compatibility testing required it, move the failed-run database aside and restore the
   pre-change backup; never overwrite it without retaining the failed copy for diagnosis.
4. Start the previous LSO binary.
5. Confirm gRPC connection, session ID and output directory.
6. Record UTC times, binary hashes and reason.

Rollback success means the previous process records locally again. Discord and PNG failures are
secondary and must not delay restoring local persistence.

## Test requirement

Time this procedure on a staging copy with the real service wrapper and filesystem permissions. A
written runbook alone does not validate the five-minute objective. Production is not fully validated
until that timed exercise, the live corpus, server benchmark and DCS-gRPC pin authentication pass.
