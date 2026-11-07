# On Windows the presence-bitmap test finds the peer excluded

**Severity: Medium** (the Windows run on the main branches fails; whether the exclusion is wrong in the daemon or only in the test's timing is not known)

Investigated on 2026-11-07 / Target commit `82dbb0e`

One item of [issues.md](../issues.md).

## Symptoms

On the Windows runner, `a_peer_that_answers_only_presence_bitmaps_loses_the_rank_and_the_file_completes_from_another_peer` fails after the file completed, with `assertion failed: !subscriber.ctx().is_excluded(&peer.id())`.
It took 16.2 seconds in run 42349325499, and the Linux and macOS jobs of that run passed it.
Only that run has shown the failure: earlier Windows runs stopped at the container tests before they reached the engine tests.

## Location

[file_exchanger.rs:853](../../daemon/modules/engine/src/core/negotiator/file/file_exchanger.rs#L853) is the test, and [file_exchanger.rs:885](../../daemon/modules/engine/src/core/negotiator/file/file_exchanger.rs#L885) is the failed assertion.

## Cause

Not investigated.
The test expects that a peer which only answers presence bitmaps loses the rank it claimed without being excluded, and the assertion shows the peer was excluded by the end.
What excluded it on Windows is not known.

## Impact

The Windows job of every `ci` run on a main branch fails while this stays.
If the exclusion comes from the daemon's own judgment rather than the test, a peer could be excluded wrongly on Windows, which was not checked.

## Remedy

Run the test repeatedly on Windows with the exclusion reason logged, then fix the cause: the test's timing if the wait is too short on a slow runner, or the exclusion rule if it fires wrongly.
