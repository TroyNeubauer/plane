//! This build script copies the `memory.x` file from the crate root into
//! a directory where the linker can always find it at build time.
//! For many projects this is optional, as the linker always searches the
//! project root directory -- wherever `Cargo.toml` is. However, if you
//! are using a workspace or have a more complicated build setup, this
//! build script becomes required. Additionally, by requesting that
//! Cargo re-run the build script whenever `memory.x` is changed,
//! updating `memory.x` ensures a rebuild of the application with the
//! new memory settings.

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    // Put `memory.x` in our output directory and ensure it's
    // on the linker search path.
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    println!("cargo:rustc-link-search={}", out.display());

    // By default, Cargo will re-run a build script whenever
    // any file in the project changes. By specifying `memory.x`
    // here, we ensure the build script is only re-run when
    // `memory.x` is changed.
    println!("cargo:rerun-if-changed=rp2040.x");
    println!("cargo:rerun-if-changed=rp235xa.x");

    println!("cargo:rustc-link-arg-bins=--nmagic");
    println!("cargo:rustc-link-arg-bins=-Tlink.x");

    if env::var("TARGET").is_ok_and(|target| target == "thumbv8m.main-none-eabihf") {
        // Pico 2
        File::create(out.join("memory.x"))
            .unwrap()
            .write_all(include_bytes!("rp235xa.x"))
            .unwrap();

        println!("cargo:rustc-cfg=rp235xa")
    } else if env::var("TARGET").is_ok_and(|target| target == "thumbv6m-none-eabi") {
        // Pico 1
        println!("cargo:rustc-link-arg-bins=-Tlink-rp.x");
        File::create(out.join("memory.x"))
            .unwrap()
            .write_all(include_bytes!("rp2040.x"))
            .unwrap();

        println!("cargo:rustc-cfg=rp2040")
    } else {
        panic!(
            "got target {:?}, expected thumbv8m or thumbv6m",
            env::var("TARGET")
        )
    }

    println!("cargo:rustc-link-arg-bins=-Tdefmt.x");
}
