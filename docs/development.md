# Development

The installer, its templates and the tests live in the `tools/` crate. The check commands are in
the [README](../README.md#development).

## Templates and self-install

- The templates in `tools/templates/` mirror the downstream paths.
  - `tools/templates/user/` holds the user-level hook config, with paths relative to `HOME`.
- They are embedded into the installer binary.
- This repository installs its own hook, so agents working on it receive the published guidance
  like any downstream repository.
- The test `repository_root_matches_the_templates` fails when `load.sh` or a hook config at the
  repository root differs from its template in `tools/templates/`.
- After changing `load.sh` or a hook config template, run the installer on this repository again:
  `cargo run --quiet --manifest-path tools/Cargo.toml -- .`
  - It replaces the loader entry in each root hook config with the template's entry.

## Loader tests

- The loader tests use a local bare repository as the remote (`AGENT_GUIDANCE_URL=file://...`).
- They run every case under `/bin/bash` and under the `bash` on `PATH`.

## CI

The jobs are in `.github/workflows/test.yml`.

| Job | Runners | Steps |
|---|---|---|
| `test` | `ubuntu-latest`, `macos-latest` | `cargo fmt --check`, `cargo clippy`, `cargo test` |
| `shellcheck` | `ubuntu-latest` | `shellcheck` on `load.sh` |
