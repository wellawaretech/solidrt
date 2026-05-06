use rquickjs::function::MutFn;
use rquickjs::{Ctx, Function, JsLifetime, Persistent, Value};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;

use crate::logger::Logger;
use crate::pending::PendingOps;

// ---------------------------------------------------------------------------
// Cross-thread event channels (Send + Sync, used by callers outside the JS thread)
// ---------------------------------------------------------------------------

struct EventSlot {
    buf: std::sync::Mutex<VecDeque<String>>,
    capacity: usize,
    log: bool,
}

pub(crate) struct EventChannels {
    slots: HashMap<String, EventSlot>,
    notify: tokio::sync::Notify,
}

impl EventChannels {
    pub(crate) fn new(events: Vec<(String, usize, bool)>) -> Self {
        let mut slots = HashMap::new();
        for (name, capacity, log) in events {
            slots.insert(name, EventSlot {
                buf: std::sync::Mutex::new(VecDeque::with_capacity(capacity)),
                capacity,
                log,
            });
        }
        Self { slots, notify: tokio::sync::Notify::new() }
    }

    pub(crate) fn send(&self, event: &str, data: String, logger: &Logger) -> bool {
        if let Some(slot) = self.slots.get(event) {
            if slot.log {
                logger.debug(&format!("emit \"{event}\""));
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

    pub(crate) async fn notified(&self) {
        self.notify.notified().await;
    }
}

// ---------------------------------------------------------------------------
// JS-thread event listeners (on / off / emit_event)
// ---------------------------------------------------------------------------

type Listener = (u32, Persistent<Function<'static>>);

#[derive(Clone, JsLifetime)]
struct ListenerMap(#[qjs(skip_trace)] Rc<RefCell<HashMap<String, Vec<Listener>>>>);

impl Default for ListenerMap {
    fn default() -> Self {
        Self(Rc::new(RefCell::new(HashMap::new())))
    }
}

pub(crate) fn init_events(ctx: &Ctx<'_>) {
    ctx.store_userdata(ListenerMap::default()).unwrap();
    let next_id: Rc<Cell<u32>> = Rc::new(Cell::new(1));

    let on_fn = Function::new(
        ctx.clone(),
        MutFn::from(move |event: String, callback: Function<'_>| {
            let ctx = callback.ctx().clone();
            let listeners = ctx.userdata::<ListenerMap>().unwrap();
            let pending = ctx.userdata::<PendingOps>().unwrap();
            let persistent = Persistent::save(&ctx, callback);
            let id = next_id.get();
            next_id.set(id + 1);
            let mut map = listeners.0.borrow_mut();
            let is_new = !map.contains_key(&event);
            map.entry(event).or_default().push((id, persistent));
            if is_new {
                pending.hold();
            }
            id
        }),
    )
    .unwrap();

    let off_fn = Function::new(
        ctx.clone(),
        MutFn::from(move |event: String, id: u32, ctx: Ctx<'_>| {
            let listeners = ctx.userdata::<ListenerMap>().unwrap();
            let pending = ctx.userdata::<PendingOps>().unwrap();
            let mut map = listeners.0.borrow_mut();
            if let Some(cbs) = map.get_mut(&event) {
                cbs.retain(|(lid, _)| *lid != id);
                if cbs.is_empty() {
                    map.remove(&event);
                    pending.release();
                }
            }
        }),
    )
    .unwrap();

    let globals = ctx.globals();
    globals.set("on", on_fn).unwrap();
    globals.set("off", off_fn).unwrap();
}

pub fn emit_event(ctx: &Ctx<'_>, event: &str, data: String) {
    let listeners = ctx.userdata::<ListenerMap>().unwrap();
    let snapshot: Vec<_> = listeners
        .0
        .borrow()
        .get(event)
        .map(|cbs| cbs.iter().map(|(_, p)| p.clone()).collect())
        .unwrap_or_default();
    let arg = ctx
        .json_parse(data)
        .unwrap_or(Value::new_undefined(ctx.clone()));
    for p in snapshot {
        if let Ok(f) = p.restore(ctx) {
            let _ = f.call::<_, ()>((arg.clone(),));
        }
    }
}

/// Drain event channels and dispatch to JS listeners.
pub(crate) fn drain_and_dispatch(ctx: &Ctx<'_>, event_channels: &Arc<EventChannels>) {
    for (event, data) in event_channels.drain_all() {
        emit_event(ctx, &event, data);
    }
}
