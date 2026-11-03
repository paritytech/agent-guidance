# Rust

Rules for writing and reviewing Rust that hold across projects. A repository's own conventions win
where they differ.

## Lints and formatting

- Treat warnings as errors. Fix the cause of a lint; never silence it with `#[allow]` or
  `#[expect]`. A silencer keeps the defect and makes it look intentional.
- For cast lints, change the source type first, then widen the destination. Prefer `try_from` over
  `as`.
- After a bulk regex edit, reread every signature it touched.
- Format with the repository's pinned toolchain, and only the files you changed. Check
  `git diff --stat` and revert stray reformats.

## Unsafe

- Use a safe std API when one exists. Unsafe is a last resort.
- One unsafe operation per `unsafe {}` block, each with its own `// SAFETY:` comment that states the
  invariant and why it holds.
- Do not mark a function `unsafe` if it can check its preconditions itself.

## Features and cfg

- Cargo features are additive. A feature compiled in with no runtime opt-in behaves like the
  feature off, because Cargo unifies features across the build.
- Build modes that change behavior or ABI go behind `--cfg` (declared with check-cfg), not a cargo
  feature.
- Production builds carry no debug-only behavior.

## Types

- Do not derive `Copy`, `Clone`, `Debug`, `PartialEq` or `Eq`, or require them as bounds, unless
  something uses them.
- No synthetic `Default` on a type with no natural default. Construct the value at the boundary.
- No `#[inline]` on private or crate-internal functions. Use it only on small public items that
  need cross-crate inlining.

## Tests

- Put unit tests in a `#[cfg(test)] mod tests;` declaration backed by its own file
  (`foo/tests.rs`), not inline in the production file.
- Test private behavior from that submodule. A `cfg(test)` hook in production code is a last
  resort; keep it minimal.
- Give test fixtures and helpers the narrowest visibility that works.
- See `.agent-guidance/guidance/testing-and-ci.md` for running and judging tests.

## Docs

- A doc comment is a one-line `///` summary, a blank `///`, then a short body: the contract, units
  and failure conditions. A summary with no body on a non-trivial item is a smell.
- Item-level notes go in `///` on the item. Use `//` only for line-local reasoning, since `//` does
  not show in rustdoc or IDE hover.
- Do not repeat trait docs on impls.
- Every `pub mod` has a `//!` block with its purpose and entry point.
- Put identifiers in backticks and intra-doc links in brackets. Use `Self::` for siblings.
- A link target that exists only under some cfg is plain code, not a link, or the link breaks in
  the other configurations.
- After rewrapping doc prose, check line starts: `+ `, `- ` and `1. ` turn the line into a list
  item, and a `[x]:` at the start of a paragraph is parsed as a link definition and dropped.
- After API, rename or cfg changes, run `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` for each
  cfg and feature combination. Check, clippy and tests do not catch broken intra-doc links.
