# Product Backlog & Sprint Plan

Source of truth for requirements: [`project_info.md`](../project_info.md).
Each story is done when its acceptance criteria are covered by tests (unit
tests with fakes, plus `#[ignore]`d smoke tests for real OS adapters),
`cargo clippy -- -D warnings` is clean and CI is green.

Legend: ✅ done · 🔜 next sprint · ⏳ later

---

## Sprint 1 — Tested core + CLI walking skeleton ✅

Goal: prove every OS integration end-to-end with a minimal CLI, behind
test seams, before any GUI work.

| ID | Story | Status |
|----|-------|--------|
| S1-1 | As a player I can see whether **TCP auto-tuning** is `disabled`, including on localized Windows. | ✅ `network/autotuning.rs` |
| S1-2 | As a player I can **apply/revert** auto-tuning (`disabled` / `normal`). | ✅ |
| S1-3 | As a player I can see which **active IPv4 adapters** exist and whether each has `TcpAckFrequency=1` + `TCPNoDelay=1`. | ✅ `network/nagle.rs`, `network/ipv4.rs` |
| S1-4 | As a player I can **apply** Nagle tuning to active adapters and **revert** it from every interface. | ✅ idempotent apply |
| S1-5 | As a player I get one **overall status** (Optimized / Needs fix / Unknown) that will drive the tray colour. | ✅ `optimizer.rs` |
| S1-6 | As the app, I decide **what to auto-heal**, only when auto-heal is on *and* elevated. | ✅ `optimizer::heal_plan` |
| S1-7 | As a player my **settings persist** in `%APPDATA%`, and a missing or corrupt file never stops the app from starting. | ✅ `config.rs` |
| S1-8 | As a player I can **register/unregister an elevated logon task**. | ✅ `system/startup.rs` (logic; not yet wired to CLI) |
| S1-9 | Rolling **latency / jitter / loss statistics**. | ✅ `network/ping.rs` (stats only) |
| S1-10 | CI on `windows-latest`: fmt, clippy, tests, smoke tests. | ✅ `.github/workflows/ci.yml` |

## Sprint 2 — Correctness on real machines + elevation 🔜

| ID | Story | Acceptance criteria |
|----|-------|---------------------|
| S2-1 | **Only patch truly connected adapters.** The registry keeps stale `DhcpIPAddress` values for disconnected adapters (seen on the dev machine). | Cross-check registry GUIDs with `GetAdaptersAddresses` (`OperStatus == Up`, non-loopback/tunnel). Matching logic unit-tested with fake adapter lists. |
| S2-2 | **Friendly adapter names** ("Ethernet", "Wi-Fi") instead of GUIDs. | Name comes from IP Helper `FriendlyName`. Falls back to the GUID. |
| S2-3 | **Elevation detection** (`system/elevation.rs`). | `is_elevated()` via `GetTokenInformation(TokenElevation)`. Smoke test. |
| S2-4 | **Relaunch as Administrator** with the same args. | `ShellExecuteW("runas")`. The argument-building logic is unit-tested. |
| S2-5 | CLI: `startup on/off`, `--json` status output. | Wires S1-8. Clear error when not elevated. |
| S2-6 | **Reboot-recommended flag** after an auto-tuning change. | State is tracked in memory and shown by the CLI/GUI. |
| S2-7 | netsh **OEM code-page decoding** (proper localized labels). | Decode with `GetOEMCP` + `MultiByteToWideChar`. The existing parser tests still pass. |

## Sprint 3 — Tray + GUI shell ⏳

- `eframe`/`egui` window (~420×520) with header, elevation badge and status banner.
- Read-only banner with a "Restart as Administrator" button (S2-4).
- `tray-icon`: Green `#22C55E` / Red `#EF4444` / Amber (unknown) anti-aliased icons generated with `image`. Icon pixel generation is unit-tested.
- Tray menu: Status, Open Dashboard, Apply, Revert, Quit.
- Close-to-tray via `ViewportCommand::CancelClose`; `--minimized` start.
- UI state lives in a testable `AppState` / `Action` reducer, so egui code stays thin.

## Sprint 4 — Background auto-heal ⏳

- Worker thread: check at launch and every `check_interval_secs`, execute `heal_plan`. Uses an injectable clock and ports so it can be tested.
- Toast notification or tray update when a heal happens.
- Automation checkboxes in the GUI (auto-heal, startup task, minimize to tray), persisted via `Config`.

## Sprint 5 — Live latency monitor ⏳

- Sampler via `IcmpSendEcho`, with a TCP-connect fallback. Feeds `LatencyStats`.
- Target host input, Start/Stop, Ping / Avg / Jitter / Loss, optional `egui_plot` sparkline.
- Verify that the default target (`secura.tibia.com`) resolves and responds to ICMP. If not, choose a Frankfurt fallback.

## Sprint 6 — Release ⏳

- `build.rs` + `winres`: `asInvoker` manifest and app icon.
- `cargo build --release` size check, manual QA checklist, GitHub release artifact.

---

## Definition of Done

1. The behaviour is specified by a failing test first (red → green → refactor).
2. OS side effects go through a port (`CommandRunner`, `InterfaceRegistry`, …) with a fake for tests.
3. `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test` all pass.
4. Real adapters have an `#[ignore]`d **read-only** smoke test. Tests never modify system settings.
5. The README and this backlog are updated.
