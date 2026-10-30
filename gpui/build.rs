//! Embed the Windows application icon and manifest (windows/app.rc →
//! windows/app.ico + windows/app.manifest). MSVC host/target: rc.exe via
//! embed-resource (locates the Windows SDK itself). GNU (cross or native
//! mingw): windres → COFF .o → rustc link-arg.
//!
//! The manifest is ours because gpui-base enables gpui-fast's
//! `windows-manifest-provided-by-host`: the host app must supply Common
//! Controls v6 and PerMonitorV2 DPI awareness itself.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }

    let windows_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).join("windows");
    let rc = windows_dir.join("app.rc");
    println!("cargo:rerun-if-changed={}", rc.display());
    println!(
        "cargo:rerun-if-changed={}",
        windows_dir.join("app.ico").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        windows_dir.join("app.manifest").display()
    );

    #[cfg(target_os = "windows")]
    if target.ends_with("-msvc") {
        // The .rc carries the manifest; Failed must still panic.
        embed_resource::compile(&rc, embed_resource::NONE)
            .manifest_optional()
            .unwrap();
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let obj = out_dir.join("app_icon.o");

    let windres = if target.starts_with("x86_64-pc-windows-gnu")
        || target.starts_with("x86_64-w64-windows-gnu")
    {
        "x86_64-w64-mingw32-windres"
    } else if target.starts_with("i686-") {
        "i686-w64-mingw32-windres"
    } else {
        "windres"
    };

    let status = Command::new(windres)
        .current_dir(&windows_dir)
        .arg("--input")
        .arg("app.rc")
        .arg("--output")
        .arg(&obj)
        .arg("--output-format=coff")
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn {windres}: {e}"));
    if !status.success() {
        panic!("{windres} failed with {status}");
    }

    println!("cargo:rustc-link-arg={}", obj.display());
}
