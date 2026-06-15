// Application manifest requesting administrator privileges. Required so that
// PresentMon/ETW can read performance telemetry for all processes.
const ADMIN_MANIFEST: &str = r#"
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity
    version="1.0.0.0"
    processorArchitecture="*"
    name="EasyFPS"
    type="win32"
  />
  <description>EasyFPS - Lightweight FPS Counter</description>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <!-- Windows 10 and Windows 11 -->
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>
"#;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "windows" {
        return;
    }

    let mut res = winres::WindowsResource::new();

    // Set icon if it exists
    if std::path::Path::new("assets/icon.ico").exists() {
        res.set_icon("assets/icon.ico");
    }

    // The admin manifest is only embedded in release builds (the shipped
    // artifact). winres links the manifest into *every* binary of this package,
    // including the unit-test harness, and a `requireAdministrator` binary
    // cannot be launched by `cargo test` without elevation (error 740).
    // Embedding it only for release keeps `cargo test` working while ensuring
    // the distributed executable still requests administrator rights.
    //
    // Set EASYFPS_FORCE_MANIFEST=1 to embed it in debug builds too.
    let profile = std::env::var("PROFILE").unwrap_or_default();
    let force = std::env::var_os("EASYFPS_FORCE_MANIFEST").is_some();
    if profile == "release" || force {
        res.set_manifest(ADMIN_MANIFEST);
    }

    res.compile().expect("Failed to compile Windows resources");
}
