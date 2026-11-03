# Code review

How to review a pull request and publish the review, and how to use the same procedure as a
self-review before opening your own PR. Comments go out under the user's account:

- They read like the user.
- They do not repeat points already addressed.
- They land as one review, so the author gets the whole round at once.

Placeholders in braces: `{N}` is the PR number, `{owner}` and `{repo}` come from
`gh repo view --json owner,name` (never hardcode them), `{head}` is the PR head sha, `{scratch}` is
a scratch directory for this PR inside the repository under an ignored directory.

## Triggers

- "Review this PR {N}" starts a first review.
- "Continue review" starts a follow-up round.
- Without a PR number, ask the user. Never guess it from notes or the current branch.

## Project rules

Before gathering, look for project-specific review rules and apply them on top of this file. Within
its repository, a project rule wins over this file.

- Read the review sections of `AGENTS.md` and `CLAUDE.md` at the root and on every touched path.
- Look for review addenda in the harness's project memory or the ignored notes directory in the
  repository.
- Load the per-PR notes for {N} if they exist. Keep them in the harness's project memory or in an
  ignored directory in the repository.

## Every round

In a PR review the orchestrator launches the 3b area agents, the bug pass and the draft reviewers.
One drafting agent gets their reports, reads the diff and verifies each finding itself. After
sign-off the orchestrator runs Mechanics. The drafting agent may write the payload files into
`{scratch}`.

1. Gather (read-only). The PR description, `gh pr diff {N}`, the commits with head sha and
   committer date (`pulls/{N}/commits`), all review threads through GraphQL (REST has no
   `isResolved`), conversation comments (`issues/{N}/comments`), earlier reviews, and the per-PR
   notes.
   - For a follow-up PR, read the predecessor PR's threads before arguing against a design settled
     there. Check interactions with recently merged PRs the user names.
   - Compare thread dates with the head commit date. A thread newer than the head is unaddressed,
     even if a commit message says "address review comments".
   - Use explicit shas everywhere, never `FETCH_HEAD`: parallel agents overwrite it.
2. Rules. Read every guidance file on each touched path plus the project rules. Build a pass/fail
   checklist covering every rule. The checklist is for the user only. Each concrete violation
   becomes an inline comment written as a plain engineering point that does not cite the guidance
   file.
3. Investigate. All three parts are mandatory on every round, small PRs included, and each is done
   before the draft is written.
   - 3a. The drafting agent reads the whole diff (or the whole delta on a follow-up) itself.
   - 3b. The orchestrator launches area agents in parallel, one per area of the diff (for example
     core logic, client or API side, tests, versioning). Each traces callers, interactions and
     edge cases in depth and reports file:line evidence.
   - 3c. The orchestrator runs the harness's built-in code review pass for bugs, never in a mode
     that posts comments or applies fixes. On a follow-up, it runs on the delta range (last
     reviewed sha to head).

   The drafting agent verifies every finding from 3b and 3c against the code before it enters the
   draft. It drops what it cannot verify and says so in the draft.

   Build or run locally only when a finding needs proof: fetch `pull/{N}/head` into a worktree
   under the same ignored directory as `{scratch}`, run targeted tests, and record the command and
   result.
4. CI. Mention a failing job only if it touches the diff (`gh run view {id} --log-failed`, then
   filter for errors). Before blaming a red job on the PR or calling it flaky, check whether the
   base branch fails the same way.
5. Draft. Put the exact text of everything in the draft: the event, the body, each new inline
   comment (path, line, text), each thread reply (thread URL, text), and each thread to resolve
   with a one-line reason. Write the thread table (see Follow-up) before drafting any finding. Add
   the rules checklist and any dropped findings for the user only. Where an answer depends on the
   user's stance, offer the variants first.

   The draft opens with a procedure checklist, one line per step 1 to 5, including 3a, 3b (areas
   launched) and 3c (review pass ran, finding count). Finish every line before sign-off, and never
   mark a line done that was not done.
