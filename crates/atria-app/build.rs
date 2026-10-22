fn main() {
    // winos.rs declares raw user32 imports (FlashWindowExW, clipboard, hotkey
    // message window). The `windows` 0.53 static lib does not carry every
    // symbol, so link user32 explicitly on Windows targets.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=user32");
    }
    tauri_build::build()
}
