mod version;

pub mod infrastructure;
pub mod interfaces;
pub mod server;

pub use infrastructure::configure_infrastructure_build;
pub use interfaces::configure_interfaces_build;
pub use server::configure_server_build;
