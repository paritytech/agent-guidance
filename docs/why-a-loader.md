# Why a loader

The guidance has to reach every repository and every contributor, in several harnesses, change
without a PR in each repository, and never run fetched code. Copying it into each repository or
shipping it as a plugin each misses one of these.

| | Loader | Copy in each `AGENTS.md` and `CLAUDE.md` | Plugin |
|---|---|---|---|
| Who gets it | Everyone who opens the repository | Everyone who opens the repository | People who install it, with exceptions in Claude Code |
| Harnesses | Claude Code, Codex, Gemini CLI, Cursor, Copilot | Every harness that reads those files | A separate setup in each harness |
| An update needs | A merge here | A PR in every repository | Each person to update, unless the marketplace sets auto-update |
| Runs code from the source | No, it fetches text only | No | Yes, through plugin hooks and MCP servers |

## Copying into each repository

- Every guidance change becomes a PR in every repository, reviewed by its maintainers, and their
  review time is what the guidance tries to save.
- A repository that skips a PR keeps old rules with no sign that they are old.

## A plugin

- A plugin can ship hooks and MCP servers. With auto-update on, a push to the marketplace can run
  new code on every machine that has the plugin, with no review in the downstream repository. The
  loader fetches only text, and a new loader reaches a repository only through a commit reviewed
  there.
- Claude Code can register a marketplace and enable its plugins from a repository's
  `.claude/settings.json` once the folder is trusted.
  - A plugin stored in that marketplace repository then loads without a per-person install.
  - The entry can set `autoUpdate`, so updates need no action from each person.
  - This covers Claude Code only and needs a commit in each repository, like the loader.
  - Cloud sessions do not load plugins or marketplaces that a repository declares.
- An organization can turn plugins on for its claude.ai members. That reaches only Claude Code
  users signed in to that organization, not external contributors.
- Laptops are not centrally managed and external contributors cannot be asked to install anything.
  Wherever a harness needs a per-person install, many sessions would run without the plugin.
- The shared Agent Plugins format, read by Codex, Cursor and Copilot, carries only skills and MCP
  servers. Hooks are specific to each harness.
- A Claude Code plugin does not load a `CLAUDE.md` as context. Always-on instructions need a
  SessionStart hook like the loader's.
- A copy in each person's own `~/.claude/CLAUDE.md` has the same reach problem as a plugin and no
  update path.

## What the loader costs

- The fetched text steers agents that can run commands, so push access here is high-trust. See
  [Trust model](../README.md#trust-model).
- Each harness asks the user to trust the workspace or the hook once.
- Codex Desktop and Cursor cloud agents do not run the hook.
- In Cursor and the Copilot cloud agent the context may not reach the model.
- The fallback line covers these cases (see [Harnesses](harnesses.md#caveats)).
- An update reaches the agent in the session after the one that fetched it.
- The first session needs network access and, while the repository is private, GitHub access.
- Each repository commits the loader once and re-runs the installer when the loader changes.
