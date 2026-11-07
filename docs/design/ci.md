# Design of the continuous integration

## 1. About this document

This document covers when the checks run, which checks run for which change, which operating systems run them, and which result gates a merge.
The main readers are developers who change `.github/workflows/` and AI agents that carry the decisions forward.
This document assumes the vocabulary of [terms.md](../terms.md#2-terminology). Term definitions are not given here.

### 1.1 Division of responsibilities among documents

| Document or source of truth | Owns |
| --- | --- |
| [design.md](../design.md#11-division-of-responsibilities-among-documents) | Overall structure, and the map of the document family |
| This document | Triggers, change scope, jobs, the platform split, the merge gate, and the runtime conventions of the workflows |
| [storage.md](./storage.md#42-durability-guarantees-and-boundaries) | The operating systems the daemon targets |
| [web-client.md](./web-client.md#5-build-and-development) | The client's build and its test tools |
| [daemon-api.md](./daemon-api.md#42-code-generation-from-openapi) | The generation of the Rust interface from OpenAPI |
| [README.md](../../README.md) | How to run the same checks by hand |
| [Makefile.toml](../../daemon/Makefile.toml) | The Rust lint and test commands that the workflows call |
| [.github/workflows](../../.github/workflows) | The implementation of this design |

This document names what each job runs but does not define the commands, which live in the files above and in the client's scripts. It also holds no tool version and no history of a failed run.

### 1.2 Tenses used in this document

The body describes the completed design, in the present tense.
The implementation status and the remaining work are consolidated in §9.
To check what runs now, read §9 first.

## 2. Responsibilities and boundaries

The continuous integration answers one question for a change: may it merge?
It runs commands the repository already defines and holds no build logic of its own, so a failure in the workflow can be reproduced by running the same command on a developer's machine, apart from the operating system and the tool versions.
It does not release, deploy, or publish, and it holds no secret.

**A required check stops a change only before the merge.**
A check that runs after the merge can report a failure but cannot keep the change out of `main`, so the checks that must hold for every merge run on the pull request.

## 3. Triggers and change scope

### 3.1 Triggers

One workflow, `ci`, runs on a pull request, on a push to `main` or `release/*`, and on a manual dispatch.
A pull request run tests the result of merging the branch into its base, which a push to the branch does not.
A push to `main` or `release/*` runs the merged state again on the operating systems of §5.2, and a manual dispatch runs the ref it is given in the same way.

### 3.2 Change scope

A `changes` job lists the files the run covers with `git diff --name-only` and decides which scopes the change touches.
For a pull request the list is the difference from the base, and for a push it is the difference from the previous head.
A manual dispatch and a change under `.github/` select every scope.

| Scope | Paths | Jobs |
| --- | --- | --- |
| rust | `daemon/**` | `rust-lint`, `rust-test` |
| ui | `ui-web/**`, `daemon/openapi.yaml` | `ui` |
| interface | `daemon/openapi.yaml`, `daemon/entrypoints/interface/**`, `daemon/entrypoints/gen-interface.sh` | `interface` |
| e2e | the rust scope or the ui scope | `e2e` |

A path can belong to more than one scope: `daemon/openapi.yaml` is in the rust, ui, and interface scopes, so a change to it also selects `e2e`.
A change that touches no scope, such as a documentation-only change, runs `changes` and `gate` only.
The browser flow depends on both the daemon and the client, so a change to the daemon or to the client selects it.

## 4. Jobs

| Job | Runs when | Operating system | What it checks |
| --- | --- | --- | --- |
| `changes` | always | Linux | Decides the scopes of §3.2 |
| `rust-lint` | rust | Linux | `cargo make lint` |
| `rust-test` | rust | §5 | The Rust tests with all features |
| `ui` | ui | Linux | The client's lint, build, and unit tests |
| `e2e` | e2e | Linux | The browser flow against two daemons |
| `interface` | interface | Linux | The generated Rust interface equals the regeneration of `openapi.yaml` |
| `gate` | always | Linux | §6 |

### 4.1 Rust

`rust-test` calls `cargo nextest` directly and does not go through `cargo make test`, because that task depends on the lint and `rust-lint` already runs it.
The run uses a nextest profile for the workflows that does not stop at the first failure, so one run lists every failing test.
The profile keeps the group that limits the loopback transfer tests to two at a time, because four tests at once push the tests that wait on a transfer past their limit on a four-core runner ([nextest.toml](../../daemon/.config/nextest.toml)).
The matrix does not cancel the other operating systems when one fails.

### 4.2 Client and browser flow

The `ui` job runs the lint, build, and unit-test scripts of [package.json](../../ui-web/package.json) on a Node version pinned in the repository, with the tools of [web-client.md §5.1](./web-client.md#51-stack-and-layout).
The `e2e` job builds the daemon, installs Chromium, and runs the flow of `npm run test:e2e` with the binary named by `AXUS_DAEMON_BIN`.
The flow is one serial group and holds state across its steps, so a retry would start from a used daemon; the job has no retry, and a failed run is run again as a whole.
A failed run keeps the trace that Playwright writes.
The transfer step waits about a minute and a half for the two daemons to find each other, and the job's time limit accounts for it.

### 4.3 Generated interface

The `interface` job runs `gen-interface.sh` and compares the result with the tree.
The generator writes the build date into `entrypoints/interface/README.md` on every run, so that line is excluded from the comparison.
The script needs Java and downloads the generator, so a failed download fails the job, and the remedy is to run it again.

## 5. Platforms

### 5.1 Pull requests

A pull request runs the Rust jobs on Linux only.
The wait stays short, and the platform-specific code is checked after the merge.

### 5.2 Main and release branches

A push to `main` or `release/*`, and a manual dispatch, run `rust-test` on Linux, macOS, and Windows, the targets of [storage.md §4.2](./storage.md#42-durability-guarantees-and-boundaries).
A result of this run does not hold a change back, so a failure on macOS or Windows is the signal to fix the next change.

The test binary `omnius-core-migration::postgres` starts a Postgres container and needs a container runtime.
It is excluded on Windows and macOS, whose runners cannot start the container.
The observed failures and their sources are in [knowledge.md §2](../knowledge.md#2-list).

## 6. The merge gate

`gate` always runs, needs every other job, and passes when each of them succeeded or was skipped by the scope.
It fails when any of them failed or was cancelled.
It is the only status check that branch protection requires on `main`, so adding or removing a job does not change the protection.

A skipped job does not report, and a native `paths:` filter on a workflow skips the whole workflow, so a required check cannot be a job that the scope may skip.
`gate` reports in every case, which is why the protection names it alone.

## 7. Runtime conventions

| Item | Convention |
| --- | --- |
| Concurrency | One group per workflow and ref. A new pull request run cancels the previous one; a run on `main` or `release/*` is never cancelled |
| Token | The workflow grants the read-only `contents` permission, and no job asks for more |
| Time limits | Every job sets `timeout-minutes`: 30 for the Rust jobs and `e2e`, which builds the daemon, 20 for `ui` and `interface`, and 5 for `changes` and `gate` |
| Rust cache | Keys scoped by operating system and job, saved only on `main`, so the pull requests read the cache of `main` and do not crowd the cache quota |
| Node cache | The npm cache, and the Playwright browsers keyed by the hash of the lockfile |
| Setup steps | The free-space step, the submodule checkout, the timestamp restore, LLVM 20.1, cargo-make, and nextest stay as the current workflows have them |
| Actions | Pinned to a version tag and updated weekly by Dependabot, which also covers the Rust crates and the client's npm packages |

## 8. Design decisions

### 8.1 Decided

#### Gate merges on a single required check

**Decision**
Branch protection on `main` requires the `gate` check and nothing else.

**Reason**
Three pull requests merged while the `test` check was failing, because `main` had no protection.
One check that reports in every case lets the jobs change without editing the protection, and §6 explains why no other job can be the required one.
The cost is that a broken `gate` stops every merge; an administrator removes the requirement to recover.

**Rejected alternatives**
Leaving the checks advisory keeps every merge possible and is what let a failing check through.

#### Run on pull requests and on pushes to the main branches

**Decision**
The workflow runs on `pull_request`, on a push to `main` or `release/*`, and on a manual dispatch. A push to another branch runs nothing.

**Reason**
A pull request run tests the merged result, and the run on the main branch tests it again on every platform.

**Rejected alternatives**
Running on a push to every branch shows a result before a pull request exists, but it tests the branch head rather than the merge, and a branch with a pull request runs twice.

#### Compute the change scope inside one workflow

**Decision**
One workflow decides the scopes of §3.2 in a `changes` job, and the later jobs use its answer.

**Reason**
The scope has to reach a check that always reports (§6), and only a job inside the workflow can skip itself and still let `gate` report.
The cost is a small script that the repository owns.

**Rejected alternatives**
Separate workflows with a native `paths:` filter are simpler, but a skipped workflow never reports, so no check of it could be required.

#### Test the other operating systems after the merge

**Decision**
A pull request runs Linux only, and a push to `main` or `release/*` runs Linux, macOS, and Windows.

**Reason**
The pull request wait stays short, and a platform-specific regression is found within one merge by the run on `main`.
The cost is that such a regression reaches `main` before it is seen.

**Rejected alternatives**
Running all three on every pull request catches the regression before the merge, which suits the `cfg(target_os)` code in the engine, but it multiplies the wait for every change; it is deferred in §8.2.

#### Run the browser flow on pull requests

**Decision**
The `e2e` job runs on a pull request whenever the rust or ui scope changes.

**Reason**
`main` does not stop a change, so a flow that ran only there would report a break after it merged.
The job runs beside the Rust tests, so it adds little to the wait once the daemon build is cached.

**Rejected alternatives**
Running it only on `main` and on a manual dispatch costs nothing on a pull request, but the flow is the only check that drives the client against real daemons.

#### Check the generated interface by regenerating it

**Decision**
The `interface` job regenerates the interface from `openapi.yaml` and fails if the tree differs, apart from the build date.

**Reason**
[daemon-api.md §4.2](./daemon-api.md#42-code-generation-from-openapi) makes regeneration from scratch a precondition, and a hand edit of a generated file is hard to see in a review.
Regenerating the current tree changes only the build date line, so the comparison can hold.
The cost is the generator download, which can fail for a network reason.

**Rejected alternatives**
Leaving it to the review costs nothing, but nothing then enforces the precondition.

### 8.2 Deferred

#### Run all three operating systems on every pull request

**Current state**
Only Linux runs before the merge (§5.1).

**Why not decide now**
The `main` run shows the platform failures within one merge, and the wait of a pull request is the cost of deciding earlier.

**Condition for deciding**
Decide when a failure that only the macOS or Windows run on `main` shows occurs a second time.

#### Run the container-dependent tests on macOS and Windows

**Current state**
The tests that need a container run on Linux only (§5.2).

**Why not decide now**
Neither runner can start the container, and nothing has asked for the container tests there.

**Condition for deciding**
Decide when a hosted runner for the platform can start the container, or when the test no longer needs one.

#### Pin the actions to a commit

**Current state**
The actions are pinned to a version tag that Dependabot updates.

**Why not decide now**
The workflow holds a read-only token and no secret, so a compromised tag has little to reach.

**Condition for deciding**
Decide when a workflow gets a secret, a token with write permission, or publishes an artifact.

#### Run the checks on a schedule

**Current state**
Nothing runs unless a pull request or a main branch receives a change, or a run is dispatched by hand.

**Why not decide now**
No break from a toolchain or dependency update has been seen to hide until the next change.

**Condition for deciding**
Decide when an unrelated pull request is the first to show a break caused by a toolchain or dependency update.

## 9. Current state and remaining work

The `ci` workflow implements the jobs, the scope, and `gate`, and it replaced the earlier `test-daemon-slim` and `test-daemon-all`.
`main` requires `gate` in its branch protection, so a pull request whose `gate` does not pass cannot merge.
The workflow itself is complete.
The macOS run fails at engine tests that Linux passes, and the Windows run fails at one of them intermittently, so a run that tests Rust on the main branches can be red until the bugs in [issues.md](../issues.md) are fixed.

For confirmed bugs, see [issues.md](../issues.md).
