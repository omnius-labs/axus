# Plan: the continuous integration workflow

## 1. About this plan

This plan replaces the two Rust-only workflows with the single `ci` workflow of [ci.md](../design/ci.md#1-about-this-document), adds the client, the browser flow, and the generated-interface checks, and makes `gate` a required check of `main`.

### 1.1 Division of responsibilities among documents

| Document | Owns |
| --- | --- |
| This document | The order, steps, and acceptance checks of this work |
| [ci.md](../design/ci.md#1-about-this-document) | Triggers, change scope, jobs, the platform split, the merge gate, and the runtime conventions |
| [web-client.md](../design/web-client.md#5-build-and-development) | The client's build and test tools that the `ui` and `e2e` jobs use |
| [daemon-api.md](../design/daemon-api.md#42-code-generation-from-openapi) | The generation that the `interface` job repeats |
| [README.md](../../README.md) | The procedures that the new CI section joins |

The family map is held by [design.md](../design.md#11-division-of-responsibilities-among-documents).

### 1.2 Creation date and target commit

Created on 2026-11-07 against `558c053`.

## 2. Purpose and completion conditions

A change merges only when the checks that match what it touches have passed, and a failing check can no longer be merged past.

### 2.1 Completion conditions

1. A pull request that touches only files outside `daemon/`, `ui-web/`, and `.github/` runs `changes` and `gate` only, and `gate` passes.
2. A pull request that touches only `ui-web/**` runs `ui` and `e2e`, and does not run `rust-lint`, `rust-test`, or `interface`.
3. A pull request that changes a Rust source file under `daemon/`, outside `daemon/entrypoints/interface/`, runs `rust-lint`, `rust-test` on Linux only, and `e2e`, and does not run `ui` or `interface`.
4. A pull request that changes `daemon/openapi.yaml` runs `rust-lint`, `rust-test`, `ui`, `interface`, and `e2e`.
5. A push to `main` or `release/*` that changes `daemon/**`, and a manual dispatch, run `rust-test` on Linux, macOS, and Windows, and the run passes with the container-dependent tests excluded only on the operating systems whose runner cannot start the container.
6. A hand edit to a file under `daemon/entrypoints/interface/` fails `interface`, and the unedited tree passes it.
7. A `rust-test` run with several failing tests reports every one of them, and no test is left unrun because of an earlier failure.
8. `gate` fails when a job it needs fails or is cancelled, and passes when jobs are skipped by the scope.
9. `main` requires the `gate` check, and a pull request whose `gate` failed cannot be merged.
10. The workflow grants only the read-only `contents` permission, every job sets a time limit, and a new push to a pull request cancels its previous run while a run on `main` is never cancelled.
11. Dependabot proposes updates for the npm packages of `ui-web/`.
12. The README describes what the workflow runs and how to run the same checks by hand.

### 2.2 Non-goals

- Running all three operating systems on a pull request, pinning the actions to a commit, a scheduled run, and the container-dependent tests on Windows are deferred in [ci.md §8.2](../design/ci.md#82-deferred).
- A release or deploy workflow is outside the boundary of [ci.md §2](../design/ci.md#2-responsibilities-and-boundaries).

## 3. Current state

Two workflows exist.
[test-daemon-slim.yml](../../.github/workflows/test-daemon-slim.yml#L3-L9) runs on a push to every branch except `main` and `release/*`, with no path filter, and runs the Rust lint and the tests through `cargo make test` on Linux ([L112-L113](../../.github/workflows/test-daemon-slim.yml#L112-L113)).
[test-daemon-all.yml](../../.github/workflows/test-daemon-all.yml#L3-L11) runs on a push to `main` or `release/*` that touches `daemon/**` other than `daemon/docs/**`, on [three operating systems](../../.github/workflows/test-daemon-all.yml#L68-L72), and the matrix cancels the others when one fails.
Both cancel an earlier run of the same ref ([slim L11-L13](../../.github/workflows/test-daemon-slim.yml#L11-L13), [all L14-L15](../../.github/workflows/test-daemon-all.yml#L14-L15)).

`cargo make test` depends on the lint, so `lint` runs twice in each workflow ([Makefile.toml:22-26](../../daemon/Makefile.toml#L22-L26)).
[nextest.toml](../../daemon/.config/nextest.toml#L1-L7) defines the group that limits the loopback transfer tests, and no profile for the workflows.
[dependabot.yml](../../.github/dependabot.yml#L1-L24) covers the actions and the Rust crates.
[gen-interface.sh](../../daemon/entrypoints/gen-interface.sh#L1-L15) regenerates the interface, and no workflow runs it.

The failure of the Windows job on `main` is recorded in [container-tests-fail-on-windows-runner](../issues/container-tests-fail-on-windows-runner.md).
`main` has no branch protection, and #155, #156, and #157 merged while their `test` check was failing.
This is not enough because a failing check does not stop a merge, the client and its browser flow are not checked, and a change to the client runs Rust lint and tests that it cannot affect.

## 4. Approach

The new `.github/workflows/ci.yml` holds the jobs of [ci.md §4](../design/ci.md#4-jobs), added one group at a time, and the two old workflows are removed with the first group so that the checks never run twice.
The gate comes last, after every job has run on real pull requests.

### 4.1 Decisions this plan depends on

| Decision | Source |
| --- | --- |
| `gate` is the single required check | [ci.md §8.1](../design/ci.md#gate-merges-on-a-single-required-check) |
| Triggers on pull requests and on pushes to the main branches | [ci.md §8.1](../design/ci.md#run-on-pull-requests-and-on-pushes-to-the-main-branches) |
| The change scope is computed inside one workflow | [ci.md §8.1](../design/ci.md#compute-the-change-scope-inside-one-workflow) |
| Other operating systems run after the merge | [ci.md §8.1](../design/ci.md#test-the-other-operating-systems-after-the-merge) |
| The browser flow runs on pull requests | [ci.md §8.1](../design/ci.md#run-the-browser-flow-on-pull-requests) |
| The generated interface is checked by regenerating it | [ci.md §8.1](../design/ci.md#check-the-generated-interface-by-regenerating-it) |

### 4.2 Impact

- Adds `.github/workflows/ci.yml` and removes `test-daemon-slim.yml` and `test-daemon-all.yml`. The README badge points at the new workflow.
- Adds a nextest profile for the workflows, and `.node-version` for the client.
- Adds the npm ecosystem to `dependabot.yml`.
- Changes the required check names of `main`, which is the repository setting of step 6, outside the code.
- No Rust code, API, configuration file, or storage format changes. No term is added, so [terms.md](../terms.md) is unchanged.

## 5. Steps

| No. | Content | Depends on | Status |
| --- | --- | --- | --- |
| [0](#s-0) | Observe the container tests on macOS | | not started |
| [1](#s-1) | The `ci` workflow with the scope, the Rust jobs, and the gate; remove the old workflows | 0 | not started |
| [2](#s-2) | The `ui` job | 1 | not started |
| [3](#s-3) | The `e2e` job | 2 | not started |
| [4](#s-4) | The `interface` job | 1 | not started |
| [5](#s-5) | Dependabot for the client's packages | | not started |
| [6](#s-6) | Require `gate` on `main` | 1, 2, 3, 4 | not started |

<a id="s-0"></a>
### Step 0. Observe the container tests on macOS

**Locations to change**
A throwaway branch that sets `fail-fast: false` in the matrix of [test-daemon-all.yml](../../.github/workflows/test-daemon-all.yml#L70-L72). The branch is not merged.

**What to do**
Push the throwaway branch, run `gh workflow run test-daemon-all.yml --ref scratch/ci-macos-probe`, follow it with `gh run watch`, and read the result of `omnius-core-migration::postgres` on the macOS job with `gh run view --log`.
Record whether the tests ran and passed or could not start the container.
The exclusion of step 1 covers Windows, and macOS only if this shows that the container cannot start.

**Completion criterion**
The macOS result for the `omnius-core-migration::postgres` tests is written in the description of the pull request of step 1.

<a id="s-1"></a>
### Step 1. The `ci` workflow with the scope, the Rust jobs, and the gate

**Locations to change**
New `.github/workflows/ci.yml` with `changes`, `rust-lint`, `rust-test`, and `gate`. Removed [test-daemon-slim.yml](../../.github/workflows/test-daemon-slim.yml#L1-L113) and [test-daemon-all.yml](../../.github/workflows/test-daemon-all.yml#L1-L119). [nextest.toml](../../daemon/.config/nextest.toml#L1-L7) for the profile. [README.md](../../README.md#L7) for the badge and a new CI section.

**What to do**
Write the triggers of [ci.md §3.1](../design/ci.md#31-triggers), the scope of [ci.md §3.2](../design/ci.md#32-change-scope), the token, the concurrency, the time limits, and the cache keys of [ci.md §7](../design/ci.md#7-runtime-conventions).
Add a nextest profile that does not stop at the first failure and keeps the transfer group.
`rust-test` calls `cargo nextest` with that profile, runs on Linux for a pull request and on the three systems for a push, and excludes `binary_id(omnius-core-migration::postgres)` on the systems that step 0 showed cannot start the container.
`gate` needs every job and checks the results with `always()`.
Add the CI section to the README: what runs for which change, and the commands to run the same checks by hand.

**Completion criterion**
A documentation-only pull request shows `changes` and `gate` passing and no other job, a pull request that touches `daemon/**` shows `rust-lint` and `rust-test` on Linux passing, and a manual dispatch on the branch shows `rust-test` on three systems passing.

<a id="s-2"></a>
### Step 2. The `ui` job

**Locations to change**
`.github/workflows/ci.yml`, and a new `.node-version` at the repository root.

**What to do**
Add the `ui` job: set up the pinned Node version with the npm cache, then run `npm ci`, `npm run lint`, `npm run build`, and `npm run test` in `ui-web/`.
The pinned version is one that satisfies the `engines` of the client's build tools.

**Completion criterion**
A pull request that touches only `ui-web/src` shows `ui` passing and no Rust job, and a pull request that introduces a Biome error in `ui-web/src` fails `ui`.

<a id="s-3"></a>
### Step 3. The `e2e` job

**Locations to change**
`.github/workflows/ci.yml`, and [playwright.config.ts](../../ui-web/playwright.config.ts#L4-L17) if the failure trace needs a setting.

**What to do**
Add the `e2e` job for the rust or ui scope: set up Node, build the daemon with `cargo build -p omnius-axus-daemon` on the Rust cache of `rust-test`, install Chromium with its system packages, and run `npm run test:e2e` with `AXUS_DAEMON_BIN` set.
Cache the browsers by the hash of `ui-web/package-lock.json`, set no retry, and upload `ui-web/test-results` when the job fails.

**Completion criterion**
A pull request that touches `ui-web/src` shows `e2e` passing in under 20 minutes, and a pull request that breaks a label the flow reads fails it and attaches the trace.

<a id="s-4"></a>
### Step 4. The `interface` job

**Locations to change**
`.github/workflows/ci.yml`, and [gen-interface.sh](../../daemon/entrypoints/gen-interface.sh#L1-L15) only if the comparison needs the script to leave the tree untouched.

**What to do**
Add the `interface` job for the interface scope: set up Java and Node, run `gen-interface.sh`, and fail if `git diff` shows a difference other than the `Build date` line of `entrypoints/interface/README.md`.

**Completion criterion**
A pull request that edits a generated file by hand fails `interface`, and a pull request that changes `openapi.yaml` together with the regenerated interface passes it.

<a id="s-5"></a>
### Step 5. Dependabot for the client's packages

**Locations to change**
[dependabot.yml](../../.github/dependabot.yml#L15-L24).

**What to do**
Add the `npm` ecosystem for `/ui-web`, weekly and grouped like the existing ones.

**Completion criterion**
`.github/dependabot.yml` lists the npm ecosystem for `/ui-web`, and the Dependabot configuration check on the pull request reports no error.

<a id="s-6"></a>
### Step 6. Require `gate` on `main`

**Locations to change**
The branch protection setting of `main` in the repository, through `gh api`. No file changes.

**What to do**
After steps 1 to 4 have run on merged pull requests, ask the maintainer to confirm, and then require the `gate` status check on `main` and nothing else.

**Completion criterion**
The required status checks of `main` are exactly `gate`, and a pull request whose `gate` failed is reported as blocked.

## 6. Acceptance verification

Each row that needs a pull request uses a throwaway branch that is closed unmerged.
Run the commands of a row from the root of a clone, on a branch created from `main`, in a shell where `run_id() { gh run list --workflow ci.yml --branch "$1" --event "$2" --limit 1 --json databaseId -q '.[0].databaseId'; }` is defined.
The pull request of condition 7 is not a draft, because a draft cannot be merged whatever the checks say.

| Condition | Command | Expected result |
| --- | --- | --- |
| 1 | `git switch -c scratch/ci-docs main && printf '\n' >> docs/plans.md && git commit -qam scratch && git push -q -u origin scratch/ci-docs && gh pr create --draft --fill && gh pr checks --watch` | `changes` and `gate` are `pass`, and no other job is listed |
| 2 | `git switch -c scratch/ci-ui main && printf '// scratch\n' >> ui-web/src/main.tsx && git commit -qam scratch && git push -q -u origin scratch/ci-ui && gh pr create --draft --fill && gh pr checks --watch` | `changes`, `ui`, `e2e`, and `gate` are `pass`; `rust-lint`, `rust-test`, and `interface` are not listed |
| 3 | `git switch -c scratch/ci-rust main && printf '// scratch\n' >> daemon/entrypoints/daemon/src/main.rs && git commit -qam scratch && git push -q -u origin scratch/ci-rust && gh pr create --draft --fill && gh pr checks --watch` | `changes`, `rust-lint`, `rust-test (ubuntu-latest)`, `e2e`, and `gate` are `pass`; `ui`, `interface`, and the other operating systems are not listed |
| 4 | `git switch -c scratch/ci-openapi main && printf '# scratch\n' >> daemon/openapi.yaml && git commit -qam scratch && git push -q -u origin scratch/ci-openapi && gh pr create --draft --fill && gh pr checks --watch` | `rust-lint`, `rust-test`, `ui`, `interface`, `e2e`, and `gate` are all listed and `pass` |
| 5 | `git push -q origin scratch/ci-rust:refs/heads/release/ci-probe && gh workflow run ci.yml --ref scratch/ci-rust && sleep 20 && gh run watch "$(run_id release/ci-probe push)" --exit-status && gh run watch "$(run_id scratch/ci-rust workflow_dispatch)" --exit-status && gh run view "$(run_id scratch/ci-rust workflow_dispatch)" --json jobs -q '.jobs[] \| select(.name \| test("windows")) \| .databaseId' \| xargs -I{} gh run view --job {} --log \| { rg -c 'omnius-core-migration::postgres' \|\| echo 0; }` | Both runs end `success`, each with `rust-test` on the three systems; the count printed for the Windows job is `0`, and the macOS job follows the observation of step 0 |
| 6 | On `scratch/ci-rust`, `printf '// scratch\n' >> daemon/entrypoints/interface/src/lib.rs && git commit -qam edit && git push -q && gh pr checks --watch` | `interface` is `fail` and its log shows the diff; condition 4 shows the unedited tree passing |
| 7 | `git switch -c scratch/ci-failures main && printf '#[cfg(test)]\nmod scratch {\n    #[test]\n    fn first() {\n        panic!("first");\n    }\n\n    #[test]\n    fn second() {\n        panic!("second");\n    }\n}\n' >> daemon/entrypoints/daemon/src/main.rs && git commit -qam scratch && git push -q -u origin scratch/ci-failures && gh pr create --fill && gh pr checks --watch; gh run view "$(gh run list --workflow ci.yml --branch scratch/ci-failures --limit 1 --json databaseId -q '.[0].databaseId')" --log-failed \| rg 'Summary\|not run due to test failure'` | The nextest summary counts both failures, and no `not run due to test failure` line appears |
| 8 | Conditions 1 to 4 and condition 7 | `gate` is `pass` when jobs were skipped by the scope, and `fail` when `rust-test` failed |
| 9 | `gh api repos/omnius-labs/axus/branches/main/protection --jq '.required_status_checks.contexts'`, then `gh pr view scratch/ci-failures --json mergeStateStatus -q .mergeStateStatus` | The contexts are `["gate"]`, and the state is `BLOCKED` |
| 10 | `rg -n 'permissions:\|contents: read\|timeout-minutes\|cancel-in-progress' .github/workflows/ci.yml` | A `permissions` block with `contents: read`, a `timeout-minutes` on every job, and a `cancel-in-progress` that is true for a pull request only |
| 11 | `rg -n -A1 'package-ecosystem: npm' .github/dependabot.yml` | The npm ecosystem with `directory: "/ui-web"` |
| 12 | `rg -n -A14 '^## .*CI' README.md` | A CI section that lists the job run for each kind of change and the local commands `cargo make lint`, `cargo nextest run --all-features`, and `npm run test:e2e` |

Conditions 1 to 8 run after step 4, condition 9 after step 6, and the branches are deleted at the end with `gh pr close --delete-branch`.

## 7. Risks and rollback

- **The required check (step 6)** changes a repository setting, which `git revert` does not undo. If `gate` is misconfigured, no pull request can merge. An administrator removes the requirement with `gh api -X DELETE repos/omnius-labs/axus/branches/main/protection/required_status_checks`, and the setting is applied again after the fix.
- **Replacing the workflows (step 1)** reverts with `git revert` of the merge, which restores the two old workflows. Between the merge and the setting of step 6, a failing `gate` does not stop a merge.
- **The generator download (step 4)** needs the network, so a failed run is run again; if it fails often, the job is made non-required by leaving it out of `gate`'s `needs`, and the decision in [ci.md §8.1](../design/ci.md#check-the-generated-interface-by-regenerating-it) is revisited.
