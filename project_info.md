# Tibia Network Optimizer — Rust Windows Tray & GUI Project Specification

> **Target Context:** Pass this specification directly to Claude Code in VS Code to scaffold, build, and test a lightweight Windows desktop/system-tray utility in Rust.

---

## 1. Project Overview & Problem Statement

When playing **Tibia** on European servers (specifically **Secura**, hosted in Frankfurt, Germany) from Portugal (e.g., MEO ISP), Windows default TCP settings cause severe input dropping, un-cast spells, and character rubber-banding—even when raw ping is around ~100ms.

This happens because:

1. **Windows TCP Receive Window Auto-Tuning** dynamically resizes buffers, introducing jitter on older game engines.
2. **Nagle's Algorithm** bundles small TCP packets (like individual tile-step or spell hotkeys) for up to 200ms before transmission.
3. Major Windows Updates frequently reset these network settings back to defaults.

### Goal

Build a fast, lightweight, single-binary **Rust** Windows desktop utility (`tibia-net-optimizer.exe`) that:

- Lives in the **Windows System Tray** with color-coded status icons (Green = Optimized, Red/Yellow = Needs Fix).
- Provides a clean, lightweight **GUI window** (via `eframe`/`egui`) when clicked, showing current network optimization states, active network interface details, and a live latency/jitter monitor.
- Automatically checks and enforces optimal Windows TCP/Registry settings on startup and periodically in the background.
- Includes one-click toggles to apply or revert settings cleanly.
- Can register itself to run silently with elevated privileges at Windows startup via **Windows Task Scheduler** (avoiding UAC pop-up prompts on boot).

---

## 2. Core Technical Requirements (Windows Networking)

### Requirement A: TCP Receive Window Auto-Tuning

- **Check Command:** Run `netsh int tcp show global` silently (`CREATE_NO_WINDOW = 0x08000000`).
  - Parse stdout for `Receive Window Auto-Tuning Level` (note: handle localized Windows outputs gracefully if possible, or check for `disabled` vs `normal`).
- **Enable Optimization (Apply):**
  ```cmd
  netsh int tcp set global autotuninglevel=disabled
  ```
- **Revert to Windows Default:**
  ```cmd
  netsh int tcp set global autotuninglevel=normal
  ```
- _Note:_ Requires Administrator privileges to modify; reading (`show global`) works without elevation. Changes take full effect after a Windows reboot (display a subtle UI notice if a reboot is recommended after toggling).

### Requirement B: Disable Nagle's Algorithm (Windows Registry)

- **Registry Path:**
  `HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\<Interface-GUID>`
- **Target Interface Detection:**
  - Enumerate all subkeys under `Interfaces`.
  - Identify active network interfaces that have a valid IPv4 address:
    - Check `DhcpIPAddress` (REG_SZ, non-empty and not `"0.0.0.0"`) OR `IPAddress` (REG_MULTI_SZ containing a valid non-loopback IPv4 string).
  - Display detected active adapter(s) and their IPv4 addresses in the GUI so the user can see which adapter is being patched.
- **Registry Values (DWORD 32-bit) to Enforce:**
  - `TcpAckFrequency` = `1`
  - `TCPNoDelay` = `1`
- **Revert Action:**
  - Delete `TcpAckFrequency` and `TCPNoDelay` values (or set back to `0` / delete key values to restore Windows default behavior).
- _Note:_ Writing to `HKLM` requires Administrator privileges; reading works as a standard user.

### Requirement C: Background Monitoring & Auto-Heal

- Run a background thread/timer (e.g., checking every 60 seconds and immediately upon application launch).
- If **"Auto-Heal / Enforce on Startup"** is enabled and either setting drifts back to default (e.g., after a Windows Update), automatically re-apply the optimizations and send a Windows toast notification or update the tray icon status.

### Requirement D: Live Secura / Frankfurt Latency & Jitter Monitor (Bonus Feature)

