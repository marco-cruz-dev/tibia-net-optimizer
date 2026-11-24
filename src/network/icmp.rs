//! ICMP echo via `IcmpSendEcho` / `Icmp6SendEcho2` (Windows; no admin
//! rights needed).

use std::net::{Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::NetworkManagement::IpHelper::{
    Icmp6CreateFile, Icmp6SendEcho2, IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMPV6_ECHO_REPLY_LH,
    ICMP_ECHO_REPLY,
};
use windows_sys::Win32::Networking::WinSock::{AF_INET6, SOCKADDR_IN6};

use super::probe::ip_status_text;

/// `IP_SUCCESS`
const IP_SUCCESS: u32 = 0;

/// IP Helper reports failures through `GetLastError` with `IP_*` codes
/// (e.g. 11010 = timed out), which `io::Error` can't name.
fn describe(error: std::io::Error) -> String {
    match error.raw_os_error() {
        Some(code) if (11000..12000).contains(&code) => ip_status_text(code as u32),
        _ => error.to_string(),
    }
}

/// Sends one echo request; returns the round-trip time (1 ms resolution).
pub fn echo(ip: Ipv4Addr, timeout: Duration) -> Result<Duration, String> {
    let payload = *b"tibia-net-optimizer";
    // Reply buffer: one ICMP_ECHO_REPLY + payload + 8 bytes for an ICMP error.
    let mut reply = vec![0u8; std::mem::size_of::<ICMP_ECHO_REPLY>() + payload.len() + 8];
    let timeout_ms = timeout.as_millis().clamp(1, u32::MAX as u128) as u32;

    // SAFETY: the handle is checked and always closed; buffers are sized as
    // declared and outlive the call; the reply is only read when count > 0.
    unsafe {
        let handle = IcmpCreateFile();
        if handle == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let count = IcmpSendEcho(
            handle,
            u32::from_ne_bytes(ip.octets()), // network byte order in memory
            payload.as_ptr().cast(),
            payload.len() as u16,
            std::ptr::null(),
            reply.as_mut_ptr().cast(),
            reply.len() as u32,
            timeout_ms,
        );
        let error = std::io::Error::last_os_error();
        IcmpCloseHandle(handle);
        if count == 0 {
            return Err(describe(error));
        }
        let r = &*(reply.as_ptr() as *const ICMP_ECHO_REPLY);
        if r.Status != IP_SUCCESS {
            return Err(ip_status_text(r.Status));
        }
        Ok(Duration::from_millis(r.RoundTripTime as u64))
    }
}

fn sockaddr6(ip: Ipv6Addr) -> SOCKADDR_IN6 {
    // SAFETY: SOCKADDR_IN6 is plain old data; all-zero is a valid value.
    let mut sa: SOCKADDR_IN6 = unsafe { std::mem::zeroed() };
    sa.sin6_family = AF_INET6;
    sa.sin6_addr.u.Byte = ip.octets();
    sa
}

/// ICMPv6 echo (the Tibia client reaches Cloudflare over IPv6).
pub fn echo6(ip: Ipv6Addr, timeout: Duration) -> Result<Duration, String> {
    let payload = *b"tibia-net-optimizer";
    // ICMPV6_ECHO_REPLY + payload + 8 bytes ICMP header + room for IO_STATUS_BLOCK.
    let mut reply = vec![0u8; std::mem::size_of::<ICMPV6_ECHO_REPLY_LH>() + payload.len() + 8 + 32];
    let timeout_ms = timeout.as_millis().clamp(1, u32::MAX as u128) as u32;
    let source = sockaddr6(Ipv6Addr::UNSPECIFIED);
    let dest = sockaddr6(ip);

    // SAFETY: as for `echo`; addresses and buffers outlive the synchronous
    // call (no event/APC is passed, so it completes before returning).
    unsafe {
        let handle = Icmp6CreateFile();
        if handle == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let count = Icmp6SendEcho2(
            handle,
            std::ptr::null_mut(),
            None,
            std::ptr::null(),
            &source,
            &dest,
            payload.as_ptr().cast(),
            payload.len() as u16,
            std::ptr::null(),
            reply.as_mut_ptr().cast(),
            reply.len() as u32,
            timeout_ms,
        );
        let error = std::io::Error::last_os_error();
        IcmpCloseHandle(handle);
        if count == 0 {
            return Err(describe(error));
        }
        let r = &*(reply.as_ptr() as *const ICMPV6_ECHO_REPLY_LH);
        if r.Status != IP_SUCCESS {
            return Err(ip_status_text(r.Status));
        }
        Ok(Duration::from_millis(r.RoundTripTime as u64))
    }
}
