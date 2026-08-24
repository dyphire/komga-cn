use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use komga_application::identity_access::{
    AuthUser, AuthUserSessionSnapshot, RememberMeRuntime, SessionRuntime,
    user_age_restriction_from_persisted_columns, user_from_session_snapshot,
    user_roles_from_persisted_names, user_session_snapshot,
};
use rusqlite::{Connection, OptionalExtension, Row, params};
use sha2::{Digest, Sha256};

use crate::random_hex_token;

pub(crate) fn session_token_store() -> &'static SessionRegistry {
    &SESSION_REGISTRY
}

static SESSION_REGISTRY: LazyLock<SessionRegistry> = LazyLock::new(SessionRegistry::new);

pub(crate) struct SessionRegistry {
    sessions: SessionStore,
    remember_me: RememberMeTokenStore,
}

struct SessionStore {
    counter: AtomicU64,
    sessions: RwLock<HashMap<String, Arc<SessionTokenRecord>>>,
    session_max_inactive_seconds_by_runtime_key: RwLock<HashMap<String, u64>>,
}

struct RememberMeTokenStore {
    remember_me_settings_by_runtime_key: RwLock<HashMap<String, RememberMeRuntimeSettings>>,
    remember_me_database_paths_by_runtime_key: RwLock<HashMap<String, PathBuf>>,
}

#[derive(Clone)]
pub(crate) struct RememberMeRuntimeSettings {
    pub(crate) key: String,
    pub(crate) duration_days: u64,
}

struct SessionTokenRecord {
    user: Option<AuthUserSessionSnapshot>,
    issued_at_epoch_seconds: u64,
    last_accessed_epoch_seconds: AtomicU64,
    runtime_key: String,
    oauth2_authorization_states: Mutex<HashMap<String, String>>,
}

struct UserSharingLabels {
    allow: Vec<String>,
    exclude: Vec<String>,
}

struct UserSharingLabelRow {
    label: String,
    allow: bool,
}

struct PersistedUserCoreRow {
    user: AuthUser,
    age: Option<i64>,
    age_restriction_allow_only: Option<bool>,
}

const DEFAULT_REMEMBER_ME_DURATION_DAYS: u64 = 365;
const SESSION_MAX_INACTIVE_SECONDS: u64 = 30 * 24 * 60 * 60;
const REMEMBER_ME_SIGNATURE_ALGORITHM: &str = "SHA256";

impl SessionRegistry {
    fn new() -> Self {
        Self {
            sessions: SessionStore::new(),
            remember_me: RememberMeTokenStore::new(),
        }
    }
}

impl SessionStore {
    fn new() -> Self {
        Self {
            counter: AtomicU64::new(1),
            sessions: RwLock::new(HashMap::new()),
            session_max_inactive_seconds_by_runtime_key: RwLock::new(HashMap::new()),
        }
    }
}

impl RememberMeTokenStore {
    fn new() -> Self {
        Self {
            remember_me_settings_by_runtime_key: RwLock::new(HashMap::new()),
            remember_me_database_paths_by_runtime_key: RwLock::new(HashMap::new()),
        }
    }
}

impl SessionStore {
    fn session_max_inactive_seconds_for_runtime_key(&self, runtime_key: &str) -> u64 {
        self.session_max_inactive_seconds_by_runtime_key
            .read()
            .expect("session settings lock should not be poisoned")
            .get(runtime_key)
            .cloned()
            .unwrap_or(SESSION_MAX_INACTIVE_SECONDS)
    }

    fn sync_session_settings(&self, runtime_key: &str, max_inactive_seconds: u64) {
        let runtime_key = normalized_runtime_key(runtime_key);
        self.session_max_inactive_seconds_by_runtime_key
            .write()
            .expect("session settings lock should not be poisoned")
            .insert(
                runtime_key,
                normalized_session_max_inactive_seconds(max_inactive_seconds),
            );
    }

