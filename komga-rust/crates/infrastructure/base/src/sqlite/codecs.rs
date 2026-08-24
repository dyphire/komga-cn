pub fn parse_sqlite_group_concat_values(raw: &str) -> Vec<String> {
    const SEPARATOR: char = '\u{1e}';

    if raw.trim().is_empty() {
        return vec![];
    }

    raw.split(SEPARATOR)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn clamp_kotlin_int_u32(value: i64) -> u32 {
    value.clamp(0, i64::from(i32::MAX)) as u32
}