6. Sign-off. Nothing reaches GitHub before the user approves the draft. Show the full draft and
   wait for an explicit yes.
7. Submit as one atomic review (see Mechanics), then resolve the approved threads.
8. Write or update the per-PR notes (see Per-PR notes).

## First review

- Do not repeat anything an existing thread raised or the author already fixed.
- Answer open threads where the author or another reviewer asked the maintainers to decide, as
  thread replies inside the same review. Answer another reviewer's thread only when the code
  already settles it.
- Triage every unresolved thread with the table from Follow-up step 3. Resolve other reviewers'
  threads the code settles. Reply in any thread that one of the new comments walks back or
  settles.
- Event: propose REQUEST_CHANGES when the approach must change or there is a correctness or
  compatibility bug (public API, wire format, storage layout, consensus). Otherwise propose
  COMMENT. The user confirms.
- Body: no verdict sentence, it reads as condescending. Start with the points not tied to a line:
  PR description, scope or splitting, CI that touches the diff, other people's questions settled
  by the code. Leave the body empty if there are none. No recap, no praise.
- Nits go inline, prefixed with `Nit: `, as non-blocking.

## Follow-up

1. Load the per-PR notes and the user's last review (`pulls/{N}/reviews`, body and `commit_id`).
   That `commit_id` is the last reviewed head.
2. Do a full pass on the delta since the last reviewed head, with the rigor of a first review. If
   the author rebased or merged the base branch, use `git range-diff`, or diff against the old head
   merged onto the new base, so base-branch changes are not reviewed as PR changes. New findings go
   inline.
3. Triage every unresolved thread from every reviewer against the new head:
   - Addressed and verified in the code: resolve. This covers the user's threads and other
     reviewers' threads the code clearly settles. Mention resolved foreign threads in one line of
     the body.
   - The author replied in the user's thread with a question or pushback: reply in that thread,
     attached to the pending review. A thread just replied in stays open so the author can answer.
   - The user's point not addressed, or only partly: no thread reply. List it in the body with a
     link to the thread and say what is still missing. The thread stays open.
   - Another reviewer's thread that is not clearly settled: leave it alone.
   - Conversation-tab comments since the last round: answer every question or decision request
     addressed to the user or the maintainers in the review body, the only place a review can
     answer them.

   The draft carries a table with one row per unresolved thread (URL, the author's reply, the
   disposition: resolve, reply and resolve, body link and keep open, leave alone) plus a line on the
   conversation tab.
4. Event: propose APPROVE when every one of the user's points is settled and the delta has nothing
   blocking. The body is "All my comments are properly addressed.", optionally followed by the line
   on resolved foreign threads and non-blocking leftover asks. No recap, no CI state. Otherwise
   propose COMMENT, or REQUEST_CHANGES under the first-review rule.
5. If the user already approved and a rebase only fixes nits, resolve the threads and post no new
   review.
6. Asks added after the user approved go in a COMMENT review, so the approval stays.

## What to raise

- Standing lenses: correctness first; compatibility of public API, wire formats and stored data;
  cargo or build features stay additive; duplicated logic; repeated reads or work on hot paths;
  tests that exercise the real execution path; the changelog or release-note file matches the
  diff.
- Version bumps follow strict SemVer on the crate's own public API, not the size of a dependency's
  bump. Phrase changelog and version asks as recommendations.
- When the PR claims to implement a spec, every missing part of that spec is in scope and blocking,
  even if the gap predates the PR. Never push it to a follow-up.
- The same holds for a related bug in the semantics the PR touches: ask for the fix and a test in
  this PR, not for an issue. "Pre-existing" is never a reason to drop a bug in a function the PR
  edits or a path it makes newly reachable.
- Pre-existing bugs outside the code the PR touches go in a separate issue, not the review. If one
  is security-relevant, open no public issue and tell the user privately.
