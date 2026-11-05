# Installer

`agent-guidance-install` writes the loader, the hook configs and the fallback line into a
repository, and leaves committing to the maintainer.

## Repository root

- The installer takes an optional path to the repository root.
- Without it, it uses the top level of the git repository that contains the current directory.
- It fails outside a git repository.

## Files it writes

- It overwrites `.agent-guidance-loader/load.sh` with the loader it embeds, with mode 755.
  - When the existing loader's `LOADER_VERSION` is greater than its own, it writes no file at
    all. See [Upgrading the loader](#upgrading-the-loader).
- It creates or updates `.claude/settings.json`, `.codex/hooks.json`, `.gemini/settings.json` and
  `.cursor/hooks.json`.
  - A missing file is created from the template.
  - In an existing file without a loader entry, the template's entry is appended to the event's
    list.
  - An existing loader entry that differs from the template is replaced in place, and further
    loader entries are removed.
    - Edits made to the loader entry, such as a longer timeout, are lost, the same as edits to
      `load.sh`.
    - A command that passes the loader path to another program, such as
      `notify-me .agent-guidance-loader/load.sh`, looks the same as a loader call and is
      replaced too.
  - All other content in an existing file is kept.
  - Some files are left unchanged. The installer then prints the template to merge by hand and
    exits with status 1.
    - A file that is not valid JSON or does not have the expected shape.
    - A file with an entry that runs more than the loader. Replacing that entry would delete the
      rest.
      - A Claude Code, Codex or Gemini CLI `hooks` group with a second hook of any type, such as
        a second command or an HTTP hook.
      - A loader command with shell syntax that runs another command: `;`, `&`, `|`, a line
        break, a backtick, `$(`, `<(` or `>(`, for example `load.sh claude && notify-me`. The
        `$(git rev-parse --show-toplevel)` of the Codex template is allowed.
- It always writes `.github/hooks/agent-guidance.json` from the template.
- It adds `/.agent-guidance/` and `/.agent-guidance.tmp.*/` to `.gitignore` unless equivalent lines
  exist.
- When git ignores a hook config, for example through a global excludes file with `.claude/` or
  `.codex/`, it adds lines that make git track the config to the clone's local exclude file
  (`.git/info/exclude`, as `git rev-parse --git-path info/exclude` finds it).
  - The shared `.gitignore` is not changed, since it must not encode one person's global
    excludes. The exclude file is not committed and is not listed in the `git add` command.
  - Git cannot re-include a file in an excluded directory. For each ignored directory above the
    file, from the top down, it adds two lines that re-include the directory and ignore
    everything else in it, such as `!/.claude/` and `/.claude/*`. The file line, such as
    `!/.claude/settings.json`, comes last. Other files, such as `.codex/worktrees/`, stay
    ignored.
  - When only the file is ignored, it adds just the file line.
  - The lines come after existing ones, so they win over an earlier `/.codex/` line in the
    exclude file.
  - A group of lines is added again unless it already appears as a whole, in the same order.
  - The exclude file takes precedence over `core.excludesFile` but not over a tracked
    `.gitignore`. When the repository's own `.gitignore` ignores a hook config, that rule wins
    and the warning below shows.
  - The lines are anchored at the top level. When ROOT is a directory below it, the installer
    leaves the exclude file alone, and the warning below shows.
- It puts the fallback line at the top of `AGENTS.md`, followed by a blank line and the existing
  text, unless the file already mentions `agent-guidance`.
  - The line is quoted in the [README](../README.md#installing-in-a-repository).
- It makes `CLAUDE.md` import `AGENTS.md` with the line `@AGENTS.md`.
  - A missing or empty `CLAUDE.md` is written with that line alone.
  - A `CLAUDE.md` that contains `@AGENTS.md` or `@./AGENTS.md`, is a symbolic link to
    `AGENTS.md` or `./AGENTS.md`, or is a hard link to `AGENTS.md`, is left alone, also when
    `AGENTS.md` is missing. A hard-linked `CLAUDE.md` changes with `AGENTS.md`, so both are
    listed as changed.
  - When `AGENTS.md` is missing or empty, the content of any other `CLAUDE.md` moves to
    `AGENTS.md` below the fallback line, and `CLAUDE.md` is replaced with the import.
  - When `CLAUDE.md` does not import `AGENTS.md` but either file mentions `agent-guidance`, both
    files are left alone. Earlier installers added the fallback line to both files.
  - When both files hold other instructions and `CLAUDE.md` does not import `AGENTS.md`, the
    installer checks this before it writes anything, writes no file, and exits with status 1.
    Merge `CLAUDE.md` into `AGENTS.md`, replace `CLAUDE.md` with the line `@AGENTS.md`, and run
    the installer again.
  - An `AGENTS.md` that is a symbolic link to `CLAUDE.md` makes both names one file. The
    fallback line is written to `CLAUDE.md`, which is created when it is missing, and listed as
    the changed file.

## Symbolic links

The installer plans every write before it writes anything, and never writes through a symbolic
link.

- A file counts as written when its planned contents differ from what is on disk. Setting the
  loader's executable bit counts as a write of `load.sh`.
- When a file it would write, or a directory above it below the repository root, is a symbolic
  link, it writes no file and exits with status 1. The error lists every such link, for example
  `.claude`, `.gitignore` or `.agent-guidance-loader/load.sh`.
- A link to a file that is already up to date is not written, so a second run works.
- An `AGENTS.md` that links to `CLAUDE.md`, or a `CLAUDE.md` that links to `AGENTS.md`, is
  handled as described above: the installer writes the real file.
- The local exclude file below `.git/` and the repository root itself are not checked.

## Upgrading the loader

The installer binary embeds the loader it writes, so an installer built before a loader version
bump writes the old loader again. Update the installer first, then run it in the repository:

```sh
cargo install --git https://github.com/paritytech/agent-guidance
cd path/to/repository
agent-guidance-install
```

Commit the changed files it lists. The first line of its output reports the loader version:

| Existing loader | Output | Loader file |
|---|---|---|
| Older version | `loader v<N> -> v<M>` | Replaced |
| Same version | `loader v<M> (current)` | Replaced if its content differs |
| Missing, or no readable `LOADER_VERSION` line | `loader ? -> v<M>` | Written |

- `<M>` is the version embedded in the installer and `<N>` the version of the existing loader.
- The installer reads the version from the `LOADER_VERSION=<digits>` line and never runs the file.
- When the existing loader is newer, the installer was built before a version bump, and its hook
  configs may be as old as its loader.
  - It checks this before it writes anything, and then writes no file.
  - It prints both versions and the `cargo install` command to stderr, and exits with status 1.
  - A version too large for an integer type counts as newer.

## After a run

- It never commits.
- It reports the loader version, then lists the changed files and the `git add` command to run.
- A second run changes nothing.
- It warns when git still ignores one of the files it wrote. Add such a file with `git add -f`.
  - A tracked `.gitignore` that ignores the file can cause this, since it takes precedence over
    the lines added to the exclude file.
- It reports when lines it added to the local exclude file made git track a hook config. That
  file is not committed.

## Publishing

- The crate is marked `publish = false`.
- Before publishing it to crates.io, add a license and make the packaged tests independent of files
  outside `tools/`.
