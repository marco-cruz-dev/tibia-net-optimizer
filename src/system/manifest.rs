//! The Windows application manifest embedded by `build.rs` (which includes
//! this file with `#[path]`, so it must stay dependency-free).
//!
//! - `asInvoker`: always start without a UAC prompt, read-only until the user
//!   chooses "Restart as Administrator" (or the elevated logon task runs).
//! - PerMonitorV2 DPI awareness: sharp text on high-DPI and mixed monitors.
//! - Windows 10/11 compatibility GUID.
//! - Common Controls v6: themed native menus/dialogs (tray menu).
//!
//! Deliberately *no* `activeCodePage=UTF-8`: it would change the process OEM
//! code page and break decoding of `netsh` output.

pub const APP_MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="TibiaNetOptimizer" version="1.0.0.0"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <!-- Windows 10 and 11 -->
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0"
        processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_as_invoker_without_ui_access() {
        assert!(APP_MANIFEST.contains(r#"level="asInvoker""#));
        assert!(APP_MANIFEST.contains(r#"uiAccess="false""#));
        assert!(!APP_MANIFEST.contains("requireAdministrator"));
    }

    #[test]
    fn is_dpi_aware_and_targets_windows_10() {
        assert!(APP_MANIFEST.contains("<dpiAwareness"));
        assert!(APP_MANIFEST.contains("PerMonitorV2"));
        assert!(APP_MANIFEST.contains("{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"));
    }

    #[test]
    fn uses_common_controls_v6_and_not_utf8_codepage() {
        assert!(APP_MANIFEST.contains(r#"name="Microsoft.Windows.Common-Controls""#));
        assert!(APP_MANIFEST.contains(r#"version="6.0.0.0""#));
        assert!(!APP_MANIFEST.contains("activeCodePage"));
    }

    #[test]
    fn is_a_well_formed_assembly() {
        assert!(APP_MANIFEST.starts_with("<?xml"));
        assert!(APP_MANIFEST.trim_end().ends_with("</assembly>"));
        assert_eq!(APP_MANIFEST.matches("<assembly ").count(), 1, "one root element");
        assert_eq!(APP_MANIFEST.matches("</assembly>").count(), 1);
    }
}