    fn invalidate_user_sessions_for_runtime_key(&self, runtime_key: &str, target_user_id: &str) {
        let runtime_key = normalized_runtime_key(runtime_key);
        let mut sessions = self
            .sessions
            .write()
            .expect("session registry lock should not be poisoned");
        sessions.retain(|_, entry| {
            !(entry.runtime_key == runtime_key
                && entry
                    .user
                    .as_ref()
                    .is_some_and(|user| user.id == target_user_id))
        });
    }

    fn store_oauth2_authorization_state(
        &self,
        runtime_key: &str,
        session_token: &str,
        registration_id: &str,
        state: &str,
    ) {
        let runtime_key = normalized_runtime_key(runtime_key);
        let now = now_epoch_seconds();
        let session_max_inactive_seconds =
            self.session_max_inactive_seconds_for_runtime_key(runtime_key.as_str());

        self.remove_session_if_expired(
            session_token,
            runtime_key.as_str(),
            session_max_inactive_seconds,
            now,
        );

        let session = {
            let mut sessions = self
                .sessions
                .write()
                .expect("session registry lock should not be poisoned");
            sessions
                .entry(session_token.to_string())
                .or_insert_with(|| {
                    Arc::new(SessionTokenRecord {
                        user: None,
                        issued_at_epoch_seconds: now,
                        last_accessed_epoch_seconds: AtomicU64::new(now),
                        runtime_key: runtime_key.clone(),
                        oauth2_authorization_states: Mutex::new(HashMap::new()),
                    })
                })
                .clone()
        };

        if session.runtime_key != runtime_key {
            let mut sessions = self
                .sessions
                .write()
                .expect("session registry lock should not be poisoned");
            sessions.insert(
                session_token.to_string(),
                Arc::new(SessionTokenRecord {
                    user: None,
                    issued_at_epoch_seconds: now,
                    last_accessed_epoch_seconds: AtomicU64::new(now),
                    runtime_key,
                    oauth2_authorization_states: Mutex::new(HashMap::from([(
                        registration_id.to_string(),
                        state.to_string(),
                    )])),
                }),
            );
            return;
        }

        session
            .last_accessed_epoch_seconds
            .store(now, Ordering::Relaxed);
        session
            .oauth2_authorization_states
            .lock()
            .expect("oauth2 authorization states lock should not be poisoned")
            .insert(registration_id.to_string(), state.to_string());
    }

    fn take_oauth2_authorization_state(
        &self,
        runtime_key: &str,
        session_token: &str,
        registration_id: &str,
    ) -> Option<String> {
        let runtime_key = normalized_runtime_key(runtime_key);
        let now = now_epoch_seconds();
        let session_max_inactive_seconds =
            self.session_max_inactive_seconds_for_runtime_key(runtime_key.as_str());

        if self.remove_session_if_expired(
            session_token,
            runtime_key.as_str(),
            session_max_inactive_seconds,
            now,
        ) {
            return None;
        }

        let session = self
            .sessions
            .read()
            .expect("session registry lock should not be poisoned")
            .get(session_token)
            .cloned()?;
        if session.runtime_key != runtime_key {
            return None;
        }

        session
            .last_accessed_epoch_seconds
            .store(now, Ordering::Relaxed);
        session
            .oauth2_authorization_states
            .lock()
            .expect("oauth2 authorization states lock should not be poisoned")
            .remove(registration_id)
    }

    fn remove_session_if_expired(
        &self,
        session_token: &str,
        runtime_key: &str,
        session_max_inactive_seconds: u64,
        now: u64,
    ) -> bool {
        let should_remove = self
            .sessions
            .read()
            .expect("session registry lock should not be poisoned")
            .get(session_token)
            .is_some_and(|session| {
                session_record_expired(
                    Some(session.as_ref()),
                    runtime_key,
                    session_max_inactive_seconds,
                    now,
                )
            });
        if !should_remove {
            return false;
        }

        self.sessions
            .write()
            .expect("session registry lock should not be poisoned")
            .remove(session_token);
        true
    }

