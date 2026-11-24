# Product Backlog & Sprint Plan

This file is the source of truth for requirements. It absorbed the original
product specification, and the problem statement lives in the
[README](../README.md#why). Each story is done when its acceptance criteria
are covered by tests (unit
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

## Manual QA checklist (changes the system; run on a test machine)

| OS | Steps |
|----|-------|
| Windows | `apply --elevate` → accept UAC → `status` shows optimized and "Reboot recommended". `startup on --elevate` → Task Scheduler shows "TibiaNetOptimizer". `revert --elevate` → defaults restored. `startup off --elevate`. |
| Linux | `sudo tibia-net-optimizer apply` → `/etc/sysctl.d/99-tibia-net-optimizer.conf` exists, `ip route show default` shows `quickack 1`. Reconnect Wi-Fi → quickack is still on (NetworkManager hook). Reboot → still optimized. `sudo ... revert` → original values restored and files removed. `sudo ... startup on/off` → `systemctl is-enabled tibia-net-optimizer`. |
| macOS | `sudo tibia-net-optimizer apply` → `sysctl net.inet.tcp.delayed_ack` = 0 and the plist exists in `/Library/LaunchDaemons`. Reboot → still 0. `revert` → `delayed_ack` back to 3 and plist removed. `startup on/off`. |
| Auto-heal (Windows) | `startup on --elevate` and log in again (the app starts elevated in the tray). Tick "Auto-heal". In an admin terminal: `netsh int tcp set global autotuninglevel=normal`. Within 60 s: toast "...re-applied: TCP auto-tuning", the tray turns green, and `status` shows disabled. Click Revert in the dashboard → auto-heal unticks with an explanation, and nothing gets re-applied. |
| Copy | Select any text in the dashboard → Ctrl+C pastes elsewhere. **Copy diagnostics** → "Diagnostics copied to the clipboard."; paste shows the full summary. |
| Connection monitor | Open the dashboard without Tibia → "Waiting for Tibia". Log in to a world → within ~10 s the status changes to "Measuring…" with an entry point `2606:4700:…` (Cloudflare), Ping ≈ 11 ms, and the sparkline draws. Change world or relog → it follows. Log out → back to waiting, with no loss counted. Hide to the tray, then reopen → it resumes. Or run `$env:TNO_PING_IP="<edge>"; cargo test --test system_smoke -- --ignored pings_ip_from_env --nocapture`. |
| GUI (all) | Launch with no args → dashboard shows real status and a coloured tray icon. Left-click the tray → window shows. Right-click → menu: Status, Open Dashboard, Apply, Revert, separator, Quit. Close the window → hides to the tray (with "Minimize to tray" on) or quits (off). Toggle a card / Optimize Now / Revert: Windows admin → in-process; Windows user → "Restart as Administrator" relaunches elevated; Linux/macOS user → password prompt (pkexec / macOS dialog). `--minimized` → starts in the tray only. |

## Sprint 3 — Cross-platform core: Linux & macOS ✅ (spike pending)

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

Done test-first (69 new tests run red against stubs, then green). The suite
grew from 94 to 155 unit tests. All Unix logic is plain Rust that compiles
everywhere, so it is unit-tested on Windows. `cargo clippy --all-targets -D
warnings` is clean for `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu`
and `aarch64-apple-darwin`.

| ID | Story | Status |
|----|-------|--------|
| S3-0 | **Spike:** measure whether the Linux/macOS tweaks help Tibia. | ⏳ Method and hypotheses are in [spikes/S3-0-unix-tweaks.md](spikes/S3-0-unix-tweaks.md). Needs measurements on real Linux/macOS machines. |
| S3-1 | **Generic `Tweak` model.** | ✅ `tweak/mod.rs`. `optimizer` and `report` work on `TweakStatus` lists, and Windows is refactored onto `tweak/windows.rs`. Text output is unchanged apart from layout. **JSON changed** to a generic `tweaks[]` array plus a `platform` field. |
| S3-2 | **`Sysctl` port.** | ✅ `unix/sysctl.rs`. One `sysctl -n` / `sysctl -w` adapter serves both OSes, using the absolute path `/usr/sbin/sysctl` because `/usr/sbin` isn't on the PATH on Debian. The planned `/proc/sys` adapter was dropped because one adapter is enough. |
| S3-3 | **Linux tweaks.** | ✅ `tcp_moderate_rcvbuf`, `tcp_autocorking` (`tweak/sysctl.rs`) and `quickack` (`tweak/quickack.rs`). Routes are parsed from the text output of `ip route show default` rather than `ip -j`, because `quickack` placement in the JSON varies by iproute2 version. Settings persist in a `sysctl.d` drop-in that records the original values. A NetworkManager dispatcher hook re-applies quickack after DHCP or reconnects, and is only installed when NetworkManager is present. |
| S3-4 | **macOS tweaks.** | ✅ `doautorcvbuf`, `delayed_ack`, persisted by a LaunchDaemon plist that runs `sysctl -w` at boot and records the originals in XML comments. Revert removes it. No `launchctl` call is needed because the plist only runs at load. |
| S3-5 | **Elevation and relaunch** on Unix. | ✅ `unix/elevate.rs`: `geteuid`. In a terminal it uses `sudo` and runs in the same terminal. Without one it uses `pkexec` on Linux or an AppleScript admin prompt on macOS. Shell and AppleScript quoting are unit-tested. |
| S3-6 | **Start at boot** on Unix. | ✅ `unix/service.rs`: a systemd one-shot unit runs `apply` after `network-online.target`, or a LaunchDaemon on macOS. Both sit behind a new `StartupManager` port, which Task Scheduler also implements. Tray autostart (XDG / LaunchAgent) moves to Sprint 4. |
| S3-7 | **Platform config paths.** | ✅ `config::config_dir_for` (XDG / Application Support / APPDATA), tested with injected env vars. |
| S3-8 | **CI matrix** for Windows, Ubuntu and macOS with read-only smoke tests. | ✅ Workflow and `tests/unix_smoke.rs` are written. ⏳ Green on GitHub is only confirmed after a push. |

Known limitations / follow-ups:
- Under `sudo` on Linux, `$HOME` is root's, so settings go to root's config.
  This only matters once the GUI stores settings; revisit in Sprint 5.
- The quickack persistence relies on NetworkManager. On systemd-networkd or
  ifupdown systems, quickack must be re-applied with `startup on`.
- "Reboot recommended" is only computed on Windows (boot time on Unix is `None`).

## Sprint 4 — Tray + GUI shell ✅ (Linux/macOS GUI unverified at runtime)

Goal: launching the app with no command opens a compact dashboard, and the
app lives in the system tray with a colour-coded icon. All CLI commands keep
working.

### UX specification

**Window:** about 420×520 px, compact, not resizable.
1. **Header:** app title and a privilege badge. It shows **"Admin Active"**
   (green) when elevated, or **"Read-Only"** (amber) otherwise. The amber
   badge opens a banner: *"Running in Read-Only Mode — Restart as
   Administrator to apply changes"*, with a button that relaunches the app
   elevated (S2-4).
2. **Status banner:**
   - Green: *"Optimized for Tibia — Zero Packet Bundling"*
   - Red: *"Suboptimal Settings Detected — Click 'Optimize Now'"*
   - Amber: *"Status unknown — some settings couldn't be read"*
3. **One card per tweak:** label, current value, state, and a toggle that is
   enabled only with admin/root rights. A tooltip explains why the setting
   matters (for example, auto-tuning resizes buffers and causes jitter;
   Nagle bundles small packets for up to 200 ms). The Windows Nagle card
   lists the connected adapters with name and IPv4, so the user can see what
   gets patched.
4. **Actions:** **Optimize Now** and **Revert to Defaults**.
5. **Automation:** three checkboxes, persisted in `Config`:
   - "Auto-heal settings if the OS resets them" (the loop itself is Sprint 5)
   - "Run at startup with admin rights" (Task Scheduler / systemd / launchd)
   - "Minimize to tray instead of closing"
6. **Reboot notice** after a Windows auto-tuning change.

**Tray:**
- Icon: a 32×32 anti-aliased circle generated in memory (no `.ico` files at
  runtime). Green `#22C55E` = optimized, red `#EF4444` = needs fix, amber
  `#F59E0B` = unknown.
- Left-click: show the window and bring it to the front.
- Right-click menu: `Status: Optimized` (disabled info item), `Open
  Dashboard`, `Apply Optimizations Now`, `Revert to Defaults`, separator,
  `Quit`.

**Behaviour:**
- With "Minimize to tray" on, closing the window hides it
  (`ViewportCommand::CancelClose`). `--minimized` (used by the startup entry)
  starts hidden.
- The UI repaints about every 250 ms so tray clicks and background results
  show up immediately.
- Slow OS calls (netsh, registry, sysctl, schtasks) run on a worker thread,
  so the UI never freezes.

### Stories

Done test-first (49 tests run red, then green). The suite grew from 155 to
197 unit tests. Stack: eframe 0.37 with glow, and tray-icon 0.26. On Linux,
tray-icon uses the pure-Rust `ksni` backend, so no GTK is needed and Linux
can be cross-checked from Windows.

| ID | Story | Status |
|----|-------|--------|
| S4-1 | **App state reducer** (`app/state.rs`). | ✅ Pure `update(Msg) -> Vec<Effect>`. Covers refresh, worker results, requests (blocked while busy), restart, checkboxes, show/hide/close/quit and notices. |
| S4-2 | **Action routing.** | ✅ `state::route`: in-process when elevated; on Unix without root, the worker runs `pkexec`/`osascript <exe> apply\|revert [--only ID]` / `startup on\|off`; on Windows without admin, "Restart as Administrator". The CLI gained `--only <setting-id>`. |
| S4-3 | **View model** (`app/view.rs`). | ✅ Every UX text from the spec is asserted, along with tooltips for all 7 tweaks and the enabled/disabled rules. |
| S4-4 | **Worker thread** (`app/worker.rs`). | ✅ Owns the platform on its own thread. Tested with thread-safe fake tweaks, a fake startup manager and a fake elevator. The CLI and the worker now share `actions::run_tweaks/run_action` and `report::collect`. |
| S4-5 | **Icon generator** (`app/icon.rs`). | ✅ 32 px anti-aliased circles in green `#22C55E`, red `#EF4444`, amber `#F59E0B` and grey (checking). Pixels are unit-tested. |
| S4-6 | **egui window + tray** (`ui/`, default-on `gui` feature). | ✅ Verified on Windows with a screenshot of real data, a `WM_CLOSE` test (hides to the tray, process stays alive), and `--minimized` (window hidden, process running). If no tray is available, the app falls back to "close = quit". |
| S4-7 | **CLI:** GUI by default. | ✅ No command or `--minimized` opens the GUI; `status`, `apply`, `revert` and `startup` are unchanged. On Windows, the console is detached when it was created only for the app. |
| S4-8 | **CI** builds the GUI on all three OSes, plus a CLI-only (`--no-default-features`) clippy pass. | ✅ Green on GitHub for Windows, Ubuntu and macOS (run 45407019993). |

Known limitations / follow-ups:
- A `--minimized` start flashes the window for one frame, because eframe
  always shows it after the first paint. Sprint 7 (GUI subsystem polish).
- The Linux tray needs a StatusNotifierItem host (KDE, most GNOME setups with
  the AppIndicator extension, XFCE, ...). Without one, the app runs window-only.
- Tray clicks and the Linux/macOS GUI haven't been exercised at runtime yet
  (manual QA).
- Binary size grew from 0.26 MB to 5.4 MB (release). `--no-default-features`
  still produces the small CLI-only build.

## Sprint 5 — Background auto-heal ✅

Goal: when Windows (or a Linux/macOS update) silently resets a setting, the
app notices and re-applies it, then tells the user.

Done test-first (32 tests run red, then green). The suite grew from 197 to
218 unit tests.

| ID | Story | Status |
|----|-------|--------|
| S5-1 | **Periodic checks in the worker.** | ✅ The worker re-checks `check_interval_secs` after each status (default 60 s, minimum 10 s) and emits `Status` on its own, even while the window is hidden. It remembers the last reboot timestamp. Tested with a 30 ms interval. |
| S5-2 | **Heal decision in the reducer.** | ✅ Heals only when auto-heal is on, the app runs **in-process elevated** (Unix without root would prompt for a password every minute) and nothing else is in flight. A heal that "succeeds" but doesn't stick is retried only on the next periodic check, so there is no tight loop. |
| S5-3 | **Never fight the user.** | ✅ Reverting all settings, or one, turns auto-heal off, saves that, and appends *"Auto-heal was turned off so it won't undo your change."* |
| S5-4 | **Desktop notification.** | ✅ On success, and on a *new* heal error (repeats are suppressed). Windows: native WinRT toast, verified on the dev machine. Linux: `notify-send`. macOS: `osascript display notification`. Tray icon and tooltip update through the normal status flow. |
| S5-5 | **Auto-heal hint** when enabled without admin/root. | ✅ Windows points to "Run at startup", which starts the app elevated at login. Linux/macOS explain that "Run at startup" re-applies at every boot. |
| S5-6 | **Settings under `sudo`/`pkexec`.** | ✅ `config::invoking_user` reads `SUDO_UID`/`SUDO_GID`/`SUDO_USER` or `PKEXEC_UID` (ignoring root itself). The config lives in the real user's home, looked up with `getpwuid_r`/`getpwnam_r`, and ownership is handed back with `chown` after saving. |

Known limitations / follow-ups:
- On Windows, auto-heal only runs while the app is elevated, so in practice
  when started via "Run at startup" (Task Scheduler `/rl highest`). On
  Linux/macOS, the boot-time `startup` entry re-applies once per boot; there
  is no root GUI running in the background.
- Without an Explorer notification setting for the app, toasts are shown
  under PowerShell's identity (the standard fallback for unpackaged apps).
  A proper AppUserModelID comes with the Sprint 7 installer.
- The end-to-end heal (an elevated GUI with real settings drifting) is on the
  manual QA checklist.

## Sprint 6 — Live connection monitor ✅

The monitor measures **the user's real Tibia connection**. It finds the
running client's game connection and pings it with ICMP. There is no target
box, no Start button and no third-party test server.

**Finding: how Secura traffic really flows (measured 2026-11-24, while playing):**
- The Tibia client's game connection goes to `[2606:4700:90:0:4b6e:7f99:a2f2:fd80]:7171`. ARIN RDAP says that address belongs to **Cloudflare, Inc.**, so CipSoft fronts its game worlds with Cloudflare's TCP proxy (Spectrum). It's anycast: the user reaches the **Lisbon (LIS)** edge, according to `tibia.com/cdn-cgi/trace`. Game-world IPs aren't public (`secura.tibia.com` has no DNS record).
- RTT to that edge is **11 ms** (ICMPv6, 0% loss). The rest of the way to the Frankfurt game world runs on Cloudflare's backbone. A public Frankfurt host (52 ms over the ISP's own route) measures a path the game doesn't use, so the first iteration's Frankfurt default was dropped.
- **Update, same day:** during a later session the client held **two** game-port connections at once: the Cloudflare edge (answers ICMP, 11 ms) **and** `18.192.74.45:7171`, an AWS EC2 host in Frankfurt (`eu-central-1`, per RDAP and reverse DNS) that **blocks ICMP**. Which one carries which traffic isn't visible from outside. The first detector took the first connection; Windows lists IPv4 first, so it switched to the AWS host and every probe timed out ("os error 11010" = `IP_REQ_TIMED_OUT`). Fixed: all candidates are kept and the first one that answers ICMP is measured.
- **Consequences:**
  1. The client's TCP session (and so Nagle, delayed ACK and receive window) is with the nearby Cloudflare edge. The local tweaks still matter: Nagle/delayed-ACK delays happen on the user's machine regardless of RTT. Auto-tuning matters less on an 11 ms link.
  2. The part of the path the user's ISP and Wi-Fi affect is user → Cloudflare edge, which is exactly what the monitor measures.
  3. Probe with **ICMP only**. Each TCP connect to :7171 is forwarded by Spectrum to CipSoft's game server, so TCP probing every 2 s would hammer the origin.
  4. The edge→world leg can't be measured from outside (explained in the UI tooltip).

Done test-first in two iterations. The first used a fixed target; the
second (S6-6) switched to auto-detection. The suite went from 218 to 251 unit
tests. 3 smoke tests run on CI, plus 2 that only run when Tibia is connected
or an IP is supplied.

| ID | Story | Status |
|----|-------|--------|
| S6-1 | **Find the game connection** (`network/gameconn.rs`). | ✅ Pure selection: the process `client`/`client.exe` (not `client_launcher`) on port 7171/7172; the website (443) and other apps are ignored. Tested with the connections measured while playing. Sources: Windows `GetExtendedTcpTable` (IPv4 + IPv6, owning PIDs) plus Toolhelp process names, without opening any process handle, so BattlEye-protected processes are fine (`network/conntable.rs`; output matched `Get-NetTCPConnection` exactly). Linux `/proc/net/tcp{,6}` with the hex-address parser and socket-inode ownership. macOS `lsof -Fpcn`. The parsers are unit-tested. |
| S6-2 | **ICMP probers** (`network/probe.rs`, `network/icmp.rs`). | ✅ Windows: `IcmpSendEcho` (IPv4) and `Icmp6SendEcho2` (IPv6), no admin needed. Verified against the Secura edge: **11–12 ms**. Linux/macOS: the system `ping`/`ping6` binary (already privileged), with `LC_ALL=C` and parsed output. Command construction and parsing are unit-tested. |
| S6-3 | **Monitor thread** (`app/ping.rs`). | ✅ `GameProbe` re-detects every 5 ticks (≈10 s, following relogs and world changes) and after 3 losses in a row. A tick every 2 s with a 1 s timeout. Stopping never blocks the UI, and late samples carry an old generation that is ignored. |
| S6-4 | **State and view.** | ✅ Runs automatically while the dashboard is visible (hiding to the tray stops it). "No game" is *waiting*, not packet loss. A new edge resets the stats; reconnecting to the same edge keeps the history. Status: looking → waiting for Tibia → measuring, with the edge address shown. Jitter to 0.1 ms (green < 5, amber < 15, red), loss % (green 0, amber ≤ 2, red). |
| S6-5 | **UI.** | ✅ Status line, entry-point address, stats grid, an egui-painted sparkline, and an explainer tooltip (ICMP only, Cloudflare, what can't be measured). Verified at runtime without input: the monitor auto-started and moved to "Waiting for Tibia". Captured with PrintWindow. |
| S6-7 | **Multiple game connections and ICMP-blocking hosts** (bug found in use: "os error 11010"). | ✅ Detection returns *all* game-port connections. `GameProbe` keeps the current target if it's still present, otherwise uses the first that answers ICMP, and switches after 3 losses. If none answer, it shows "server doesn't answer ping" (not packet loss, and never a TCP fallback). Windows `IP_*` codes are shown as words ("timed out"). Verified live on Secura: the AWS host timed out, and the Cloudflare edge was picked at 11–12 ms. |
| S6-9 | **Recognisable app and tray icon** (user request; replaces the plain grey/coloured dot). | ✅ An original mark (not Tibia/CipSoft artwork): a heater shield (RPG cue) with a white ping pulse (network), on a blue `#2563EB` → teal `#0F766E` gradient with a dark navy outline. The tray uses the same shield plus a **status dot** (green/red/amber/grey) with a dark ring, so it's recognisable *and* shows status. Drawn in code with 1 px anti-aliasing (`app/icon.rs`; signed distances to the shield polygon and pulse polyline), so there are no image files. Unit-tested for silhouette, gradient, pulse, dot colours and anti-aliasing; reviewed visually on light and dark backgrounds at 256/64/32/16 px (`cargo test --lib app::icon -- --ignored` writes previews). |
| S6-8 | **Copy text** (user request). | ✅ eframe's clipboard support enabled (it was off in the slim build, so Ctrl+C did nothing), all labels selectable, and a **Copy diagnostics** button that copies a plain-text summary: version, platform, privileges, every setting, automation, monitor status, address, stats, last error and notice. The summary is unit-tested. |

Known limitations:
- `IcmpSendEcho` has 1 ms resolution, which is fine for jitter at this scale.
- Linux needs `ping` (iputils) and macOS `lsof`. Both are present by default.

## Sprint 7 — Release ✅ (release workflow: dry run pending)

Done test-first (11 tests run red, then green). The suite grew from 264 to
275 unit tests.

| ID | Story | Status |
|----|-------|--------|
| S7-1 | **ICO from the shield.** | ✅ The rasterizer moved to the dependency-free `app/icon_raster.rs`, shared by the app and `build.rs`. ICO writer: 32-bit BMP entries at 16/24/32/48/64/256 px. Header, directory, BMP header, BGRA order, bottom-up rows and AND-mask size are unit-tested. |
| S7-2 | **Windows resources.** | ✅ `build.rs` (Windows targets only; cross-checks skip it) embeds the icon, the manifest (`asInvoker`, PerMonitorV2, Windows 10/11, Common Controls v6, deliberately no `activeCodePage`) and version info via `winresource`. Verified: Explorer extracts the shield from the exe, and the version info and manifest are present. |
| S7-3 | **No console flash; two binaries.** | ✅ `tibia-net-optimizer.exe` is a GUI-subsystem program (PE subsystem 2). With CLI arguments it attaches to the parent terminal (piped output verified). **`tno.exe`** (subsystem 3) is the CLI; with no arguments it prints the status. `startup on` always registers the dashboard exe, and `--elevate` always relaunches `tno` (path logic unit-tested). Hints now say `tno apply`. |
| S7-4 | **Release workflow** (`.github/workflows/release.yml`). | ✅ Written: a tag `v*` builds Windows x86_64 (zip), Linux x86_64 (tar.gz) and macOS universal (`lipo`, tar.gz), each with both binaries, README, LICENSE and CHANGELOG, then publishes a GitHub Release with `SHA256SUMS.txt` and notes taken from `CHANGELOG.md`. A manual run is a dry run (artifacts only). ⏳ Not run yet; it needs a push. |
| S7-5 | **Docs.** | ✅ `CHANGELOG.md` (0.1.0), README Install section (two programs, unsigned-binary notes), sizes (6 MB dashboard, 0.65 MB `tno`). |

Known limitations / follow-ups:
- The binaries aren't code-signed or notarized (SmartScreen/Gatekeeper warnings), and there's no installer.
  An installer would also give toasts a proper AppUserModelID.
- No macOS `.app` bundle yet (the tarball ships plain binaries).
- The `--minimized` one-frame flash remains (eframe shows the window after its first paint).

Previously planned notes:

- Windows `build.rs` + `winres`: an `asInvoker` manifest, so the app always opens read-only and elevates on request, plus the app icon. Generate the `.ico` (16/32/48/256) at build time from the same `app/icon.rs` rasterizer, so Explorer, the taskbar and the window all show the same shield. Use the GUI subsystem so no console flashes on start, while keeping CLI output when run from a terminal.
- The release profile is already tuned for size (`opt-level = "z"`, LTO, strip).
- Release builds for Windows (`.exe`), Linux (tarball, optional `.deb`) and macOS (universal binary / `.app`).
- Size check, manual QA checklist per OS, GitHub release artifacts.

---

## Definition of Done

1. The behaviour is specified by a failing test first (red → green → refactor).
2. OS side effects go through a port (`CommandRunner`, `InterfaceRegistry`, …) with a fake for tests.
3. `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test` all pass.
4. Real adapters have an `#[ignore]`d **read-only** smoke test. Tests never modify system settings.
5. The README and this backlog are updated.
