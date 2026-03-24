use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use tower_sessions::MemoryStore;

use super::user::PlaceholderUser;

pub(in crate::app) static SESSION_REGISTRY: LazyLock<SessionRegistry> =
    LazyLock::new(SessionRegistry::new);

pub(in crate::app) struct SessionRegistry {
    #[allow(dead_code)]
    store: MemoryStore,
    counter: AtomicU64,
    sessions: Mutex<HashMap<String, PlaceholderUser>>,
}

impl SessionRegistry {
    fn new() -> Self {
        Self {
            store: MemoryStore::default(),
            counter: AtomicU64::new(1),
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn issue_session_token(&self, user: &PlaceholderUser) -> String {
        let next = self.counter.fetch_add(1, Ordering::Relaxed);
        let token = format!("komga-session-{}-{next}", std::process::id());
        self.sessions
            .lock()
            .expect("session registry lock should not be poisoned")
            .insert(token.clone(), user.clone());
        token
    }

    pub(super) fn resolve_user(&self, token: &str) -> Option<PlaceholderUser> {
        self.sessions
            .lock()
            .expect("session registry lock should not be poisoned")
            .get(token)
            .cloned()
    }

    pub(in crate::app) fn invalidate_session_token(&self, token: &str) {
        self.sessions
            .lock()
            .expect("session registry lock should not be poisoned")
            .remove(token);
    }
}
