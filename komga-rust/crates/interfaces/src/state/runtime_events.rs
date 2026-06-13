use std::sync::Arc;

use komga_application::runtime_sse::{
    RuntimeSseEvent, RuntimeSseEventBatch, RuntimeSseEventLog, RuntimeSseEventSink,
    RuntimeSseEventSource, RuntimeSseEventStore, RuntimeSseEventSubscription,
};
use tokio::sync::watch;

pub struct RuntimeSseEventHub {
    store: RuntimeSseEventStore,
    updates: watch::Sender<u64>,
}

impl RuntimeSseEventHub {
    pub fn new() -> Arc<Self> {
        let (updates, _) = watch::channel(0_u64);
        Arc::new(Self {
            store: RuntimeSseEventStore::default(),
            updates,
        })
    }
}

impl RuntimeSseEventSink for RuntimeSseEventHub {
    fn register(&self, event: RuntimeSseEvent) {
        let event_id = self.store.register(event);
        let _ = self.updates.send(event_id);
    }
}

impl RuntimeSseEventLog for RuntimeSseEventHub {
    fn current_cursor(&self) -> u64 {
        self.store.current_cursor()
    }

    fn pending_events(
        &self,
        last_seen_event_id: u64,
        user_id: &str,
        admin: bool,
    ) -> RuntimeSseEventBatch {
        self.store
            .pending_events(last_seen_event_id, user_id, admin)
    }
}

impl RuntimeSseEventSource for RuntimeSseEventHub {
    fn subscribe(&self) -> Box<dyn RuntimeSseEventSubscription> {
        Box::new(RuntimeSseEventHubSubscription {
            updates: self.updates.subscribe(),
        })
    }
}

struct RuntimeSseEventHubSubscription {
    updates: watch::Receiver<u64>,
}

#[async_trait::async_trait]
impl RuntimeSseEventSubscription for RuntimeSseEventHubSubscription {
    async fn changed(&mut self) -> bool {
        self.updates.changed().await.is_ok()
    }
}
