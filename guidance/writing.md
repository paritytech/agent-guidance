# Writing

How to write text that people read: PR descriptions, commit messages, issues, review comments,
docs, code comments and replies to the user. Maintainers skim, so every sentence carries a fact
they need.

## Structure

- Lead with one or two sentences: the problem and its cause, or the change and why.
- Follow with short sections such as Changes, Validation and Not covered. Use headings only when
  the text needs them; a three-line text has none.
- Write one fact per bullet. Split a bullet that needs several sentences into sub-bullets.
- Put measurements and comparisons in a table.
- Give concrete limits: a status line is one or two sentences, an inline review comment two or
  three, a PR description fits on one screen.
- A long paragraph is a defect at any total length. Prose is for the lead; the rest goes in lists
  or tables.
- Leave out what the reader already has: the diff, the commit list, the linked issue, commit
  hashes, branch names, background a maintainer knows.

## Sentences

- Write plain declarative sentences in the present tense and active voice.
- State the fact, the cause, the change and the evidence. Nothing else.
- Name the file, function, command or number instead of describing it with an adjective.
- Use the literal phrase. Write "a parameter worth varying", not a metaphor for it.
- Keep one name for one thing. Do not call it "the cache", then "the caching layer".
- Claim only what you checked. Mark a conjecture as one. Name an affected setup only when
  verified.
- No story time: do not narrate the investigation, the order things were tried, or who decided
  what. A rejected approach gets one line, only when a reader would otherwise retry it.
- No dates or timestamps in prose ("Status (date):", "Corrected on"). Git and GitHub date every
  edit. When a claim was wrong, fix the text in place.
- No emoji.

## Formatting

- Sentence-case headings.
- No bold-keyword bullets ("**Foo**: does bar"). Write the sentence.
- No em dashes. Use a comma, colon, parentheses or a new sentence.
- Do not hard-wrap text that a web UI reflows: GitHub issues, PR descriptions, comments, release
  notes. One paragraph per line. Hard-wrap only where the file is the artifact: source, code
  comments, commit messages, Markdown kept in a repository.
- PR titles and commit subjects: 70 characters or fewer, imperative, no trailing period.

## Code references

- In anything posted to GitHub (issues, PR text, review bodies and comments), link code with a
  permalink: a blob URL pinned to a commit sha, with a line range such as `#L10-L20`. It stays
  correct after the file changes. For files in the same repository GitHub renders it as a snippet.
- Use plain `path/to/file.rs:42` only in terminal replies and code comments.

## Code comments

- Default to no comment. Write one only for a non-obvious why.
- State the invariant or constraint, not its history: no incident provenance, dates, commit refs,
  issue numbers for context, session or tool references, private paths.
- Test: does the comment make sense to someone who never saw the debugging session?

## Constructions to drop

Each of these marks text as generated. Write the plain statement instead.

- Contrast framing: "not just X, but Y"; "it's not X, it's Y" when nobody said X; arguing with no
  one ("This isn't about blaming"). State Y.
- Lists of three for rhythm ("faster, safer, and more maintainable"). List what is true, in any
  number.
- Run-ups: "Here's the thing:", "Let's dive in", "Here's what changed:". Start with the content.
- A question answered right away ("Why does this matter? Because"). State the answer.
- Closing summaries and one-line closers: "In summary", "Overall", "Bottom line". Stop after the
  last fact.
- Praise and chat residue: "Great question!", "You're absolutely right", "I hope this helps", "Let
  me know if". Cut.
- Stacked hedges ("could potentially help to possibly"). Pick one qualifier or none.
- "serves as", "stands as", "functions as". Write "is".
- An "-ing" tail (", ensuring robust handling of edge cases"). Cut it or make it a sentence with a
  fact.
- "ensures that" as filler. Say what the code does.
- Inflated significance ("a pivotal step toward", "plays a crucial role in"). Say what it does.
- False ranges ("everything from parsing to deployment"). List the items.
- Vague links ("related to the auth flow"). Name the call or file.
- Describing the document ("This PR description outlines"). Write the content.
- Unbacked claims ("all changes verified", "preserves existing behavior"). Show the command and
  result, or drop the claim.

## Words

Replace these with the plain word. A replacement is given because a bare ban leads to a synonym.

| Word | Write instead |
|---|---|
| delve | look at |
| leverage, utilize, harness (verb) | use |
| streamline | simplify |
| enhance, elevate | improve, or name the change |
| foster, resonate | cut |
| bolster | strengthen |
| underscore, highlight, showcase | show |
| facilitate, empower | let |
| unlock | enable |
| garner | get |
| navigate (figurative) | handle |
| align with | match |
| robust | name the property |
| comprehensive | full, or a count |
| crucial, pivotal, vital, key, critical (not a severity) | cut |
| meticulous | careful |
| intricate | complex |
| seamless, nuanced, multifaceted, holistic | cut |
| cutting-edge, groundbreaking, powerful | cut |
| valuable | useful |
| landscape, ecosystem (figurative) | cut, or name the thing |
| tapestry, testament, journey, game-changer | cut |
| realm | area |
| interplay | interaction |
| additionally, furthermore, moreover | also, or cut |
| notably, importantly, essentially, fundamentally, ultimately | cut |
| it's worth noting, it's important to note, at its core | cut |
| in order to | to |
| a wide range of | many, or list them |
| gated, gating | say what happens: "only runs when", "requires" |

## Examples

PR description. Not "Introduces a robust refactor of the caching layer." Write:

> Move cache eviction into `CacheStore` so `fetch()` stops doing it inline. p95 drops from 180 ms
> to 120 ms on the bench suite. No API change.

Review comment. Not "Great work! It might be worth considering the empty case." Write:

> `items[0]` panics when `items` is empty. Return early or use `first()`?

Status line. Not "Here's the thing: it's a race!" Write:

> The failing test is a race: `close()` can run before the writer flushes. Fix is in
> `Writer::close`. Rerunning CI.

Commit subject. Not "Enhance error handling for resilience". Write:

> Return 400 instead of 500 on malformed JSON body

## Revision pass

After drafting, and before posting or asking for sign-off:

1. Scan for the tells that survive most: contrast framing, closers, lists of three, em dashes, bold
   labels, words from the table.
2. Rewrite each hit around its point. Do not swap in a synonym.
3. Cut every sentence that repeats the diff, the title or an earlier sentence.
4. Check each claim against its source. Drop or mark what you could not check.
