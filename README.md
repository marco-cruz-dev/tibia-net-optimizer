# Tibia Network Optimizer

A lightweight cross-platform utility written in Rust. It tunes TCP settings so
that **Tibia** stops dropping inputs, losing spell casts and rubber-banding on
distant servers (for example Secura in Frankfurt played from Portugal), even
when ping looks fine. It runs on **Windows**, **Linux** and **macOS**.

> **Status: Sprint 3 of 7 done.** You can use the tested core through a CLI on
> all three OSes. Next is the system tray and GUI. See
> [docs/BACKLOG.md](docs/BACKLOG.md).

## Why

Old game engines send many small packets. OS defaults that buffer, cork or
delay them add latency and jitter:

| Problem | Windows fix | Linux fix | macOS fix |
|---------|-------------|-----------|-----------|
| **Receive-window auto-tuning** (buffers resized on the fly cause jitter) | `netsh int tcp set global autotuninglevel=disabled` | `net.ipv4.tcp_moderate_rcvbuf=0` | `net.inet.tcp.doautorcvbuf=0` |
| **Delayed ACK** (ACKs held for up to 40–200 ms) | `TcpAckFrequency=1` per connected interface (registry) | `quickack 1` on default routes | `net.inet.tcp.delayed_ack=0` |
| **Small-packet coalescing** (steps and hotkeys bundled) | `TCPNoDelay=1` per connected interface (registry) | `net.ipv4.tcp_autocorking=0` | — (per socket only) |
| **Persistence** (updates/reboots reset settings) | Registry/netsh persist; auto-heal planned | `/etc/sysctl.d/99-tibia-net-optimizer.conf` + NetworkManager hook | LaunchDaemon that re-applies at boot |

Every change can be reverted. On Linux and macOS the original values are
recorded and restored.

> The Linux/macOS benefit for Tibia is still to be measured
> ([spike S3-0](docs/spikes/S3-0-unix-tweaks.md)). Tweaks that don't help will
> be dropped.

## Features

- [x] Windows: TCP auto-tuning and Nagle / delayed-ACK tweaks. Only **connected** adapters are patched (stale DHCP leases are ignored), and friendly names are shown.
- [x] Linux: receive-buffer auto-tuning, autocorking, per-route quick ACK.
- [x] macOS: receive-buffer auto-tuning, delayed ACK.
- [x] Overall status (Optimized, Needs fix or Unknown) as text or `--json`.
- [x] Admin/root detection and `--elevate` (UAC / `sudo` / `pkexec` / macOS admin prompt).
- [x] Re-apply at startup: Task Scheduler, systemd or launchd (`startup on/off`).
- [x] "Reboot recommended" after a Windows auto-tuning change.
- [x] Settings in the OS-appropriate config directory.
- [x] Latency, jitter and packet-loss statistics engine.
- [ ] System tray icon (green/red) and egui dashboard (Sprint 4).
- [ ] Background auto-heal with notifications (Sprint 5).
- [ ] Live ping and jitter monitor for Secura/Frankfurt (Sprint 6).

## Usage (current CLI)

```text
tibia-net-optimizer [COMMAND] [OPTIONS]

Commands:
  status [--json]          Show current optimization state (default, read-only)
  apply                    Apply all optimizations for this OS (admin/root)
  revert                   Restore the OS defaults (admin/root)
  startup [on|off|status]  Re-apply automatically at boot/logon (admin/root for on/off)
                           Windows: Task Scheduler, Linux: systemd, macOS: launchd

Options:
  --elevate                Relaunch with admin rights (UAC / sudo / pkexec) if needed
  --pause                  Wait for Enter before exiting
  -h, --help               Show this help
```

```powershell
# Windows
cargo build --release
.\target\release\tibia-net-optimizer.exe                  # status, no admin needed
.\target\release\tibia-net-optimizer.exe apply --elevate  # UAC prompt, then applies
```

```sh
# Linux / macOS
cargo build --release
./target/release/tibia-net-optimizer                      # status, no root needed
sudo ./target/release/tibia-net-optimizer apply
```

Example output (Windows):

```text
TCP auto-tuning           : disabled (optimized)
Nagle / delayed ACK       : disabled on all active adapters (optimized)
  - Ethernet             [tuned]    192.168.1.185  {cdcfde38-...}
Run at startup            : no
Privileges                : standard user (read-only)

Overall                   : OPTIMIZED for Tibia
```

