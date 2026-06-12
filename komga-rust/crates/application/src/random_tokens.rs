const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

pub(crate) fn random_hex_token(byte_len: usize) -> String {
    let bytes = random_bytes(byte_len);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn random_alphanumeric(len: usize) -> String {
    random_bytes(len)
        .into_iter()
        .map(|byte| ALPHABET[(byte as usize) % ALPHABET.len()] as char)
        .collect()
}

pub(crate) fn random_uuid_like() -> String {
    let mut bytes = [0u8; 16];
    fill_random_bytes(&mut bytes);
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

fn random_bytes(len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    fill_random_bytes(&mut bytes);
    bytes
}

fn fill_random_bytes(bytes: &mut [u8]) {
    getrandom::fill(bytes).expect("system random source should be available");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_hex_token_uses_two_chars_per_byte() {
        assert_eq!(random_hex_token(12).len(), 24);
    }

    #[test]
    fn random_uuid_like_uses_version_4_variant_shape() {
        let value = random_uuid_like();

        assert_eq!(value.len(), 36);
        assert_eq!(&value[14..15], "4");
        assert!(matches!(&value[19..20], "8" | "9" | "a" | "b"));
    }
}
