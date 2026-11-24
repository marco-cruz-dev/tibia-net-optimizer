# Changelog

All notable changes to this project. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
uses [Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-11-24

First release.

### Added
- **Dashboard and tray app** (`tibia-net-optimizer`). A shield icon in the tray shows a green, red or amber status dot. The dashboard has a status banner, one card per setting (with a toggle and an explanation), Optimize Now and Revert, and automation options. Closing the window hides it to the tray.
- **Command line** (`tno`): `status [--json]`, `apply|revert [--only ID]`, `startup on|off|status`, and `--elevate`.
- **Windows tweaks:** disables TCP receive-window auto-tuning, and Nagle / delayed ACK (`TcpAckFrequency`, `TCPNoDelay`) on connected adapters only. Stale DHCP leases are ignored.
- **Linux tweaks:** `tcp_moderate_rcvbuf`, `tcp_autocorking`, and per-route quick ACK. Persisted with a `sysctl.d` drop-in and a NetworkManager hook; the original values are restored on revert.
- **macOS tweaks:** `doautorcvbuf` and `delayed_ack`, persisted with a LaunchDaemon.
- **Auto-heal:** re-checks every 60 s and re-applies settings the OS reset, with a desktop notification. It never undoes your own Revert.
- **Run at startup with admin rights:** Task Scheduler on Windows, systemd on Linux, launchd on macOS.
- **Live connection monitor:** finds the Tibia client's game connection and pings it (ICMP only), showing ping, average, jitter, loss and a sparkline. It follows relogs and world changes, and skips hosts that block ping.
- **Copy diagnostics:** copies a text summary of everything shown. All text is selectable.
- **Windows extras:** the `.exe` has an icon, an `asInvoker` + PerMonitorV2 manifest and version info. The dashboard starts without a console window.

### Notes
- The Linux/macOS benefit for Tibia hasn't been measured yet (see [#54](https://github.com/marco-cruz-dev/tibia-net-optimizer/issues/54)).
- Release binaries aren't code-signed. Windows SmartScreen and macOS Gatekeeper will warn on first launch.
