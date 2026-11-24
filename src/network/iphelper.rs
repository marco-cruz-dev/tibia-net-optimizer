//! Real [`AdapterSource`] backed by `GetAdaptersAddresses` (IPv4 only).

use std::ffi::CStr;
use std::net::Ipv4Addr;

use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_NO_DATA, NO_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
    IP_ADAPTER_ADDRESSES_LH,
};
use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows_sys::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};

use super::adapters::{AdapterSource, OsAdapter};
use crate::{Error, Result};

#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsAdapterSource;

impl AdapterSource for WindowsAdapterSource {
    fn adapters(&self) -> Result<Vec<OsAdapter>> {
        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
        let mut size: u32 = 16 * 1024;
        // u64 storage keeps the buffer suitably aligned for the structs.
        let mut buf: Vec<u64>;
        let mut attempts = 0;
        loop {
            buf = vec![0u64; (size as usize).div_ceil(8)];
            // SAFETY: `buf` is writable for `size` bytes; the API fills it with a
            // linked list of IP_ADAPTER_ADDRESSES_LH living inside the buffer.
            let rc = unsafe {
                GetAdaptersAddresses(
                    AF_INET as u32,
                    flags,
                    std::ptr::null(),
                    buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH,
                    &mut size,
                )
            };
            match rc {
                NO_ERROR => break,
                ERROR_NO_DATA => return Ok(Vec::new()),
                ERROR_BUFFER_OVERFLOW if attempts < 3 => attempts += 1,
                code => {
                    return Err(Error::Io(std::io::Error::from_raw_os_error(code as i32)));
                }
            }
        }

        let mut out = Vec::new();
        let mut cur = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        // SAFETY: walking the list the API wrote into `buf`, which outlives the loop.
        unsafe {
            while let Some(a) = cur.as_ref() {
                out.push(OsAdapter {
                    guid: CStr::from_ptr(a.AdapterName as *const _)
                        .to_string_lossy()
                        .into_owned(),
                    friendly_name: wide_to_string(a.FriendlyName),
                    is_up: a.OperStatus == IfOperStatusUp,
                    if_type: a.IfType,
                    ipv4: unicast_ipv4(a),
                });
                cur = a.Next;
            }
        }
        Ok(out)
    }
}

/// # Safety
/// `p` must be null or a NUL-terminated UTF-16 string.
unsafe fn wide_to_string(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let len = (0..).take_while(|&i| *p.add(i) != 0).count();
    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
}

/// # Safety
/// `a` must come from a successful `GetAdaptersAddresses` call.
unsafe fn unicast_ipv4(a: &IP_ADAPTER_ADDRESSES_LH) -> Vec<Ipv4Addr> {
    let mut ips = Vec::new();
    let mut u = a.FirstUnicastAddress;
    while let Some(addr) = u.as_ref() {
        let sa = addr.Address.lpSockaddr;
        if !sa.is_null() && (*sa).sa_family == AF_INET {
            let sin = &*(sa as *const SOCKADDR_IN);
            // S_addr is stored in network byte order.
            ips.push(Ipv4Addr::from(sin.sin_addr.S_un.S_addr.to_ne_bytes()));
        }
        u = addr.Next;
    }
    ips
}
