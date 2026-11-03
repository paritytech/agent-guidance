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
  - In an existing file, the hook entry is appended to the event's list unless an entry already
    runs the loader.
  - All other content in an existing file is kept.
  - A file that is not valid JSON is left unchanged.
    - The installer then prints the entry to add by hand and exits with status 1.
- It always writes `.github/hooks/agent-guidance.json` from the template.
- It adds `/.agent-guidance/` and `/.agent-guidance.tmp.*/` to `.gitignore` unless equivalent lines
  exist.
- It adds the fallback line to `AGENTS.md` and `CLAUDE.md` unless they already mention
  `agent-guidance`.
  - `CLAUDE.md` is left alone when it imports `@AGENTS.md`.
  - The line is quoted in the [README](../README.md#installing-in-a-repository).

## Upgrading the loader

The installer binary embeds the loader it writes, so an installer built before a loader version
bump writes the old loader again. Update the installer first, then run it in the repository:

```sh
cargo install --force --git https://github.com/paritytech/agent-guidance agent-guidance-install
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
  - It prints both versions and the `cargo install --force` command to stderr, and exits with
    status 1.
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
