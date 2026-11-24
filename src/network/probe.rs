//! ICMP echo probes.
//!
//! ICMP is the only probe used against Tibia's Cloudflare entry point: it is
//! answered by the Cloudflare edge itself, whereas every TCP connect to the
//! game port would be forwarded to CipSoft's game server.
//!
//! - Windows: `IcmpSendEcho` / `Icmp6SendEcho2` (no admin rights needed)
//! - Linux/macOS: the system `ping` binary (already privileged), output parsed

use std::net::IpAddr;
use std::time::Duration;

use crate::platform::Os;

/// One echo request to `ip`; the round-trip time, or why it failed (loss).
pub fn icmp_echo(ip: IpAddr, timeout: Duration) -> Result<Duration, String> {
    #[cfg(windows)]
    {
        match ip {
            IpAddr::V4(v4) => super::icmp::echo(v4, timeout),
            IpAddr::V6(v6) => super::icmp::echo6(v6, timeout),
        }
    }
    #[cfg(not(windows))]
    {
        system_ping(ip, timeout)
    }
}

/// Readable text for Windows `IP_*` status / error codes (e.g. 11010).
pub fn ip_status_text(code: u32) -> String {
    match code {
        11001 => "reply buffer too small",
        11002 => "destination network unreachable",
        11003 => "destination host unreachable",
        11004 => "destination protocol unreachable",
        11005 => "destination port unreachable",
        11010 => "timed out",
        11013 => "TTL expired in transit",
        11050 => "general failure",
        _ => return format!("ICMP error {code}"),
    }
    .to_string()
}

/// The `ping` invocation for one echo with a timeout (Linux/macOS).
pub fn ping_command(os: Os, ip: IpAddr, timeout: Duration) -> (String, Vec<String>) {
    let secs = timeout.as_millis().div_ceil(1000).max(1).to_string();
    let addr = ip.to_string();
    let v: Vec<&str> = match (os, ip) {
        (Os::Linux, IpAddr::V4(_)) => vec!["-4", "-n", "-c", "1", "-W", &secs, &addr],
        (Os::Linux, IpAddr::V6(_)) => vec!["-6", "-n", "-c", "1", "-W", &secs, &addr],
        (Os::Macos, IpAddr::V4(_)) => vec!["-n", "-c", "1", "-t", &secs, &addr],
        // macOS ping6 has no timeout flag; `system_ping` enforces one.
        (Os::Macos, IpAddr::V6(_)) => {
            return ("ping6".into(), vec!["-n".into(), "-c".into(), "1".into(), addr])
        }
        // Windows uses the native API; listed for completeness.
        (Os::Windows, _) => {
            let ms = timeout.as_millis().to_string();
            return (
                "ping".into(),
                vec!["-n".into(), "1".into(), "-w".into(), ms, addr],
            );
        }
    };
    ("ping".into(), v.into_iter().map(String::from).collect())
}

/// Extracts the RTT from `ping` output (`time=11.2 ms`, `time<1 ms`).
pub fn parse_ping_time(output: &str) -> Option<Duration> {
    let idx = output.find("time=").or_else(|| output.find("time<"))?;
    let number: String = output[idx + 5..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let ms: f64 = number.parse().ok()?;
    Some(Duration::from_micros((ms * 1000.0).round() as u64))
}

#[cfg(not(windows))]
fn system_ping(ip: IpAddr, timeout: Duration) -> Result<Duration, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::Instant;

    let (program, args) = ping_command(Os::current(), ip, timeout);
    let mut child = Command::new(&program)
        .args(&args)
        .env("LC_ALL", "C") // stable, English output
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    // ping6 on macOS has no timeout flag, so enforce one here.
    let deadline = Instant::now() + timeout + Duration::from_millis(500);
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(_) => break,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("timed out".into());
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let mut out = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut out);
    }
    parse_ping_time(&out).ok_or_else(|| "no reply".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    const V6: &str = "2606:4700:90:0:4b6e:7f99:a2f2:fd80";

    #[test]
    fn linux_ping_commands() {
        let t = Duration::from_secs(1);
        assert_eq!(
            ping_command(Os::Linux, "1.2.3.4".parse().unwrap(), t),
            (
                "ping".to_string(),
                s(&["-4", "-n", "-c", "1", "-W", "1", "1.2.3.4"])
            )
        );
        assert_eq!(
            ping_command(Os::Linux, V6.parse().unwrap(), Duration::from_millis(2500)),
            ("ping".to_string(), s(&["-6", "-n", "-c", "1", "-W", "3", V6]))
        );
    }

    #[test]
    fn macos_ping_commands() {
        let t = Duration::from_secs(1);
        assert_eq!(
            ping_command(Os::Macos, "1.2.3.4".parse().unwrap(), t),
            ("ping".to_string(), s(&["-n", "-c", "1", "-t", "1", "1.2.3.4"]))
        );
        assert_eq!(
            ping_command(Os::Macos, V6.parse().unwrap(), t),
            ("ping6".to_string(), s(&["-n", "-c", "1", V6]))
        );
    }

    #[test]
    fn parses_ping_times() {
        let linux =
            "PING 2606:4700::1 56 data bytes\n64 bytes from 2606:4700::1: icmp_seq=1 ttl=58 time=11.2 ms\n";
        assert_eq!(parse_ping_time(linux), Some(Duration::from_micros(11_200)));
        let mac = "64 bytes from 1.2.3.4: icmp_seq=0 ttl=57 time=12.345 ms\n";
        assert_eq!(parse_ping_time(mac), Some(Duration::from_micros(12_345)));
        let integer = "16 bytes from ::1, icmp_seq=0 hlim=64 time=3 ms\n";
        assert_eq!(parse_ping_time(integer), Some(Duration::from_millis(3)));
        assert_eq!(
            parse_ping_time("64 bytes from x: time<1 ms"),
            Some(Duration::from_millis(1))
        );
    }

    #[test]
    fn windows_icmp_codes_read_as_words() {
        assert_eq!(ip_status_text(11010), "timed out");
        assert_eq!(ip_status_text(11003), "destination host unreachable");
        assert_eq!(ip_status_text(11002), "destination network unreachable");
        assert_eq!(ip_status_text(11013), "TTL expired in transit");
        assert_eq!(ip_status_text(11050), "general failure");
        assert_eq!(ip_status_text(42), "ICMP error 42");
    }

    #[test]
    fn no_time_means_no_reply() {
        assert_eq!(parse_ping_time(""), None);
        assert_eq!(
            parse_ping_time("1 packets transmitted, 0 received, 100% packet loss"),
            None
        );
        assert_eq!(parse_ping_time("time=abc ms"), None);
    }
}
