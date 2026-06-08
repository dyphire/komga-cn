use super::{
    KoboLibrarySyncRequest, KoboSyncPage, KoboSyncPageRequest, KomgaSyncTokenPayload,
    build_komga_sync_token_payload, decode_or_passthrough_sync_token,
    is_kobo_store_sync_token_candidate, parse_komga_sync_token_payload,
};

pub(super) struct KoboSyncLifecycle {
    decoded_sync_token: Option<String>,
    sync_token_payload: Option<KomgaSyncTokenPayload>,
}

impl KoboSyncLifecycle {
    pub(super) fn from_sync_token(sync_token: Option<&str>) -> Self {
        let decoded_sync_token = sync_token.and_then(decode_or_passthrough_sync_token);
        let sync_token_payload = decoded_sync_token
            .as_deref()
            .and_then(parse_komga_sync_token_payload);

        Self {
            decoded_sync_token,
            sync_token_payload,
        }
    }

    pub(super) fn page_request(&self, request: &KoboLibrarySyncRequest) -> KoboSyncPageRequest {
        KoboSyncPageRequest {
            user: request.user.clone(),
            current_api_key_id: request.current_api_key_id.clone(),
            ongoing_sync_point_id: self
                .sync_token_payload
                .as_ref()
                .and_then(|payload| payload.ongoing_sync_point_id.clone()),
            last_successful_sync_point_id: self
                .sync_token_payload
                .as_ref()
                .and_then(|payload| payload.last_successful_sync_point_id.clone()),
            limit: request.limit,
        }
    }

    pub(super) fn raw_kobo_sync_token(&self) -> Option<String> {
        self.sync_token_payload
            .as_ref()
            .map(|payload| payload.raw_kobo_sync_token.clone())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                self.decoded_sync_token
                    .as_deref()
                    .filter(|value| is_kobo_store_sync_token_candidate(value))
                    .map(str::to_string)
            })
    }

    pub(super) fn store_sync_token<'a>(
        &self,
        raw_kobo_sync_token: &'a Option<String>,
    ) -> Option<&'a str> {
        raw_kobo_sync_token
            .as_deref()
            .filter(|value| is_kobo_store_sync_token_candidate(value))
    }

    pub(super) fn sync_point_to_remove<'a>(
        &self,
        page: &'a KoboSyncPage,
        should_continue: bool,
    ) -> Option<&'a str> {
        if should_continue {
            return None;
        }

        page.from_sync_point_id
            .as_deref()
            .filter(|sync_point_id| *sync_point_id != page.to_sync_point_id)
    }

    pub(super) fn outgoing_sync_token_payload(
        &self,
        page: &KoboSyncPage,
        raw_kobo_sync_token: Option<String>,
        should_continue: bool,
    ) -> String {
        let previous = self.sync_token_payload.clone().map(|mut payload| {
            payload.ongoing_sync_point_id =
                page.should_continue.then(|| page.to_sync_point_id.clone());
            if let Some(raw) = raw_kobo_sync_token.as_ref() {
                payload.raw_kobo_sync_token = raw.clone();
            }
            payload
        });

        build_komga_sync_token_payload(
            previous,
            raw_kobo_sync_token,
            page.to_sync_point_id.as_str(),
            should_continue,
        )
    }
}
