#[cfg(all(
    not(webui_dist_present),
    not(test),
    not(feature = "allow-missing-webui-dist")
))]
compile_error!(concat!(
    "komga-webui/dist is missing at ",
    env!("KOMGA_WEBUI_DIST_DIR"),
    ". Build the web UI first by running `cd komga-webui && npm run build`."
));

#[cfg(webui_dist_present)]
mod imp {
    use rust_embed::Embed;
    use std::borrow::Cow;

    #[derive(Embed)]
    #[folder = "../../../komga-webui/dist"]
    struct EmbeddedWebUiAssets;

    pub(in crate::operational) struct WebUiAssets;

    impl WebUiAssets {
        pub(in crate::operational) fn get(path: &str) -> Option<Cow<'static, [u8]>> {
            EmbeddedWebUiAssets::get(path).map(|asset| asset.data)
        }
    }
}

#[cfg(not(webui_dist_present))]
mod imp {
    use std::borrow::Cow;

    pub(in crate::operational) struct WebUiAssets;

    impl WebUiAssets {
        pub(in crate::operational) fn get(_path: &str) -> Option<Cow<'static, [u8]>> {
            None
        }
    }
}

pub(super) use imp::WebUiAssets;
