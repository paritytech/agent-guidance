# Loader

The loader is `.agent-guidance-loader/load.sh` in the downstream repository. Each hook config runs
it with the harness name as its argument: `claude`, `codex`, `gemini`, `cursor` or `copilot`. A
second argument `user` selects [user mode](#user-mode).

## How it works

### Cache

- The loader finds the repository root from its own location and uses `<root>/.agent-guidance/`
  as the cache.
- The installer adds `/.agent-guidance/` and `/.agent-guidance.tmp.*/` to `.gitignore`.
- The cache counts as present only when `.agent-guidance/.git` is a directory.
- Every git command is confined to the cache (`GIT_CEILING_DIRECTORIES`), so git never falls
  through to the downstream repository.
- The loader owns `.agent-guidance/`: local edits there are discarded when an update is checked
  out.

### Without a cache

- The loader runs `git clone --depth 1` into a temporary directory.
- It moves the result into place when the clone is complete.
- A `.agent-guidance/` directory without `.git` is replaced only after the clone succeeded.

### With a cache

- Session start does not wait for the network unless the cache is stale (see
  [Stale cache](#stale-cache)).
- If the previous fetch left a different commit in `FETCH_HEAD`, the loader checks it out with
  `git reset --hard`, a local operation.
  - If the checkout fails, for example because another session holds the index lock, the current
    files are served.
- The loader then serves the files and starts `git fetch --depth 1` in the background before it
  exits.
- The background fetch only updates `.git`, so an update reaches the agent one session after it
  was fetched.

### Stale cache

- A successful fetch or clone touches `.agent-guidance/.git/agent-guidance-last-fetch`.
- When that file is missing or older than 7 days, the loader fetches before serving instead.
  - The file can go stale, for example, when a harness kills background jobs.
- If that fetch fails, the cached copy is served.

### Network limits

Git never prompts: the loader sets `GIT_TERMINAL_PROMPT=0` and runs SSH in batch mode.

| Event | Limit |
|---|---|
| A stalled HTTP transfer is aborted | after 3 s |
| A watchdog stops each blocking git command | after 4 s |
| The background fetch is stopped | after 30 s |

- The watchdog stops a hung git command, so the loader finishes in about 5 s even when the
  network hangs.
- The background fetch runs in its own process group with all output sent to `/dev/null`, so the
  harness neither waits for it nor stops it when the loader exits.

### Output

- With a cache, the context is one header line with the absolute cache path, followed by
  `GUIDANCE.md`.
- Without a cache (the first session offline), the context is one line that points the agent to
  the fallback line in `AGENTS.md`, which `CLAUDE.md` imports.
- The loader prints one JSON object on stdout in the shape the harness reads. See
  [output shapes](harnesses.md#output-shapes).
- For Claude Code, Codex and Gemini CLI the object also carries a status line for the user. See
  [Status line](#status-line).
- Diagnostics go to stderr.
- The exit status is always 0, so the loader never blocks a session.

### Status line

In Claude Code, Codex and Gemini CLI, every session shows the user one line from the
`systemMessage` field that says whether the guidance was loaded. Cursor and Copilot have no such
field (see [What reaches the user](harnesses.md#what-reaches-the-user)).

| Path | Status line |
|---|---|
| Fresh cache | `parity-agent-guidance: loaded <sha> (cached)` |
| Stale cache, fetch succeeded | `parity-agent-guidance: loaded <sha> (fetched)` |
| No cache, clone succeeded | `parity-agent-guidance: loaded <sha> (cloned)` |
| Stale cache, fetch failed or timed out | `parity-agent-guidance: fetch failed, loaded cached <sha>` |
| Checkout of `FETCH_HEAD` failed | `parity-agent-guidance: could not apply fetch, loaded cached <sha>` |
| No cache, clone failed or timed out | `parity-agent-guidance: NOT loaded, could not clone <url>` |
| No repository root | `parity-agent-guidance: NOT loaded, cannot locate the repository root from <path>` |
| git missing | `parity-agent-guidance: NOT loaded, git not found` |
| Any other cache that cannot be served | `parity-agent-guidance: NOT loaded, no usable copy in .agent-guidance/` |
| The same, after a failed fetch | `parity-agent-guidance: NOT loaded, fetch failed, no usable copy in .agent-guidance/` |

- `<sha>` is the abbreviated commit of the cache, from `git rev-parse --short HEAD`. It is left out
  when git cannot read it.
- `<url>` is shown without a user name or password, so a token in `AGENT_GUIDANCE_URL` does not
  reach the screen. The stderr diagnostic keeps the full URL.
- With a fresh cache, a failing background fetch does not show: it ends after the line is printed.
- When the loader is out of date (see [Loader versions](#loader-versions)), the line ends with
  `; loader v<N> out of date (v<M> available): run cargo install --git
  https://github.com/paritytech/agent-guidance, then agent-guidance-install, and commit`.
  - `<N>` is the committed loader's version and `<M>` the fetched `loader-version`.
- The line is escaped the same way as the context.
- In [user mode](#user-mode) every line starts with `parity-agent-guidance (user): ` instead.
  - The "no usable copy" lines name the absolute cache path, such as
    `/home/me/.claude/agent-guidance/.agent-guidance/`, instead of `.agent-guidance/`.
  - "No repository root" reads `cannot locate the loader directory from <path>`.
  - In a repository that has its own loader hook, the only output is the line
    `parity-agent-guidance (user): skipped, this repository loads its own`.

## User mode

`agent-guidance-install --user` installs the loader for one person's Claude Code sessions in every
repository (see [User install](installer.md#user-install)). The user hook runs
`"$HOME"/.claude/agent-guidance/load.sh claude user`.

- The root is the loader's own directory, so the cache is
  `~/.claude/agent-guidance/.agent-guidance/`. Cloning, fetching, the last-fetch marker and the
  network limits work as in a repository.
- Before any git command, the loader reads `.claude/settings.json` in the project directory
  (`$CLAUDE_PROJECT_DIR`, or the working directory when it is unset).
  - When that file contains `.agent-guidance-loader/load.sh claude`, the start of the repository
    hook command, the repository's own hook delivers the guidance. The loader prints only
    `{"systemMessage":"parity-agent-guidance (user): skipped, this repository loads its own"}`
    and exits.
  - Other mentions of the loader path, such as a permission rule
    `Bash(.agent-guidance-loader/load.sh:*)`, do not count.
  - The check uses `grep -F` and executes nothing from the file.
- The context header reads ``Company agent guidance follows. Paths below that start with
  `.agent-guidance/` refer to `<cache>/`.`` followed by `GUIDANCE.md` unchanged.
- Without a cache, the context is the single line `Company agent guidance could not be fetched for
  this session.` It does not point to `AGENTS.md`, since the repository may not have the fallback
  line.
- The out-of-date notice ends with `run cargo install --git
  https://github.com/paritytech/agent-guidance, then agent-guidance-install --user`. There is
  nothing to commit.
- Any other second argument runs the loader in repository mode and writes a note to stderr.

## Requirements

- bash 3.2 or later, so macOS `/bin/bash` works.
- git.
- Standard utilities: `awk`, `tr`, `find`, `touch`, `sleep`, `cat`, `mv`, `rm`, `dirname`, and
  `grep` in user mode.
- `python3 -I` for JSON escaping when available, and awk otherwise. Both produce the same bytes.

## Loader versions

- `load.sh` sets `LOADER_VERSION`.
- The `loader-version` file in this repository holds the current version.
- Bump both together whenever `tools/templates/.agent-guidance-loader/load.sh` or a hook config
  template in `tools/templates/` changes.
  - A hook config change reaches a repository only when the installer runs there again, and the
    bump is what tells its maintainers to do that.
- When the fetched `loader-version` is greater than the version of the committed loader, the
  loader tells the user how to upgrade.
  - In Claude Code, Codex and Gemini CLI the notice is at the end of the
    [status line](#status-line), and the context does not mention it.
  - Cursor and Copilot have no status line. For them the loader appends a line to the context
    that asks the agent to tell the user the same steps.
- The installer embeds the loader it writes, so the upgrade has two steps. See
  [Upgrading the loader](installer.md#upgrading-the-loader).
  1. Update the installer with
     `cargo install --git https://github.com/paritytech/agent-guidance`.
  2. Run `agent-guidance-install` in the repository and commit the result.
- A version bump means every downstream repository has to upgrade and commit the new loader.
  - Until then its sessions show that notice.
