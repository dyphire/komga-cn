use std::sync::OnceLock;

use super::backend_contract::RuntimeIdentityAccessBackend;
use super::test_backend::compose_test_runtime_identity_access_backend;

static BACKEND: OnceLock<RuntimeIdentityAccessBackend> = OnceLock::new();

pub fn install_runtime_identity_access(backend: RuntimeIdentityAccessBackend) {
    let _ = BACKEND.set(backend);
}

pub(super) fn backend() -> &'static RuntimeIdentityAccessBackend {
    BACKEND.get_or_init(compose_test_runtime_identity_access_backend)
}
