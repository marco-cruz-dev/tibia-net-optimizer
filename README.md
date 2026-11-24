# Tibia Network Optimizer

A lightweight Windows utility written in Rust. It tunes TCP settings so that
**Tibia** stops dropping inputs, losing spell casts and rubber-banding on
distant servers (for example Secura in Frankfurt played from Portugal), even
when ping looks fine.

> **Status: Sprint 1 of 6.** The core logic is done and tested, and you can
> use it through a CLI. The system tray and GUI are planned. See
> [docs/BACKLOG.md](docs/BACKLOG.md).

## Why

Three Windows defaults hurt old game engines that send many small packets:

| Problem | Effect | Fix applied |
|---------|--------|-------------|
| **TCP Receive Window Auto-Tuning** | Buffers are resized on the fly, which causes jitter | `netsh int tcp set global autotuninglevel=disabled` |
| **Nagle's algorithm / delayed ACK** | Small packets such as steps and hotkeys are held back and bundled, for up to about 200 ms | `TcpAckFrequency=1` and `TCPNoDelay=1` (DWORD) on each active interface under `HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{GUID}` |
| **Windows Updates** | Updates silently reset these settings | Auto-heal (planned, Sprint 4) |

Every change can be reverted to the Windows defaults (`autotuninglevel=normal`,
with the registry values deleted).

## Features

- [x] Detect and apply/revert TCP auto-tuning. Works on localized Windows.
- [x] Detect active IPv4 adapters and apply/revert the Nagle / delayed-ACK tweaks.
- [x] Overall status: Optimized, Needs fix or Unknown.
- [x] Settings persisted in `%APPDATA%\TibiaNetOptimizer\config.json`.
- [x] Task Scheduler logic for an elevated, prompt-free start at logon (not yet exposed in the CLI).
- [x] Latency, jitter and packet-loss statistics engine.
- [ ] Ignore disconnected adapters that still have a stale DHCP lease (Sprint 2).
- [ ] Elevation detection and "Restart as Administrator" (Sprint 2).
- [ ] System tray icon (green/red) and egui dashboard (Sprint 3).
- [ ] Background auto-heal with notifications (Sprint 4).
- [ ] Live ping and jitter monitor for Secura/Frankfurt (Sprint 5).

## Usage (current CLI)

```powershell
cargo build --release
.\target\release\tibia-net-optimizer.exe status   # read-only, no admin needed
.\target\release\tibia-net-optimizer.exe apply    # run from an Administrator terminal
.\target\release\tibia-net-optimizer.exe revert   # restore Windows defaults (admin)
```

Example output:

```text
TCP auto-tuning : disabled (optimized)
Nagle / ACK     : active on some adapters (not optimized)
  - {ac78b7b4-...} 192.168.1.153   [default]
  - {cdcfde38-...} 192.168.1.185   [tuned]
Startup task    : not registered

Overall         : NEEDS FIX — run `tibia-net-optimizer apply` as Administrator
```

Reboot after changing auto-tuning so it takes full effect.

## Development

### Prerequisites

- Windows 10/11
- Rust stable (MSVC toolchain): `winget install Rustlang.Rustup`
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
2. **Red:** write a failing test that describes the behaviour.
3. **Green:** write the smallest change that makes it pass.
4. **Refactor:** clean up while keeping the tests green, then run clippy and fmt.
5. Update the backlog and README, then commit.

To keep tests fast and safe, the code follows a **ports and adapters** design:

```text
             ┌──────────────── pure, unit-tested ────────────────┐
 main.rs ──► │ optimizer   autotuning::parse_level   nagle logic │
 (CLI/GUI)   │ config      ipv4 validation           ping stats  │
             └───────────────┬──────────────────────┬────────────┘
                             │ CommandRunner        │ InterfaceRegistry   ◄── ports (traits)
                     ┌───────▼────────┐    ┌────────▼───────────────┐
 tests use fakes ──► │ SystemCommand  │    │ WindowsInterface       │   ◄── thin adapters,
                     │ Runner (netsh, │    │ Registry (winreg/HKLM) │       smoke-tested
                     │ schtasks)      │    └────────────────────────┘
                     └────────────────┘
```

Tests **never modify** system settings. The smoke tests only read.

### Project layout

```text
src/
├── main.rs              CLI walking skeleton (GUI replaces it in Sprint 3)
├── lib.rs
├── error.rs             Error type (AccessDenied, CommandFailed, Parse, Io)
├── config.rs            JSON settings in %APPDATA%
├── optimizer.rs         Overall status and auto-heal decisions
├── network/
│   ├── autotuning.rs    netsh parse/apply/revert
│   ├── nagle.rs         Interface detection, TcpAckFrequency/TCPNoDelay
│   ├── ipv4.rs          Usable-IPv4 validation
│   ├── registry.rs      winreg adapter (Windows only)
│   └── ping.rs          Rolling latency/jitter/loss stats
└── system/
    ├── command.rs       CommandRunner port, CREATE_NO_WINDOW runner, test fake
    └── startup.rs       schtasks /rl highest logon task
tests/system_smoke.rs    #[ignore]d read-only tests against the real OS
docs/BACKLOG.md          Sprint plan and Definition of Done
project_info.md          Original product specification
```

## Disclaimer

This tool changes system-wide Windows networking settings. It does not touch
the Tibia client or its network traffic. Use it at your own risk; you can
revert every change with `revert`. It is not affiliated with CipSoft.

## License

[GPL-3.0](LICENSE)
