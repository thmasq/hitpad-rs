use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let memory_file = if env::var("CARGO_FEATURE_MCU_RP2350").is_ok() {
        "memory-rp235xa.x"
    } else {
        "memory-rp2040.x"
    };

    fs::copy(memory_file, out_dir.join("memory.x")).expect("Failed to copy memory map");

    println!("cargo:rustc-link-search={}", out_dir.display());
    println!("cargo:rerun-if-changed=memory-rp2040.x");
    println!("cargo:rerun-if-changed=memory-rp235xa.x");
    println!("cargo:rerun-if-changed=build.rs");
}
