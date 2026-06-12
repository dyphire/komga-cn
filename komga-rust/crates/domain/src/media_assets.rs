#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ThumbnailType {
    Generated,
    UserUploaded,
    Sidecar,
}

impl ThumbnailType {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "GENERATED" => Some(Self::Generated),
            "USER_UPLOADED" => Some(Self::UserUploaded),
            "SIDECAR" => Some(Self::Sidecar),
            _ => None,
        }
    }

    pub fn persisted_name(self) -> &'static str {
        match self {
            Self::Generated => "GENERATED",
            Self::UserUploaded => "USER_UPLOADED",
            Self::Sidecar => "SIDECAR",
        }
    }
}
