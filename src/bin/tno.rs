//! `tno`: the Tibia Network Optimizer command line (console program).
//! With no arguments it prints the status; `tno --help` lists the commands.

fn main() -> std::process::ExitCode {
    tibia_net_optimizer::entry::main(tibia_net_optimizer::entry::Mode::Cli)
}
