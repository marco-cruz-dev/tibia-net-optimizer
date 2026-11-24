//! `tibia-net-optimizer`: the dashboard + tray app (CLI arguments also work).
//! On Windows it is a GUI-subsystem program, so no console window flashes
//! when it starts (for example at login); use `tno` for the command line.

#![cfg_attr(all(windows, feature = "gui"), windows_subsystem = "windows")]

fn main() -> std::process::ExitCode {
    tibia_net_optimizer::entry::main(tibia_net_optimizer::entry::Mode::Gui)
}
