use super::AuthUserRole;

#[derive(Clone, Debug)]
pub struct SharedLibrariesInput {
    pub all: bool,
    pub library_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct AuthUserAgeRestrictionInput {
    pub age: i64,
    pub allow_only: bool,
}

#[derive(Clone, Debug)]
pub struct CreateAuthUserInput {
    pub user_id: String,
    pub email: String,
    pub password_hash: String,
    pub roles: Vec<AuthUserRole>,
    pub shared_libraries: SharedLibrariesInput,
    pub labels_allow: Vec<String>,
    pub labels_exclude: Vec<String>,
    pub age_restriction: Option<AuthUserAgeRestrictionInput>,
}

#[derive(Clone, Debug)]
pub struct UpdateAuthUserInput {
    pub roles: Option<Vec<AuthUserRole>>,
    pub shared_libraries: Option<SharedLibrariesInput>,
    pub labels_allow: Option<Vec<String>>,
    pub labels_exclude: Option<Vec<String>>,
    pub age_restriction: Option<Option<AuthUserAgeRestrictionInput>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateAuthUserResult {
    pub updated: bool,
    pub expire_sessions: bool,
}
