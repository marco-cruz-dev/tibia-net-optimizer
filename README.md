# Tibia Network Optimizer

A lightweight cross-platform utility written in Rust. It tunes TCP settings so
that **Tibia** stops dropping inputs, losing spell casts and rubber-banding on
distant servers (for example Secura in Frankfurt played from Portugal), even
when ping looks fine. It runs on **Windows**, **Linux** and **macOS**.

> **Status: Sprint 6 of 7 done.** The app has a tray icon, dashboard, auto-heal with desktop notifications, a live
> ping/jitter monitor, and a full CLI, on all three OSes. Next is release packaging. See [docs/BACKLOG.md](docs/BACKLOG.md).

## Why

Old game engines send many small packets. OS defaults that buffer, cork or
delay them add latency and jitter:

| Problem | Windows fix | Linux fix | macOS fix |
|---------|-------------|-----------|-----------|
| **Receive-window auto-tuning** (buffers resized on the fly cause jitter) | `netsh int tcp set global autotuninglevel=disabled` | `net.ipv4.tcp_moderate_rcvbuf=0` | `net.inet.tcp.doautorcvbuf=0` |
| **Delayed ACK** (ACKs held for up to 40–200 ms) | `TcpAckFrequency=1` per connected interface (registry) | `quickack 1` on default routes | `net.inet.tcp.delayed_ack=0` |
| **Small-packet coalescing** (steps and hotkeys bundled) | `TCPNoDelay=1` per connected interface (registry) | `net.ipv4.tcp_autocorking=0` | — (per socket only) |
| **Persistence** (updates/reboots reset settings) | Auto-heal: re-checked every 60 s and re-applied | `/etc/sysctl.d/99-tibia-net-optimizer.conf` + NetworkManager hook | LaunchDaemon that re-applies at boot |

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
- [x] **Dashboard + system tray**: a colour-coded tray icon (green/red/amber), a status banner, a card per setting with a toggle and explanation, Optimize Now / Revert, and automation checkboxes. Closing the window minimizes it to the tray.
- [x] **Auto-heal**: re-checks every 60 s (also from the tray) and re-applies anything the OS reset, with a desktop notification. It never undoes your own Revert: reverting turns auto-heal off. It needs admin rights, so on Windows use 'Run at startup' to have the app start elevated.
- [x] Under sudo/pkexec, settings stay in your own config folder.
- [x] **Live connection monitor**: finds your running Tibia client's game connection and pings it (ICMP only), showing ping, average, **jitter** (what makes steps stutter), packet loss and a sparkline. Tibia routes game traffic through Cloudflare, so this measures you → your nearest Cloudflare entry point (for example Lisbon, ~11 ms). That is the part your ISP and Wi-Fi affect. It follows relogs and world changes, skips game hosts that block ping, and runs only while the window is open.
- [x] **Copy diagnostics**: one click copies a text summary of everything on screen; all text is selectable too.

## Usage

Run the app with no arguments to open the **dashboard and tray icon**. Without admin/root rights the dashboard is read-only: on Windows it offers **Restart as Administrator**, and on Linux/macOS each change asks for your password.

```text
tibia-net-optimizer [COMMAND] [OPTIONS]

Commands:
  (none)                   Open the dashboard and tray icon (--minimized: tray only)
  status [--json]          Show current optimization state (read-only)
  apply [--only ID]        Apply all optimizations for this OS, or just one (admin/root)
  revert [--only ID]       Restore the OS defaults, or just one setting (admin/root)
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
.\target\release\tibia-net-optimizer.exe                  # dashboard + tray
.\target\release\tibia-net-optimizer.exe status           # text status, no admin needed
.\target\release\tibia-net-optimizer.exe apply --elevate  # UAC prompt, then applies
```

```sh
# Linux / macOS
cargo build --release
./target/release/tibia-net-optimizer                      # dashboard + tray
./target/release/tibia-net-optimizer status               # text status, no root needed
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

- Rust stable, 1.95 or newer (eframe 0.37)
- **Windows:** MSVC toolchain (`winget install Rustlang.Rustup`) and Visual Studio Build Tools ("Desktop development with C++")
- **Linux/macOS:** a standard `rustup` install. No GUI system libraries are needed to build: the Linux tray uses a pure-Rust D-Bus backend. Runtime tools used: `sysctl`, `ip` (iproute2), `systemctl`, and `pkexec` (GUI elevation). Running the GUI needs OpenGL and, for the tray, a StatusNotifierItem host (KDE, XFCE, GNOME with the AppIndicator extension, ...).
- **CLI-only build:** `cargo build --release --no-default-features` (no GUI, about 0.3 MB instead of 5.4 MB).

### Commands

```sh
cargo test                                   # fast unit tests (fakes, no OS changes)
cargo test -- --ignored --nocapture          # read-only smoke tests against this machine
cargo clippy --all-targets -- -D warnings    # lint
cargo fmt --all                              # format
cargo run                                    # dashboard + tray
cargo run -- status                          # CLI status

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
 ui/ (egui window + tray, thin) ──Msg──► app::state (pure reducer) ──Effect──► app::worker (thread)
 main.rs (CLI)                                   app::view (pure view model)          │
    │  platform::current()  ──►  Platform { os, tweaks: Vec<Box<dyn Tweak>>, startup } ◄┘
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
├── main.rs              Entry point: GUI by default, or a CLI command
├── cli.rs               Argument parsing and relaunch args
├── actions.rs           Apply/revert/startup actions shared by the CLI and the GUI worker
├── app/                 GUI logic, toolkit-independent and unit-tested:
│                        state (reducer), view (view model), icon (pixels), worker (thread)
├── ui/                  egui window + tray glue (feature `gui`)
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
└── system/              Command runner, elevation, console, encoding, boot time, Task Scheduler
tests/
├── system_smoke.rs      Windows read-only smoke tests
└── unix_smoke.rs        Linux/macOS read-only smoke tests
docs/
├── BACKLOG.md           Sprint plan, Definition of Done, manual QA checklist
└── spikes/              Investigations (S3-0: Unix tweak effectiveness)
```

## Disclaimer

This tool changes system-wide networking settings. It does not touch the
Tibia client or its network traffic. Use it at your own risk; you can revert
every change with `revert`. It is not affiliated with CipSoft.

## License

[GPL-3.0](LICENSE)
