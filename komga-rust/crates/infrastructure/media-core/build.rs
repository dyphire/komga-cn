use std::env;
use std::path::PathBuf;

use komga_build_support::configure_pdfium_build;

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    configure_pdfium_build(&manifest_dir);
}
