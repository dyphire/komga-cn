use serde_json::Value;

struct AuthorMatchQuery {
    name: String,
    role: String,
}

impl AuthorMatchQuery {
    fn parse(value: String) -> Option<Self> {
        let normalized = super::persisted::common_helpers::decode_query_component(&value);
        normalized.split_once(',').map(|(name, role)| Self {
            name: name.trim().to_string(),
            role: role.trim().to_string(),
        })
    }
}

pub(super) fn author_query_to_author_match(value: String) -> Value {
    let Some(author) = AuthorMatchQuery::parse(value) else {
        return Value::Object(serde_json::Map::new());
    };

    serde_json::json!({ "name": author.name, "role": author.role })
}
#[cfg(test)]
mod tests {
    use super::author_query_to_author_match;

    #[test]
    fn decode_query_component_decodes_plus_and_percent_encoding() {
        let decoded = super::super::persisted::common_helpers::decode_query_component(
            "John+Doe%2Cwriter%20team",
        );
        assert_eq!(decoded, "John Doe,writer team");
    }

    #[test]
    fn author_query_to_author_match_splits_name_and_role() {
        let parsed = author_query_to_author_match("Jane+Doe,writer".to_string());
        assert_eq!(parsed["name"], "Jane Doe");
        assert_eq!(parsed["role"], "writer");
    }
}