- Periodically ping (ICMP via `ping-rs` / Windows `IcmpSendEcho` API, or TCP connect latency test) a reliable Frankfurt endpoint (e.g., `secura.tibia.com` or `1.1.1.1` / Frankfurt test host) every 2–5 seconds while the GUI window is open.
- Display:
  - **Current Ping (ms)**
  - **Average Ping (ms)**
  - **Jitter (ms)** (variance between consecutive pings—critical metric for Tibia step smoothness)
  - A simple sparkline/plot using `egui_plot` (optional).

### Requirement E: Startup Automation (Task Scheduler Integration)

- Standard `HKCU\...\Run` startup entries fail or trigger UAC prompts for apps needing Admin rights.
- Provide a toggle in the GUI: **"Run silently at Windows Startup (Task Scheduler)"**.
- When toggled ON, execute:
  ```cmd
  schtasks /create /tn "TibiaNetOptimizer" /tr ""<PATH_TO_EXE>" --minimized" /sc onlogon /rl highest /f
  ```
- When toggled OFF, execute:
  ```cmd
  schtasks /delete /tn "TibiaNetOptimizer" /f
  ```
- Check task existence on startup via `schtasks /query /tn "TibiaNetOptimizer"` to keep the UI toggle state accurate.

---

## 3. Recommended Tech Stack & Crates (`Cargo.toml`)

Keep dependencies lightweight, pure-Rust where possible, and compile to a single standalone `.exe`:

```toml
[package]
name = "tibia-net-optimizer"
version = "0.1.0"
edition = "2021"

[dependencies]
eframe = "0.29"               # Lightweight native GUI window (glow/wgpu)
egui = "0.29"
tray-icon = "0.19"            # Native Windows system tray icon + context menu
winreg = "0.52"               # Clean Windows Registry read/write
image = "0.25"                # Generate programmatic tray icons (Green/Red circles) in memory
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"            # Persist user config (auto-heal, minimized start) in %APPDATA%
windows = { version = "0.58", features = [
    "Win32_Foundation",
    "Win32_Security",
    "Win32_System_Threading",
    "Win32_NetworkManagement_IpHelper",
    "Win32_NetworkManagement_Ndis",
] }

[build-dependencies]
winres = "0.1"                # Embed Windows manifest & application icon

[profile.release]
opt-level = "z"               # Optimize for small binary size
lto = true
codegen-units = 1
strip = true
```

---

## 4. Application Architecture & UX Flow

### Directory Structure

```text
tibia-net-optimizer/
├── Cargo.toml
├── build.rs                  # Embeds Windows UAC manifest (asInvoker or requireAdministrator)
├── src/
│   ├── main.rs               # Entry point, CLI args (--minimized), eframe + tray-icon event loop
│   ├── network/
│   │   ├── mod.rs
│   │   ├── autotuning.rs     # netsh global TCP auto-tuning check/set/revert
│   │   ├── nagle.rs          # HKLM Registry interface detection & TCPNoDelay/TcpAckFrequency
│   │   └── ping.rs           # Background latency & jitter sampler
│   ├── system/
│   │   ├── mod.rs
│   │   ├── elevation.rs      # Check if running as Admin; helper to relaunch elevated if needed
│   │   └── startup.rs        # Windows Task Scheduler (/rl highest) registration & status check
│   ├── config.rs             # Load/save settings JSON in %APPDATA%\TibiaNetOptimizer\config.json
│   └── ui/
│       ├── mod.rs
│       ├── tray.rs           # System tray icon generation (Green/Red) & right-click menu
│       └── window.rs         # Main egui dashboard (Status cards, toggles, adapter list, ping stats)
```

### UAC / Privilege Strategy (Crucial UX Detail)

- Use `asInvoker` in `build.rs` so the app can always open and display status even without Admin rights.
- On launch, check `is_elevated()` via Windows Token Elevation API.
- If **not elevated**:
  - Show a prominent banner at the top of the UI: _"Running in Read-Only Mode — Click to Restart as Administrator to Apply Changes"_ (which calls `ShellExecuteW` with `"runas"`).
