use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use tower_sessions::MemoryStore;

use super::user::{
    PlaceholderUser, PlaceholderUserSessionSnapshot, user_from_session_snapshot,
    user_session_snapshot,
};

pub(in crate::app) static SESSION_REGISTRY: LazyLock<SessionRegistry> =
    LazyLock::new(SessionRegistry::new);

pub(in crate::app) struct SessionRegistry {
    #[allow(dead_code)]
    store: MemoryStore,
    counter: AtomicU64,
    sessions: Mutex<HashMap<String, SessionEntry>>,
    remember_me_tokens: Mutex<HashMap<String, RememberMeTokenRecord>>,
    remember_me_store_paths_by_namespace: Mutex<HashMap<String, PathBuf>>,
}

#[derive(Clone)]
struct SessionEntry {
    user: PlaceholderUser,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct RememberMeTokenRecord {
    user: PlaceholderUserSessionSnapshot,
    expires_at_epoch_seconds: u64,
    namespace: String,
}

const REMEMBER_ME_MAX_AGE_SECONDS: u64 = 30 * 24 * 60 * 60;
const REMEMBER_ME_STORE_FILE_NAME: &str = "remember-me-tokens.json";

impl SessionRegistry {
    fn new() -> Self {
        Self {
            store: MemoryStore::default(),
            counter: AtomicU64::new(1),
            sessions: Mutex::new(HashMap::new()),
            remember_me_tokens: Mutex::new(HashMap::new()),
            remember_me_store_paths_by_namespace: Mutex::new(HashMap::new()),
        }
    }

    pub(in crate::app) fn configure_remember_me_store(&self, store_root: &Path) -> String {
        let store_file = store_root.join(REMEMBER_ME_STORE_FILE_NAME);
        let namespace = remember_me_namespace_for_path(&store_file);
        {
            let mut paths = self
                .remember_me_store_paths_by_namespace
                .lock()
                .expect("remember-me store paths lock should not be poisoned");
            paths.insert(namespace.clone(), store_file.clone());
        }

        let mut token_guard = self
            .remember_me_tokens
            .lock()
            .expect("remember-me registry lock should not be poisoned");
        token_guard.retain(|_, record| record.namespace != namespace);
        let loaded = load_remember_me_tokens(Some(store_file.as_path()));
        token_guard.extend(loaded);
        prune_expired_tokens(&mut token_guard);
        persist_remember_me_tokens_for_namespace(&namespace, &token_guard, &store_file);
        namespace
    }

    pub(super) fn issue_session_token(&self, user: &PlaceholderUser) -> String {
        let next = self.counter.fetch_add(1, Ordering::Relaxed);
        let token = format!("komga-session-{}-{next}", std::process::id());
        self.sessions
            .lock()
            .expect("session registry lock should not be poisoned")
            .insert(token.clone(), SessionEntry { user: user.clone() });
        token
    }

    pub(super) fn issue_remember_me_token(
        &self,
        user: &PlaceholderUser,
        namespace: &str,
    ) -> Option<String> {
        let store_file = self.remember_me_store_path_for_namespace(namespace)?;
        let next = self.counter.fetch_add(1, Ordering::Relaxed);
        let issued_at = now_epoch_seconds();
        let expires_at = issued_at.saturating_add(REMEMBER_ME_MAX_AGE_SECONDS);
        let token = format!(
            "komga-remember-me-{namespace}-{}-{next}-{expires_at}",
            std::process::id(),
        );

        let mut guard = self
            .remember_me_tokens
            .lock()
            .expect("remember-me registry lock should not be poisoned");
        guard.insert(
            token.clone(),
            RememberMeTokenRecord {
                user: user_session_snapshot(user),
                expires_at_epoch_seconds: expires_at,
                namespace: namespace.to_string(),
            },
        );

        if persist_remember_me_tokens_for_namespace(namespace, &guard, &store_file) {
            Some(token)
        } else {
            guard.remove(&token);
            None
        }
    }

    pub(super) fn resolve_user(&self, token: &str) -> Option<PlaceholderUser> {
        self.sessions
            .lock()
            .expect("session registry lock should not be poisoned")
            .get(token)
            .map(|entry| entry.user.clone())
    }

