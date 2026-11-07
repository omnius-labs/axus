# The Windows job of the main-branch workflow fails at the tests that start a Postgres container

**Severity: Medium** (the three-system check on `main` cannot pass, so a failure that only macOS or Windows shows is hidden behind one that is always there)

Investigated on 2026-11-07 / Target commit `558c053`

One item of [issues.md](../issues.md).

## Symptoms

In the run of the merge of #159 (run 42336054488), the Windows job fails at `omnius-core-migration::postgres` `migrate_twice_test` and `create_table_syntax_error_test` with `no matching manifest for windows(10.0.26100)/amd64`.
The Linux and macOS jobs of that run were cancelled.
The macOS job was cancelled before it reached those tests in every other run inspected on 2026-11-06 and 2026-11-07, so whether the container starts on macOS is not known.
The last run of `test-daemon-all` that succeeded was on 2026-08-19.

## Location

[postgres.rs:13](../../daemon/refs/core-rs/modules/migration/tests/postgres.rs#L13) starts the container with `PostgresContainer::new(POSTGRES_VERSION)`, and each test of the file does the same.
The matrix in [test-daemon-all.yml:68-72](../../.github/workflows/test-daemon-all.yml#L68-L72) runs the tests on Windows and macOS, and [`cargo make test`](../../.github/workflows/test-daemon-all.yml#L118-L119) selects every test.

## Cause

The image is a Linux image, and the Windows runner cannot start it.
Nothing excludes the test binary on that system, and the matrix cancels the other systems when one fails, so the failure hides the results of the others.
Why the run stopped succeeding after 2026-08-19 was not investigated.

## Impact

Every run on `main` is red, so a real failure on any system is not told apart from this one.
A change that breaks only macOS is not seen while the Windows job cancels the macOS job first.

## Remedy

Exclude the test binary on the systems whose runner cannot start the container, and stop the matrix from cancelling the other systems ([ci.md §5.2](../design/ci.md#52-main-and-release-branches)).