    fn issue_session_token(&self, user: &AuthUser, runtime_key: &str) -> String {
        let runtime_key = normalized_runtime_key(runtime_key);
        let next = self.counter.fetch_add(1, Ordering::Relaxed);
        let token = format!("komga-session-{next}-{}", random_hex_token(24));
        let now = now_epoch_seconds();
        let mut sessions = self
            .sessions
            .write()
            .expect("session registry lock should not be poisoned");
        sessions.insert(
            token.clone(),
            Arc::new(SessionTokenRecord {
                user: Some(user_session_snapshot(user)),
                issued_at_epoch_seconds: now,
                last_accessed_epoch_seconds: AtomicU64::new(now),
                runtime_key,
                oauth2_authorization_states: Mutex::new(HashMap::new()),
            }),
        );
        token
    }

    fn resolve_session_user(&self, token: &str) -> anyhow::Result<Option<AuthUser>> {
        let Some(session) = self
            .sessions
            .read()
            .expect("session registry lock should not be poisoned")
            .get(token)
            .cloned()
        else {
            return Ok(None);
        };
        let now = now_epoch_seconds();
        let session_max_inactive_seconds =
            self.session_max_inactive_seconds_for_runtime_key(session.runtime_key.as_str());
        let last_seen = session
            .last_accessed_epoch_seconds
            .load(Ordering::Relaxed)
            .max(session.issued_at_epoch_seconds);
        let inactive_for = now.saturating_sub(last_seen);
        if inactive_for >= session_max_inactive_seconds {
            self.remove_session_if_expired(
                token,
                session.runtime_key.as_str(),
                session_max_inactive_seconds,
                now,
            );
            return Ok(None);
        }

        session
            .last_accessed_epoch_seconds
            .store(now, Ordering::Relaxed);
        let Some(user) = session.user.as_ref() else {
            return Ok(None);
        };
        Ok(Some(user_from_session_snapshot(user)))
    }

    fn invalidate_user_sessions(&self, target_user_id: &str) {
        let mut sessions = self
            .sessions
            .write()
            .expect("session registry lock should not be poisoned");
        sessions.retain(|_, entry| {
            entry
                .user
                .as_ref()
                .is_none_or(|user| user.id != target_user_id)
        });
    }

    fn invalidate_session_token(&self, token: &str) {
        let mut sessions = self
            .sessions
            .write()
            .expect("session registry lock should not be poisoned");
        sessions.remove(token);
    }
}

impl RememberMeTokenStore {
    fn remember_me_database_path_for_runtime_key(&self, runtime_key: &str) -> Option<PathBuf> {
        self.remember_me_database_paths_by_runtime_key
            .read()
            .expect("remember-me database paths lock should not be poisoned")
            .get(runtime_key)
            .cloned()
    }

    fn remember_me_settings_for_runtime_key(&self, runtime_key: &str) -> RememberMeRuntimeSettings {
        self.remember_me_settings_by_runtime_key
            .read()
            .expect("remember-me settings lock should not be poisoned")
            .get(runtime_key)
            .cloned()
            .unwrap_or_else(|| default_remember_me_settings(runtime_key))
    }

    fn sync_remember_me_settings(&self, runtime_key: &str, key: &str, duration_days: u64) {
        let runtime_key = normalized_runtime_key(runtime_key);
        self.remember_me_settings_by_runtime_key
            .write()
            .expect("remember-me settings lock should not be poisoned")
            .insert(
                runtime_key,
                RememberMeRuntimeSettings {
                    key: normalized_remember_me_key(key),
                    duration_days: normalized_remember_me_duration_days(duration_days),
                },
            );
    }

    fn remember_me_max_age_seconds(&self, runtime_key: &str) -> u64 {
        let runtime_key = normalized_runtime_key(runtime_key);
        remember_me_duration_days_to_seconds(
            self.remember_me_settings_for_runtime_key(runtime_key.as_str())
                .duration_days,
        )
    }

    fn sync_remember_me_database_path(&self, runtime_key: &str, database_file: &Path) {
        let runtime_key = normalized_runtime_key(runtime_key);
        self.remember_me_database_paths_by_runtime_key
            .write()
            .expect("remember-me database paths lock should not be poisoned")
            .insert(runtime_key, database_file.to_path_buf());
    }

