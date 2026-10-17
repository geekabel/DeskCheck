# Contributing to DeskCheck

## Project layout

```
Cargo.toml               workspace: shared version, edition, lints
crates/deskcheck-core    domain model and rules, no system dependency
crates/deskcheck-pulse   sound backend: reads PulseAudio or pipewire-pulse
crates/deskcheck-mutter  screen backend: reads and changes GNOME (Mutter DisplayConfig)
crates/deskcheck-config  settings file: names and colors, saved safely
crates/deskcheck-app     the window (GTK 4.6 and libadwaita 1.1)
crates/deskcheck         the `deskcheck` binary
gnome-shell-extension    tells DeskCheck where the mouse is, on Wayland only
KANBAN.md                the plan and the status of every task
```

## Build dependencies

On Ubuntu 22.04:

```sh
sudo apt install build-essential pkg-config libpulse-dev libgtk-4-dev libadwaita-1-dev
```

`libpulse-dev` is the PulseAudio client library. It also talks to PipeWire
through `pipewire-pulse`.

`libgtk-4-dev` and `libadwaita-1-dev` are for the window. On Ubuntu 22.04 they
are GTK 4.6 and libadwaita 1.1, the oldest versions DeskCheck supports.

## The look of the window

The window follows the mockups of the design. Its colors and sizes are in
`crates/deskcheck-app/resources/style.css` (and `style-dark.css` for dark
mode). The icons are the ones of the mockups: their sources, drawn with lines,
are in `crates/deskcheck-app/icons/`. After changing or adding one, run:

```sh
pip install picosvg
crates/deskcheck-app/icons/build.sh
```

It writes the filled icons GTK can color, and the list of resources that
`build.rs` puts in the app.

## The mouse on Wayland

On X11, DeskCheck asks the X server where the mouse is. On Wayland, only
GNOME Shell knows it, so DeskCheck comes with a small extension that answers
one D-Bus call (`GetPosition`) and does nothing else:

```sh
gnome-shell-extension/install.sh
# log out and back in, then:
gnome-extensions enable deskcheck-pointer@geekabel.github.io
```

## Try it on a real machine

```sh
cargo run -p deskcheck-app                            # the window
cargo run -p deskcheck -- audio                       # who hears what, and problems
cargo run -p deskcheck -- audio --verbose             # with every device
cargo run -p deskcheck -- doctor                      # the sound problems and their fixes
cargo run -p deskcheck -- doctor --fix                # apply the fixes
cargo run -p deskcheck -- levels                      # which microphone hears you, live
cargo run -p deskcheck -- watch                       # each change of sound and screens, live
cargo run -p deskcheck -- test output                 # play a short sound
cargo run -p deskcheck -- test input                  # record 3 seconds, then play them back
cargo run -p deskcheck -- use input jabra             # make a microphone the default
cargo run -p deskcheck -- volume output default 80    # set a volume
cargo run -p deskcheck -- mute input default          # mute (unmute to undo)
cargo run -p deskcheck -- move recording firefox usb  # move an app to a device
cargo run -p deskcheck -- displays                    # which screen is which
cargo run -p deskcheck -- displays --verbose          # with identity and position
cargo run -p deskcheck -- mouse                       # which screen the mouse is on
cargo run -p deskcheck -- rename screen 2 Laptop      # give screen 2 a name
cargo run -p deskcheck -- rename screen 2 --reset     # remove that name
cargo run -p deskcheck-pulse --example audio_dump     # what the sound server reports
cargo run -p deskcheck-mutter --example display_dump  # what GNOME reports about screens
cargo run -p deskcheck-config --example config_dump   # where the settings file is
cargo test -p deskcheck-pulse -- --ignored            # tests that need a sound server
cargo test -p deskcheck-mutter -- --ignored           # tests that need a GNOME session
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
