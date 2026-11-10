# agent-guidance

Company-wide guidance for AI coding agents, delivered to every repository that installs the loader.
`GUIDANCE.md` holds the root instructions and an index of topic files in `guidance/`.

## How it works

A repository commits the loader and its hook configs once. See [docs/loader.md](docs/loader.md).

- At session start the hook runs the loader.
- The loader fetches this repository into `.agent-guidance/` and injects `GUIDANCE.md`.
- Changes merged here reach every repository without a change downstream.
- The loader fetches in the background, so the next session gets the update.
- Where no hook delivers the guidance, the [fallback line](#installing-in-a-repository) in
  `AGENTS.md` points the agent to it.

## Why a loader

A copy in each repository turns every change into a PR per repository. A plugin needs a setup per
harness, and an update can run new code on every machine that has it. See
[docs/why-a-loader.md](docs/why-a-loader.md).

## Installing in a repository

A maintainer runs the installer once per repository. Contributors need only bash and git.

```sh
cargo install --git https://github.com/paritytech/agent-guidance
cd path/to/repository
agent-guidance-install
```

- It writes the loader and one hook config per harness.
- It adds the cache to `.gitignore`.
- It makes `CLAUDE.md` import `AGENTS.md`.
- It puts this fallback line at the top of `AGENTS.md`:
  > Company agent guidance lives in `.agent-guidance/GUIDANCE.md`. If that
  > directory is missing, clone https://github.com/paritytech/agent-guidance into
  > `.agent-guidance/` and read it.
- It never commits.
- It prints the `git add` command to run.
- [docs/installer.md](docs/installer.md) lists every file it writes and how to
  [upgrade the loader](docs/installer.md#upgrading-the-loader).

## Using it in all your repositories

A user-level hook gives you the guidance in Claude Code in every repository you open.

```sh
cargo install --git https://github.com/paritytech/agent-guidance
agent-guidance-install --user
```

- It covers only your own Claude Code sessions.
- Repositories still need the repository install for other contributors and harnesses.
- Its status lines start with `parity-agent-guidance (user): `, such as
  `parity-agent-guidance (user): loaded <sha> (cached)`.
- In a repository with its own loader hook it injects nothing and shows
  `parity-agent-guidance (user): skipped, this repository loads its own`.
- See [User install](docs/installer.md#user-install) and [User mode](docs/loader.md#user-mode).

## Verify it works

Start a session in Claude Code, Codex or Gemini CLI in a repository with the loader installed.

- A line such as `parity-agent-guidance: loaded <sha> (cached)` appears, or
  `parity-agent-guidance (user): loaded ...` with the
  [user hook](#using-it-in-all-your-repositories).
- A line that does not start with `parity-agent-guidance: loaded` or
  `parity-agent-guidance (user): loaded` means a problem, except the user hook's
  `skipped, this repository loads its own`. The [status line table](docs/loader.md#status-line)
  lists each line.
- Ask the agent "Quote the guidance verification marker."
- It answers with the `Verification marker:` line from `GUIDANCE.md`.
- Change the marker here to confirm that an update arrived.
- With an existing cache, the new marker appears in the second session after the push.
- To check the loader alone, run `.agent-guidance-loader/load.sh claude` from the repository root.

## Harnesses

Hooks run only in a trusted workspace. Details are in [docs/harnesses.md](docs/harnesses.md).

| Harness | Config file | Main limitation |
|---|---|---|
| Claude Code | `.claude/settings.json` | None beyond trust on first use |
| Codex | `.codex/hooks.json` | Hooks need trust in `/hooks`; no SessionStart in Codex Desktop |
| Gemini CLI | `.gemini/settings.json` | None beyond trust on first use |
| Cursor | `.cursor/hooks.json` | `additional_context` may not reach the model (open bugs) |
| Copilot CLI and cloud agent | `.github/hooks/agent-guidance.json` | Context not confirmed for the cloud agent |

## Trust model

`GUIDANCE.md` steers agents that can run commands, so push access here is high-trust.

- The loader fetches only text and never executes a fetched file.
- A new loader reaches a repository only through a commit by its maintainer.
- `main` requires a PR with one approving review and passing CI, also for admins.

## Writing guidance

`GUIDANCE.md` has two sections:

- "Root instructions" applies to every repository.
- "Index" lists each topic file by its path from the downstream root, with one line that says when
  to read it:
  - `.agent-guidance/guidance/writing.md`: read when writing PR text, commits, issues, review
    comments, docs, code comments or replies to the user.

## Development

The installer, its templates and the tests are in `tools/`. Test setup, CI and how to resync the
root hook configs are in [docs/development.md](docs/development.md). Run before committing:

```sh
cargo test --manifest-path tools/Cargo.toml
cargo fmt --check --manifest-path tools/Cargo.toml
cargo clippy --all-targets --manifest-path tools/Cargo.toml -- -D warnings
shellcheck tools/templates/.agent-guidance-loader/load.sh
```