    pub(super) fn resolve_user_by_remember_me_token(&self, token: &str) -> Option<PlaceholderUser> {
        let namespace = remember_me_namespace_from_token(token)?;
        let store_file = self.remember_me_store_path_for_namespace(namespace.as_str())?;
        let now = now_epoch_seconds();
        let mut guard = self
            .remember_me_tokens
            .lock()
            .expect("remember-me registry lock should not be poisoned");

        if !guard.contains_key(token) {
            guard.extend(load_remember_me_tokens(Some(store_file.as_path())));
        }

        let is_expired = guard
            .get(token)
            .is_some_and(|entry| entry.expires_at_epoch_seconds <= now);
        if is_expired {
            guard.remove(token);
            persist_remember_me_tokens_for_namespace(namespace.as_str(), &guard, &store_file);
            return None;
        }

        guard
            .get(token)
            .map(|entry| user_from_session_snapshot(&entry.user))
    }

    pub(in crate::app) fn invalidate_session_token(&self, token: &str) {
        self.sessions
            .lock()
            .expect("session registry lock should not be poisoned")
            .remove(token);
    }

    pub(in crate::app) fn invalidate_remember_me_token(&self, token: &str) {
        let Some(namespace) = remember_me_namespace_from_token(token) else {
            return;
        };
        let Some(store_file) = self.remember_me_store_path_for_namespace(namespace.as_str()) else {
            return;
        };
        let mut guard = self
            .remember_me_tokens
            .lock()
            .expect("remember-me registry lock should not be poisoned");
        guard.remove(token);
        persist_remember_me_tokens_for_namespace(namespace.as_str(), &guard, &store_file);
    }

    fn remember_me_store_path_for_namespace(&self, namespace: &str) -> Option<PathBuf> {
        self.remember_me_store_paths_by_namespace
            .lock()
            .expect("remember-me store paths lock should not be poisoned")
            .get(namespace)
            .cloned()
    }
}

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn load_remember_me_tokens(path: Option<&Path>) -> HashMap<String, RememberMeTokenRecord> {
    let Some(path) = path else {
        return HashMap::new();
    };

    let Ok(serialized) = fs::read_to_string(path) else {
        return HashMap::new();
    };

    let Ok(records) = serde_json::from_str::<HashMap<String, RememberMeTokenRecord>>(&serialized)
    else {
        return HashMap::new();
    };

    records
}

fn persist_remember_me_tokens(
    path: Option<&Path>,
    tokens: &HashMap<String, RememberMeTokenRecord>,
) -> bool {
    let Some(path) = path else {
        return false;
    };

    if let Some(parent) = path.parent()
        && fs::create_dir_all(parent).is_err()
    {
        return false;
    }

    let Ok(serialized) = serde_json::to_string(tokens) else {
        return false;
    };

    let temporary = path.with_extension("tmp");
    if fs::write(&temporary, serialized).is_err() {
        return false;
    }
    if fs::rename(&temporary, path).is_err() {
        let _ = fs::remove_file(&temporary);
        return false;
    }

    true
}

fn persist_remember_me_tokens_for_namespace(
    namespace: &str,
    tokens: &HashMap<String, RememberMeTokenRecord>,
    store_file: &Path,
) -> bool {
    let scoped_tokens = tokens
        .iter()
        .filter(|(_, record)| record.namespace == namespace)
        .map(|(token, record)| (token.clone(), record.clone()))
        .collect::<HashMap<_, _>>();
    persist_remember_me_tokens(Some(store_file), &scoped_tokens)
}

fn remember_me_namespace_for_path(path: &Path) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn remember_me_namespace_from_token(token: &str) -> Option<String> {
    token
        .strip_prefix("komga-remember-me-")
        .and_then(|suffix| suffix.split('-').next())
        .map(str::trim)
        .filter(|namespace| !namespace.is_empty())
        .map(str::to_string)
}

fn prune_expired_tokens(tokens: &mut HashMap<String, RememberMeTokenRecord>) {
    let now = now_epoch_seconds();
    tokens.retain(|_, entry| entry.expires_at_epoch_seconds > now);
}
