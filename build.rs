use std::path::Path;

fn main() {
    // The vosk crate links against the native libvosk dynamic library.
    // We keep it in ./lib (see scripts/fetch-voice-assets.sh) and point both
    // the linker and the runtime loader (rpath) at it.
    let lib_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("lib");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
}
