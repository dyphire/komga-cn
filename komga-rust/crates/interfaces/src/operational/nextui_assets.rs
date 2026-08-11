#[cfg(nextui_dist_present)]
mod imp {
    use rust_embed::Embed;
    use std::borrow::Cow;

    #[derive(Embed)]
    #[folder = "../../../next-ui/dist"]
    struct EmbeddedNextUiAssets;

    pub(in crate::operational) struct NextUiAssets;

    impl NextUiAssets {
        pub(in crate::operational) fn get(path: &str) -> Option<Cow<'static, [u8]>> {
            EmbeddedNextUiAssets::get(path).map(|asset| asset.data)
        }
    }
}

#[cfg(not(nextui_dist_present))]
mod imp {
    use std::borrow::Cow;

    pub(in crate::operational) struct NextUiAssets;

    impl NextUiAssets {
        pub(in crate::operational) fn get(_path: &str) -> Option<Cow<'static, [u8]>> {
            None
        }
    }
}

pub(super) use imp::NextUiAssets;
