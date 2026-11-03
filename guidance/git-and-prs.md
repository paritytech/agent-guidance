# Git and PRs

How to branch, commit and open PRs so that review costs the maintainers as little time as
possible. Writing code is cheap. Review is the bottleneck.

## Worktrees and local state

- Start every task on its own branch in its own git worktree. The main checkout belongs to the
  user, who edits there by hand, and parallel sessions would share it.
- Put worktrees and scratch files inside the repository under a directory that git ignores. Never
  beside the repository or in a shared parent directory. If the directory is not ignored, add it to
  `.git/info/exclude`, not to the tracked `.gitignore`.
- Never use `git stash`. The stash is shared by every worktree, so a parallel session's pop can take
  your entry. Commit work in progress or copy it aside.
- Never put `git checkout -- .`, `git reset --hard` or `rm -rf` of a source directory in a
  scripted chain. Commit work in progress first.
- Use explicit commit shas in commands. Do not rely on `FETCH_HEAD`: parallel sessions overwrite
  it.

## Local and remote actions

- Commit locally as often as useful. No permission needed.
- Ask before anything that leaves the machine: push, open, update or merge a PR, comment, reply,
  review, file or edit an issue. A go applies to the action it was given for, in the current turn.
- When asking, show inline the exact text that will go out: PR title and body, comment text,
  issue title and body. The user approves the text, not a summary of it.
- After a go on a PR, watch CI and report the result. Pushing a fix needs its own go.
- A credential or permission limit is never a reason to work around branch protection. Report it
  and ask.

## Open PRs

- Never force-push, rebase or amend a branch with an open PR. Add commits on top, so reviewers can
  see what changed since their last round.
- Resolve merge conflicts hunk by hunk. Never take a whole file from one side.
- When the repository requires approval of the most recent push, pushing to someone else's PR
  branch makes you the last pusher, and your approval no longer counts on that PR.

## Scope

- One topic per PR. Related fixes from one investigation go in one PR; each PR costs a CI run and a
  review.
- Keep the diff small. Split a large change into PRs a reviewer can take one at a time.
- Unrelated fixes found along the way (build, toolchain, infra, a bug elsewhere) go in their own
  branch and PR.
- No drive-bys: an unrelated cleanup, rename or reformat is its own PR. A toolchain bump that
  reformats untouched files is its own PR.
- Companion edits (registries, schema versions, config samples, docs that describe the change) land
  in the same PR.
- Do not add helpers or types to a shared or core crate for a one-off local need. The bar is more
  than one consumer today and a stable API.
- Do not weaken a design to avoid overlapping another open branch. Coordinate instead.
- Implement a maintainer's requested design as worded. Prefer clarity over fewer lines.

## Before opening a PR

- Review the change with `.agent-guidance/guidance/code-review.md`, section "Self-review before
  opening a PR", and fix what it raises in the branch.
- Run the repository's build, tests, formatter and linters on the final head. Builds and tests are
  cargo unless the repository says otherwise.
- Check `git diff --stat` against the base for stray files and reformats.

## PR description

A reviewer should understand the PR in about a minute. Follow
`.agent-guidance/guidance/writing.md`.

- Title: imperative, 70 characters or fewer, states the change.
- Problem: the symptom (the exact error line when there is one), then the cause. Mark conjecture.
- Changes: short bullets, one per change a reviewer has to know about. Not a file-by-file recap.
- Alternatives: only the ones a reviewer would ask about, one line each with why rejected.
- Verification: the commands run and their results. Link CI runs when they are the evidence.
- No story of the session, no dates, no commit hashes or branch names.
- Link the issue it closes with `Closes #N`.
