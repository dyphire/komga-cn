pub(in crate::discovery::persisted::browse) fn first_group_key(title: &str) -> String {
    title
        .trim()
        .chars()
        .next()
        .map(|ch| ch.to_lowercase().collect::<String>())
        .unwrap_or_else(|| "#".to_string())
}
