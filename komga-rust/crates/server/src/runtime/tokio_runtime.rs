use std::io;
use tokio::runtime::{Builder, Runtime};

pub(crate) fn multi_thread_runtime() -> io::Result<Runtime> {
    let mut builder = Builder::new_multi_thread();
    builder.enable_all();
    enable_io_uring_on_linux(&mut builder);
    builder.build()
}

pub(crate) fn current_thread_runtime() -> io::Result<Runtime> {
    let mut builder = Builder::new_current_thread();
    builder.enable_all();
    enable_io_uring_on_linux(&mut builder);
    builder.build()
}

#[cfg(target_os = "linux")]
fn enable_io_uring_on_linux(builder: &mut Builder) {
    builder.enable_io_uring();
}

#[cfg(not(target_os = "linux"))]
fn enable_io_uring_on_linux(_builder: &mut Builder) {}
