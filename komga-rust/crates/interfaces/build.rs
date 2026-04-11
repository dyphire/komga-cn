use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir should exist"));
    let webui_dist_dir = manifest_dir.join("../../../komga-webui/dist");

    println!("cargo:rustc-check-cfg=cfg(webui_dist_present)");
    println!(
        "cargo:rustc-env=KOMGA_WEBUI_DIST_DIR={}",
        webui_dist_dir.display()
    );
    println!("cargo:rerun-if-changed={}", webui_dist_dir.display());

    if webui_dist_dir.is_dir() {
        println!("cargo:rustc-cfg=webui_dist_present");
    }
}
