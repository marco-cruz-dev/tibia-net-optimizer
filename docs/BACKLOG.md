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

## Sprint 2 — Correctness on real machines + elevation ✅

Done test-first: tests were written against `todo!()` stubs, run red, then
implemented to green and refactored. The suite grew from 53 to 94 unit tests
and from 3 to 6 smoke tests.

| ID | Story | Status |
|----|-------|--------|
| S2-1 | **Only patch truly connected adapters.** The registry keeps stale `DhcpIPAddress` values for disconnected adapters. | ✅ `network/adapters.rs` (pure join) + `network/iphelper.rs` (`GetAdaptersAddresses`). Falls back to registry-only if the OS query fails. On the dev machine this removed a false "needs fix" caused by a disconnected Wi-Fi adapter. |
| S2-2 | **Friendly adapter names** ("Ethernet", "Wi-Fi"). | ✅ From `FriendlyName`, with the GUID as fallback. |
| S2-3 | **Elevation detection.** | ✅ `system/elevation.rs` `is_elevated()` (`TokenElevation`). Smoke-tested. |
| S2-4 | **Relaunch as Administrator** with the same args. | ✅ `--elevate` → `ShellExecuteW("runas")`. Arguments are quoted to `CommandLineToArgvW` rules (unit-tested). The child gets `--pause` so its output stays visible. |
| S2-5 | CLI: `startup on/off/status`, `status --json`, clear admin errors. | ✅ `cli.rs` (pure parser) + `report.rs` (text/JSON report reused by the GUI). |
| S2-6 | **Reboot-recommended flag** after an auto-tuning change. | ✅ Change time persisted in config and compared with boot time (`system/boot.rs`). |
| S2-7 | netsh **OEM code-page decoding.** | ✅ `system/encoding.rs`: UTF-8 if valid, otherwise `GetOEMCP` + `MultiByteToWideChar`. |

Not verified automatically (it changes the system or opens a UAC prompt):
`apply`/`revert` as admin, `startup on/off` and `--elevate`. These go on the
manual QA checklist.

## Sprint 3 — Cross-platform core: Linux & macOS ⏳

Goal: the same CLI (and later the same GUI) runs on Linux and macOS and
applies each OS's closest equivalents. Windows behaviour stays the same.

**Background.** Neither Linux nor macOS has a system-wide switch for Nagle.
`TCP_NODELAY` is set per socket by the application. The useful knobs are
receive-buffer auto-tuning and delayed/quick ACK:

| Concept | Windows | Linux | macOS |
|---------|---------|-------|-------|
| Receive-window auto-tuning | `netsh … autotuninglevel` | `sysctl net.ipv4.tcp_moderate_rcvbuf` (0 = off) | `sysctl net.inet.tcp.doautorcvbuf` (0 = off) |
| Delayed ACK | `TcpAckFrequency=1` (registry, per interface) | `ip route change <default> … quickack 1` (per route) | `sysctl net.inet.tcp.delayed_ack=0` |
| Small-packet coalescing | `TCPNoDelay=1` (registry) | `sysctl net.ipv4.tcp_autocorking=0` | — (per socket only) |
| Persist across reboots | Registry / netsh are persistent | `/etc/sysctl.d/99-tibia-net-optimizer.conf`; the route flag must be re-applied by a NetworkManager dispatcher or systemd unit | A LaunchDaemon plist that runs `sysctl -w` at boot (`/etc/sysctl.conf` is not reliable on modern macOS) |
| Elevation check | Token elevation | `geteuid() == 0` | `geteuid() == 0` |
| Relaunch elevated | `ShellExecuteW("runas")` | `pkexec` | `osascript … with administrator privileges` |
| Start at login | Task Scheduler `/rl highest` | systemd system unit (root tweaks) + XDG autostart (tray) | LaunchDaemon (root tweaks) + LaunchAgent (tray) |
| Config dir | `%APPDATA%` | `$XDG_CONFIG_HOME` or `~/.config` | `~/Library/Application Support` |