- A follow-up PR is fine when the gap only errs on the safe side (for example it overcharges).
- For every new failure or revert path, work out who besides the caller can trigger it and what it
  costs them. Never drop a griefing finding as settled design unless an earlier round weighed
  griefing. Put it to the user with a concrete attacker, the cost and the impact compared with the
  base branch.
- Before flagging duplication, check whether it is deliberate (for example versioned or frozen
  types that each carry their own conversion).
- Do not flag a missing trust boundary where the other side is the same project's code.
- Check quantifiers in docs and comments ("every X", "the rest are Y") against the code. Generated
  docs often overclaim coverage.
- Do not ask for a test where the codebase has no harness for it.
- When asking for a repository rule that is newer than the PR, say so.
- Do not publicly raise costs of a design the user proposed unless the user asks.
- Never raise em dashes in the author's code or text as a review point. Keep that to the internal
  checklist.
- When the PR fixes a security hole, all public text follows
  `.agent-guidance/guidance/security.md`. Give the draft reviewer an explicit "no leaks in public
  text" check.

## Voice

Follow `.agent-guidance/guidance/writing.md`. For review text in particular:

- Plain maintainer prose under the user's name, engineer to engineer. No praise, no verdicts, no
  recap.
- Each inline comment is two or three sentences: the failure, one clause of cause with concrete
  numbers, the fix. A step-by-step proof goes to the user in the draft, not into the comment.
