# Testing and CI

How to prove a change works, how to judge a test run, and how to triage CI. Builds and tests are
cargo unless the repository says otherwise.

## Proving a change

- Prove a behavior change or fix with a test that fails before the change and passes after it. The
  test must tell the two versions apart.
- Failing to reproduce a bug is not proof that it is fixed.
- Capture data (logs, traces, a failing input) before proposing a fix.
- Test the real execution path, not a mock of the code under change.
- Do not tune system parameters such as timeouts to fit undersized test infrastructure. Fix the
  real cost, then size the runner.

## Running tests

- Build and test every feature and cfg combination the change affects, not only the default.
  Compile the conditional configurations again after a rebase or merge.
- Triage regressions with retries off and no fail-fast (for nextest:
  `cargo nextest run --retries 0 --no-fail-fast`). Retries let real failures pass as flaky, and
  fail-fast hides later failures.
- Run long suites in the background and do other work meanwhile.
- Run edit scripts in the foreground and confirm the result with `git status` before testing. A
  failed edit followed by a green run on an unchanged tree looks like a pass.
- Freeze the source while the final tests run. Any later edit means running them again.
- Parallel sessions that share build or install directories collide. When results look odd, rerun
  alone first.
- Announce benchmarks and timed runs and wait for a go. Background load ruins timings.

## Judging a run

- Judge a run by its real exit code and the per-test results. Not by a summary line, a fail-fast
  report, or a pipe's status (`cmd | tee log` returns the exit code of `tee`).
- Redirect output with `> file 2>&1` and read the file, rather than piping into `tee`.
- Keep evidence kinds apart: local runs, CI runs and measurements. Say which one a claim rests on.
- Never claim suite success from a partial run or a summary.

## Triaging CI

- Many red checks often come from one real failure: the workflow cancels the remaining jobs. Jobs
  that end with "The operation was canceled." and conclusion `cancelled` were cancelled by that
  failure. Fix the first real failure.
- Before blaming a red job on the change or calling it flaky, run the base-branch check in step 4
  of `.agent-guidance/guidance/code-review.md`.
- Read the failing log (`gh run view {id} --log-failed`) and quote the actual error.
- Bot-triggered workflows may run from the bot's branch. Check `headBranch` and `headSha` with
  `gh run view {id} --json headBranch,headSha` before trusting the result.

## Reruns and dispatches

- No blind reruns. One unchanged retry is fine for a verified infrastructure failure.
- Do not dispatch CI without a code change, for example to measure flakiness. At most one dispatch
  per question, with its cost stated.
- Do not reorder jobs to influence runner priority.
- Merge only on complete green CI on the exact head. Failed, cancelled, skipped or timed-out jobs
  are not green.
