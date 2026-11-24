//! Latency probes: what to probe (target parsing), how (ICMP or TCP
//! connect), and the probers themselves.

use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::platform::Os;

/// Default monitor target: Vultr's public Frankfurt ping endpoint (answers
/// ICMP and TCP 443). `secura.tibia.com` has no DNS record.
pub const DEFAULT_TARGET: &str = "fra-de-ping.vultr.com";
pub const DEFAULT_TCP_PORT: u16 = 443;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeTarget {
    pub host: String,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeMethod {
    Icmp,
    Tcp(u16),
}

impl ProbeMethod {
    /// Short label for the UI, e.g. `ICMP` or `TCP :443`.
    pub fn label(&self) -> String {
        match self {
            ProbeMethod::Icmp => "ICMP".into(),
            ProbeMethod::Tcp(port) => format!("TCP :{port}"),
        }
    }
}

fn parse_port(p: &str) -> Result<u16, String> {
    match p.parse::<u16>() {
        Ok(port) if port != 0 => Ok(port),
        _ => Err(format!("invalid port `{p}`")),
    }
}

/// Parses `host`, `host:port`, `a.b.c.d:port`, `[v6]:port` or a bare IPv6.
pub fn parse_target(input: &str) -> Result<ProbeTarget, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("enter a host name or IP address".into());
    }
    if s.chars().any(char::is_whitespace) {
        return Err("spaces are not allowed".into());
    }
    let target = |host: &str, port| ProbeTarget {
        host: host.to_string(),
        port,
    };
    if let Some(rest) = s.strip_prefix('[') {
        let (host, after) = rest.split_once(']').ok_or("missing `]` after IPv6 address")?;
        if host.is_empty() {
            return Err("empty IPv6 address".into());
        }
        let port = match after {
            "" => None,
            p => Some(parse_port(
                p.strip_prefix(':').ok_or("expected `:port` after `]`")?,
            )?),
        };
        return Ok(target(host, port));
    }
    if s.matches(':').count() > 1 {
        // A bare IPv6 address (ports need the [v6]:port form).
        return s
            .parse::<std::net::Ipv6Addr>()
            .map(|_| target(s, None))
            .map_err(|_| format!("invalid address `{s}`"));
    }
    match s.split_once(':') {
        Some((host, port)) if !host.is_empty() => Ok(target(host, Some(parse_port(port)?))),
        Some(_) => Err("missing host name".into()),
        None => Ok(target(s, None)),
    }
}

/// A port means TCP connect; otherwise ICMP where it needs no privileges
/// (Windows), else TCP to 443.
pub fn method_for(target: &ProbeTarget, os: Os) -> ProbeMethod {
    match (target.port, os) {
        (Some(port), _) => ProbeMethod::Tcp(port),
        (None, Os::Windows) => ProbeMethod::Icmp,
        (None, _) => ProbeMethod::Tcp(DEFAULT_TCP_PORT),
    }
}

/// Resolves the target (first address, IPv4 preferred).
pub fn resolve(target: &ProbeTarget, method: ProbeMethod) -> Result<SocketAddr, String> {
    let port = match method {
        ProbeMethod::Tcp(p) => p,
        ProbeMethod::Icmp => 0,
    };
    let addrs: Vec<SocketAddr> = (target.host.as_str(), port)
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve {}: {e}", target.host))?
        .collect();
    addrs
        .iter()
        .find(|a| a.is_ipv4())
        .or_else(|| addrs.first())
        .copied()
        .ok_or_else(|| format!("no address for {}", target.host))
}

/// Time to complete a TCP handshake (≈ one round trip).
pub fn tcp_probe(addr: SocketAddr, timeout: Duration) -> Result<Duration, String> {
    let start = Instant::now();
    TcpStream::connect_timeout(&addr, timeout)
        .map(|_| start.elapsed())
        .map_err(|e| e.to_string())
}

/// One probe of `target` with the method for this OS.
pub fn probe_once(target: &ProbeTarget, timeout: Duration) -> Result<Duration, String> {
    let method = method_for(target, Os::current());
    let addr = resolve(target, method)?;
    match method {
        ProbeMethod::Tcp(_) => tcp_probe(addr, timeout),
        ProbeMethod::Icmp => icmp_probe(addr, timeout),
    }
}

#[cfg(windows)]
fn icmp_probe(addr: SocketAddr, timeout: Duration) -> Result<Duration, String> {
    match addr {
        SocketAddr::V4(v4) => super::icmp::echo(*v4.ip(), timeout),
        // IcmpSendEcho is IPv4-only; fall back to a TCP probe for IPv6.
        SocketAddr::V6(_) => tcp_probe(SocketAddr::new(addr.ip(), DEFAULT_TCP_PORT), timeout),
    }
}

#[cfg(not(windows))]
fn icmp_probe(addr: SocketAddr, timeout: Duration) -> Result<Duration, String> {
    tcp_probe(SocketAddr::new(addr.ip(), DEFAULT_TCP_PORT), timeout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn t(host: &str, port: Option<u16>) -> ProbeTarget {
        ProbeTarget {
            host: host.into(),
            port,
        }
    }

    #[test]
    fn parses_hosts_and_ports() {
        assert_eq!(
            parse_target("fra-de-ping.vultr.com"),
            Ok(t("fra-de-ping.vultr.com", None))
        );
        assert_eq!(parse_target("  1.2.3.4:7172 "), Ok(t("1.2.3.4", Some(7172))));
        assert_eq!(parse_target("[2001:db8::1]:443"), Ok(t("2001:db8::1", Some(443))));
        assert_eq!(parse_target("2001:db8::1"), Ok(t("2001:db8::1", None)));
        assert_eq!(parse_target("[2001:db8::1]"), Ok(t("2001:db8::1", None)));
    }

    #[test]
    fn rejects_invalid_targets() {
        for bad in [
            "",
            "   ",
            "host:abc",
            "host:0",
            "host:70000",
            "two words",
            "[::1",
            "host:",
        ] {
            assert!(parse_target(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn method_depends_on_port_and_os() {
        assert_eq!(
            method_for(&t("h", Some(7172)), Os::Windows),
            ProbeMethod::Tcp(7172)
        );
        assert_eq!(method_for(&t("h", Some(7172)), Os::Linux), ProbeMethod::Tcp(7172));
        assert_eq!(method_for(&t("h", None), Os::Windows), ProbeMethod::Icmp);
        assert_eq!(method_for(&t("h", None), Os::Linux), ProbeMethod::Tcp(443));
        assert_eq!(method_for(&t("h", None), Os::Macos), ProbeMethod::Tcp(443));
    }

    #[test]
    fn method_labels() {
        assert_eq!(ProbeMethod::Icmp.label(), "ICMP");
        assert_eq!(ProbeMethod::Tcp(443).label(), "TCP :443");
    }

    #[test]
    fn tcp_probe_measures_a_local_handshake() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let rtt = tcp_probe(listener.local_addr().unwrap(), Duration::from_secs(1)).unwrap();
        assert!(rtt < Duration::from_secs(1), "{rtt:?}");
    }

    #[test]
    fn tcp_probe_reports_refused_connections() {
        // Bind then drop to get a port that is (almost certainly) closed.
        let addr = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        assert!(tcp_probe(addr, Duration::from_millis(500)).is_err());
    }

    #[test]
    fn resolve_handles_literal_addresses() {
        let a = resolve(&t("127.0.0.1", None), ProbeMethod::Tcp(80)).unwrap();
        assert_eq!(a, "127.0.0.1:80".parse().unwrap());
    }
}