- If **elevated** (or launched via Task Scheduler `/rl highest`):
  - Enable all toggle switches and auto-heal features seamlessly.

### GUI Layout (`egui` Window — ~420x520px, non-resizable or clean compact layout)

1. **Header Bar:** App Title + Elevation Badge (`Admin Active` in green or `Read-Only` button in amber).
2. **Overall Status Banner:**
   - Green card: _"Optimized for Tibia — Zero Packet Bundling"_
   - Red/Amber card: _"Suboptimal Settings Detected — Click 'Optimize Now'"_
3. **Optimization Controls (Cards with Toggle Switches):**
   - **TCP Auto-Tuning (`autotuninglevel=disabled`)**: Current state + Switch + Tooltip explaining buffer jitter.
   - **Nagle's Algorithm (`TCPNoDelay` & `TcpAckFrequency`)**: Current state + Switch + List of detected active network adapters (Name/GUID + IPv4).
4. **Live Connection Monitor:**
   - Target host input (default: `secura.tibia.com` or Frankfurt endpoint) + Start/Stop Ping button.
   - Displays **Ping (ms)**, **Jitter (ms)**, and **Packet Loss (%)**.
5. **Automation Settings:**
   - Checkbox: `Auto-heal settings if Windows resets them`
   - Checkbox: `Run silently at Windows Startup (Admin via Task Scheduler)`
   - Checkbox: `Minimize to System Tray instead of closing`

### System Tray Behavior

- **Tray Icon:** Programmatically generated 16x16 / 32x32 RGBA circle icon (Green `#22C55E` when both optimizations are active; Red `#EF4444` if either is disabled).
- **Left-Click Tray Icon:** Show / bring main `egui` window to focus.
- **Right-Click Context Menu:**
  - `Status: Optimized` (disabled info item)
  - `Open Dashboard`
  - `Apply Optimizations Now`
  - `Revert to Windows Defaults`
  - `---`
  - `Quit`

---

## 5. Instructions for Claude Code (Step-by-Step Build Plan)

When executing this project in VS Code with Claude Code, follow these steps in order:

1. **Scaffold Project:** Initialize `cargo init tibia-net-optimizer` and populate `Cargo.toml` with the dependencies listed in Section 3.
2. **Implement Core Networking Modules (`src/network/`):**
   - Implement `autotuning.rs` using `std::process::Command` with `.creation_flags(0x08000000)` (`CREATE_NO_WINDOW`) so no console windows flash.
   - Implement `nagle.rs` using `winreg` to scan `HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces` for active IPv4 interfaces and manage `TcpAckFrequency` and `TCPNoDelay`.
   - Write unit tests for parsing `netsh` output and validating IPv4 strings.
3. **Implement Elevation & Task Scheduler (`src/system/`):**
   - Implement `elevation.rs` using `GetTokenInformation` (`TokenElevation`) and a `relaunch_as_admin()` helper.
   - Implement `startup.rs` wrapping `schtasks.exe` silently with `CREATE_NO_WINDOW`.
4. **Build System Tray & `egui` UI (`src/ui/`):**
   - Generate crisp anti-aliased status icons in memory using the `image` crate (no external `.ico` files required at runtime).
   - Wire `tray-icon` events into `eframe`'s update loop (`ctx.request_repaint_after(Duration::from_millis(250))`) so tray clicks and background auto-heal updates reflect immediately.
   - Intercept window close (`egui::ViewportCommand::CancelClose`) when "Minimize to Tray" is active so clicking `X` hides the window to the tray cleanly.
5. **Verify & Compile Release Build:**
   - Run `cargo build --release` and verify the single executable in `target/release/tibia-net-optimizer.exe` runs cleanly, elevates properly when requested, updates the Registry/netsh values accurately, and minimizes to the system tray.
