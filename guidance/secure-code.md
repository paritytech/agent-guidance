# Secure code

Rules for specific kinds of code, on top of "Writing code" in
`.agent-guidance/guidance/security.md`. Apply the sections that match what you are building.

## Cryptography and signatures

- Use established libraries. Never write your own construction.
- Domain-separate every hash and signature. A signed payload names its domain or chain, its
  intended recipient, a nonce or fresh challenge, and every parameter it authorizes.
- Verify every signature and every part of a proof before using it, and reject a proof with
  unused parts.
- Never use a value a block producer, validator or caller can influence as randomness.

## Polkadot SDK runtimes

- Give every dispatchable a weight benchmarked for its worst case: maximum-size storage, every
  branch, proof size included. No placeholder or hand-assembled weights. The weight grows with
  every input that grows the work, and the unused part is refunded.
- Charge for everything that consumes shared capacity, including messages a call sends. A free or
  unsigned call rejects input that does no useful work.
- Use bounded types for call arguments and storage. Take a deposit sized to the storage a user
  causes, including state created as a side effect.
- Bound the work in hooks and per-block logic. Spread larger work over blocks, and always move
  past an item that fails.
- Decode untrusted bytes with a depth and size limit.
- Run dispatch logic that can fail after writing inside a storage transaction.
- Respect the existential deposit and consumer and provider reference counts on every balance
  change.
- Migrations: bump the storage version, test with `try-runtime`, keep each step within a block's
  weight, and never create funds without a source.
- XCM: keep barriers and filters as allow-lists, keep aggregate limits when relaxing a pattern,
  never allow calls that dispatch other calls through a filter unless the inner calls are filtered
  too, and act with the least privileged origin.

## Smart contracts and dapps

- Follow checks, effects, interactions, and guard every external call against reentrancy.
- Restrict every privileged function, and allow initialization to run once only.
- Protect signed messages against replay with a nonce and a deadline.
- Never trust the frontend: the contract or backend enforces every rule.
- Show the user exactly what they sign. Never put secrets or private keys in frontend code.
- Treat RPC endpoints, indexers and other contracts as untrusted data sources.

## Services and backends

- Derive a client's identity (for rate limits, logs, allow-lists) only from sources you control,
  never from a header the client can set.
- Give sessions an absolute maximum age that survives refresh, and re-check permissions when a
  credential is refreshed.
- Expire and cap per-client state.
- Never block on per-peer or per-client I/O while holding a lock or inside a shared event loop.
