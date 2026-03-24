use std::sync::Once;

pub const COMPAT_ADMIN_BASIC_AUTH_BASE64: &str = "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=";

pub fn ensure_compat_auth_env() {
    static INIT: Once = Once::new();
    INIT.call_once(|| unsafe {
        std::env::set_var("KOMGA_RUST_AUTH_USER_ID", "admin-1");
        std::env::set_var("KOMGA_RUST_AUTH_USER_EMAIL", "admin@example.org");
        std::env::set_var("KOMGA_RUST_AUTH_USER_PASSWORD", "admin");
        std::env::set_var(
            "KOMGA_RUST_AUTH_USER_ROLES",
            "ADMIN,FILE_DOWNLOAD,PAGE_STREAMING,USER",
        );
        std::env::set_var("KOMGA_RUST_AUTH_USER_SHARED_ALL_LIBRARIES", "true");

        std::env::set_var("KOMGA_RUST_AUTH_USER2_ID", "user-1");
        std::env::set_var("KOMGA_RUST_AUTH_USER2_EMAIL", "user@example.org");
        std::env::set_var("KOMGA_RUST_AUTH_USER2_PASSWORD", "user");
        std::env::set_var("KOMGA_RUST_AUTH_USER2_ROLES", "USER");
        std::env::set_var("KOMGA_RUST_AUTH_USER2_SHARED_ALL_LIBRARIES", "true");

        std::env::set_var("KOMGA_COMPAT_API_KEY", "compat-api-key");
        std::env::set_var("KOMGA_COMPAT_BASIC_AUTH", COMPAT_ADMIN_BASIC_AUTH_BASE64);
    });
}
