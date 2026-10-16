# Contributing to DeskCheck

## Project layout

```
Cargo.toml              workspace: shared version, edition, lints
crates/deskcheck-core   domain model and rules, no system dependency
crates/deskcheck        the `deskcheck` binary
KANBAN.md               the plan and the status of every task
```

## Branches

- `main`: released versions only.
- `develop`: integration branch.
- One task = one branch `feat/dc-XX-short-name`, created from `develop`.
- When the task is done, merge it into `develop` with `git merge --ff-only`.

## Commit messages

We follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/#specification).

```
<type>(<optional scope>): <description>

<optional body>

<optional footer(s)>
```

- `feat`: a new feature. `fix`: a bug fix.
- Other types: `docs`, `chore`, `ci`, `build`, `refactor`, `test`, `perf`, `style`.
- A breaking change uses `!` before the colon (`feat(audio)!: ...`)
  or a `BREAKING CHANGE: ...` footer.
- Keep the description short, in the imperative mood, without a final period.

## Documentation first

1. For Rust and Cargo, use only the official documentation:
   [The Rust Book](https://doc.rust-lang.org/book/),
   [The Cargo Book](https://doc.rust-lang.org/cargo/),
   [The Reference](https://doc.rust-lang.org/reference/),
   [std](https://doc.rust-lang.org/std/).
2. Before using a library, read its documentation (docs.rs, its book).
3. If a problem stays unsolved after reading the docs, look at the
   library's GitHub issues, then at forums.

## Checks before a merge

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same checks in an `ubuntu:22.04` container, plus a build with the
minimum Rust version (`rust-version` in `Cargo.toml`).

## Target platform

Ubuntu 22.04 first: GTK 4.6, libadwaita 1.1, GNOME 42, PulseAudio
(or PipeWire through `pipewire-pulse`). Do not use APIs newer than these.
