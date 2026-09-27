//! Embed Windows resources: the application icon and VERSIONINFO.
//! Gates on `CARGO_CFG_TARGET_OS` (the target), not `cfg!(target_os)` — the
//! build script itself always compiles for the host, so Linux→MSVC cross
//! builds (cargo-xwin) embed resources exactly like a native Windows build.
//!
//! Do **not** embed `windows/app.manifest` here: `gpui-pre` (via the
//! `windows-manifest` feature force-enabled by gpui-pre-platform) already
//! embeds RT_MANIFEST id=1 with Common Controls v6 + PerMonitorV2. A second
//! id=1 resource fails to link on MSVC with CVTRES CVT1100 / LNK1123 (see
//! agenda-gpui/memoria-gpui's gpui-pre patch notes). Keep `windows/app.manifest`
//! in-tree as the documented product manifest; gpui's embedded copy covers
//! runtime.
//!
//! `KOSMOS_DICTATION_VERSION` (X.Y.Z) overrides the stamped version: the
//! Cortex component build sets it to the desktop release version so the
//! packaged `Kosmos Dictation.exe` reports the same version as the
//! installer. Falls back to `CARGO_PKG_VERSION` for standalone builds.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let windows_dir = manifest_dir.join("windows");
    println!(
        "cargo:rerun-if-changed={}",
        windows_dir.join("app.ico").display()
    );
    println!("cargo:rerun-if-env-changed=KOSMOS_DICTATION_VERSION");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let rc = out_dir.join("dictation.rc");
    std::fs::write(&rc, resource_script(&windows_dir)).expect("write Dictation resource script");

    let target = env::var("TARGET").unwrap_or_default();
    if target.ends_with("-msvc") {
        // MSVC (native or cargo-xwin): llvm-rc/rc.exe via embed-resource.
        // Icon + VERSIONINFO only — no RT_MANIFEST (gpui-pre owns id=1).
        // `.manifest_required()` here means "compilation must succeed" (embed-resource
        // API), not "RC must contain RT_MANIFEST".
        embed_resource::compile(&rc, embed_resource::NONE)
            .manifest_required()
            .expect("embed Dictation icon and version resources");
        return;
    }

    // GNU (cross or native mingw): windres → COFF .o → rustc link-arg.
    let obj = out_dir.join("dictation_resources.o");
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
        .arg("--input")
        .arg(&rc)
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

fn resource_script(windows_dir: &Path) -> String {
    let [major, minor, patch] = dictation_version();
    let version = format!("{major}.{minor}.{patch}");
    let icon = windows_dir
        .join("app.ico")
        .display()
        .to_string()
        .replace('\\', "\\\\");
    format!(
        r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "CompanyName", "Kazui"
            VALUE "FileDescription", "Kosmos Dictation"
            VALUE "FileVersion", "{version}"
            VALUE "InternalName", "Kosmos Dictation.exe"
            VALUE "LegalCopyright", "Copyright (C) Kazui"
            VALUE "OriginalFilename", "Kosmos Dictation.exe"
            VALUE "ProductName", "Kosmos Dictation"
            VALUE "ProductVersion", "{version}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END
"#
    )
}

fn dictation_version() -> [u32; 3] {
    let raw = env::var("KOSMOS_DICTATION_VERSION")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let mut parts = raw.trim().split('.');
    let mut version = [0u32; 3];
    for slot in &mut version {
        *slot = parts
            .next()
            .and_then(|part| part.parse().ok())
            .expect("Dictation version must be X.Y.Z");
    }
    assert!(parts.next().is_none(), "Dictation version must be X.Y.Z");
    version
}
