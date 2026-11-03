# agent-guidance

Company-wide guidance for AI coding agents, delivered to every repository that installs the loader.
`GUIDANCE.md` holds the instructions that apply everywhere and an index of topic files under
`guidance/`.

## How it works

A repository commits a small loader script and one hook config per agent harness, once. Loader
internals are in [docs/loader.md](docs/loader.md).

- At the start of each agent session the hook runs the loader.
- The loader fetches this repository into `.agent-guidance/` and injects `GUIDANCE.md`.
- Changes merged here reach every repository without a change downstream.
- A session fetches updates in the background, and the session after it uses them.
- Where no hook delivers the guidance, the [fallback line](#installing-in-a-repository) in
  `AGENTS.md` and `CLAUDE.md` points the agent to it.

## Why a loader

Copying the guidance into each repository's `AGENTS.md` and `CLAUDE.md` would turn every change
into a PR per repository. A plugin needs a separate setup in each harness, and an update can run
new code on every machine that has it. The comparison and the loader's costs are in
[docs/why-a-loader.md](docs/why-a-loader.md).

## Installing in a repository

A maintainer runs the installer once per repository. Contributors only need bash and git.
[docs/installer.md](docs/installer.md) lists what the installer writes, file by file.

```sh
cargo install --git https://github.com/paritytech/agent-guidance
cd path/to/repository
agent-guidance-install
```

- The installer writes the loader and one hook config per harness.
- It adds cache entries to `.gitignore`.
- It adds the fallback line below to `AGENTS.md` and `CLAUDE.md`.
- It never commits. It prints the `git add` command to run.
- To upgrade the loader, see [Upgrading the loader](docs/installer.md#upgrading-the-loader).

The fallback line covers sessions where no hook delivers the guidance:

> Company agent guidance lives in `.agent-guidance/GUIDANCE.md`. If that
> directory is missing, clone https://github.com/paritytech/agent-guidance into
> `.agent-guidance/` and read it.

## Verify it works

In a repository with the loader installed, start a session in Claude Code, Codex or Gemini CLI.

- A status line such as `parity-agent-guidance: loaded <sha> (cached)` appears at session start.
- Anything other than `loaded ...` means a problem. The
  [status line table](docs/loader.md#status-line) lists each message.

To check end to end that the agent received the guidance, ask it:

> Quote the guidance verification marker.

- It should answer with the `Verification marker:` line from `GUIDANCE.md`.
- Change the marker here when you need to confirm that an update arrived.
  - With an existing cache, the new marker appears in the second session after the change is pushed.
- To check the loader alone, run `.agent-guidance-loader/load.sh claude` from the repository root.

## Harnesses

Hook commands, output shapes and all caveats are in [docs/harnesses.md](docs/harnesses.md).

| Harness | Config file | Main limitation |
|---|---|---|
| Claude Code | `.claude/settings.json` | None beyond trust on first use |
| Codex | `.codex/hooks.json` | Hooks need trust in `/hooks`; no SessionStart in Codex Desktop |
| Gemini CLI | `.gemini/settings.json` | None beyond trust on first use |
| Cursor | `.cursor/hooks.json` | `additional_context` may not reach the model (open bugs) |
| Copilot CLI and cloud agent | `.github/hooks/agent-guidance.json` | Context not confirmed for the cloud agent |

Every harness runs project hooks only after the workspace or folder is trusted on first use.

## Trust model

`GUIDANCE.md` is injected into every session as instructions to the agent, and agents can run
commands, so push access here is high-trust: a change steers the agents of everyone who loads it.

- The loader fetches only text from this repository and never executes a fetched file.
- Changes to the loader itself reach a repository only when a maintainer updates the installer,
  runs it there and commits the result.
- `main` requires a PR with one approving review and passing CI, also for admins.

## Writing guidance

`GUIDANCE.md` has two sections:

- "Root instructions" applies to every repository.
- "Index" lists the topic files, each as a path from the downstream repository root with one line
  that says when to read it:
  - `.agent-guidance/guidance/writing.md`: read when writing PR text, commits, issues, review
    comments, docs, code comments or replies to the user.

- Keep the injected context short and put details in topic files.
  - The tests fail when the context for `GUIDANCE.md` exceeds 8,000 characters.
  - Claude Code caps it at 10,000 characters.
  - Codex moves anything longer than about 2,500 tokens to a file.
- Do not use XML-style tags: Gemini CLI escapes `<` and `>` in injected context.

## Development

The installer, its templates and the tests live in the `tools/` crate. The templates in
`tools/templates/` mirror the downstream paths and are embedded into the installer binary.

- Run `cargo test --manifest-path tools/Cargo.toml` and
  `shellcheck tools/templates/.agent-guidance-loader/load.sh`.
- The loader tests use a local bare repository as the remote (`AGENT_GUIDANCE_URL=file://...`).
- They run every case under `/bin/bash` and under the `bash` on `PATH`.
- CI runs `cargo test` on Ubuntu and macOS, and `shellcheck` on Ubuntu only.
- This repository installs its own hook, so agents working on it receive the published guidance
  like any downstream repository.
- The test `repository_root_matches_the_templates` fails when `load.sh` or a hook config at the
  repository root differs from its template in `tools/templates/`.
- After changing `load.sh` or a hook config template, run the installer on this repository again:
  `cargo run --quiet --manifest-path tools/Cargo.toml -- .`
  - It replaces the loader entry in each root hook config with the template's entry.
