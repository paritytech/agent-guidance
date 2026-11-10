# Harnesses

The installer writes one hook config per harness. Each runs the loader at session start with the
harness name as its argument and reads the output shape that harness documents.

## Hook configs

| Harness | Config file | Event | Command | Timeout |
|---|---|---|---|---|
| Claude Code | `.claude/settings.json` | `SessionStart`, every source | `"$CLAUDE_PROJECT_DIR"/.agent-guidance-loader/load.sh claude` | 15 s |
| Codex | `.codex/hooks.json` | `SessionStart`, every source | `"$(git rev-parse --show-toplevel)/.agent-guidance-loader/load.sh" codex` | 15 s |
| Gemini CLI | `.gemini/settings.json` | `SessionStart` | `"$GEMINI_PROJECT_DIR/.agent-guidance-loader/load.sh" gemini` | 15000 ms |
| Cursor | `.cursor/hooks.json` | `sessionStart` | `.agent-guidance-loader/load.sh cursor` | 15 s |
| Copilot CLI and cloud agent | `.github/hooks/agent-guidance.json` | `sessionStart` | `./.agent-guidance-loader/load.sh copilot` | 15 s |

- The Claude Code and Codex hooks have no matcher.
- They run for every session source: startup, resume, clear, compact and fork.
- The installer sets up a user-level hook only for Claude Code, in `~/.claude/settings.json`. See
  [User install](installer.md#user-install).

## Output shapes

| Argument | Printed JSON |
|---|---|
| `claude`, `codex`, `gemini`, none | `{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":...},"systemMessage":...}` |
| `cursor` | `{"additional_context":...}` |
| `copilot` | `{"additionalContext":...}` |

- Each harness gets only its own documented keys.
- Codex rejects unknown keys in SessionStart output, so one object with every key would break it.

## What reaches the user

The context goes to the model only. The status line (see [Status line](loader.md#status-line))
goes in the field that the harness shows to the user, where one exists.

| Harness | Field shown to the user | Source |
|---|---|---|
| Claude Code | `systemMessage` | [Hooks reference](https://code.claude.com/docs/en/hooks) |
| Codex | `systemMessage` | [Hooks](https://learn.chatgpt.com/docs/hooks) |
| Gemini CLI | `systemMessage` | [Hooks reference](https://github.com/google-gemini/gemini-cli/blob/main/docs/hooks/reference.md) |
| Cursor | None | [Hooks](https://cursor.com/docs/agent/hooks) |
| Copilot | None | [Hooks configuration](https://docs.github.com/en/copilot/reference/hooks-configuration) |

- Codex accepts `systemMessage` as a top-level key of SessionStart output.
- Claude Code and Codex style `systemMessage` as a warning, so a successful load also shows as a
  warning-styled line there.
- Cursor and Copilot document no such field, so their output does not carry the status line.
- Stderr is not shown on exit 0, so the loader's diagnostics stay hidden in every harness.

## Caveats

### Codex

- Needs Codex 0.114.0 or later.
- SessionStart hooks do not run in Codex Desktop (openai/codex#35863).
- Each hook must be trusted in `/hooks`.
  - Trust is recorded against the hook's hash, so a changed hook must be approved again.
- `codex exec` has no review screen. The hook runs there only if it was already trusted in the TUI
  or `--dangerously-bypass-hook-trust` is passed.
- `--dangerously-bypass-hook-trust` skips only the per-hook hash approval, not project trust.
- With Codex 0.162.0-alpha.2, `codex exec` did not load `.codex/hooks.json` at all until the
  repository was trusted. To trust it, do one of these:
  - Accept the trust prompt once in the interactive TUI.
  - Add `[projects."<repository path>"]` with `trust_level = "trusted"` to
    `~/.codex/config.toml`.
- A trust entry for a parent directory, such as the home directory, did not cover the repository.
- The hook command uses `git rev-parse --show-toplevel`, so it fails when Codex runs outside a git
  repository.

### Gemini CLI

- Hooks are enabled by default in current versions (`hooksConfig.enabled`).
- Hooks run only in trusted folders.
- The user is warned when a project hook changes.
- Gemini CLI runs hooks with a sanitized environment.

### Cursor

- The docs describe sessionStart as fire-and-forget: the agent does not wait for it.
- The docs say sessionStart does not run in Cursor cloud agents.
- Acknowledged, unfixed bugs can stop sessionStart `additional_context` from reaching the model
  (forum threads 157141, 158452, 174143).
- The fallback line in `AGENTS.md` covers these cases.

### Copilot

- The docs say `additionalContext` is consumed for `sessionStart`.
- This is not confirmed for the cloud agent.
- Copilot CLI 1.0.8 dropped it (github/copilot-cli#2142).

### All harnesses

- Every harness requires workspace or folder trust on first use before it runs project hooks.
- Copilot CLI and Cursor also read hooks from `.claude/settings.json`.
  - They ignore the Claude-shaped output, so no double injection is expected. This is not
    verified.
- With an `insteadOf` rule that rewrites GitHub HTTPS URLs to SSH, an SSH agent that asks for
  approval on each use (1Password, hardware keys) may prompt during the background fetch.
