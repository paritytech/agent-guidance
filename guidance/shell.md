# Shell

How to write shell commands that allow rules match, so the harness neither asks the user nor
denies them.

- "Keep each call simple" and the first two bullets of "Files" apply when the harness asks the
  user about, or denies, a shell command that no allow rule matches. Skip them while it approves
  such commands itself, such as Claude Code in auto mode.
- The rest applies to every shell command.
- Pass these rules on in the brief of every worker whose harness asks or denies.

## Keep each call simple

- One command per call, or a short pipe into a filter such as `grep`, `head` or `wc`.
- Run independent commands as separate calls, in parallel.
- No command substitution (`$(...)` or backticks).
- No shell variables that you set and then expand.
- No `for` or `while` loops.
- No `&&` or `;` chains that join several tools.
- No `cd dir && ...`. Use absolute paths or the tool's own flag (`git -C`,
  `cargo --manifest-path`).

## Files

- Write files with the harness's file-write and edit tools. No heredocs, no `echo >`.
- Read files with the harness's read and search tools, not `cat`, `sed -n` or `grep` over files.
- Write payloads for `gh api` (JSON, GraphQL, comment bodies) to a file under the repository's
  ignored scratch directory, then pass the file (`--input file`, `-F body=@file`).

## Multi-step logic

- When a task needs multi-step shell logic, put it in a script file in the scratch directory, or
  use the repository's existing task runner, and run it in one short call.
- Destructive commands in scripts: see "Worktrees and local state" in
  `.agent-guidance/guidance/git-and-prs.md`.

## Results

- Exit codes, redirecting output and long runs: see "Running tests" and "Judging a run" in
  `.agent-guidance/guidance/testing-and-ci.md`. The same rules apply to any command.

## Commands for the user

- Commands proposed to the user are single-line, ready to paste.
- Commands that need a TTY or interactive login (for example `gh auth refresh`) are for the user to
  run. Say so instead of trying.
- zsh does not word-split an unquoted `$VAR`. Do not rely on splitting in commands meant for the
  user's shell.
