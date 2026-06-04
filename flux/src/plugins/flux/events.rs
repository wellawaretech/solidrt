use crate::logger::report_uncaught;
use crate::pending::PendingOps;
use rquickjs::function::MutFn;
use rquickjs::{Ctx, Function, IntoJs, Persistent, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// The event-bus mechanism: a string-keyed listener registry plus dispatch.
// flux owns the mechanism but imposes no policy - it has no notion of which
// events exist, no JS on/once surface, and no concept of sticky events.
// Consumers (lattice's UI events, a future flux:process) build their own
// surface on top via register_listener + emit_event.

// (id, callback, once)
type Listener = (u32, Persistent<Function<'static>>, bool);

// next_id lives alongside the map so register_listener can reach everything
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
    Self(Rc::new(RefCell::new(ListenerMapInner { map: HashMap::new(), next_id: 1 })))
  }
}

// Stores the listener registry as context userdata. No JS surface is exposed;
// that is the consumer's job.
pub(crate) fn init(ctx: &Ctx<'_>) {
  ctx.store_userdata(ListenerMap::default()).unwrap();
}

// Registers a listener for `event`, returning an unsubscribe function that
// captures only the event name and integer ID, so it cannot keep a JS function
// rooted past listener removal. Safe to call multiple times (second call is a
// no-op).
//
// The first listener for an event name calls pending.hold() so the engine loop
// does not exit while there are active listeners to service; the last removal
// releases it. once=true removes the listener after its first invocation.
pub fn register_listener<'js>(
  ctx: &Ctx<'js>,
  event: String,
  callback: Function<'js>,
  once: bool,
) -> rquickjs::Result<Function<'js>> {
  let persistent = Persistent::save(ctx, callback);
  let id: u32;

  // Scope the userdata borrows so ctx is free to move into Function::new below.
  {
    let store = ctx.userdata::<ListenerMap>().unwrap();
    let pending = ctx.userdata::<PendingOps>().unwrap();
    let mut inner = store.0.borrow_mut();

    id = inner.next_id;
    inner.next_id += 1;

    let is_first_for_event = !inner.map.contains_key(&event);
    inner.map.entry(event.clone()).or_default().push((id, persistent, once));

    if is_first_for_event {
      pending.hold();
    }
  }

  Function::new(
    ctx.clone(),
    MutFn::from(move |ctx: Ctx<'_>| {
      let store = ctx.userdata::<ListenerMap>().unwrap();
      let pending = ctx.userdata::<PendingOps>().unwrap();
      let mut inner = store.0.borrow_mut();
      if let Some(cbs) = inner.map.get_mut(&event) {
        cbs.retain(|(lid, _, _)| *lid != id);
        if cbs.is_empty() {
          inner.map.remove(&event);
          pending.release();
        }
      }
    }),
  )
}

// Whether `event` currently has any registered listeners. Lets a consumer tear
// down an external resource (e.g. an OS signal watcher) once the last listener
// for an event is gone.
pub fn has_listeners(ctx: &Ctx<'_>, event: &str) -> bool {
  let store = ctx.userdata::<ListenerMap>().unwrap();
  let inner = store.0.borrow();
  inner.map.get(event).is_some_and(|cbs| !cbs.is_empty())
}

// Dispatches an event to all registered listeners.
// Called from closures pushed via ExecHandle, so it always runs on the JS thread.
pub fn emit_event<'js, D: IntoJs<'js>>(ctx: &Ctx<'js>, event: &str, data: D) {
  let arg = data.into_js(ctx).unwrap_or_else(|_| Value::new_undefined(ctx.clone()));

  // Snapshot before calling into JS so a listener that mutates the map
  // (e.g. calls its own unsubscribe) does not invalidate iteration.
  // Also remember which entries were once-listeners so we can prune them
  // after dispatch.
  let store = ctx.userdata::<ListenerMap>().unwrap();
  let (snapshot, once_ids): (Vec<Persistent<Function<'static>>>, Vec<u32>) = {
    let inner = store.0.borrow();
    match inner.map.get(event) {
      Some(cbs) => (
        cbs.iter().map(|(_, p, _)| p.clone()).collect(),
        cbs.iter().filter(|(_, _, once)| *once).map(|(id, _, _)| *id).collect(),
      ),
      None => (Vec::new(), Vec::new()),
    }
  };

  for listener in snapshot {
    if let Ok(f) = listener.restore(ctx) {
      if let Err(e) = f.call::<_, ()>((arg.clone(),)) {
        report_uncaught(ctx, e, &format!("\"{event}\" listener"));
      }
    }
  }

  // Prune once-listeners we just fired. A listener could have unsubscribed
  // itself during dispatch; retain() is a no-op for already-removed IDs.
  if !once_ids.is_empty() {
    let pending = ctx.userdata::<PendingOps>().unwrap();
    let mut inner = store.0.borrow_mut();
    if let Some(cbs) = inner.map.get_mut(event) {
      cbs.retain(|(id, _, _)| !once_ids.contains(id));
      if cbs.is_empty() {
        inner.map.remove(event);
        pending.release();
      }
    }
  }
}