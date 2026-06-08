use std::collections::HashMap;

use icu::collator::{
    Collator,
    options::{CollatorOptions, Strength},
};
use icu::locale::locale;
use komga_domain::discovery::{
    DiscoveryQueryContext, PageEnvelope, content_allowed_by_restrictions,
};

use super::{
    CollectionListPort, CollectionReadModel, CollectionSearchPort, CollectionSeriesPort,
    PersistedCollectionAccessRecord,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionListQuery {
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
    pub search: Option<String>,
}

pub struct CollectionListService<'a, C, S, R>
where
    C: CollectionListPort + ?Sized,
    S: CollectionSeriesPort + ?Sized,
    R: CollectionSearchPort + ?Sized,
{
    collections: &'a C,
    series: &'a S,
    search: &'a R,
}

impl<'a, C, S, R> CollectionListService<'a, C, S, R>
where
    C: CollectionListPort + ?Sized,
    S: CollectionSeriesPort + ?Sized,
    R: CollectionSearchPort + ?Sized,
{
    pub fn new(collections: &'a C, series: &'a S, search: &'a R) -> Self {
        Self {
            collections,
            series,
            search,
        }
    }

    pub async fn list_collections(
        &self,
        visibility_context: &DiscoveryQueryContext,
        request_scope_context: Option<&DiscoveryQueryContext>,
        query: CollectionListQuery,
    ) -> Result<PageEnvelope<CollectionReadModel>, String> {
        let mut content = if self.collections.persisted_collections_exist().await? {
            self.load_collections().await?
        } else {
            vec![]
        };
        let search_limit = content.len().max(1);

        for collection in &mut content {
            self.apply_visibility(collection, visibility_context, request_scope_context)
                .await?;
        }
        content.retain(|collection| !collection.series_ids.is_empty());

        if let Some(search) = query.search.as_deref() {
            sort_collections_by_search(self.search, &mut content, search, search_limit).await?;
        } else {
            sort_collections_by_name(&mut content);
        }

        Ok(paginate_collections(
            content,
            query.page,
            query.size,
            query.unpaged,
        ))
    }

    async fn load_collections(&self) -> Result<Vec<CollectionReadModel>, String> {
        let rows = self.collections.load_persisted_collections().await?;

        let mut collections = Vec::with_capacity(rows.len());
        for row in rows {
            collections.push(self.collection_read_model(row).await?);
        }

        Ok(collections)
    }

    async fn collection_read_model(
        &self,
        row: PersistedCollectionAccessRecord,
    ) -> Result<CollectionReadModel, String> {
        let id = row.id;
        Ok(CollectionReadModel {
            id: id.clone(),
            name: row.name,
            ordered: row.ordered,
            series_ids: self
                .collections
                .load_persisted_collection_series_ids(&id)
                .await?,
            created_date: row.created_date,
            last_modified_date: row.last_modified_date,
            filtered: false,
        })
    }

    async fn apply_visibility(
        &self,
        collection: &mut CollectionReadModel,
        visibility_context: &DiscoveryQueryContext,
        request_scope_context: Option<&DiscoveryQueryContext>,
    ) -> Result<(), String> {
        let mut visible_series_ids = Vec::with_capacity(collection.series_ids.len());
        let mut matches_requested_scope = request_scope_context.is_none();

        for series_id in &collection.series_ids {
            let Some(series_library_id) = self.series.load_series_library_id(series_id).await?
            else {
                continue;
            };

            if let Some(request_context) = request_scope_context
                && !matches_requested_scope
                && self
                    .series_visible_to_context(
                        request_context,
                        series_id,
                        Some(series_library_id.as_str()),
                    )
                    .await?
            {
                matches_requested_scope = true;
            }

            if self
                .series_visible_to_context(
                    visibility_context,
                    series_id,
                    Some(series_library_id.as_str()),
                )
                .await?
            {
                visible_series_ids.push(series_id.clone());
            }
        }

        if visible_series_ids.len() != collection.series_ids.len() {
            collection.filtered = true;
        }
        collection.series_ids = if matches_requested_scope {
            visible_series_ids
        } else {
            vec![]
        };

        Ok(())
    }

    async fn series_visible_to_context(
        &self,
        context: &DiscoveryQueryContext,
        series_id: &str,
        known_library_id: Option<&str>,
    ) -> Result<bool, String> {
        let library_id = match known_library_id {
            Some(value) => value.to_string(),
            None => {
                let Some(row) = self.series.load_series_library_id(series_id).await? else {
                    return Ok(false);
                };
                row
            }
        };

        if let Some(authorized_libraries) = context.authorized_library_ids.as_ref()
            && !authorized_libraries
                .iter()
                .any(|candidate| candidate.as_str() == library_id.as_str())
        {
            return Ok(false);
        }

        let Some(restrictions) = context.restrictions.as_ref() else {
            return Ok(true);
        };

        let restriction_record = self.series.load_series_restrictions(series_id).await?;
        Ok(content_allowed_by_restrictions(
            restrictions,
            restriction_record.age_rating,
            &restriction_record.labels,
        ))
    }
}

async fn sort_collections_by_search(
    search: &(impl CollectionSearchPort + ?Sized),
    content: &mut Vec<CollectionReadModel>,
    query: &str,
    search_limit: usize,
) -> Result<(), String> {
    let ranked_ids = search.search_collection_ids(query, search_limit).await?;
    let ranks = ranked_ids
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect::<HashMap<&str, usize>>();
    content.retain(|collection| ranks.contains_key(collection.id.as_str()));
    content.sort_by_key(|collection| {
        ranks
            .get(collection.id.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });
    Ok(())
}

fn sort_collections_by_name(content: &mut [CollectionReadModel]) {
    let collator = collections_unicode_collator();
    content.sort_by(|left, right| collator.compare(left.name.as_str(), right.name.as_str()));
}

fn paginate_collections(
    content: Vec<CollectionReadModel>,
    page: usize,
    size: usize,
    unpaged: bool,
) -> PageEnvelope<CollectionReadModel> {
    let page_size = if size == 0 { 20 } else { size };
    let total_elements = content.len();
    if unpaged {
        return PageEnvelope::from_slice(content, 0, total_elements.max(1), total_elements);
    }

    let offset = page.saturating_mul(page_size);
    let page_content = if offset >= total_elements {
        vec![]
    } else {
        content
            .into_iter()
            .skip(offset)
            .take(page_size)
            .collect::<Vec<_>>()
    };
    PageEnvelope::from_slice(page_content, page, page_size, total_elements)
}

fn collections_unicode_collator() -> icu::collator::CollatorBorrowed<'static> {
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Tertiary);
    Collator::try_new(locale!("und").into(), options)
        .expect("unicode collator for collections sorting should construct")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_trait::async_trait;
    use komga_domain::common_ids::LibraryId;
    use komga_domain::discovery::DiscoveryQueryContext;

    use crate::discovery::{
        CollectionListPort, CollectionSearchPort, CollectionSeriesPort,
        PersistedCollectionAccessRecord, PersistedSeriesRestrictionRecord,
    };

    use super::{CollectionListQuery, CollectionListService};

    #[tokio::test]
    async fn list_collections_applies_visibility_scope_before_search_ranking() {
        let ports = TestCollectionPorts::new();
        let service = CollectionListService::new(&ports, &ports, &ports);

        let page = service
            .list_collections(
                &context_with_libraries(["library-a", "library-b"]),
                Some(&context_with_libraries(["library-a"])),
                CollectionListQuery {
                    page: 0,
                    size: 20,
                    unpaged: false,
                    search: Some("space".to_string()),
                },
            )
            .await
            .expect("collections should resolve");

        assert_eq!(page.total_elements, 1);
        let collection = page
            .content
            .first()
            .expect("visible collection should remain");
        assert_eq!(collection.id, "collection-visible");
        assert_eq!(collection.series_ids, vec!["series-a".to_string()]);
        assert!(collection.filtered);
    }

    #[tokio::test]
    async fn list_collections_sorts_by_name_before_pagination() {
        let ports = TestCollectionPorts::new();
        let service = CollectionListService::new(&ports, &ports, &ports);

        let page = service
            .list_collections(
                &context_with_libraries(["library-a", "library-b", "library-c"]),
                None,
                CollectionListQuery {
                    page: 0,
                    size: 2,
                    unpaged: false,
                    search: None,
                },
            )
            .await
            .expect("collections should resolve");

        assert_eq!(page.total_elements, 3);
        assert_eq!(
            page.content
                .iter()
                .map(|collection| collection.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Alpha", "Beta"],
        );
    }

    fn context_with_libraries<const N: usize>(libraries: [&str; N]) -> DiscoveryQueryContext {
        DiscoveryQueryContext {
            user_id: None,
            is_admin: false,
            authorized_library_ids: Some(libraries.into_iter().map(LibraryId::from).collect()),
            restrictions: None,
        }
    }

    struct TestCollectionPorts {
        collections: Vec<PersistedCollectionAccessRecord>,
        collection_series: HashMap<String, Vec<String>>,
        series_libraries: HashMap<String, String>,
        search_hits: HashMap<String, Vec<String>>,
    }

    impl TestCollectionPorts {
        fn new() -> Self {
            Self {
                collections: vec![
                    collection_record("collection-request-miss", "Beta"),
                    collection_record("collection-visible", "Alpha"),
                    collection_record("collection-unsearched", "Gamma"),
                ],
                collection_series: HashMap::from([
                    (
                        "collection-visible".to_string(),
                        vec!["series-a".to_string(), "series-denied".to_string()],
                    ),
                    (
                        "collection-request-miss".to_string(),
                        vec!["series-b".to_string()],
                    ),
                    (
                        "collection-unsearched".to_string(),
                        vec!["series-c".to_string()],
                    ),
                ]),
                series_libraries: HashMap::from([
                    ("series-a".to_string(), "library-a".to_string()),
                    ("series-b".to_string(), "library-b".to_string()),
                    ("series-c".to_string(), "library-c".to_string()),
                    ("series-denied".to_string(), "library-c".to_string()),
                ]),
                search_hits: HashMap::from([(
                    "space".to_string(),
                    vec![
                        "collection-request-miss".to_string(),
                        "collection-visible".to_string(),
                    ],
                )]),
            }
        }
    }

    #[async_trait]
    impl CollectionListPort for TestCollectionPorts {
        async fn persisted_collections_exist(&self) -> Result<bool, String> {
            Ok(!self.collections.is_empty())
        }

        async fn load_persisted_collections(
            &self,
        ) -> Result<Vec<PersistedCollectionAccessRecord>, String> {
            Ok(self.collections.clone())
        }

        async fn load_persisted_collection_series_ids(
            &self,
            collection_id: &str,
        ) -> Result<Vec<String>, String> {
            Ok(self
                .collection_series
                .get(collection_id)
                .cloned()
                .unwrap_or_default())
        }
    }

    #[async_trait]
    impl CollectionSeriesPort for TestCollectionPorts {
        async fn load_series_library_id(&self, series_id: &str) -> Result<Option<String>, String> {
            Ok(self.series_libraries.get(series_id).cloned())
        }

        async fn load_series_restrictions(
            &self,
            _series_id: &str,
        ) -> Result<PersistedSeriesRestrictionRecord, String> {
            Ok(PersistedSeriesRestrictionRecord {
                age_rating: None,
                labels: vec![],
            })
        }
    }

    #[async_trait]
    impl CollectionSearchPort for TestCollectionPorts {
        async fn search_collection_ids(
            &self,
            query: &str,
            _limit: usize,
        ) -> Result<Vec<String>, String> {
            Ok(self.search_hits.get(query).cloned().unwrap_or_default())
        }
    }

    fn collection_record(id: &str, name: &str) -> PersistedCollectionAccessRecord {
        PersistedCollectionAccessRecord {
            id: id.to_string(),
            name: name.to_string(),
            ordered: false,
            created_date: "2024-01-01 00:00:00".to_string(),
            last_modified_date: "2024-01-01 00:00:00".to_string(),
        }
    }
}
