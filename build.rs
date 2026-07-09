use std::env;
use std::path::Path;

fn main() {
    // Only the `voice` feature pulls in the vosk crate, which links against the
    // native libvosk dynamic library. Without it there is nothing to link.
    if env::var_os("CARGO_FEATURE_VOICE").is_none() {
        return;
    }

    // We keep libvosk in ./lib (see scripts/fetch-voice-assets.sh) and point
    // both the linker and the runtime loader (rpath) at it.
    let lib_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("lib");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
}