`status --json` returns `{ platform, overall, elevated, tweaks: [{ id, label,
state, summary, details[] }], startup, reboot_recommended }`.

## Development

### Prerequisites

- Rust stable, 1.82 or newer
- **Windows:** MSVC toolchain (`winget install Rustlang.Rustup`) and Visual Studio Build Tools ("Desktop development with C++")
- **Linux/macOS:** a standard `rustup` install. Runtime tools used: `sysctl`, `ip` (iproute2), `systemctl`

### Commands

```sh
cargo test                                   # fast unit tests (fakes, no OS changes)
cargo test -- --ignored --nocapture          # read-only smoke tests against this machine
cargo clippy --all-targets -- -D warnings    # lint
cargo fmt --all                              # format
cargo run -- status                          # try it

# Lint other OSes without leaving yours (needs `rustup target add ...`):
cargo clippy --all-targets --target x86_64-unknown-linux-gnu -- -D warnings
cargo clippy --all-targets --target aarch64-apple-darwin -- -D warnings
```

CI runs fmt, clippy, the unit tests and the smoke tests on **Windows, Ubuntu
and macOS** ([.github/workflows/ci.yml](.github/workflows/ci.yml)).

### TDD workflow

This project uses short, agile sprints and test-driven development:

1. Pick the next story in [docs/BACKLOG.md](docs/BACKLOG.md).
2. **Red:** write the tests against a stub (`todo!()`) and watch them fail.
3. **Green:** write the smallest change that makes them pass.
4. **Refactor:** clean up while keeping the tests green, then run clippy (for all three targets) and fmt.
5. Update the backlog and README, then commit.

### Architecture

A **ports and adapters** design keeps the tests fast, safe and OS-independent:

```text
 main.rs (CLI, later GUI)
    │  platform::current()  ──►  Platform { os, tweaks: Vec<Box<dyn Tweak>>, startup }
    ▼
 ┌──────────────────────────── pure, unit-tested on every OS ───────────────────────────┐
 │ Tweak model · optimizer (overall, heal plan) · report (text/JSON) · cli · config     │
 │ Windows: AutoTuningTweak, NagleTweak   Linux/macOS: SysctlTweak, QuickAckTweak       │
 │ parsers/renderers: netsh, ip route, sysctl.d, launchd plist, systemd unit, quoting   │
 └───────┬──────────────┬──────────────┬──────────────┬───────────────┬─────────────────┘
         │ CommandRunner│ Interface    │ AdapterSource│ Sysctl /      │ StartupManager   ◄── ports
         │              │ Registry     │              │ SettingsStore │
   netsh, schtasks,  winreg (HKLM)  IP Helper     sysctl, files   schtasks / systemd /  ◄── thin adapters,
   sysctl, ip, systemctl                                           launchd                 smoke-tested
```

Tests **never modify** system settings. The smoke tests only read.

### Project layout

```text
src/
├── main.rs              Platform-agnostic CLI shell
├── cli.rs               Argument parsing and relaunch args
├── report.rs            Status report: text and JSON
├── optimizer.rs         Overall status and auto-heal plan over tweak statuses
├── config.rs            JSON settings in the per-OS config directory
├── error.rs
├── tweak/               Tweak trait and implementations
│   ├── windows.rs       AutoTuningTweak, NagleTweak
│   ├── sysctl.rs        SysctlTweak (Linux/macOS), records originals
│   └── quickack.rs      Linux per-route quick ACK
├── platform/            Per-OS assembly: windows.rs, linux.rs, macos.rs
├── network/             Windows networking: netsh, registry, IP Helper, adapters; ping stats
├── unix/                sysctl port, persistence (sysctl.d / plist), ip route,
│                        systemd/launchd, sudo/pkexec/osascript
└── system/              Command runner, elevation, encoding, boot time, Task Scheduler
tests/
├── system_smoke.rs      Windows read-only smoke tests
└── unix_smoke.rs        Linux/macOS read-only smoke tests
docs/
├── BACKLOG.md           Sprint plan, Definition of Done, manual QA checklist
└── spikes/              Investigations (S3-0: Unix tweak effectiveness)
project_info.md          Original product specification
```

## Disclaimer

This tool changes system-wide networking settings. It does not touch the
Tibia client or its network traffic. Use it at your own risk; you can revert
every change with `revert`. It is not affiliated with CipSoft.

## License

[GPL-3.0](LICENSE)
