use std::collections::HashMap;

use crate::random_tokens::random_hex_token;
use crate::runtime_sse::{RuntimeSseEvent, RuntimeSseEventSink};
use icu::collator::{
    Collator,
    options::{CollatorOptions, Strength},
};
use icu::locale::locale;
use komga_domain::discovery::{
    DiscoveryQueryContext, PageEnvelope, content_allowed_by_restrictions,
};

use super::{
    CollectionMutationPort, CollectionProjectionPort, CollectionReadModel, CollectionSearchPort,
    CollectionSeriesPort, PersistedCollectionAccessRecord,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionListQuery {
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
    pub search: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionMutationInput {
    pub name: String,
    pub ordered: bool,
    pub series_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionCreateResult {
    pub collection_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CollectionMutationError {
    DuplicateName,
    Persistence(String),
}

impl std::fmt::Display for CollectionMutationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CollectionMutationError::DuplicateName => write!(f, "Collection name already exists"),
            CollectionMutationError::Persistence(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CollectionMutationError {}

pub struct CollectionProjectionService<'a, C, S, R>
where
    C: CollectionProjectionPort + ?Sized,
    S: CollectionSeriesPort + ?Sized,
    R: CollectionSearchPort + ?Sized,
{
    collections: &'a C,
    series: &'a S,
    search: &'a R,
}

pub struct CollectionMutationService<'a, C>
where
    C: CollectionMutationPort + ?Sized,
{
    collections: &'a C,
    runtime_events: &'a dyn RuntimeSseEventSink,
}

impl<'a, C, S, R> CollectionProjectionService<'a, C, S, R>
where
    C: CollectionProjectionPort + ?Sized,
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

    pub async fn collection_detail(
        &self,
        context: &DiscoveryQueryContext,
        collection_id: &str,
    ) -> Result<Option<CollectionReadModel>, String> {
        let Some(collection) = self.load_collection_detail(collection_id).await? else {
            return Ok(None);
        };

        self.visible_collection(context, collection).await
    }

    pub async fn visible_collection_series_ids(
        &self,
        context: &DiscoveryQueryContext,
        collection_id: &str,
    ) -> Result<Vec<String>, String> {
        Ok(self
            .collection_detail(context, collection_id)
            .await?
            .map(|collection| collection.series_ids)
            .unwrap_or_default())
    }

    pub async fn visible_collections(
        &self,
        context: &DiscoveryQueryContext,
        collections: Vec<CollectionReadModel>,
    ) -> Result<Vec<CollectionReadModel>, String> {
        let mut visible = Vec::with_capacity(collections.len());
        for collection in collections {
            if let Some(collection) = self.visible_collection(context, collection).await? {
                visible.push(collection);
            }
        }
        Ok(visible)
    }

    async fn visible_collection(
        &self,
        context: &DiscoveryQueryContext,
        mut collection: CollectionReadModel,
    ) -> Result<Option<CollectionReadModel>, String> {
        self.apply_visibility(&mut collection, context, None)
            .await?;
        if collection.series_ids.is_empty() {
            return Ok(None);
        }

        Ok(Some(collection))
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
        let id = row.id.clone();
        let series_ids = self
            .collections
            .load_persisted_collection_series_ids(&id)
            .await?;
        Ok(collection_from_record(row, series_ids))
    }

    async fn load_collection_detail(
        &self,
        collection_id: &str,
    ) -> Result<Option<CollectionReadModel>, String> {
        let Some(row) = self
            .collections
            .load_persisted_collection_detail(collection_id)
            .await?
        else {
            return Ok(None);
        };
        let series_ids = self
            .collections
            .load_persisted_collection_series_ids(collection_id)
            .await?;

        Ok(Some(collection_from_record(row, series_ids)))
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
                && series_visible_to_context(
                    self.series,
                    request_context,
                    series_id,
                    Some(series_library_id.as_str()),
                )
                .await?
            {
                matches_requested_scope = true;
            }

            if series_visible_to_context(
                self.series,
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
}

fn collection_from_record(
    row: PersistedCollectionAccessRecord,
    series_ids: Vec<String>,
) -> CollectionReadModel {
    CollectionReadModel {
        id: row.id,
        name: row.name,
        ordered: row.ordered,
        series_ids,
        created_date: row.created_date,
        last_modified_date: row.last_modified_date,
        filtered: false,
    }
}

async fn series_visible_to_context(
    series: &(impl CollectionSeriesPort + ?Sized),
    context: &DiscoveryQueryContext,
    series_id: &str,
    known_library_id: Option<&str>,
) -> Result<bool, String> {
    let library_id = match known_library_id {
        Some(value) => value.to_string(),
        None => {
            let Some(row) = series.load_series_library_id(series_id).await? else {
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

    let restriction_record = series.load_series_restrictions(series_id).await?;
    Ok(content_allowed_by_restrictions(
        restrictions,
        restriction_record.age_rating,
        &restriction_record.labels,
    ))
}

impl<'a, C> CollectionMutationService<'a, C>
where
    C: CollectionMutationPort + ?Sized,
{
    pub fn new(collections: &'a C, runtime_events: &'a dyn RuntimeSseEventSink) -> Self {
        Self {
            collections,
            runtime_events,
        }
    }

    pub async fn create_collection(
        &self,
        input: CollectionMutationInput,
    ) -> Result<CollectionCreateResult, CollectionMutationError> {
        self.ensure_unique_collection_name(&input.name, None)
            .await?;

        let collection_id = generated_collection_id();
        self.collections
            .persist_collection_create(
                &collection_id,
                &input.name,
                input.ordered,
                &input.series_ids,
            )
            .await
            .map_err(CollectionMutationError::Persistence)?;

        self.runtime_events
            .register(RuntimeSseEvent::CollectionAdded {
                collection_id: collection_id.clone(),
                series_ids: input.series_ids.clone(),
            });
        self.collections
            .upsert_collection_search_document(&collection_id)
            .await
            .map_err(CollectionMutationError::Persistence)?;

        Ok(CollectionCreateResult { collection_id })
    }

    pub async fn update_collection(
        &self,
        collection_id: &str,
        input: CollectionMutationInput,
    ) -> Result<bool, CollectionMutationError> {
        let Some(existing) = self
            .load_collection_for_mutation(collection_id)
            .await
            .map_err(CollectionMutationError::Persistence)?
        else {
            return Ok(false);
        };
        if !existing.name.eq_ignore_ascii_case(&input.name) {
            self.ensure_unique_collection_name(&input.name, Some(collection_id))
                .await?;
        }

        let updated = self
            .collections
            .persist_collection_update(collection_id, &input.name, input.ordered, &input.series_ids)
            .await
            .map_err(CollectionMutationError::Persistence)?;
        if !updated {
            return Ok(false);
        }

        self.runtime_events
            .register(RuntimeSseEvent::CollectionChanged {
                collection_id: collection_id.to_string(),
                series_ids: input.series_ids.clone(),
            });
        self.collections
            .upsert_collection_search_document(collection_id)
            .await
            .map_err(CollectionMutationError::Persistence)?;

        Ok(true)
    }

    pub async fn delete_collection(
        &self,
        collection_id: &str,
    ) -> Result<bool, CollectionMutationError> {
        let existing = self
            .load_collection_for_mutation(collection_id)
            .await
            .map_err(CollectionMutationError::Persistence)?;
        let deleted = self
            .collections
            .delete_persisted_collection(collection_id)
            .await
            .map_err(CollectionMutationError::Persistence)?;
        if !deleted {
            return Ok(false);
        }

        if let Some(collection) = existing {
            self.runtime_events
                .register(RuntimeSseEvent::CollectionDeleted {
                    collection_id: collection_id.to_string(),
                    series_ids: collection.series_ids,
                });
        }
        self.collections
            .delete_collection_search_document(collection_id)
            .await
            .map_err(CollectionMutationError::Persistence)?;

        Ok(true)
    }

    async fn ensure_unique_collection_name(
        &self,
        name: &str,
        allowed_collection_id: Option<&str>,
    ) -> Result<(), CollectionMutationError> {
        let collections = self
            .collections
            .load_persisted_collections()
            .await
            .map_err(CollectionMutationError::Persistence)?;
        let duplicate = collections.iter().any(|collection| {
            allowed_collection_id != Some(collection.id.as_str())
                && collection.name.eq_ignore_ascii_case(name)
        });
        if duplicate {
            return Err(CollectionMutationError::DuplicateName);
        }

        Ok(())
    }

    async fn load_collection_for_mutation(
        &self,
        collection_id: &str,
    ) -> Result<Option<CollectionReadModel>, String> {
        let Some(row) = self
            .collections
            .load_persisted_collection_detail(collection_id)
            .await?
        else {
            return Ok(None);
        };
        let series_ids = self
            .collections
            .load_persisted_collection_series_ids(collection_id)
            .await?;

        Ok(Some(collection_from_record(row, series_ids)))
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

fn generated_collection_id() -> String {
    format!("collection-{}", random_hex_token(12))
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use komga_domain::common_ids::LibraryId;
    use komga_domain::discovery::DiscoveryQueryContext;

    use crate::discovery::{
        CollectionMutationPort, CollectionProjectionPort, CollectionSearchPort,
        CollectionSeriesPort, PersistedCollectionAccessRecord, PersistedSeriesRestrictionRecord,
    };
    use crate::runtime_sse::RuntimeSseEventStore;

    use super::{
        CollectionListQuery, CollectionMutationError, CollectionMutationInput,
        CollectionMutationService, CollectionProjectionService, collection_from_record,
    };

    #[tokio::test]
    async fn collection_projection_service_applies_visibility_scope_before_search_ranking() {
        let ports = TestCollectionPorts::new();
        let service = CollectionProjectionService::new(&ports, &ports, &ports);

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
    async fn collection_projection_service_sorts_by_name_before_pagination() {
        let ports = TestCollectionPorts::new();
        let service = CollectionProjectionService::new(&ports, &ports, &ports);

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

    #[tokio::test]
    async fn collection_projection_service_applies_visibility_consistently_across_collection_surfaces()
     {
        let ports = TestCollectionPorts::new();
        let service = CollectionProjectionService::new(&ports, &ports, &ports);
        let context = context_with_libraries(["library-a", "library-b"]);
        let seed_collection = collection_from_record(
            collection_record("collection-visible", "Alpha"),
            vec!["series-a".to_string(), "series-denied".to_string()],
        );

        let detail = service
            .collection_detail(&context, "collection-visible")
            .await
            .expect("collection detail should resolve")
            .expect("collection should remain visible");
        let visible_series_ids = service
            .visible_collection_series_ids(&context, "collection-visible")
            .await
            .expect("visible collection series ids should resolve");
        let visible_collections = service
            .visible_collections(&context, vec![seed_collection])
            .await
            .expect("visible collections should resolve");

        assert_eq!(detail.series_ids, vec!["series-a".to_string()]);
        assert_eq!(visible_series_ids, detail.series_ids);
        assert_eq!(
            visible_collections
                .iter()
                .flat_map(|collection| collection.series_ids.iter())
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["series-a"],
        );
        assert!(detail.filtered);
        assert!(visible_collections[0].filtered);
    }

    #[tokio::test]
    async fn collection_mutation_service_rejects_duplicate_names_before_persistence() {
        let ports = TestCollectionPorts::new();
        let runtime_events = RuntimeSseEventStore::default();
        let service = CollectionMutationService::new(&ports, &runtime_events);

        let error = service
            .create_collection(collection_mutation_input("alpha", ["series-a"]))
            .await
            .expect_err("duplicate name should fail");

        assert_eq!(error, CollectionMutationError::DuplicateName);
        assert!(ports.created_collection_ids().is_empty());
        assert!(ports.search_upserts().is_empty());
    }

    #[tokio::test]
    async fn collection_mutation_service_allows_unchanged_historical_duplicate_name() {
        let mut ports = TestCollectionPorts::new();
        ports
            .collections
            .push(collection_record("collection-legacy-duplicate", "Alpha"));
        let runtime_events = RuntimeSseEventStore::default();
        let service = CollectionMutationService::new(&ports, &runtime_events);

        let updated = service
            .update_collection(
                "collection-visible",
                collection_mutation_input("Alpha", ["series-a"]),
            )
            .await
            .expect("unchanged duplicate name should not fail validation");

        assert!(updated);
        assert_eq!(ports.updated_collection_ids(), vec!["collection-visible"]);
        assert_eq!(ports.search_upserts(), vec!["collection-visible"]);
    }

    #[tokio::test]
    async fn collection_mutation_service_syncs_search_after_create_update_and_delete() {
        let ports = TestCollectionPorts::new();
        let runtime_events = RuntimeSseEventStore::default();
        let service = CollectionMutationService::new(&ports, &runtime_events);

        let created = service
            .create_collection(collection_mutation_input("Delta", ["series-a"]))
            .await
            .expect("collection create should complete");
        let updated = service
            .update_collection(
                "collection-visible",
                collection_mutation_input("Alpha", ["series-a"]),
            )
            .await
            .expect("collection update should complete");
        let deleted = service
            .delete_collection("collection-visible")
            .await
            .expect("collection delete should complete");

        assert!(created.collection_id.starts_with("collection-"));
        assert!(updated);
        assert!(deleted);
        assert_eq!(
            ports.created_collection_ids(),
            vec![created.collection_id.clone()]
        );
        assert_eq!(ports.updated_collection_ids(), vec!["collection-visible"]);
        assert_eq!(ports.deleted_collection_ids(), vec!["collection-visible"]);
        assert_eq!(
            ports.search_upserts(),
            vec![created.collection_id, "collection-visible".to_string()],
        );
        assert_eq!(ports.search_deletes(), vec!["collection-visible"]);
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
        created_collections: Mutex<Vec<String>>,
        updated_collections: Mutex<Vec<String>>,
        deleted_collections: Mutex<Vec<String>>,
        search_upserts: Mutex<Vec<String>>,
        search_deletes: Mutex<Vec<String>>,
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
                created_collections: Mutex::new(Vec::new()),
                updated_collections: Mutex::new(Vec::new()),
                deleted_collections: Mutex::new(Vec::new()),
                search_upserts: Mutex::new(Vec::new()),
                search_deletes: Mutex::new(Vec::new()),
            }
        }

        fn created_collection_ids(&self) -> Vec<String> {
            self.created_collections
                .lock()
                .expect("created collections lock should not be poisoned")
                .clone()
        }

        fn updated_collection_ids(&self) -> Vec<String> {
            self.updated_collections
                .lock()
                .expect("updated collections lock should not be poisoned")
                .clone()
        }

        fn deleted_collection_ids(&self) -> Vec<String> {
            self.deleted_collections
                .lock()
                .expect("deleted collections lock should not be poisoned")
                .clone()
        }

        fn search_upserts(&self) -> Vec<String> {
            self.search_upserts
                .lock()
                .expect("search upserts lock should not be poisoned")
                .clone()
        }

        fn search_deletes(&self) -> Vec<String> {
            self.search_deletes
                .lock()
                .expect("search deletes lock should not be poisoned")
                .clone()
        }
    }

    #[async_trait::async_trait]
    impl CollectionProjectionPort for TestCollectionPorts {
        async fn persisted_collections_exist(&self) -> Result<bool, String> {
            Ok(!self.collections.is_empty())
        }

        async fn load_persisted_collections(
            &self,
        ) -> Result<Vec<PersistedCollectionAccessRecord>, String> {
            Ok(self.collections.clone())
        }

        async fn load_persisted_collection_detail(
            &self,
            collection_id: &str,
        ) -> Result<Option<PersistedCollectionAccessRecord>, String> {
            Ok(self
                .collections
                .iter()
                .find(|collection| collection.id == collection_id)
                .cloned())
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

    #[async_trait::async_trait]
    impl CollectionMutationPort for TestCollectionPorts {
        async fn load_persisted_collections(
            &self,
        ) -> Result<Vec<PersistedCollectionAccessRecord>, String> {
            Ok(self.collections.clone())
        }

        async fn load_persisted_collection_detail(
            &self,
            collection_id: &str,
        ) -> Result<Option<PersistedCollectionAccessRecord>, String> {
            Ok(self
                .collections
                .iter()
                .find(|collection| collection.id == collection_id)
                .cloned())
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

        async fn persist_collection_create(
            &self,
            collection_id: &str,
            _name: &str,
            _ordered: bool,
            _series_ids: &[String],
        ) -> Result<(), String> {
            self.created_collections
                .lock()
                .expect("created collections lock should not be poisoned")
                .push(collection_id.to_string());
            Ok(())
        }

        async fn persist_collection_update(
            &self,
            collection_id: &str,
            _name: &str,
            _ordered: bool,
            _series_ids: &[String],
        ) -> Result<bool, String> {
            self.updated_collections
                .lock()
                .expect("updated collections lock should not be poisoned")
                .push(collection_id.to_string());
            Ok(self
                .collections
                .iter()
                .any(|collection| collection.id == collection_id))
        }

        async fn delete_persisted_collection(&self, collection_id: &str) -> Result<bool, String> {
            self.deleted_collections
                .lock()
                .expect("deleted collections lock should not be poisoned")
                .push(collection_id.to_string());
            Ok(self
                .collections
                .iter()
                .any(|collection| collection.id == collection_id))
        }

        async fn upsert_collection_search_document(
            &self,
            collection_id: &str,
        ) -> Result<bool, String> {
            self.search_upserts
                .lock()
                .expect("search upserts lock should not be poisoned")
                .push(collection_id.to_string());
            Ok(true)
        }

        async fn delete_collection_search_document(
            &self,
            collection_id: &str,
        ) -> Result<(), String> {
            self.search_deletes
                .lock()
                .expect("search deletes lock should not be poisoned")
                .push(collection_id.to_string());
            Ok(())
        }
    }

    #[async_trait::async_trait]
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

    #[async_trait::async_trait]
    impl CollectionSearchPort for TestCollectionPorts {
        async fn search_collection_ids(
            &self,
            query: &str,
            _limit: usize,
        ) -> Result<Vec<String>, String> {
            Ok(self.search_hits.get(query).cloned().unwrap_or_default())
        }
    }

    fn collection_mutation_input<const N: usize>(
        name: &str,
        series_ids: [&str; N],
    ) -> CollectionMutationInput {
        CollectionMutationInput {
            name: name.to_string(),
            ordered: false,
            series_ids: series_ids.into_iter().map(str::to_string).collect(),
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