    fn issue_remember_me_token(&self, user: &AuthUser, runtime_key: &str) -> Option<String> {
        let runtime_key = normalized_runtime_key(runtime_key);
        let settings = self.remember_me_settings_for_runtime_key(runtime_key.as_str());
        let expiry_epoch_millis = now_epoch_millis().saturating_add(
            remember_me_duration_days_to_seconds(settings.duration_days).saturating_mul(1000),
        );
        Some(build_remember_me_cookie_value(
            runtime_key.as_str(),
            &user.email,
            expiry_epoch_millis,
            &user.password,
            settings.key.as_str(),
        ))
    }

    fn resolve_remember_me_user(&self, token: &str) -> anyhow::Result<Option<AuthUser>> {
        let Some(parsed_token) = parse_remember_me_token(token) else {
            return Ok(None);
        };
        if parsed_token.algorithm != REMEMBER_ME_SIGNATURE_ALGORITHM {
            return Ok(None);
        }
        if parsed_token.expiry_epoch_millis <= now_epoch_millis() {
            return Ok(None);
        }
        let Some(database_file) =
            self.remember_me_database_path_for_runtime_key(parsed_token.runtime_key())
        else {
            return Ok(None);
        };
        let Some(user) = load_persisted_user_by_login_identifier(
            database_file.as_path(),
            parsed_token.login_identifier.as_str(),
        )?
        else {
            return Ok(None);
        };
        let expected_signature = remember_me_signature(
            parsed_token.login_identifier.as_str(),
            parsed_token.expiry_epoch_millis,
            user.password.as_str(),
            self.remember_me_settings_for_runtime_key(parsed_token.runtime_key())
                .key
                .as_str(),
        );
        if parsed_token.signature != expected_signature {
            return Ok(None);
        }
        Ok(Some(user))
    }

    fn invalidate_remember_me_token(&self, _token: &str) {}
}

impl SessionRegistry {
    pub(crate) fn sync_session_settings(&self, runtime_key: &str, max_inactive_seconds: u64) {
        self.sessions
            .sync_session_settings(runtime_key, max_inactive_seconds);
    }

    pub(crate) fn sync_remember_me_settings(
        &self,
        runtime_key: &str,
        key: &str,
        duration_days: u64,
    ) {
        self.remember_me
            .sync_remember_me_settings(runtime_key, key, duration_days);
    }

    pub(crate) fn remember_me_max_age_seconds(&self, runtime_key: &str) -> u64 {
        self.remember_me.remember_me_max_age_seconds(runtime_key)
    }

    pub(crate) fn sync_remember_me_database_path(&self, runtime_key: &str, database_file: &Path) {
        self.remember_me
            .sync_remember_me_database_path(runtime_key, database_file);
    }

    pub(crate) fn invalidate_user_sessions_for_runtime_key(
        &self,
        runtime_key: &str,
        target_user_id: &str,
    ) {
        self.sessions
            .invalidate_user_sessions_for_runtime_key(runtime_key, target_user_id);
    }

    pub(crate) fn store_oauth2_authorization_state(
        &self,
        runtime_key: &str,
        session_token: &str,
        registration_id: &str,
        state: &str,
    ) {
        self.sessions.store_oauth2_authorization_state(
            runtime_key,
            session_token,
            registration_id,
            state,
        );
    }

    pub(crate) fn take_oauth2_authorization_state(
        &self,
        runtime_key: &str,
        session_token: &str,
        registration_id: &str,
    ) -> Option<String> {
        self.sessions
            .take_oauth2_authorization_state(runtime_key, session_token, registration_id)
    }
}

impl RememberMeRuntime for SessionRegistry {
    fn issue_remember_me_token(&self, user: &AuthUser, runtime_key: &str) -> Option<String> {
        self.remember_me.issue_remember_me_token(user, runtime_key)
    }

    fn resolve_remember_me_user(&self, token: &str) -> anyhow::Result<Option<AuthUser>> {
        self.remember_me.resolve_remember_me_user(token)
    }

    fn invalidate_remember_me_token(&self, token: &str) {
        self.remember_me.invalidate_remember_me_token(token);
    }
}

impl SessionRuntime for SessionRegistry {
    fn issue_session_token(&self, user: &AuthUser, runtime_key: &str) -> String {
        self.sessions.issue_session_token(user, runtime_key)
    }

