# Company agent guidance

Verification marker: agent-guidance-1

## Root instructions

These apply in every repository that loads this guidance. The repository's own `AGENTS.md` or
`CLAUDE.md` wins where they conflict.

Your job is to make the maintainers' work easy. They receive more PRs than they can review, so
spend your time to save theirs: small focused PRs, self-reviewed, short descriptions, evidence
included.

### Working

- Before editing in a directory, read the `AGENTS.md` and `CLAUDE.md` files on its path.
- Follow the repository's build, test and formatting instructions.
- Ask a needed decision when it comes up, with a recommendation. Do not re-ask settled ones.
- Do not install toolchains or packages or change the machine setup without asking.
- No `git stash`, avoid scratchpads filling up disk space.

### Git and PRs

- Ask before state changing git operation on remote: push, open, update or merge a PR,
  comment, reply, file an issue. When asking, show inline the exact text that will go out (title, body,
  comment).
- Never force-push, rebase or amend a branch with an open PR. Add commits on top, so reviewers keep
  their state.
- One topic per PR. Unrelated fixes go in their own PR.
- Never add a `Co-Authored-By`, `Co-Developed-By` or similar trailer that lists an AI model
  instead of a human being.
- Before opening a PR, review the change with the procedure in
  `.agent-guidance/guidance/code-review.md` (as for someone else's PR, without posting) and fix
  what it raises.

### Writing

- Be brief. People skim what you write. Lead with one or two sentences, then short lists or
  tables. A long paragraph is a defect.
- Write plain declarative sentences a tired engineer would write: the fact, the cause, the change,
  the evidence. Name the file, function or number instead of an adjective.
- Drop AI constructions: contrast framing ("not just X, but Y", "it's not X, it's Y"), lists of
  three for rhythm, run-ups ("Here's the thing"), closing summaries, praise and chat residue,
  stacked hedges, em dashes, bold-keyword bullets. `.agent-guidance/guidance/writing.md` has the
  word list.
- No story time and no dates in prose.
- On GitHub, point at code with a permalink (blob URL pinned to a commit, with `#L10-L20`), not a
  bare file:line. For files in the same repository GitHub renders it as a code snippet.
- Leave out what the reader already sees: commit hashes ("Pushed abc123"), branch names, a recap of
  the diff.
- Reread before posting and rewrite any sentence that has one of these tells.
- Do not hard-wrap text that GitHub renders.

### Code

- Fix lint warnings in the code. Never silence them with `#[allow]`, `#[expect]` or similar.
- Put unit tests in their own test submodule file, not inline in the production file.
- Test private behavior with unit tests. A test-only hook in production code is a last resort; keep
  it as small and unintrusive as possible.
- Comments explain a non-obvious why. No history, dates or references to the session.
- Never make inline comments that state what the code does. Use doc comments. Break functions up
  if you find yourself writing inline comments. It's a code smell. Inline commends are barely
  ever needed in a well designed code base.
- Prove a fix with a test that fails before and passes after. Judge a run by its real exit code and
  per-test results, not a summary or a pipe's status.
- Public text about a security fix states the new behavior, never that the old one was
  exploitable. See `.agent-guidance/guidance/security.md`.

## Index

Read a file listed here when its description matches the task. Paths are relative to the
repository root.

- `.agent-guidance/guidance/writing.md`: read when writing PR text, commits, issues, review
  comments, docs, code comments or replies to the user.
- `.agent-guidance/guidance/orchestration.md`: read when delegating to subagents or writing a
  worker or reviewer brief.
- `.agent-guidance/guidance/git-and-prs.md`: read when branching, committing, pushing, or opening
  or updating a PR.
- `.agent-guidance/guidance/code-review.md`: read when asked to review a PR, and before opening
  your own.
- `.agent-guidance/guidance/rust.md`: read when writing or reviewing Rust.
- `.agent-guidance/guidance/testing-and-ci.md`: read when writing tests, running suites or
  triaging CI failures.
- `.agent-guidance/guidance/issues.md`: read when filing, updating or closing an issue.
- `.agent-guidance/guidance/security.md`: read when a change fixes a security hole or touches
  private findings.
- `.agent-guidance/guidance/shell.md`: read when running shell commands, especially unattended.
