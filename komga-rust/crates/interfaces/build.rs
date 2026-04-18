use std::env;
use std::path::PathBuf;

use komga_build_support::configure_interfaces_build;

fn main() {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir should exist"));
    configure_interfaces_build(&manifest_dir, env!("CARGO_PKG_VERSION"));
}
