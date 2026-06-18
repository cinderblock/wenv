// Pass the no-CRT entry point to the linker for Windows (MSVC) builds. This is
// done from a build script rather than .cargo/config.toml because release
// tooling (cargo-dist) sets RUSTFLAGS, which *replaces* config `[target]`
// rustflags — build-script link args are applied additively and survive it.
// `rustc-link-arg-bins` targets only the binary, leaving the test harness alone.
fn main() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os == "windows" && env == "msvc" {
        println!("cargo:rustc-link-arg-bins=/ENTRY:mainCRTStartup");
        println!("cargo:rustc-link-arg-bins=/SUBSYSTEM:CONSOLE");
    }
}
