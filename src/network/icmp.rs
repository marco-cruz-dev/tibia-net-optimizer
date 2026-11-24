//! ICMP echo via `IcmpSendEcho` (Windows; no admin rights needed).

use std::net::Ipv4Addr;
use std::time::Duration;

use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::NetworkManagement::IpHelper::{
    IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY,
};

/// `IP_SUCCESS`
const IP_SUCCESS: u32 = 0;

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
            return Err(format!("no reply ({error})"));
        }
        let r = &*(reply.as_ptr() as *const ICMP_ECHO_REPLY);
        if r.Status != IP_SUCCESS {
            return Err(format!("ICMP status {}", r.Status));
        }
        Ok(Duration::from_millis(r.RoundTripTime as u64))
    }
}
