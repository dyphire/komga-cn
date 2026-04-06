use serde_json::Value;

pub(super) use super::persisted::decode_query_component;

pub(super) fn author_query_to_author_match(value: String) -> Value {
    let normalized = decode_query_component(&value);
    let (name, role) = normalized
        .split_once(',')
        .map(|(name, role)| (name.trim(), role.trim()))
        .unwrap_or((normalized.trim(), ""));

    let mut payload = serde_json::Map::new();
    if !name.is_empty() {
        payload.insert("name".to_string(), Value::String(name.to_string()));
    }
    if !role.is_empty() {
        payload.insert("role".to_string(), Value::String(role.to_string()));
    }

    Value::Object(payload)
}
#[cfg(test)]
mod tests {
    use super::{author_query_to_author_match, decode_query_component};

    #[test]
    fn decode_query_component_decodes_plus_and_percent_encoding() {
        let decoded = decode_query_component("John+Doe%2Cwriter%20team");
        assert_eq!(decoded, "John Doe,writer team");
    }

    #[test]
    fn author_query_to_author_match_splits_name_and_role() {
        let parsed = author_query_to_author_match("Jane+Doe,writer".to_string());
        assert_eq!(parsed["name"], "Jane Doe");
        assert_eq!(parsed["role"], "writer");
    }
}
