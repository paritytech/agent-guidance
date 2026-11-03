# Issues

How to file, update and close issues so that the tracker holds the current state of the work.
Write the text as in `.agent-guidance/guidance/writing.md`.

- Ask the user before filing or editing an issue, and show the exact title and body.

## The tracker is the source of truth

- Write findings back to the issue while working. Local notes are invisible to the next session
  and to other people.
- When issue text is wrong, fix it in place. Do not append a dated correction.
- Link related issues both ways.
- A status line is one or two sentences: what is known and the next action.

## Filing

- One issue per bug, including bugs found in passing. Verify the bug against the source first.
- The title states the defect, not the area ("`parse_header` panics on an empty name", not "Header
  parsing issue").
- The body has the evidence: permalinks to the code pinned to a commit, the exact error line, the
  steps or input that trigger it.
- Say how urgent it is and who it affects. Name an affected setup only when verified.
- Note where it came from: "Found while reviewing #123".
- A bug in scope of the current task is fixed now, while the context is loaded, instead of filed
  as a follow-up.
- A security-relevant bug gets no public issue. See `.agent-guidance/guidance/security.md`.

## Parking and handoff

- A handoff must be reachable from GitHub. A local branch, patch or scratch file is not one. Put
  small diffs and probes inline in a collapsed details block.
- A parked issue lets a fresh session continue without asking. It records:
  - what was verified and how, with commands
  - permalinks pinned to a commit sha
  - the reference behavior, if any
  - the proposed fix and the tests that would pin it
  - open questions
- A no-change conclusion is a valid result when the evidence and the criteria for reopening are
  recorded.
- Audit attachments and pasted logs before uploading: no credentials, private paths or proprietary
  inputs.
- After posting, read the text back and open each link.

## Closing

- Close an issue when its fix is on the default branch, not when the PR opens.
- Use `Closes #N` in the PR description so the merge closes it.
- When closing without a fix, say why in one or two sentences.
