//! Desktop build script: embed the application icon into Windows executables.
//!
//! Cross-compiling from Linux runs this script on Linux, so `cfg(windows)` is
//! false; invoke the MinGW `windres` for the Windows target instead.
use std::path::PathBuf;

fn icon_path() -> Option<PathBuf> {
    ["../icon/ReaderIcon2.ico", "../icon/ReaderIcon.ico"]
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
}

fn main() {
    println!("cargo:rerun-if-changed=../icon/ReaderIcon2.ico");
    println!("cargo:rerun-if-changed=../icon/ReaderIcon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let Some(icon) = icon_path() else {
        return;
    };
    let icon = icon.canonicalize().expect("icon path must resolve");
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon(icon.to_string_lossy().as_ref());
        res.compile()
            .expect("failed to compile Windows icon resources");
    }
    #[cfg(not(windows))]
    {
        let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
        let rc = out.join("app.rc");
        let escaped = icon.display().to_string().replace('\\', "\\\\");
        std::fs::write(&rc, format!("1 ICON \"{escaped}\"\n")).expect("write app.rc");
        let obj = out.join("app_icon.o");
        let windres =
            std::env::var("WINDRES").unwrap_or_else(|_| "x86_64-w64-mingw32-windres".into());
        let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
        let pe_target = if arch == "x86_64" {
            "pe-x86-64"
        } else {
            "pe-i386"
        };
        let status = std::process::Command::new(&windres)
            .arg(format!("--target={pe_target}"))
            .args(["-O", "coff"])
            .arg("-I")
            .arg(icon.parent().expect("icon directory"))
            .arg("-i")
            .arg(&rc)
            .arg("-o")
            .arg(&obj)
            .status()
            .unwrap_or_else(|error| panic!("failed to run {windres}: {error}"));
        assert!(status.success(), "{windres} failed");
        println!("cargo:rustc-link-arg={}", obj.display());
    }
}
