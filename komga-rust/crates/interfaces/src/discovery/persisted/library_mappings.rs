use komga_application::discovery::LibraryIdMappingPort;

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|candidate| candidate == value) {
        values.push(value.to_string());
    }
}

pub(in crate::discovery) async fn remap_requested_library_ids_for_persisted(
    backend: &dyn LibraryIdMappingPort,
    requested: Option<&Vec<String>>,
) -> anyhow::Result<Option<Vec<String>>> {
    let Some(requested) = requested else {
        return Ok(None);
    };

    if requested.is_empty() {
        return Ok(None);
    }

    let persisted_ids = backend.load_persisted_library_ids().await?;

    if persisted_ids.is_empty() {
        return Ok(None);
    }

    let mut normalized = Vec::new();
    for value in requested {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }

        if persisted_ids.iter().any(|candidate| candidate == trimmed) {
            push_unique(&mut normalized, trimmed);
            continue;
        }

        let Some(index) = trimmed.parse::<usize>().ok() else {
            continue;
        };
        if index == 0 {
            continue;
        }

        let Some(mapped) = persisted_ids.get(index - 1) else {
            continue;
        };
        push_unique(&mut normalized, mapped);
    }

    Ok((!normalized.is_empty()).then_some(normalized))
}

#[cfg(test)]
mod tests {

    use super::*;

    struct FailingLibraryIdMapping;

    #[async_trait::async_trait]
    impl LibraryIdMappingPort for FailingLibraryIdMapping {
        async fn load_persisted_library_ids(&self) -> anyhow::Result<Vec<String>> {
            Err(anyhow::anyhow!("library lookup failed"))
        }
    }

    #[tokio::test]
    async fn remap_requested_library_ids_propagates_backend_errors() {
        let requested = vec!["1".to_string()];
        let error =
            remap_requested_library_ids_for_persisted(&FailingLibraryIdMapping, Some(&requested))
                .await
                .expect_err("library id lookup errors must not become unmapped filters");

        assert_eq!(error.to_string(), "library lookup failed");
    }
}