| ID | Story | Acceptance criteria |
|----|-------|---------------------|
| S3-0 | **Spike:** measure whether the Linux/macOS tweaks actually help the Tibia client (native client and Wine/CrossOver). | A short write-up in `docs/` with before/after ping and jitter numbers. Drop any tweak that doesn't help. |
| S3-1 | **Generic `Tweak` model.** Each tweak has an id, label, `check`/`apply`/`revert` and a platform. `SystemStatus` and `heal_plan` work on a list of tweaks instead of hard-coded Windows fields. | Windows is refactored onto the model with no behaviour change. All existing tests still pass. |
| S3-2 | **`Sysctl` port** (read and write keys) with a fake. A `/proc/sys` + `sysctl -w` adapter on Linux and a `sysctl` adapter on macOS. | Parse/format logic is unit-tested. Read-only smoke test on each OS. |
| S3-3 | **Linux tweaks:** `tcp_moderate_rcvbuf`, `tcp_autocorking`, `quickack` on the default route (parse `ip -j route` JSON). | Persists through a `sysctl.d` drop-in. Revert removes the drop-in and restores the values recorded before the change. |
| S3-4 | **macOS tweaks:** `doautorcvbuf`, `delayed_ack`. | Persists through `/Library/LaunchDaemons/…plist`. Revert unloads and removes it. |
| S3-5 | **Elevation and relaunch** on Unix (`geteuid`, `pkexec`, `osascript`). | Command construction is unit-tested. |
| S3-6 | **Startup at login** on Unix (systemd unit / LaunchDaemon). | Generated unit/plist text is checked with snapshot tests. |
| S3-7 | **Platform config paths** for Linux and macOS. | Unit-tested with injected env vars. |
| S3-8 | **CI matrix:** `windows-latest`, `ubuntu-latest`, `macos-latest`. Smoke tests run read-only. | All three are green. |

## Sprint 4 — Tray + GUI shell ⏳

- `eframe`/`egui` window (~420×520) with header, elevation badge and status banner.
- Read-only banner with a "Restart as Administrator" button (S2-4).
- `tray-icon`: Green `#22C55E` / Red `#EF4444` / Amber (unknown) anti-aliased icons generated with `image`. Icon pixel generation is unit-tested.
- Tray menu: Status, Open Dashboard, Apply, Revert, Quit.
- Close-to-tray via `ViewportCommand::CancelClose`; `--minimized` start.
- UI state lives in a testable `AppState` / `Action` reducer, so egui code stays thin.

- Must work on all three OSes: `tray-icon` needs GTK/libappindicator on Linux, and on macOS the event loop must run on the main thread.

## Sprint 5 — Background auto-heal ⏳

- Worker thread: check at launch and every `check_interval_secs`, execute `heal_plan`. Uses an injectable clock and ports so it can be tested.
- Toast notification or tray update when a heal happens.
- Automation checkboxes in the GUI (auto-heal, startup task, minimize to tray), persisted via `Config`.

## Sprint 6 — Live latency monitor ⏳

- Sampler via `IcmpSendEcho` on Windows. On Unix, use unprivileged ICMP sockets where allowed. TCP-connect latency is the fallback everywhere. Feeds `LatencyStats`.
- Target host input, Start/Stop, Ping / Avg / Jitter / Loss, optional `egui_plot` sparkline.
- Verify that the default target (`secura.tibia.com`) resolves and responds to ICMP. If not, choose a Frankfurt fallback.

## Sprint 7 — Release ⏳

- `build.rs` + `winres`: `asInvoker` manifest and app icon (Windows).
- Release builds for Windows (`.exe`), Linux (tarball, optional `.deb`) and macOS (universal binary / `.app`).
- Size check, manual QA checklist per OS, GitHub release artifacts.

---

## Definition of Done

1. The behaviour is specified by a failing test first (red → green → refactor).
2. OS side effects go through a port (`CommandRunner`, `InterfaceRegistry`, …) with a fake for tests.
3. `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test` all pass.
4. Real adapters have an `#[ignore]`d **read-only** smoke test. Tests never modify system settings.
5. The README and this backlog are updated.
