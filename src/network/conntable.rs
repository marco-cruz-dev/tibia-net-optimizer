//! Windows [`ConnectionSource`]: established TCP connections from
//! `GetExtendedTcpTable` (IPv4 + IPv6, with owning PIDs) and process names
//! from a Toolhelp snapshot. Read-only; no process handles are opened, so
//! protected processes (the Tibia client runs under BattlEye) are fine.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_INSUFFICIENT_BUFFER, INVALID_HANDLE_VALUE, NO_ERROR,
};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID, MIB_TCP_STATE_ESTAB,
    TCP_TABLE_OWNER_PID_CONNECTIONS,
};
use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

use super::gameconn::{ConnectionSource, OwnedConn};
use crate::{Error, Result};

#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsConnections;

/// Raw table bytes for one address family (u64 storage for alignment).
fn tcp_table(family: u16) -> Result<Vec<u64>> {
    let mut size: u32 = 16 * 1024;
    for _ in 0..4 {
        let mut buf = vec![0u64; (size as usize).div_ceil(8)];
        // SAFETY: `buf` is writable for `size` bytes.
        let rc = unsafe {
            GetExtendedTcpTable(
                buf.as_mut_ptr().cast(),
                &mut size,
                0,
                family as u32,
                TCP_TABLE_OWNER_PID_CONNECTIONS,
                0,
            )
        };
        match rc {
            NO_ERROR => return Ok(buf),
            ERROR_INSUFFICIENT_BUFFER => continue, // `size` now holds the need
            code => return Err(Error::Io(std::io::Error::from_raw_os_error(code as i32))),
        }
    }
    Err(Error::Parse("TCP table kept growing".into()))
}

/// Rows of a `MIB_TCP(6)TABLE_OWNER_PID`: a u32 count followed by the rows.
///
/// # Safety
/// `buf` must hold a table of `T` rows written by `GetExtendedTcpTable`.
unsafe fn rows<T: Copy>(buf: &[u64]) -> Vec<T> {
    let base = buf.as_ptr() as *const u8;
    let count = *(base as *const u32) as usize;
    // Rows start after the count, aligned for T.
    let offset = std::mem::align_of::<T>().max(4);
    let first = base.add(offset) as *const T;
    (0..count).map(|i| *first.add(i)).collect()
}

fn port(raw: u32) -> u16 {
    u16::from_be(raw as u16)
}

/// pid → executable name for all processes.
fn process_names() -> HashMap<u32, String> {
    let mut names = HashMap::new();
    // SAFETY: snapshot handle checked and closed; entry sized via dwSize.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return names;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut entry);
        while ok != 0 {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            names.insert(
                entry.th32ProcessID,
                String::from_utf16_lossy(&entry.szExeFile[..len]),
            );
            ok = Process32NextW(snap, &mut entry);
        }
        CloseHandle(snap);
    }
    names
}

impl ConnectionSource for WindowsConnections {
    fn established(&self) -> Result<Vec<OwnedConn>> {
        let names = process_names();
        let name_of = |pid: u32| names.get(&pid).cloned().unwrap_or_default();
        let mut out = Vec::new();

        let v4 = tcp_table(AF_INET)?;
        // SAFETY: table written by GetExtendedTcpTable for AF_INET.
        for r in unsafe { rows::<MIB_TCPROW_OWNER_PID>(&v4) } {
            if r.dwState == MIB_TCP_STATE_ESTAB as u32 {
                let ip = Ipv4Addr::from(r.dwRemoteAddr.to_ne_bytes());
                out.push(OwnedConn {
                    pid: r.dwOwningPid,
                    process: name_of(r.dwOwningPid),
                    remote: SocketAddr::new(IpAddr::V4(ip), port(r.dwRemotePort)),
                });
            }
        }

        let v6 = tcp_table(AF_INET6)?;
        // SAFETY: table written by GetExtendedTcpTable for AF_INET6.
        for r in unsafe { rows::<MIB_TCP6ROW_OWNER_PID>(&v6) } {
            if r.dwState == MIB_TCP_STATE_ESTAB as u32 {
                let ip = Ipv6Addr::from(r.ucRemoteAddr);
                out.push(OwnedConn {
                    pid: r.dwOwningPid,
                    process: name_of(r.dwOwningPid),
                    remote: SocketAddr::new(IpAddr::V6(ip), port(r.dwRemotePort)),
                });
            }
        }
        Ok(out)
    }
}
