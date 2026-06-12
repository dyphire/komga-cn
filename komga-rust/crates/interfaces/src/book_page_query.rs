use serde::Deserialize;

use crate::media_responses::BookPageResponseOptions;

#[derive(Deserialize, Default)]
pub(crate) struct BookPageQuery {
    #[serde(default)]
    convert: Option<String>,

    #[serde(default)]
    zero_based: bool,

    #[serde(default = "book_page_content_negotiation_default")]
    #[serde(rename = "contentNegotiation")]
    content_negotiation: bool,
}

impl BookPageQuery {
    pub(crate) fn into_response_options(self) -> BookPageResponseOptions {
        BookPageResponseOptions {
            convert: self.convert,
            zero_based: self.zero_based,
            content_negotiation: self.content_negotiation,
        }
    }

    pub(crate) fn into_opds_v1_response_options(self) -> BookPageResponseOptions {
        BookPageResponseOptions {
            convert: self.convert,
            zero_based: true,
            content_negotiation: false,
        }
    }

    pub(crate) fn into_opds_v2_response_options(self) -> BookPageResponseOptions {
        BookPageResponseOptions {
            convert: self.convert,
            zero_based: false,
            content_negotiation: false,
        }
    }
}

fn book_page_content_negotiation_default() -> bool {
    true
}