    fn resolve_session_user(&self, token: &str) -> anyhow::Result<Option<AuthUser>> {
        self.sessions.resolve_session_user(token)
    }

    fn invalidate_user_sessions(&self, target_user_id: &str) {
        self.sessions.invalidate_user_sessions(target_user_id);
    }

    fn invalidate_session_token(&self, token: &str) {
        self.sessions.invalidate_session_token(token);
    }
}

fn session_record_expired(
    session: Option<&SessionTokenRecord>,
    runtime_key: &str,
    session_max_inactive_seconds: u64,
    now: u64,
) -> bool {
    let Some(session) = session else {
        return false;
    };
    if session.runtime_key != runtime_key {
        return true;
    }
    let last_seen = session
        .last_accessed_epoch_seconds
        .load(Ordering::Relaxed)
        .max(session.issued_at_epoch_seconds);
    now.saturating_sub(last_seen) >= session_max_inactive_seconds
}

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn normalized_session_max_inactive_seconds(max_inactive_seconds: u64) -> u64 {
    if max_inactive_seconds == 0 {
        SESSION_MAX_INACTIVE_SECONDS
    } else {
        max_inactive_seconds
    }
}

struct ParsedRememberMeToken {
    runtime_key: String,
    login_identifier: String,
    expiry_epoch_millis: u64,
    algorithm: String,
    signature: String,
}

fn parse_remember_me_token(token: &str) -> Option<ParsedRememberMeToken> {
    let decoded = URL_SAFE_NO_PAD.decode(token.trim()).ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let mut parts = decoded.splitn(5, ':');
    let runtime_key = parts.next()?.trim();
    let login_identifier = parts.next()?.trim();
    let expiry_epoch_millis = parts.next()?.trim().parse::<u64>().ok()?;
    let algorithm = parts.next()?.trim();
    let signature = parts.next()?.trim();
    if runtime_key.is_empty()
        || login_identifier.is_empty()
        || algorithm.is_empty()
        || signature.is_empty()
    {
        return None;
    }
    Some(ParsedRememberMeToken {
        runtime_key: runtime_key.to_string(),
        login_identifier: login_identifier.to_string(),
        expiry_epoch_millis,
        algorithm: algorithm.to_string(),
        signature: signature.to_string(),
    })
}

impl ParsedRememberMeToken {
    fn runtime_key(&self) -> &str {
        self.runtime_key.as_str()
    }
}

fn default_remember_me_settings(runtime_key: &str) -> RememberMeRuntimeSettings {
    RememberMeRuntimeSettings {
        key: format!("remember-me-key-{runtime_key}"),
        duration_days: DEFAULT_REMEMBER_ME_DURATION_DAYS,
    }
}

fn normalized_runtime_key(runtime_key: &str) -> String {
    let runtime_key = runtime_key.trim();
    if runtime_key.is_empty() {
        "default".to_string()
    } else {
        runtime_key.to_string()
    }
}

fn normalized_remember_me_key(key: &str) -> String {
    let key = key.trim();
    if key.is_empty() {
        "remember-me-key-default".to_string()
    } else {
        key.to_string()
    }
}

fn normalized_remember_me_duration_days(duration_days: u64) -> u64 {
    if duration_days == 0 {
        DEFAULT_REMEMBER_ME_DURATION_DAYS
    } else {
        duration_days
    }
}

fn remember_me_duration_days_to_seconds(duration_days: u64) -> u64 {
    normalized_remember_me_duration_days(duration_days).saturating_mul(24 * 60 * 60)
}

fn now_epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn build_remember_me_cookie_value(
    runtime_key: &str,
    login_identifier: &str,
    expiry_epoch_millis: u64,
    password_hash: &str,
    key: &str,
) -> String {
    let signature =
        remember_me_signature(login_identifier, expiry_epoch_millis, password_hash, key);
    URL_SAFE_NO_PAD.encode(format!(
        "{runtime_key}:{login_identifier}:{expiry_epoch_millis}:{REMEMBER_ME_SIGNATURE_ALGORITHM}:{signature}"
    ))
}

