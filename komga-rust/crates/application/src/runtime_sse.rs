use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;
use tokio::sync::watch;

const MAX_RUNTIME_SSE_EVENTS: usize = 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeSseEventRecord {
    pub id: u64,
    pub name: String,
    pub payload: Value,
    pub admin_only: bool,
    pub user_id_only: Option<String>,
}

#[derive(Default)]
struct RuntimeSseEventState {
    last_event_id: u64,
    events: VecDeque<RuntimeSseEventRecord>,
}

struct RuntimeSseEventBus {
    state: Mutex<RuntimeSseEventState>,
    updates: watch::Sender<u64>,
}

static RUNTIME_SSE_EVENT_BUS: OnceLock<RuntimeSseEventBus> = OnceLock::new();

fn runtime_sse_event_bus() -> &'static RuntimeSseEventBus {
    RUNTIME_SSE_EVENT_BUS.get_or_init(|| {
        let (updates, _) = watch::channel(0_u64);
        RuntimeSseEventBus {
            state: Mutex::new(RuntimeSseEventState::default()),
            updates,
        }
    })
}

pub fn current_runtime_sse_event_cursor() -> u64 {
    runtime_sse_event_bus()
        .state
        .lock()
        .expect("runtime sse event state lock should not be poisoned")
        .last_event_id
}

pub fn subscribe_runtime_sse_event_updates() -> watch::Receiver<u64> {
    runtime_sse_event_bus().updates.subscribe()
}

pub fn register_runtime_sse_event(
    name: impl Into<String>,
    payload: Value,
    admin_only: bool,
    user_id_only: Option<String>,
) {
    let bus = runtime_sse_event_bus();
    let event_id = {
        let mut state = bus
            .state
            .lock()
            .expect("runtime sse event state lock should not be poisoned");
        state.last_event_id += 1;
        let event_id = state.last_event_id;
        state.events.push_back(RuntimeSseEventRecord {
            id: event_id,
            name: name.into(),
            payload,
            admin_only,
            user_id_only,
        });
        while state.events.len() > MAX_RUNTIME_SSE_EVENTS {
            state.events.pop_front();
        }
        event_id
    };

    let _ = bus.updates.send(event_id);
}

pub fn pending_runtime_sse_events(
    last_seen_event_id: u64,
    user_id: &str,
    admin: bool,
) -> (u64, Vec<RuntimeSseEventRecord>) {
    let state = runtime_sse_event_bus()
        .state
        .lock()
        .expect("runtime sse event state lock should not be poisoned");
    let current_cursor = state.last_event_id;
    let events = state
        .events
        .iter()
        .filter(|event| event.id > last_seen_event_id)
        .filter(|event| !event.admin_only || admin)
        .filter(|event| {
            event
                .user_id_only
                .as_ref()
                .is_none_or(|expected_user_id| expected_user_id == user_id)
        })
        .cloned()
        .collect::<Vec<_>>();

    (current_cursor, events)
}
