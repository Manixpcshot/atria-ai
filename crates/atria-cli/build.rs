use std::process::Command;

fn main() {
    // Embed the build timestamp so the CLI's self-updater can tell whether a
    // published release is newer than this binary (works even when the same
    // release tag is updated in place).
    let ts = std::env::var("BUILD_TIME").unwrap_or_else(|_| {
        Command::new("date").args(["-u", "+%Y-%m-%dT%H:%M:%SZ"]).output().map(|o| {
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        }).unwrap_or_else(|_| "unknown".to_string())
    });
    println!("cargo:rustc-env=BUILD_TIME={ts}");
    println!("cargo:rerun-if-env-changed=BUILD_TIME");
}
