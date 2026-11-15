# Contributing

Thank you for wanting to help. This page covers how the project is organised and what
a change needs before it can be merged.

## How the project works

Verone is designed in the open, and the design comes first.

- [docs/vision-and-scope.md](docs/vision-and-scope.md) says what Verone is and what it
  will never do.
- [docs/decisions.md](docs/decisions.md) records every decision and its reason.
- [docs/roadmap.md](docs/roadmap.md) lists the phases; each phase has a task plan in
  `docs/phases/`.
- [docs/future-ideas.md](docs/future-ideas.md) holds ideas that are not scheduled.

Small fixes are welcome at any time. For a new feature, a new dependency or a change of
approach, please open an issue first, so the idea can be weighed against the scope and
the roadmap before you spend time on it.

## Building and testing

You need Rust (the version in `rust-toolchain.toml`).

```bash
cargo build
cargo test
```

Before every commit, these three must pass. CI runs the same checks.

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Some tests boot real phones. They need Docker, kernel binder support and the Android
image, take a few minutes, and are skipped unless you ask for them:

```bash
VERONE_TEST_DOCKER=1 cargo test -p verone-engine-container --test integration
cargo build --release && tests/e2e/phase-1.sh
```

If your change touches the engine or a command, please run them and say so in the pull
request.

## Rules for code

- No `unsafe`.
- No `unwrap()` or `expect()` outside tests.
- External programs are run through `verone_core::process` with an argument list and a
  time limit, never through a shell string.
- Errors tell the user what went wrong and what to do next.
- New crates need agreement first; the project keeps its dependencies few.
- Update `CHANGELOG.md` when behaviour changes.

## Layout

| Folder | Content |
|---|---|
| `crates/verone-core` | Phones, storage, profiles, the engine interface, errors, process runner |
| `crates/verone-engine-container` | The container engine: runtime wrapper, host checks, images, lifecycle, adb |
| `crates/verone-cli` | The `verone` command |
| `tests/e2e` | End-to-end tests that run the real command on a real phone |
| `tests/phase-0` | Scripts from the Phase 0 proof tests |
| `packaging` | Package recipes |
| `docs` | Design documentation and the user guide |

## Licence

Verone is licensed under Apache-2.0. By contributing you agree that your contribution
is licensed the same way. Do not copy code from projects under the GPL or other
licences that are incompatible with Apache-2.0.

## Conduct

Be kind and assume good intent. Disagree about ideas, not people.