fn remember_me_signature(
    login_identifier: &str,
    expiry_epoch_millis: u64,
    password_hash: &str,
    key: &str,
) -> String {
    let normalized_key = normalized_remember_me_key(key);
    let payload =
        format!("{login_identifier}:{expiry_epoch_millis}:{password_hash}:{normalized_key}");
    short_sha256_hex(payload.as_bytes(), 64)
}

fn short_sha256_hex(input: &[u8], length: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let digest = hasher.finalize();
    let encoded = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    encoded.chars().take(length).collect()
}

fn load_persisted_user_by_login_identifier(
    database_file: &Path,
    login_identifier: &str,
) -> anyhow::Result<Option<AuthUser>> {
    let connection = Connection::open(database_file).map_err(anyhow::Error::from)?;
    let persisted = connection
        .query_row(
            "SELECT ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY FROM USER WHERE LOWER(EMAIL) = LOWER(?) LIMIT 1",
            params![login_identifier],
            |row: &Row<'_>| {
                Ok(PersistedUserCoreRow {
                    user: AuthUser {
                        id: row.get(0)?,
                        email: row.get(1)?,
                        password: row.get(2)?,
                        roles: Vec::new(),
                        shared_all_libraries: row.get(3)?,
                        shared_library_ids: Vec::new(),
                        labels_allow: Vec::new(),
                        labels_exclude: Vec::new(),
                        age_restriction: None,
                    },
                    age: row.get(4)?,
                    age_restriction_allow_only: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(anyhow::Error::from)?;
    let Some(persisted) = persisted else {
        return Ok(None);
    };
    let user = persisted.user;
    let age_restriction = user_age_restriction_from_persisted_columns(
        persisted.age,
        persisted.age_restriction_allow_only,
    );

    let roles = query_string_column(
        &connection,
        "SELECT ROLE FROM USER_ROLE WHERE USER_ID = ? ORDER BY ROLE",
        user.id.as_str(),
    )?
    .into_iter()
    .collect::<Vec<_>>();
    let shared_library_ids = query_string_column(
        &connection,
        "SELECT LIBRARY_ID FROM USER_LIBRARY_SHARING WHERE USER_ID = ? ORDER BY LIBRARY_ID",
        user.id.as_str(),
    )?;
    let sharing_labels = query_user_sharing_labels(&connection, user.id.as_str())?;

    Ok(Some(AuthUser {
        roles: user_roles_from_persisted_names(roles),
        shared_library_ids,
        labels_allow: sharing_labels.allow,
        labels_exclude: sharing_labels.exclude,
        age_restriction,
        ..user
    }))
}

fn query_string_column(
    connection: &Connection,
    sql: &str,
    user_id: &str,
) -> anyhow::Result<Vec<String>> {
    let mut statement = connection.prepare(sql).map_err(anyhow::Error::from)?;
    let rows = statement
        .query_map(params![user_id], |row: &Row<'_>| row.get::<_, String>(0))
        .map_err(anyhow::Error::from)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(anyhow::Error::from)
}

fn query_user_sharing_labels(
    connection: &Connection,
    user_id: &str,
) -> anyhow::Result<UserSharingLabels> {
    let mut statement = connection
        .prepare(
            "SELECT LABEL, ALLOW FROM USER_SHARING WHERE USER_ID = ? ORDER BY ALLOW DESC, LABEL",
        )
        .map_err(anyhow::Error::from)?;
    let rows = statement
        .query_map(params![user_id], |row: &Row<'_>| {
            Ok(UserSharingLabelRow {
                label: row.get(0)?,
                allow: row.get(1)?,
            })
        })
        .map_err(anyhow::Error::from)?;
    let mut labels_allow = Vec::new();
    let mut labels_exclude = Vec::new();
    for row in rows {
        let sharing_label = row.map_err(anyhow::Error::from)?;
        if sharing_label.allow {
            labels_allow.push(sharing_label.label);
        } else {
            labels_exclude.push(sharing_label.label);
        }
    }
    Ok(UserSharingLabels {
        allow: labels_allow,
        exclude: labels_exclude,
    })
}
