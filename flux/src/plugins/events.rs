use rquickjs::function::MutFn;
use rquickjs::{Ctx, Function, Persistent, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::pending::PendingOps;

type Listener = (u32, Persistent<Function<'static>>);

// next_id lives alongside the map so the named on_impl fn can reach both
// through a single userdata lookup.
struct ListenerMapInner {
    map: HashMap<String, Vec<Listener>>,
    next_id: u32,
}

// JS functions are !Send, so the listener map lives in Rc<RefCell<...>>.
// JsLifetime + skip_trace lets QuickJS store this as context userdata
// without the GC trying to trace through the Rc.
#[derive(Clone, rquickjs::JsLifetime)]
struct ListenerMap(#[qjs(skip_trace)] Rc<RefCell<ListenerMapInner>>);

impl Default for ListenerMap {
    fn default() -> Self {
        Self(Rc::new(RefCell::new(ListenerMapInner {
            map: HashMap::new(),
            next_id: 1,
        })))
    }
}

pub(crate) fn init_events(ctx: &Ctx<'_>) {
    ctx.store_userdata(ListenerMap::default()).unwrap();

    let on_fn = Function::new(ctx.clone(), on_impl).unwrap();

    let globals = ctx.globals();
    globals.set("on", on_fn).unwrap();
}

// on(event, callback) -> unsubscribe
// Registers a JS listener and returns an unsubscribe function — call it to
// remove the listener. The first listener for an event name calls pending.hold()
// so the engine loop does not exit while there are active listeners to service.
//
// Named fn (not a closure) because the return type contains a JS lifetime.
// The unsubscribe closure captures only the event name and an integer ID so
// it does not hold a Persistent<Function> alive past the listener's removal.
fn on_impl<'js>(event: String, callback: Function<'js>) -> rquickjs::Result<Function<'js>> {
    let ctx = callback.ctx().clone();
    let persistent = Persistent::save(&ctx, callback);
    let id: u32;

    // Scope the userdata borrows so ctx is free to move into Function::new below.
    {
        let store = ctx.userdata::<ListenerMap>().unwrap();
        let pending = ctx.userdata::<PendingOps>().unwrap();
        let mut inner = store.0.borrow_mut();

        id = inner.next_id;
        inner.next_id += 1;

        let is_first_for_event = !inner.map.contains_key(&event);
        inner.map.entry(event.clone()).or_default().push((id, persistent));

        if is_first_for_event {
            pending.hold();
        }
    }

    // The unsubscribe function captures only the event name and integer ID —
    // no Persistent, so it cannot keep a JS function rooted after removal.
    // Safe to call multiple times (second call is a no-op).
    Function::new(
        ctx,
        MutFn::from(move |ctx: Ctx<'_>| {
            let store = ctx.userdata::<ListenerMap>().unwrap();
            let pending = ctx.userdata::<PendingOps>().unwrap();
            let mut inner = store.0.borrow_mut();
            if let Some(cbs) = inner.map.get_mut(&event) {
                cbs.retain(|(lid, _)| *lid != id);
                if cbs.is_empty() {
                    inner.map.remove(&event);
                    pending.release();
                }
            }
        }),
    )
}

// Dispatches an event to all registered JS listeners.
// Called from closures pushed via ExecHandle, so it always runs on the JS thread.
// data is a JSON string; malformed JSON delivers undefined to listeners.
pub fn emit_event(ctx: &Ctx<'_>, event: &str, data: String) {
    let store = ctx.userdata::<ListenerMap>().unwrap();

    // Snapshot before calling into JS — a listener might call its own
    // unsubscribe function, which would mutate the map under us.
    let snapshot: Vec<Persistent<Function<'static>>> = store
        .0
        .borrow()
        .map
        .get(event)
        .map(|cbs| cbs.iter().map(|(_, p)| p.clone()).collect())
        .unwrap_or_default();

    let arg = ctx
        .json_parse(data)
        .unwrap_or(Value::new_undefined(ctx.clone()));

    for listener in snapshot {
        if let Ok(f) = listener.restore(ctx) {
            let _ = f.call::<_, ()>((arg.clone(),));
        }
    }
}
