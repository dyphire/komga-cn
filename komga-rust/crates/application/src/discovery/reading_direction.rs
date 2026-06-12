#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SeriesReadingDirection {
    LeftToRight,
    RightToLeft,
    Vertical,
    Webtoon,
}

impl SeriesReadingDirection {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "LEFT_TO_RIGHT" => Some(Self::LeftToRight),
            "RIGHT_TO_LEFT" => Some(Self::RightToLeft),
            "VERTICAL" => Some(Self::Vertical),
            "WEBTOON" => Some(Self::Webtoon),
            _ => None,
        }
    }

    pub fn persisted_name(self) -> &'static str {
        match self {
            Self::LeftToRight => "LEFT_TO_RIGHT",
            Self::RightToLeft => "RIGHT_TO_LEFT",
            Self::Vertical => "VERTICAL",
            Self::Webtoon => "WEBTOON",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_renders_persisted_names() {
        let cases = [
            (
                "LEFT_TO_RIGHT",
                SeriesReadingDirection::LeftToRight,
                "LEFT_TO_RIGHT",
            ),
            (
                "RIGHT_TO_LEFT",
                SeriesReadingDirection::RightToLeft,
                "RIGHT_TO_LEFT",
            ),
            ("VERTICAL", SeriesReadingDirection::Vertical, "VERTICAL"),
            ("WEBTOON", SeriesReadingDirection::Webtoon, "WEBTOON"),
        ];

        for (raw, direction, persisted_name) in cases {
            assert_eq!(SeriesReadingDirection::parse(raw), Some(direction));
            assert_eq!(direction.persisted_name(), persisted_name);
        }

        assert_eq!(SeriesReadingDirection::parse("UPSIDE_DOWN"), None);
    }
}
