//! Real Windows registry adapter for [`InterfaceRegistry`] (via `winreg`).
//! Reading works as a standard user; writing requires Administrator.

use std::io;

use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE};
use winreg::RegKey;

use super::nagle::{InterfaceRegistry, InterfaceSnapshot, TCP_ACK_FREQUENCY, TCP_NO_DELAY};
use crate::Result;

pub const INTERFACES_PATH: &str = r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces";

#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsInterfaceRegistry;

impl WindowsInterfaceRegistry {
    fn open_interface(guid: &str, flags: u32) -> io::Result<RegKey> {
        RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(format!("{INTERFACES_PATH}\\{guid}"), flags)
    }
}

impl InterfaceRegistry for WindowsInterfaceRegistry {
    fn interfaces(&self) -> Result<Vec<InterfaceSnapshot>> {
        let root = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(INTERFACES_PATH, KEY_READ)?;
        let mut out = Vec::new();
        for guid in root.enum_keys() {
            let guid = guid?;
            let Ok(key) = root.open_subkey_with_flags(&guid, KEY_READ) else {
                continue;
            };
            out.push(InterfaceSnapshot {
                dhcp_ip_address: key.get_value::<String, _>("DhcpIPAddress").ok(),
                ip_addresses: key.get_value::<Vec<String>, _>("IPAddress").unwrap_or_default(),
                tcp_ack_frequency: key.get_value::<u32, _>(TCP_ACK_FREQUENCY).ok(),
                tcp_no_delay: key.get_value::<u32, _>(TCP_NO_DELAY).ok(),
                guid,
            });
        }
        Ok(out)
    }

    fn set_dword(&self, guid: &str, name: &str, value: u32) -> Result<()> {
        Self::open_interface(guid, KEY_SET_VALUE)?.set_value(name, &value)?;
        Ok(())
    }

    fn delete_value(&self, guid: &str, name: &str) -> Result<()> {
        match Self::open_interface(guid, KEY_SET_VALUE)?.delete_value(name) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => Ok(other?),
        }
    }
}
