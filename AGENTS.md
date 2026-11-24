# AGENTS.md

Guidance for coding agents working in this repository. The [README](README.md)
covers features, usage, architecture and project layout. This file only lists
what you need to work safely and consistently.

## Project

A Rust utility that tunes TCP settings for low-latency Tibia play on Windows,
Linux and macOS. It ships two binaries from one crate:

- `tibia-net-optimizer` (`src/main.rs`): the dashboard and tray app (feature `gui`, on by default).
- `tno` (`src/bin/tno.rs`): the command line.

## Commands

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings   # CLI-only build
cargo test                                                        # unit tests, fakes only
cargo test -- --ignored --nocapture                               # read-only smoke tests

# Lint the other OSes from yours (rustup target add … first):
cargo clippy --all-targets --target x86_64-pc-windows-msvc -- -D warnings
cargo clippy --all-targets --target x86_64-unknown-linux-gnu -- -D warnings
cargo clippy --all-targets --target aarch64-apple-darwin -- -D warnings
```

CI ([ci.yml](.github/workflows/ci.yml)) runs fmt, both clippy passes, the unit tests and the
smoke tests on Windows, Ubuntu and macOS. Some tests are `cfg`-gated per OS, so
the test count differs between machines.

## Safety rules

- **Never change this machine's network settings.** Don't run `tno apply`, `revert`,
  `startup on|off` or `--elevate`, and don't run `sudo`, `netsh … set`, `sysctl -w` or registry writes.
  Those belong on the [manual QA checklist (#55)](https://github.com/marco-cruz-dev/tibia-net-optimizer/issues/55).
- **Tests never modify the system.** Smoke tests are `#[ignore]`d and read-only.
- **Every OS side effect goes through a port** (`CommandRunner`, `InterfaceRegistry`,
  `Sysctl`, `StartupManager`, …) with a fake in the tests. Keep logic pure and the adapters thin.
- **Connection monitor: ICMP only.** Never probe the game port (7171/7172) with TCP. Cloudflare
  forwards each connect to CipSoft's game server.
- **Icons are original artwork** drawn in code (`app/icon_raster.rs`). Don't add Tibia/CipSoft
  assets or image files.

## Definition of Done

1. The behaviour is specified by a failing test first (red → green → refactor).
2. OS side effects go through a port with a fake for tests.
3. `cargo fmt`, both clippy passes (all three targets) and `cargo test` pass.
4. Real adapters have an `#[ignore]`d **read-only** smoke test.
5. The README is updated. Steps that can't be automated are added to #55.

## Conventions

- `rustfmt.toml` sets `max_width = 110`.
- Dashboard text and tooltips are asserted in `app/view.rs` tests. If you change UI text, update those tests.
- `app/state.rs` is a pure reducer (`update(Msg) -> Vec<Effect>`). Slow OS calls belong in `app/worker.rs`, never on the UI thread.
- Keep dependencies minimal. Release builds are tuned for size.

## Work tracking

There is no backlog file. **GitHub issues and the
[project board](https://github.com/users/marco-cruz-dev/projects/2) are the backlog.**

- Create issues with the `create-issue` skill (`.claude/skills/create-issue/`). Each issue gets one
  `type:`, one `area:` and one `priority:` label, plus the next release milestone.
- Work on one issue per branch and PR. Put `Closes #N` in the PR body, and move the card to
  *In progress*, then *In review*.
- Don't commit or push unless asked.

## Releases

1. Bump `version` in `Cargo.toml`.
2. Add a `## [x.y.z] - YYYY-MM-DD` section to [CHANGELOG.md](CHANGELOG.md) (Keep a Changelog style, written
   for players). The release workflow takes its notes from that section and fails without it.
3. To check the build, run the release workflow manually first: a manual run is a dry run that only builds artifacts.
4. Push a tag `vX.Y.Z`. [release.yml](.github/workflows/release.yml) builds Windows, Linux and
   macOS packages and publishes the GitHub Release with `SHA256SUMS.txt`.
5. Close the milestone.
