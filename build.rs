//! Embeds the app icon in the Windows executable, where Explorer, the taskbar
//! and gpui's window icon all read it.

fn main() {
    println!("cargo:rerun-if-changed=assets/app/elsewhere.rc");
    println!("cargo:rerun-if-changed=assets/app/elsewhere.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/app/elsewhere.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
