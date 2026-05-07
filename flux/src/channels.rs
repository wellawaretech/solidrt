use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use rquickjs::JsLifetime;

use crate::logger::Logger;

struct EventSlot {
    buf: std::sync::Mutex<VecDeque<String>>,
    capacity: usize,
    log: bool,
}

pub(crate) struct EventChannels {
    slots: HashMap<String, EventSlot>,
    notify: Arc<tokio::sync::Notify>,
    logger: Logger,
}

impl EventChannels {
    pub(crate) fn new(events: Vec<(String, usize, bool)>, logger: Logger) -> Self {
        let mut slots = HashMap::new();
        for (name, capacity, log) in events {
            slots.insert(
                name,
                EventSlot {
                    buf: std::sync::Mutex::new(VecDeque::with_capacity(capacity)),
                    capacity,
                    log,
                },
            );
        }
        Self {
            slots,
            notify: Arc::new(tokio::sync::Notify::new()),
            logger,
        }
    }

    pub(crate) fn send(&self, event: &str, data: String) -> bool {
        if let Some(slot) = self.slots.get(event) {
            if slot.log {
                self.logger.debug(&format!("emit \"{event}\""));
            }
            let mut buf = slot.buf.lock().unwrap();
            if buf.len() >= slot.capacity {
                buf.pop_front();
            }
            buf.push_back(data);
            self.notify.notify_one();
            true
        } else {
            false
        }
    }

    pub(crate) fn drain_all(&self) -> Vec<(String, String)> {
        let mut events = Vec::new();
        for (name, slot) in &self.slots {
            let mut buf = slot.buf.lock().unwrap();
            while let Some(data) = buf.pop_front() {
                events.push((name.clone(), data));
            }
        }
        events
    }

    pub(crate) fn wakeup(&self) -> Arc<tokio::sync::Notify> {
        self.notify.clone()
    }
}

/// Newtype to store `Arc<EventChannels>` as QuickJS context userdata.
#[derive(Clone, JsLifetime)]
pub(crate) struct SharedEventChannels(
    #[qjs(skip_trace)] pub(crate) Arc<EventChannels>,
);

/// Send-safe handle for emitting events into the engine from other threads.
pub struct EventHandle {
    channels: Arc<EventChannels>,
}

impl EventHandle {
    pub(crate) fn new(channels: Arc<EventChannels>) -> Self {
        Self { channels }
    }

    pub fn emit(&self, event: &str, data: String) {
        if !self.channels.send(event, data) {
            panic!("emit: event \"{event}\" not registered with event_channel()");
        }
    }
}
