//! Windows: embed the icon and version info into Reclaimer.exe, so Explorer,
//! the Start menu and pinned taskbar entries show it. Other platforms take
//! their icon from the app bundle or .desktop file instead.

fn main() {
    println!("cargo:rerun-if-changed=icons/icon.ico");

    // `cfg(windows)` here means "building on Windows", where the resource
    // compiler lives; the env var checks we are also building *for* Windows.
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("icons/icon.ico");
        res.set("ProductName", "Reclaimer");
        res.set("FileDescription", "Reclaimer");
        res.compile().expect("embedding the Windows icon");
    }
}
