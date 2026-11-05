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
- It puts the fallback line at the top of `AGENTS.md`, followed by a blank line and the existing
  text, unless the file already mentions `agent-guidance`.
  - The line is quoted in the [README](../README.md#installing-in-a-repository).
- It makes `CLAUDE.md` import `AGENTS.md` with the line `@AGENTS.md`.
  - A missing or empty `CLAUDE.md` is written with that line alone.
  - A `CLAUDE.md` that contains `@AGENTS.md` or `@./AGENTS.md`, is a symbolic link to
    `AGENTS.md` or `./AGENTS.md`, or is a hard link to `AGENTS.md`, is left alone, also when
    `AGENTS.md` is missing.
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
  - It writes through no other symbolic link. When `AGENTS.md` or `CLAUDE.md` is another link it
    would change, it writes no file and exits with status 1.

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
- It warns when git ignores one of the files it wrote. Add such a file with `git add -f`.
  - A local `.codex/` exclude can cause this.
  - A global gitignore that ignores `.claude/` or `.codex/` triggers the same warning.

## Publishing

- The crate is marked `publish = false`.
- Before publishing it to crates.io, add a license and make the packaged tests independent of files
  outside `tools/`.
