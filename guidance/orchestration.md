# Orchestration

How to split work between a top-level agent and subagents, what goes in a brief, and how changes
get reviewed. This applies to single-step tasks too.

## Why delegate by default

- The orchestrator's context holds the user's intent, the plan, decisions and evidence. File dumps
  and build logs fill it.
- An orchestrator that is not busy editing stays responsive to new requests from the user.
- The orchestrator knows the whole task, so it writes a better brief than a worker would write for
  itself.

## Roles

- The orchestrator owns intent, planning, task assignment, decisions, review, publication and
  communication with the user. It reads instructions and concise results, not raw output.
- A worker owns one bounded task and the files or worktree it was given. It is the only writer
  there. Workers do the work directly and do not delegate further.
- Workers never push, open or update PRs, comment, or file or edit issues. They report findings;
  the orchestrator verifies them and decides what goes out.
- A worker that finds something in the same area outside its task proposes it as a candidate. It
  folds the change in only when the orchestrator agrees.
- In a PR review the orchestrator launches the 3b area agents, the bug pass and the draft
  reviewers. One drafting agent gets their reports, reads the diff and verifies each finding
  itself. After sign-off the orchestrator runs Mechanics. The drafting agent may write the payload
  files into the PR's scratch directory. See `.agent-guidance/guidance/code-review.md`.

## The brief

A worker knows only what the brief says. Include:

- The goal, what is out of scope, and what was already ruled out.
- The files or responsibilities it owns, the worktree path and branch, and the base commit.
- The full `AGENTS.md` and `CLAUDE.md` files on the touched paths, by path, to read in full. Do not
  pass a selection of rules: briefs with picked rules miss requirements.
- The shell rules from `.agent-guidance/guidance/shell.md` when the session runs with a permission
  checker.
- Where scratch files and evidence go (inside the repository, under an ignored directory).
- Acceptance criteria: the tests and checks to run, and what counts as done.
- The report format: changes or findings, validation evidence with commands and results, open
  limitations. Concise.

## Models

- Set the model explicitly on every subagent. Do not rely on a default.
- The root session's model is the maximum. Spawn subagents on the same model by default. The
  orchestrator usually runs the top tier.
- When a task is very straightforward, ask the user which lower model to use for it. Do not pick a
  lower model without asking.
- Exception: when the root session runs on Fable, ask before spawning a new Fable agent. Fable is
  expensive.
- Never claim a model setting that was not applied.
- A resumed agent keeps the model it started with.

## Review of every change

- When a worker writes or changes code, a separate reviewer agent that did not write it reviews the
  actual diff.
- The reviewer gets the frozen head, the diff, the evidence, and the same guidance files as the
  worker. It checks correctness, regression risk, compliance with the guidance, and whether the
  validation is adequate.
- Have the reviewer report a pass, fail or n/a line per guidance section, so missed rules show up.
- The worker fixes actionable findings. The reviewer then verifies the fixes.
- Any edit after review means the review and the tests run again on the new head.
- Run an independent reviewer over public text before it is posted, with explicit checks (tone,
  claims, no security leaks).

## Parallel work

- Run tasks in parallel only when their edits and shared resources (build directories, ports,
  install trees) cannot collide. Otherwise run them in sequence.
- Never run two writers on one worktree.
- Before replacing a worker, have it write a checkpoint: hypothesis, commits, evidence paths, next
  step, running processes. The replacement resumes from it.
- Workers track every process they start and finish or hand off each one.
- Clean up only resources you own, never another session's.
- Use the harness's real resume operation to continue an agent. A message to a finished agent may
  not wake it.

## Decisions

- Ask the user a needed decision when it comes up, with a recommendation. Do not park it for the
  end of the task.
- Do not re-ask a decision the user already made.
- Ask before any side effect on the user's machine: installing toolchains, targets or packages,
  setting up cross-compilation, changing global config. If a platform cannot be checked here, say
  so instead.
- Do not take screenshots of the user's screen or launch their apps while they may be using them.
- If subagents or independent review are not available, stop the affected work, say why, and ask
  the user for an exception. Do not fall back to doing it yourself silently.
- Delegation and review do not grant permission to publish, merge, or widen the task.
