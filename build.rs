//! Windows resources: the shield icon (rendered from the same code as the
//! window/tray icons), the application manifest and version info.
//! Does nothing for other targets.

use std::path::PathBuf;

#[path = "src/app/icon_raster.rs"]
#[allow(dead_code)]
mod icon_raster;
#[path = "src/system/manifest.rs"]
#[allow(dead_code)]
mod manifest;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/app/icon_raster.rs");
    println!("cargo:rerun-if-changed=src/system/manifest.rs");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let images: Vec<(u32, Vec<u8>)> = icon_raster::ICO_SIZES
        .iter()
        .map(|&size| (size, icon_raster::render(size, None)))
        .collect();
    let ico_path = out.join("app.ico");
    std::fs::write(&ico_path, icon_raster::ico_file(&images)).expect("write app.ico");
    let manifest_path = out.join("app.manifest");
    std::fs::write(&manifest_path, manifest::APP_MANIFEST).expect("write app.manifest");

    let mut res = winresource::WindowsResource::new();
    res.set_icon(ico_path.to_str().expect("utf-8 path"))
        .set_manifest_file(manifest_path.to_str().expect("utf-8 path"))
        .set("ProductName", "Tibia Network Optimizer")
        .set("FileDescription", "Tibia Network Optimizer")
        .set("LegalCopyright", "GPL-3.0-only");
    res.compile()
        .expect("compile Windows resources (needs rc.exe from the Windows SDK)");
}