- Before sign-off, cut each sentence to fact and fix. Drop softeners ("I'd like these in", "please
  pick these up"). Remove anything that reads as lecturing.
- A reply to a fixed point is "Done." or one line. Never quote the changed code or describe what the
  diff shows. Questions state only what the reader needs to decide.
- Leave out commit hashes and branch names.
- Drop nits on the wording of answers to the user's own earlier asks.
- Link code with permalinks. One paragraph per line, never hard-wrapped.

## Claim verification

- Before sign-off, the drafting agent verifies every claim in the draft against the authoritative
  source: code on the base branch, a referenced design at its current head, the spec, the primary
  text being quoted. Agent output alone does not count. The draft lists the file:line for each
  check.
- Trace every consequence claim (X gets stuck, Y is lost) step by step to source lines. Do not
  label anything inferred while it can still be checked.
- Re-fetch every quote at its primary page and quote it verbatim. Do not say what a URL serves
  without checking it.
- When the user asks whether everything is verified, answer honestly and list what is still
  inferred.
- After the last rewording, the orchestrator runs a fresh independent reviewer agent on the final
  draft. It checks each claim, anchor and the tone.
- The orchestrator also runs an independent adversarial agent on the finished draft, told to refute
  each comment and find missed issues. The drafting agent addresses both reports.
- For design documents, audit the premise and motivation too, against the primary sources the
  document cites. Re-check that referenced designs have not moved since the document's head.
- When a page blocks the tools, ask the user to save it as PDF.

## Mechanics

Publish one atomic review. Never use `pulls/{N}/comments/{id}/replies` or a standalone
`POST pulls/{N}/comments`: both publish immediately as separate reviews.

Write every payload (review JSON, GraphQL documents, body files) with the file-write tool into
`{scratch}`, never with a heredoc or `echo`, so each `gh` call stays one short line.

1. Check for an existing pending review by the user (`pulls/{N}/reviews`, `state == "PENDING"`).
   GitHub allows one per user per PR. If one exists, stop and ask.
2. Re-check the PR head sha right before posting. If it moved, review the new delta first.
3. Threads. Write `{scratch}/threads.graphql`:

   ```graphql
   query($o:String!,$r:String!,$n:Int!){
     repository(owner:$o,name:$r){
       pullRequest(number:$n){
         id
         reviewThreads(first:100){nodes{id isResolved isOutdated path line
           comments(first:50){nodes{author{login} createdAt url body}}}}
       }
     }
   }
   ```

   Run `gh api graphql -f o={owner} -f r={repo} -F n={N} -F query=@{scratch}/threads.graphql`.
4. Create the pending review with REST. Write `{scratch}/review.json` with `commit_id` and
   `comments[{path,line,side:"RIGHT",body}]` and no `event`. Run
   `gh api -X POST repos/{owner}/{repo}/pulls/{N}/reviews --input {scratch}/review.json`. It
   returns `id` and `node_id`.
5. Thread replies. Write `{scratch}/reply.graphql` once and one body file per reply:

   ```graphql
   mutation($review:ID!,$thread:ID!,$body:String!){
     addPullRequestReviewThreadReply(input:{pullRequestReviewId:$review,
       pullRequestReviewThreadId:$thread,body:$body}){comment{url}}
   }
   ```

   Run `gh api graphql -f review={node_id} -f thread={thread_id} -F body=@{scratch}/reply-1.md
   -F query=@{scratch}/reply.graphql` as one line.
6. Submit. Write `{scratch}/submit.graphql` and `{scratch}/body.md`:

   ```graphql
   mutation($review:ID!,$event:PullRequestReviewEvent!,$body:String){
     submitPullRequestReview(input:{pullRequestReviewId:$review,event:$event,body:$body}){
       pullRequestReview{url}
     }
   }
   ```

   Run `gh api graphql -f review={node_id} -f event={event} -F body=@{scratch}/body.md
   -F query=@{scratch}/submit.graphql` as one line, with `{event}` one of `COMMENT`, `APPROVE`,
   `REQUEST_CHANGES`.
7. After submitting, resolve each thread approved for resolution. Write
   `{scratch}/resolve.graphql`:

   ```graphql
   mutation($thread:ID!){resolveReviewThread(input:{threadId:$thread}){thread{isResolved}}}
   ```

   Run `gh api graphql -f thread={thread_id} -F query=@{scratch}/resolve.graphql`.
8. If a step fails after the pending review exists, fix it and continue on the same pending review.
   Never submit a partial round. If it cannot be fixed, delete the pending review
   (`DELETE pulls/{N}/reviews/{id}`) and tell the user.

### Anchors

- Anchor lines are file line numbers in the head commit, not diff offsets, and must fall inside a
  diff hunk. If the ideal line is unchanged context, pick a changed line in the same hunk.
- Get line numbers from the file at the head sha, for example `git show {head}:path/to/file`.
- A wrong line fails the whole call with 422 "Line could not be resolved".
- Verify anchors afterwards through `pulls/{N}/comments`, which has `line`.
  `reviews/{id}/comments` shows `line: null`.

### Editing the user's own comments

Correct a posted comment by editing it in place (`PATCH pulls/comments/{id}`) and delete any
correction reply (`DELETE pulls/comments/{id}`). Never post withdrawals, addenda or follow-up
corrections; they confuse the thread.

## Fixing a PR ourselves

When the user chooses to push fixes to someone else's PR, fix everything the review flagged,
including adjacent pre-existing issues in the same code, instead of leaving them as notes for the
author. The review lists the findings, the pushed commits address all of them, and the review body
says what was fixed.

- Push only on the user's explicit go.
- Never amend, rebase or force-push an open PR's branch. Add commits on top.
- When the repository requires approval of the most recent push, the user becomes the last pusher
  and their approval no longer counts on that PR.

## Per-PR notes

After every submitted round, write or update the per-PR notes: head sha reviewed, review id and
event, open asks, points conceded or dropped (never raise them again), open threads from other
reviewers, and a resume checklist of acceptance checks for the next push. On approval, trim them
to "approved, nothing open" plus any leftovers.

## Self-review before opening a PR

Run the same procedure on your own branch before asking to open a PR:

- Gather the diff against the base, the rules on every touched path, and the linked issue.
- Do all three investigation parts (3a, 3b, 3c) and use the same lenses from "What to raise".
- Verify each finding against the code, then fix it in the branch with new commits. Nothing is
  drafted as a comment and nothing is posted.
- A finding you decide not to fix goes into the PR description as a known limitation, or to the
  user, with the reason.
- Rerun the tests and checks on the final head after the fixes.
