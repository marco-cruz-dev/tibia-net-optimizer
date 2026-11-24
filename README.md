# Tibia Network Optimizer

A lightweight utility written in Rust. It tunes TCP settings so that **Tibia**
stops dropping inputs, losing spell casts and rubber-banding on distant
servers (for example Secura in Frankfurt played from Portugal), even when ping
looks fine. It targets Windows first; Linux and macOS support is planned.

> **Status: Sprint 2 of 7 done.** You can use the tested core through a CLI.
> Next is cross-platform support (Linux and macOS), followed by the system
> tray and GUI. See [docs/BACKLOG.md](docs/BACKLOG.md).

## Why

Three Windows defaults hurt old game engines that send many small packets:

| Problem | Effect | Fix applied |
|---------|--------|-------------|
| **TCP Receive Window Auto-Tuning** | Buffers are resized on the fly, which causes jitter | `netsh int tcp set global autotuninglevel=disabled` |
| **Nagle's algorithm / delayed ACK** | Small packets such as steps and hotkeys are held back and bundled, for up to about 200 ms | `TcpAckFrequency=1` and `TCPNoDelay=1` (DWORD) on each connected interface under `HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{GUID}` |
| **Windows Updates** | Updates silently reset these settings | Auto-heal (planned, Sprint 5) |

Every change can be reverted to the Windows defaults (`autotuninglevel=normal`,
with the registry values deleted).

## Features

- [x] Detect and apply/revert TCP auto-tuning. Works on localized Windows (OEM code-page aware).
- [x] Detect **connected** IPv4 adapters (registry cross-checked with the OS, so stale DHCP leases are ignored) and show their friendly names.
- [x] Apply/revert the Nagle / delayed-ACK tweaks.
- [x] Overall status: Optimized, Needs fix or Unknown. Output as text or `--json`.
- [x] Administrator detection and `--elevate` (UAC relaunch).
- [x] Elevated, prompt-free start at logon via Task Scheduler (`startup on/off`).
- [x] "Reboot recommended" until the machine restarts after an auto-tuning change.
- [x] Settings persisted in `%APPDATA%\TibiaNetOptimizer\config.json`.
- [x] Latency, jitter and packet-loss statistics engine.
- [ ] Linux and macOS support (Sprint 3).
- [ ] System tray icon (green/red) and egui dashboard (Sprint 4).
- [ ] Background auto-heal with notifications (Sprint 5).
- [ ] Live ping and jitter monitor for Secura/Frankfurt (Sprint 6).

## Usage (current CLI)

```text
tibia-net-optimizer [COMMAND] [OPTIONS]

Commands:
  status [--json]          Show current optimization state (default, read-only)
  apply                    Disable auto-tuning and Nagle on active adapters (admin)
  revert                   Restore Windows defaults (admin)
  startup [on|off|status]  Run elevated at logon via Task Scheduler (admin for on/off)

Options:
  --elevate                Relaunch as Administrator (UAC prompt) if needed
  --pause                  Wait for Enter before exiting
  -h, --help               Show this help
```

```powershell
cargo build --release
.\target\release\tibia-net-optimizer.exe                  # status, no admin needed
.\target\release\tibia-net-optimizer.exe apply --elevate  # UAC prompt, then applies
.\target\release\tibia-net-optimizer.exe status --json    # machine-readable
```

Example output:

```text
TCP auto-tuning : disabled (optimized)
Nagle / ACK     : disabled on all active adapters (optimized)
  - Ethernet             192.168.1.185   [tuned]    {cdcfde38-...}
Startup task    : not registered
Privileges      : standard user (read-only)

Overall         : OPTIMIZED for Tibia
```

Reboot after changing auto-tuning so it takes full effect. `status` reminds
you until you do.

## Development

### Prerequisites

- Windows 10/11
- Rust stable, 1.82 or newer (MSVC toolchain): `winget install Rustlang.Rustup`
- Visual Studio Build Tools with the "Desktop development with C++" workload

### Commands

```powershell
cargo test                                   # fast unit tests (fakes, no OS changes)
cargo test -- --ignored --nocapture          # read-only smoke tests against this machine
cargo clippy --all-targets -- -D warnings    # lint
cargo fmt --all                              # format
cargo run -- status                          # try it
```

CI runs all of these on `windows-latest` ([.github/workflows/ci.yml](.github/workflows/ci.yml)).

### TDD workflow

This project uses short, agile sprints and test-driven development:

1. Pick the next story in [docs/BACKLOG.md](docs/BACKLOG.md).
2. **Red:** write the tests against a stub (`todo!()`) and watch them fail.
3. **Green:** write the smallest change that makes them pass.
4. **Refactor:** clean up while keeping the tests green, then run clippy and fmt.
5. Update the backlog and README, then commit.

To keep tests fast and safe, the code follows a **ports and adapters** design:

```text
              ┌──────────────────── pure, unit-tested ─────────────────────┐
 main.rs ───► │ cli (args)    report (text/JSON)   optimizer (status, heal)│
 (CLI/GUI)    │ autotuning::parse_level   adapters::detect_active   nagle  │
              │ config   ipv4   ping stats   elevation quoting   boot      │
              └──────┬──────────────────────┬───────────────────┬──────────┘
                     │ CommandRunner        │ InterfaceRegistry │ AdapterSource   ◄── ports (traits)
             ┌───────▼────────┐  ┌──────────▼─────────┐  ┌──────▼──────────────┐
 tests use   │ SystemCommand  │  │ WindowsInterface   │  │ WindowsAdapter      │ ◄── thin adapters,
 fakes ────► │ Runner (netsh, │  │ Registry (HKLM)    │  │ Source (IP Helper)  │     smoke-tested
             │ schtasks)      │  └────────────────────┘  └─────────────────────┘
             └────────────────┘
```

Tests **never modify** system settings. The smoke tests only read.

### Project layout

```text
src/
├── main.rs              CLI shell over the library (GUI becomes the default in Sprint 4)
├── lib.rs
├── cli.rs               Argument parsing and relaunch args
├── report.rs            Status report: text and JSON
├── error.rs             Error type (AccessDenied, CommandFailed, Parse, Io)
├── config.rs            JSON settings in %APPDATA%
├── optimizer.rs         Overall status and auto-heal decisions
├── network/
│   ├── autotuning.rs    netsh parse/apply/revert
│   ├── adapters.rs      AdapterSource port; join of registry and live OS adapters
│   ├── iphelper.rs      GetAdaptersAddresses adapter (Windows)
│   ├── nagle.rs         TcpAckFrequency/TCPNoDelay detection, apply, revert
│   ├── registry.rs      winreg adapter (Windows)
│   ├── ipv4.rs          Usable-IPv4 validation
│   └── ping.rs          Rolling latency/jitter/loss stats
└── system/
    ├── command.rs       CommandRunner port, CREATE_NO_WINDOW runner, test fake
    ├── encoding.rs      OEM code-page decoding of console output
    ├── elevation.rs     is_elevated, UAC relaunch, Windows argument quoting
    ├── boot.rs          Boot time and reboot-pending logic
    └── startup.rs       schtasks /rl highest logon task
tests/system_smoke.rs    #[ignore]d read-only tests against the real OS
docs/BACKLOG.md          Sprint plan and Definition of Done
project_info.md          Original product specification
```

## Disclaimer

This tool changes system-wide networking settings. It does not touch the
Tibia client or its network traffic. Use it at your own risk; you can revert
every change with `revert`. It is not affiliated with CipSoft.

## License

[GPL-3.0](LICENSE)
