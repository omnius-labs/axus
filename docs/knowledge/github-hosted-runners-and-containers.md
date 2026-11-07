# Container-dependent tests cannot start a container on the Windows and macOS runners

**Source: literature and runtime verification** (GitHub-hosted runners reference / the postgres Docker Official Image / runs 42347912979 and 42349325499 / 2026-11-07)

One item of [knowledge.md](../knowledge.md).

## Facts

On a GitHub-hosted `windows-latest` runner (Microsoft Windows Server 2025, build 10.0.26100) on 2026-11-07, a container start failed with `no matching manifest for windows(10.0.26100)/amd64 in the manifest list entries`.
The Docker daemon requested a manifest for a Windows image, and the official `postgres` image has no Windows manifest: it is published for Linux architectures only (`amd64`, `arm64`, and others).
The pull therefore fails before the container starts.

On a GitHub-hosted `macos-latest` runner (`macos-26-arm64`) on 2026-11-07, the same start failed with `kind: TestcontainersError` and `message: "testcontainers error"`, and the test process aborted with `SIGABRT`.
The cause is not identified; this document does not claim that Docker is absent on the runner.

## Evidence

To reproduce, run `cargo nextest run -p omnius-core-migration` on `windows-latest` or `macos-latest` without the exclusion of the workflow; the container start fails before any test runs.

- Run [42347912979](https://github.com/omnius-labs/axus/actions/runs/42347912979), Windows job 128329571764: `no matching manifest for windows(10.0.26100)/amd64 in the manifest list entries`.
- Run [42349325499](https://github.com/omnius-labs/axus/actions/runs/42349325499), macOS job 128333924364: `SIGABRT` at `omnius-core-migration::postgres`, `kind: TestcontainersError`, `message: "testcontainers error"`.
- The [GitHub-hosted runners reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners) lists `windows-latest` as Windows Server 2025 and `macos-latest` as `macos-26-arm64`.
- The [postgres Docker Official Image](https://hub.docker.com/_/postgres) lists Linux architectures only.
- The current workflow excludes `binary_id(omnius-core-migration::postgres)` on Windows and macOS ([ci.yml:199](../../.github/workflows/ci.yml#L199)).

## Implications

The `omnius-core-migration::postgres` tests need a Docker daemon that can run the Postgres image.
[ci.md §5.2](../design/ci.md#52-main-and-release-branches) therefore runs the container-dependent tests on Linux only.
The exclusion follows from the runner, not from the tests, so it is removed when a hosted runner for the platform can start the container or when the test no longer needs one ([ci.md §8.2](../design/ci.md#82-deferred)).
