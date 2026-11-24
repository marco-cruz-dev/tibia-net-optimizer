//! Finding the running Tibia client's game connection.
//!
//! The client connects to a Cloudflare Spectrum anycast address on the game
//! port (measured: `[2606:4700:…]:7171`). Its other connections (443) are the
//! website. Detection is read-only: the OS connection table plus process
//! names, the same data `netstat` and Task Manager show. The client process
//! itself is never opened.

use std::net::{IpAddr, SocketAddr};

use crate::Result;

/// Tibia game/login ports.
pub const GAME_PORTS: [u16; 2] = [7171, 7172];

/// An established TCP connection and the process that owns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedConn {
    pub pid: u32,
    /// Executable name, e.g. `client.exe` (Windows) or `client` (Unix).
    pub process: String,
    pub remote: SocketAddr,
}

/// Port over the OS connection table.
pub trait ConnectionSource {
    /// Established TCP connections (IPv4 and IPv6) with owning process names.
    fn established(&self) -> Result<Vec<OwnedConn>>;
}

/// The Tibia client is called `client` / `client.exe` (its launcher is
/// `client_launcher`).
pub fn is_tibia_client(process: &str) -> bool {
    matches!(process.to_ascii_lowercase().as_str(), "client" | "client.exe")
}

/// Remote addresses of all the Tibia client's game-port connections, in table
/// order, without duplicates. There can be several: on 2026-11-24 the client
/// held one to a Cloudflare edge (answers ICMP) *and* one to an AWS Frankfurt
/// host (blocks ICMP), so the caller picks one that answers.
pub fn game_connections(conns: &[OwnedConn]) -> Vec<IpAddr> {
    let mut out: Vec<IpAddr> = Vec::new();
    for c in conns {
        let ip = c.remote.ip();
        if is_tibia_client(&c.process) && GAME_PORTS.contains(&c.remote.port()) && !out.contains(&ip) {
            out.push(ip);
        }
    }
    out
}

/// `0100007F:1C03` or a 32-hex-digit IPv6 address with port.
fn parse_hex_addr(s: &str) -> Option<SocketAddr> {
    let (addr, port) = s.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;
    let word = |i: usize| u32::from_str_radix(addr.get(i * 8..i * 8 + 8)?, 16).ok();
    let ip = match addr.len() {
        8 => IpAddr::V4(std::net::Ipv4Addr::from(word(0)?.to_le_bytes())),
        32 => {
            let mut bytes = [0u8; 16];
            for i in 0..4 {
                bytes[i * 4..i * 4 + 4].copy_from_slice(&word(i)?.to_le_bytes());
            }
            IpAddr::V6(std::net::Ipv6Addr::from(bytes))
        }
        _ => return None,
    };
    Some(SocketAddr::new(ip, port))
}

/// An established connection parsed from `/proc/net/tcp` or `/proc/net/tcp6`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcNetEntry {
    pub remote: SocketAddr,
    pub inode: u64,
}

/// Parses `/proc/net/tcp(6)` and keeps only ESTABLISHED (`01`) rows.
/// Addresses are hex, stored as 32-bit words in host (little-endian) order.
pub fn parse_proc_net_tcp(text: &str) -> Vec<ProcNetEntry> {
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 10 || f[3] != "01" {
                return None; // header, short line or not ESTABLISHED
            }
            Some(ProcNetEntry {
                remote: parse_hex_addr(f[2])?,
                inode: f[9].parse().ok()?,
            })
        })
        .collect()
}

