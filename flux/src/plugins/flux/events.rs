use crate::logger::report_uncaught;
use crate::pending::PendingOps;
use rquickjs::function::MutFn;
use rquickjs::{Ctx, Function, IntoJs, Object, Persistent, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// Events whose latest value is cached and replayed to any new subscriber.
// Resize is sticky because it represents window state that JS code may need
// to consume after the initial dispatch happened (engine reload, top-level
// await before render(), late subscribe, etc.).
const STICKY_EVENTS: &[&str] = &["resize"];

fn is_sticky(event: &str) -> bool {
  STICKY_EVENTS.contains(&event)
}

// (id, callback, once)
type Listener = (u32, Persistent<Function<'static>>, bool);

// next_id lives alongside the maps so the named on_impl fn can reach
// everything through a single userdata lookup.
struct ListenerMapInner {
  map: HashMap<String, Vec<Listener>>,
  sticky: HashMap<String, Persistent<Value<'static>>>,
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
      sticky: HashMap::new(),
      next_id: 1,
    })))
  }
}

pub(crate) fn init_events<'js>(ctx: &Ctx<'js>, flux: &Object<'js>) {
  ctx.store_userdata(ListenerMap::default()).unwrap();

  let on_fn = Function::new(ctx.clone(), on_impl).unwrap();
  flux.set("on", on_fn).unwrap();

  let once_fn = Function::new(ctx.clone(), once_impl).unwrap();
  flux.set("once", once_fn).unwrap();
}

// on(event, callback) -> unsubscribe
// Registers a JS listener and returns an unsubscribe function. The first
// listener for an event name calls pending.hold() so the engine loop does
// not exit while there are active listeners to service.
//
// For sticky events, the most recently emitted value is delivered to the
// new listener synchronously before this function returns.
fn on_impl<'js>(event: String, callback: Function<'js>) -> rquickjs::Result<Function<'js>> {
  let ctx = callback.ctx().clone();
  register(&ctx, event, callback, false)
}

// once(event, callback) -> unsubscribe
// Same as on(), except the listener is removed after its first invocation.
// For sticky events with a cached value, the callback fires synchronously
// during this call and no persistent listener is registered; the returned
// unsubscribe is then a no-op.
fn once_impl<'js>(event: String, callback: Function<'js>) -> rquickjs::Result<Function<'js>> {
  let ctx = callback.ctx().clone();

  if is_sticky(&event) {
    let cached = {
      let store = ctx.userdata::<ListenerMap>().unwrap();
      let inner = store.0.borrow();
      inner.sticky.get(&event).cloned()
    };
    if let Some(persistent_value) = cached {
      if let Ok(value) = persistent_value.restore(&ctx) {
        if let Err(e) = callback.call::<_, ()>((value,)) {
          report_uncaught(&ctx, e, &format!("once(\"{event}\") listener"));
        }
        return Function::new(ctx, MutFn::from(|_: Ctx<'_>| {}));
      }
    }
  }

  register(&ctx, event, callback, true)
}

// Shared registration path for on/once. Returns an unsubscribe function
// that captures only the event name and integer ID, so it cannot keep a
// JS function rooted past listener removal. Safe to call multiple times
// (second call is a no-op).
fn register<'js>(
  ctx: &Ctx<'js>,
  event: String,
  callback: Function<'js>,
  once: bool,
) -> rquickjs::Result<Function<'js>> {
  // Sticky replay for non-once subscribers happens BEFORE registering so
  // the cached value is observed exactly once on subscribe, not duplicated
  // by a subsequent natural emit.
  if !once && is_sticky(&event) {
    let cached = {
      let store = ctx.userdata::<ListenerMap>().unwrap();
      let inner = store.0.borrow();
      inner.sticky.get(&event).cloned()
    };
    if let Some(persistent_value) = cached {
      if let Ok(value) = persistent_value.restore(ctx) {
        if let Err(e) = callback.call::<_, ()>((value,)) {
          report_uncaught(ctx, e, &format!("on(\"{event}\") listener"));
        }
      }
    }
  }

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

// Dispatches an event to all registered JS listeners.
// Called from closures pushed via ExecHandle, so it always runs on the JS thread.
pub fn emit_event<'js, D: IntoJs<'js>>(ctx: &Ctx<'js>, event: &str, data: D) {
  let arg = data
    .into_js(ctx)
    .unwrap_or_else(|_| Value::new_undefined(ctx.clone()));

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

  if is_sticky(event) {
    let persistent_arg = Persistent::save(ctx, arg.clone());
    store.0.borrow_mut().sticky.insert(event.to_string(), persistent_arg);
  }

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