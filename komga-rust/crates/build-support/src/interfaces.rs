use std::path::Path;

use crate::version::emit_version_env;

pub fn configure_interfaces_build(manifest_dir: &Path, fallback_version: &str) {
    println!("cargo:rerun-if-changed=build.rs");
    emit_version_env(fallback_version);

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
