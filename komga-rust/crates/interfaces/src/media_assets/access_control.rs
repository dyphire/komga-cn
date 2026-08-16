use crate::discovery_auth::context::to_query_context;
use crate::discovery_auth::principal::principal_from_user;
use crate::helpers::to_domain_query_context;
use crate::media_assets::types::PersistedBookMedia;
use crate::state::MediaAssetsState;
use komga_application::identity_access::{
    AuthUser, user_shared_all_libraries, user_shared_library_ids,
};
use komga_application::media_assets::BookMediaReaderPort;

pub(super) fn user_can_access_library(user: &AuthUser, library_id: &str) -> bool {
    user_shared_all_libraries(user)
        || user_shared_library_ids(user)
            .iter()
            .any(|shared_library_id| shared_library_id == library_id)
}

pub(super) fn user_has_unrestricted_all_libraries(user: &AuthUser) -> bool {
    user_shared_all_libraries(user)
        && principal_from_user(user).is_none_or(|principal| !principal.restrictions.is_restricted())
}

pub(crate) async fn user_can_access_book_media(
    reader: &dyn BookMediaReaderPort,
    book_id: &str,
    user: &AuthUser,
    media: &PersistedBookMedia,
) -> anyhow::Result<bool> {
    if !user_can_access_library(user, &media.library_id) {
        return Ok(false);
    }

    let Some(restrictions) = reader.book_restrictions(book_id).await? else {
        return Ok(true);
    };

    Ok(principal_allows_content(
        user,
        restrictions.age_rating,
        &restrictions.labels,
    ))
}

fn principal_allows_content(user: &AuthUser, age_rating: Option<u32>, labels: &[String]) -> bool {
    let Some(principal) = principal_from_user(user) else {
        return true;
    };
    if !principal.restrictions.is_restricted() {
        return true;
    }

    principal.is_content_allowed(age_rating, labels)
}

pub(super) async fn user_can_access_series_media(
    app: &MediaAssetsState,
    series_id: &str,
    user: &AuthUser,
) -> anyhow::Result<bool> {
    let Some(library_id) = app.series_access.load_series_library_id(series_id).await? else {
        return Ok(false);
    };
    if !user_can_access_library(user, &library_id) {
        return Ok(false);
    }

    let restriction_record = app
        .series_access
        .load_series_restrictions(series_id)
        .await?;
    Ok(principal_allows_content(
        user,
        restriction_record.age_rating,
        &restriction_record.labels,
    ))
}

pub(super) async fn user_can_access_readlist_media(
    app: &MediaAssetsState,
    readlist_id: &str,
    user: &AuthUser,
) -> anyhow::Result<bool> {
    Ok(visible_readlist_book_ids_for_user(app, readlist_id, user)
        .await?
        .is_some_and(|book_ids| !book_ids.is_empty()))
}

pub(super) async fn visible_readlist_book_ids_for_user(
    app: &MediaAssetsState,
    readlist_id: &str,
    user: &AuthUser,
) -> anyhow::Result<Option<Vec<String>>> {
    let principal = principal_from_user(user)
        .expect("authenticated user should resolve to discovery principal");
    let context = to_domain_query_context(to_query_context(&principal, None));
    app.persisted_set_visibility
        .visible_readlist_book_ids(&context, readlist_id)
        .await
}

pub(super) async fn user_can_access_collection_media(
    app: &MediaAssetsState,
    collection_id: &str,
    user: &AuthUser,
) -> anyhow::Result<bool> {
    Ok(
        !visible_collection_series_ids_for_user(app, collection_id, user)
            .await?
            .is_empty(),
    )
}

pub(super) async fn visible_collection_series_ids_for_user(
    app: &MediaAssetsState,
    collection_id: &str,
    user: &AuthUser,
) -> anyhow::Result<Vec<String>> {
    let principal = principal_from_user(user)
        .expect("authenticated user should resolve to discovery principal");
    let context = to_domain_query_context(to_query_context(&principal, None));
    app.persisted_set_visibility
        .visible_collection_series_ids(&context, collection_id)
        .await
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use komga_application::identity_access::AuthUserRole;
    use komga_application::media_assets::{
        BookAccessRestrictions, BookMediaRecord, BookPageRecord, EntityThumbnailBinary,
    };

    use super::*;

    struct TestBookMediaReader {
        restrictions: Result<Option<BookAccessRestrictions>, String>,
    }

    #[async_trait::async_trait]
    impl BookMediaReaderPort for TestBookMediaReader {
        async fn book_media(&self, _book_id: &str) -> anyhow::Result<Option<BookMediaRecord>> {
            Ok(None)
        }

        async fn book_media_is_ready(&self, _book_id: &str) -> anyhow::Result<bool> {
            Ok(false)
        }

        async fn book_pages(&self, _book_id: &str) -> anyhow::Result<Vec<BookPageRecord>> {
            Ok(Vec::new())
        }

        async fn book_page(
            &self,
            _book_id: &str,
            _page_number: u64,
        ) -> anyhow::Result<Option<BookPageRecord>> {
            Ok(None)
        }

        async fn book_restrictions(
            &self,
            _book_id: &str,
        ) -> anyhow::Result<Option<BookAccessRestrictions>> {
            self.restrictions.clone().map_err(anyhow::Error::msg)
        }

        async fn selected_book_thumbnail(
            &self,
            _book_id: &str,
        ) -> anyhow::Result<Option<EntityThumbnailBinary>> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn book_media_access_propagates_restriction_load_errors() {
        let reader = TestBookMediaReader {
            restrictions: Err("restriction lookup failed".to_string()),
        };

        let result =
            user_can_access_book_media(&reader, "book-a", &unrestricted_user(), &book_media())
                .await;

        assert_eq!(
            result
                .expect_err("restriction lookup should fail")
                .to_string(),
            "restriction lookup failed"
        );
    }

    fn unrestricted_user() -> AuthUser {
        AuthUser {
            id: "user-a".to_string(),
            email: "user@example.org".to_string(),
            password: String::new(),
            roles: vec![AuthUserRole::Admin],
            shared_all_libraries: true,
            shared_library_ids: Vec::new(),
            labels_allow: Vec::new(),
            labels_exclude: Vec::new(),
            age_restriction: None,
        }
    }

    fn book_media() -> BookMediaRecord {
        BookMediaRecord {
            library_id: "library-a".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: PathBuf::from("/tmp/book.cbz"),
            media_type: "application/zip".to_string(),
            page_count: 1,
        }
    }
}
