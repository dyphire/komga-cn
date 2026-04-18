use std::env;
use std::path::PathBuf;

use komga_build_support::configure_infrastructure_build;

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("out dir"));
    configure_infrastructure_build(&manifest_dir, &out_dir);
}
