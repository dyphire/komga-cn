pub fn random_hex_token(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    getrandom::fill(&mut bytes).expect("system random source should be available");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_hex_token_uses_two_chars_per_byte() {
        assert_eq!(random_hex_token(12).len(), 24);
    }
}
