use base64::Engine;

#[path = "operational_surface/access.rs"]
mod access;
#[path = "operational_surface/actuator.rs"]
mod actuator;
#[path = "operational_surface/cors.rs"]
mod cors;
#[path = "operational_surface/settings.rs"]
mod settings;
#[path = "operational_surface/sse.rs"]
mod sse;

fn basic_auth(credentials: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(credentials)
    )
}
