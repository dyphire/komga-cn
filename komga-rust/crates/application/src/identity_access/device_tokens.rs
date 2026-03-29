use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn sanitize_identifier(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

pub fn generated_kobo_token_triplet(user_key: &str) -> (String, String, String) {
    let key = user_key.trim();
    let normalized = if key.is_empty() {
        "anonymous".to_string()
    } else {
        sanitize_identifier(key)
    };
    let access = random_hex(24);
    let refresh = random_hex(24);
    let tracking = random_uuid_like();

    (
        format!("kobo-{normalized}-{access}"),
        format!("kobo-{normalized}-{refresh}"),
        tracking,
    )
}

pub fn generated_kobo_api_token(auth_token: &str, authenticated_user_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(auth_token.trim().as_bytes());
    hasher.update(b":");
    hasher.update(authenticated_user_id.trim().as_bytes());
    let digest = hasher.finalize();
    format!("KOMGA.{}", STANDARD_NO_PAD.encode(digest))
}

fn random_hex(len: usize) -> String {
    let mut bytes = vec![0u8; len.div_ceil(2)];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let _ = file.read_exact(&mut bytes);
    } else {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        let mut hasher = Sha256::new();
        hasher.update(nanos.as_bytes());
        let digest = hasher.finalize();
        let copy_len = bytes.len().min(digest.len());
        bytes[..copy_len].copy_from_slice(&digest[..copy_len]);
    }

    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    hex.chars().take(len).collect()
}

pub fn random_uuid_like() -> String {
    let mut bytes = [0u8; 16];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let _ = file.read_exact(&mut bytes);
    } else {
        let seed = random_hex(32);
        let seed_bytes = seed.as_bytes();
        for (idx, byte) in bytes.iter_mut().enumerate() {
            *byte = seed_bytes[idx % seed_bytes.len()];
        }
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}
