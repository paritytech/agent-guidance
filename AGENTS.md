Company agent guidance lives in `.agent-guidance/GUIDANCE.md`. If that
directory is missing, clone https://github.com/paritytech/agent-guidance into
`.agent-guidance/` and read it.

The rest of this file applies only when the agent-guidance repository itself is the project root.
Ignore it when reading `.agent-guidance/` from another repository.

In this repository:

- Agents receive the published guidance through the SessionStart hook, not the working copy of
  `GUIDANCE.md`.
- Edits to `GUIDANCE.md` reach agents once they are on the remote's default branch and a later
  session has fetched them.

# Working on this repository

## Guidance content

- `GUIDANCE.md` is the only file injected into sessions, so keep it small.
  - The test suite fails above 8,000 characters of injected context.
  - Claude Code caps hook context at 10,000 characters.
  - Codex moves context longer than about 2,500 tokens to a file.
  - Keep the root instructions brief.
  - Put everything else in `guidance/<topic>.md` and list it in the Index.
- Index entries use paths that open from a downstream repository root
  (`.agent-guidance/guidance/<topic>.md`), each with a one-line "read when" description, because
  agents read them from the downstream checkout.
  - The `guidance/writing.md` entry shows the shape.
- Do not use XML-style tags in guidance. Gemini CLI escapes `<` and `>` in injected context.
- Keep the verification marker line in `GUIDANCE.md`. The README's
  [verify step](README.md#verify-it-works) depends on it.
- Every change to `GUIDANCE.md` or `guidance/` reaches every session in every adopting repository
  on the next update, without downstream review.
  - Review such changes the same way as code. See the [trust model](README.md#trust-model).

See [Writing guidance](README.md#writing-guidance) for the conventions.

## Loader

`tools/templates/.agent-guidance-loader/load.sh` must keep these invariants, because it runs at the
start of every session in every adopting repository:

- It never executes fetched content.
- stdout is exactly one JSON object in the calling harness's documented shape.
- It always exits 0.
- It is compatible with bash 3.2 (macOS `/bin/bash`).
- It never blocks on the network when a fresh cache exists.
  - With a stale cache (no successful fetch recorded in the last 7 days) it fetches before serving,
    bounded by the 4 s watchdog.

When changing the loader:

- Bump `LOADER_VERSION` in `load.sh` and the `loader-version` file together. A test enforces this.
- Downstream repositories pick up a new loader only when the installer is run there again.
- See [How it works](docs/loader.md#how-it-works) and
  [Loader versions](docs/loader.md#loader-versions).

## Hook configs and templates

- Check each harness's current hook documentation before changing a hook config or an output
  shape. See [Harnesses](docs/harnesses.md).
- Do not copy a format from one harness to another. Codex rejects unknown keys in SessionStart
  output.
- The templates in `tools/templates/` include dot-directories.
  - The negations in `.gitignore` keep them tracked despite global excludes.
  - After adding a template, check that it is packaged with
    `cargo package --list --allow-dirty --manifest-path tools/Cargo.toml`.

## Checks

Run these before committing, all with `--manifest-path tools/Cargo.toml`:

- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`

Also run `shellcheck tools/templates/.agent-guidance-loader/load.sh`. See
[Development](README.md#development).
