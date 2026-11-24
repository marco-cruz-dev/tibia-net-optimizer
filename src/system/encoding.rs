//! Decoding console tool output (`netsh` writes the OEM code page, not UTF-8).

/// UTF-8 if the bytes are valid UTF-8, otherwise the system OEM code page
/// (Windows) or lossy UTF-8 (elsewhere).
pub fn decode_console_output(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    #[cfg(windows)]
    {
        // SAFETY: GetOEMCP has no preconditions.
        let oem = unsafe { windows_sys::Win32::Globalization::GetOEMCP() };
        decode_with_codepage(bytes, oem)
    }
    #[cfg(not(windows))]
    {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Decodes `bytes` from Windows code page `codepage` (e.g. 850).
#[cfg(windows)]
pub fn decode_with_codepage(bytes: &[u8], codepage: u32) -> String {
    use windows_sys::Win32::Globalization::MultiByteToWideChar;

    if bytes.is_empty() {
        return String::new();
    }
    let Ok(len) = i32::try_from(bytes.len()) else {
        return String::from_utf8_lossy(bytes).into_owned();
    };
    // SAFETY: first call sizes the output; second writes into a buffer of
    // exactly that many UTF-16 units.
    unsafe {
        let needed = MultiByteToWideChar(codepage, 0, bytes.as_ptr(), len, std::ptr::null_mut(), 0);
        if needed <= 0 {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        let mut wide = vec![0u16; needed as usize];
        let written = MultiByteToWideChar(codepage, 0, bytes.as_ptr(), len, wide.as_mut_ptr(), needed);
        String::from_utf16_lossy(&wide[..written.max(0) as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_utf8_passes_through() {
        assert_eq!(
            decode_console_output("Nível : disabled".as_bytes()),
            "Nível : disabled"
        );
        assert_eq!(decode_console_output(b""), "");
    }

    #[cfg(windows)]
    #[test]
    fn decodes_explicit_oem_codepage() {
        // 0xA1 is 'í' in CP850 (and CP437/CP860).
        assert_eq!(
            decode_with_codepage(&[b'N', 0xA1, b'v', b'e', b'l'], 850),
            "Nível"
        );
        assert_eq!(decode_with_codepage(b"", 850), "");
    }

    #[cfg(windows)]
    #[test]
    fn invalid_utf8_is_decoded_as_oem_not_replacement_chars() {
        let s = decode_console_output(&[b'N', 0xA1, b'v', b'e', b'l']);
        assert!(!s.contains('\u{FFFD}'), "got {s:?}");
        assert!(s.starts_with('N') && s.ends_with("vel"));
    }
}
