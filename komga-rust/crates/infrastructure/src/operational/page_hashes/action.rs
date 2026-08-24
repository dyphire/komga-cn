use komga_application::operational::PageHashAction;

pub(crate) fn parse_persisted_page_hash_action(value: &str) -> Option<PageHashAction> {
    match value {
        "DELETE_MANUAL" => Some(PageHashAction::DeleteManual),
        "DELETE_AUTO" => Some(PageHashAction::DeleteAuto),
        "IGNORE" => Some(PageHashAction::Ignore),
        _ => None,
    }
}

pub(crate) fn persisted_page_hash_action(action: PageHashAction) -> &'static str {
    match action {
        PageHashAction::DeleteManual => "DELETE_MANUAL",
        PageHashAction::DeleteAuto => "DELETE_AUTO",
        PageHashAction::Ignore => "IGNORE",
    }
}
