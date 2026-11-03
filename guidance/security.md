# Security

How to handle a change that fixes a security hole (for example a denial-of-service or pricing gap)
and findings that must stay private. A fix is public before deployments upgrade, so public text
must not tell an attacker what to exploit.

## Public text stays neutral

- This covers every public artifact: the PR title and description, commit subjects and messages,
  the changelog or release-note file, issues, review comments, thread replies and review bodies.
- State the new behavior. Never say that the old path "was free", "was uncharged", "could be
  abused" or is "now charged".
- Have an independent reviewer check all public text before it goes out, with an explicit "no leaks
  in public text" check.
- Weigh severity. A low-impact pre-existing gap may be public, framed as parity with the reference
  behavior.

## Private channels

- Internally found security bugs get no public tracking issue. Fix them in a neutral PR that is
  reviewed internally.
- Put exploit-shaped details and requests in a private message to the author. Draft it ready to
  send and hand it to the user; copy it to the clipboard when the harness can.
- A security-relevant pre-existing bug found during review goes to the user privately, not into a
  public issue or the review.

## Cleaning up leaks

- Audit earlier public comments and review bodies for leaks, including the user's own.
- Edit a leaking comment in place (`PATCH` the comment, `PUT` the review). The API cannot delete
  old revisions, so tell the user to delete them in the web UI.
- Check other people's comments for quotes of leaked text.

## Reviewing security-relevant code

- Apply the griefing and trust-boundary checks in "What to raise" in
  `.agent-guidance/guidance/code-review.md`.
- Audit attachments before uploading, as in "Parking and handoff" in
  `.agent-guidance/guidance/issues.md`.