/// Parses `lsof -nP -iTCP -sTCP:ESTABLISHED -Fpcn` (macOS) output.
pub fn parse_lsof(text: &str) -> Vec<OwnedConn> {
    let mut out = Vec::new();
    let (mut pid, mut process) = (None, String::new());
    for line in text.lines() {
        let value = line.get(1..).unwrap_or("");
        match line.chars().next() {
            Some('p') => {
                pid = value.parse().ok();
                process.clear();
            }
            Some('c') => process = value.to_string(),
            Some('n') => {
                let remote = value
                    .split_once("->")
                    .and_then(|(_, r)| r.parse::<SocketAddr>().ok());
                if let (Some(pid), Some(remote)) = (pid, remote) {
                    out.push(OwnedConn {
                        pid,
                        process: process.clone(),
                        remote,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

pub const LSOF: &str = "/usr/sbin/lsof";

/// macOS connection source (`lsof`), generic over the command runner.
pub struct LsofConnections<R> {
    pub runner: R,
}

impl<R: crate::system::command::CommandRunner> ConnectionSource for LsofConnections<R> {
    fn established(&self) -> Result<Vec<OwnedConn>> {
        let out = self
            .runner
            .run(LSOF, &["-nP", "-iTCP", "-sTCP:ESTABLISHED", "-Fpcn"])?;
        // lsof exits 1 when nothing matches; that just means no connections.
        if !out.success && out.stdout.trim().is_empty() {
            return Ok(Vec::new());
        }
        Ok(parse_lsof(&out.stdout))
    }
}

/// Linux connection source: `/proc/net/tcp{,6}` plus `/proc/<pid>/fd` socket
/// inodes for ownership (the user's own processes are readable).
#[cfg(target_os = "linux")]
pub struct ProcConnections;

#[cfg(target_os = "linux")]
impl ConnectionSource for ProcConnections {
    fn established(&self) -> Result<Vec<OwnedConn>> {
        use std::collections::HashMap;
        use std::fs;

        let mut entries = Vec::new();
        for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
            if let Ok(text) = fs::read_to_string(table) {
                entries.extend(parse_proc_net_tcp(&text));
            }
        }
        if entries.is_empty() {
            return Ok(Vec::new());
        }
        let wanted: std::collections::HashSet<u64> = entries.iter().map(|e| e.inode).collect();
        let mut owner: HashMap<u64, (u32, String)> = HashMap::new();
        for proc_dir in fs::read_dir("/proc")?.flatten() {
            let Ok(pid) = proc_dir.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let Ok(fds) = fs::read_dir(proc_dir.path().join("fd")) else {
                continue; // other users' processes
            };
            for fd in fds.flatten() {
                let Ok(link) = fs::read_link(fd.path()) else {
                    continue;
                };
                let link = link.to_string_lossy();
                let Some(inode) = link
                    .strip_prefix("socket:[")
                    .and_then(|s| s.strip_suffix(']'))
                    .and_then(|s| s.parse::<u64>().ok())
                else {
                    continue;
                };
                if wanted.contains(&inode) {
                    let comm = fs::read_to_string(proc_dir.path().join("comm")).unwrap_or_default();
                    owner.insert(inode, (pid, comm.trim().to_string()));
                }
            }
        }
        Ok(entries
            .into_iter()
            .filter_map(|e| {
                owner.get(&e.inode).map(|(pid, process)| OwnedConn {
                    pid: *pid,
                    process: process.clone(),
                    remote: e.remote,
                })
            })
            .collect())
    }
}

/// The connection source for the running OS.
pub fn system_connections() -> Box<dyn ConnectionSource + Send> {
    #[cfg(windows)]
    return Box::new(super::conntable::WindowsConnections);
    #[cfg(target_os = "linux")]
    return Box::new(ProcConnections);
    #[cfg(target_os = "macos")]
    return Box::new(LsofConnections {
        runner: crate::system::command::SystemCommandRunner,
    });
}

/// Detects the game connection's remote address (best effort).
pub fn detect_game_targets(source: &dyn ConnectionSource) -> Vec<IpAddr> {
    source
        .established()
        .map(|c| game_connections(&c))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    fn conn(pid: u32, process: &str, remote: &str) -> OwnedConn {
        OwnedConn {
            pid,
            process: process.into(),
            remote: remote.parse().unwrap(),
        }
    }

    const PROXY: &str = "2606:4700:90:0:4b6e:7f99:a2f2:fd80";

    #[test]
    fn recognises_the_client_but_not_the_launcher() {
        for name in ["client.exe", "client", "Client.exe"] {
            assert!(is_tibia_client(name), "{name}");
        }
        for name in [
            "client_launcher.exe",
            "LeagueClient.exe",
            "RiotClientServices.exe",
            "clients",
        ] {
            assert!(!is_tibia_client(name), "{name}");
        }
    }

    #[test]
    fn picks_the_game_port_connection_of_the_client() {
        // Mirrors what was measured while playing on Secura.
        let conns = vec![
            conn(36400, "client.exe", "[2606:4700::6812:7208]:443"),
            conn(36400, "client.exe", &format!("[{PROXY}]:7171")),
            conn(4696, "LeagueClient.exe", "[2606:4700:440b::ac40:9249]:443"),
        ];
        assert_eq!(game_connections(&conns), vec![PROXY.parse::<IpAddr>().unwrap()]);
    }

    #[test]
    fn keeps_every_game_connection_in_table_order() {
        // Measured later the same day: Windows lists IPv4 before IPv6, so the
        // ICMP-blocking AWS host comes first.
        let conns = vec![
            conn(26724, "client.exe", "18.192.74.45:7171"),
            conn(26724, "client.exe", &format!("[{PROXY}]:7171")),
            conn(26724, "client.exe", &format!("[{PROXY}]:7171")), // duplicate
            conn(26724, "client.exe", "[2606:4700::6812:7208]:443"),
        ];
        assert_eq!(
            game_connections(&conns),
            vec!["18.192.74.45".parse::<IpAddr>().unwrap(), PROXY.parse().unwrap()]
        );
    }

    #[test]
    fn ignores_other_processes_on_game_ports_and_website_traffic() {
        let conns = vec![
            conn(1, "otclient.exe", "1.2.3.4:7171"),
            conn(2, "client.exe", "104.18.185.8:443"),
        ];
        assert!(game_connections(&conns).is_empty());
        assert!(game_connections(&[]).is_empty());
    }

    #[test]
    fn accepts_ipv4_game_connections_and_port_7172() {
        let conns = vec![conn(9, "client", "198.51.100.7:7172")];
        assert_eq!(
            game_connections(&conns),
            vec!["198.51.100.7".parse::<IpAddr>().unwrap()]
        );
    }

    #[test]
    fn parses_proc_net_tcp_ipv4() {
        // 0100007F = 127.0.0.1 ; 0x1C03 = 7171 ; st 01 = ESTABLISHED, 0A = LISTEN
        let text = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 11111 1 0000000000000000 100 0 0 10 0\n\
   1: 0A00A8C0:D2F4 0100007F:1C03 01 00000000:00000000 00:00000000 00000000  1000        0 22222 1 0000000000000000 20 4 30 10 -1\n";
        assert_eq!(
            parse_proc_net_tcp(text),
            vec![ProcNetEntry {
                remote: "127.0.0.1:7171".parse().unwrap(),
                inode: 22222
            }]
        );
    }

    #[test]
    fn parses_proc_net_tcp6() {
        // 2606:4700:90:0:4b6e:7f99:a2f2:fd80 as four 32-bit words, each printed
        // as a little-endian integer: 26064700 → 00470626, 00900000 → 00009000,
        // 4b6e7f99 → 997F6E4B, a2f2fd80 → 80FDF2A2.
        let remote_hex = "0047062600009000997F6E4B80FDF2A2";
        let text = format!(
            "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 00000000000000000000000001000000:A1B2 {remote_hex}:1C03 01 00000000:00000000 00:00000000 00000000  1000        0 33333 1 0000000000000000 20 4 30 10 -1\n"
        );
        let entries = parse_proc_net_tcp(&text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].inode, 33333);
        assert_eq!(
            entries[0].remote,
            SocketAddr::new(IpAddr::V6(PROXY.parse::<Ipv6Addr>().unwrap()), 7171)
        );
    }

    #[test]
    fn proc_net_tcp_ignores_garbage() {
        assert!(parse_proc_net_tcp("").is_empty());
        assert!(parse_proc_net_tcp("header only\nnot a row\n").is_empty());
    }

    #[test]
    fn lsof_source_runs_lsof_and_treats_no_match_as_empty() {
        use crate::system::command::fake::FakeRunner;
        let runner =
            FakeRunner::new().respond_ok(&format!("p777\ncclient\nn[2001:db8::2]:5->[{PROXY}]:7171\n"));
        let src = LsofConnections { runner: &runner };
        assert_eq!(detect_game_targets(&src), vec![PROXY.parse::<IpAddr>().unwrap()]);
        assert_eq!(
            runner.calls(),
            vec![vec![LSOF, "-nP", "-iTCP", "-sTCP:ESTABLISHED", "-Fpcn"]]
        );
        let none = FakeRunner::new().respond_fail("");
        assert_eq!(LsofConnections { runner: &none }.established().unwrap(), vec![]);
    }

    #[test]
    fn parses_lsof_field_output() {
        let text = format!(
            "p501\nccom.apple.Safari\nn192.168.1.2:50000->17.0.0.1:443\n\
             p777\ncclient\nn[2001:db8::2]:50123->[{PROXY}]:7171\nn192.168.1.2:50124->104.18.185.8:443\n"
        );
        assert_eq!(
            parse_lsof(&text),
            vec![
                conn(501, "com.apple.Safari", "17.0.0.1:443"),
                conn(777, "client", &format!("[{PROXY}]:7171")),
                conn(777, "client", "104.18.185.8:443"),
            ]
        );
    }
}
