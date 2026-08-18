use crate::logger::report_uncaught;
use crate::pending::PendingOps;
use forge::events::{ListenerRegistry, StickyCache};
use rquickjs::function::MutFn;
use rquickjs::{Ctx, Function, IntoJs, Persistent, Value};
use std::cell::RefCell;
use std::rc::Rc;

// Marshalling for the event-bus mechanism: hold the engine-free
// `forge::events::ListenerRegistry` keyed by event name, store JS callbacks as
// Persistent handles, and turn the registry's is_first/is_last signals into
// PendingOps hold/release so the engine loop stays alive while there are
// listeners. The bus also carries the sticky mechanism: emit_sticky caches the
// latest value per event and sticky_cached reads it back for replay on
// subscribe. flux owns these mechanisms but imposes no policy - it has no
// notion of which events exist, no JS on/once surface, and no say in which
// events are sticky. Consumers (lattice's UI events, a future flux:process)
// build their own surface on top via register_listener + emit_event /
// emit_sticky + sticky_cached.

// The registry stores JS functions as Persistent handles. JS functions are
// !Send, so it lives in Rc<RefCell<...>>. JsLifetime + skip_trace lets QuickJS
// store this as context userdata without the GC trying to trace through the Rc.
#[derive(Clone, rquickjs::JsLifetime)]
struct ListenerMap(#[qjs(skip_trace)] Rc<RefCell<ListenerRegistry<Persistent<Function<'static>>>>>);

impl Default for ListenerMap {
  fn default() -> Self {
    Self(Rc::new(RefCell::new(ListenerRegistry::default())))
  }
}

// Sticky events cache their most recent value for replay to late subscribers
// (engine reload, top-level await before render(), late subscribe). The
// engine-free cache lives in forge, generic over the payload; flux
// instantiates it with the engine's value handle, exactly like the listener
// registry above. Whether an event is sticky is decided by whoever emits it
// (emit_sticky vs emit_event).
#[derive(Clone, rquickjs::JsLifetime, Default)]
struct StickyMap(#[qjs(skip_trace)] Rc<RefCell<StickyCache<Persistent<Value<'static>>>>>);

// Stores the listener registry and sticky cache as context userdata. No JS
// surface is exposed; that is the consumer's job.
pub(crate) fn init(ctx: &Ctx<'_>) {
  ctx.store_userdata(ListenerMap::default()).expect("store listener map");
  ctx.store_userdata(StickyMap::default()).expect("store sticky cache");
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
    let (new_id, is_first) = store.0.borrow_mut().insert(event.clone(), persistent, once);
    id = new_id;
    if is_first {
      pending.hold();
    }
  }

  Function::new(
    ctx.clone(),
    MutFn::from(move |ctx: Ctx<'_>| {
      let store = ctx.userdata::<ListenerMap>().unwrap();
      let pending = ctx.userdata::<PendingOps>().unwrap();
      if store.0.borrow_mut().remove(&event, id) {
        pending.release();
      }
    }),
  )
}

// Whether `event` currently has any registered listeners. Lets a consumer tear
// down an external resource (e.g. an OS signal watcher) once the last listener
// for an event is gone.
pub fn has_listeners(ctx: &Ctx<'_>, event: &str) -> bool {
  let store = ctx.userdata::<ListenerMap>().unwrap();
  let reg = store.0.borrow();
  reg.has_listeners(event)
}

// Dispatches an event to all registered listeners.
// Called from closures pushed via ExecHandle, so it always runs on the JS thread.
pub fn emit_event<'js, D: IntoJs<'js>>(ctx: &Ctx<'js>, event: &str, data: D) {
  let arg = data.into_js(ctx).unwrap_or_else(|_| Value::new_undefined(ctx.clone()));

  // Snapshot before calling into JS so a listener that mutates the map (e.g.
  // calls its own unsubscribe) does not invalidate iteration. Also remember the
  // once-listeners so we can prune them after dispatch.
  let store = ctx.userdata::<ListenerMap>().unwrap();
  let (snapshot, once_ids) = store.0.borrow().snapshot(event);

  for listener in snapshot {
    if let Ok(f) = listener.restore(ctx) {
      if let Err(e) = f.call::<_, ()>((arg.clone(),)) {
        report_uncaught(ctx, e, &format!("\"{event}\" listener"));
      }
    }
  }

  // Prune once-listeners we just fired. A listener could have unsubscribed itself
  // during dispatch; prune is a no-op for already-removed IDs.
  if !once_ids.is_empty() {
    let pending = ctx.userdata::<PendingOps>().unwrap();
    if store.0.borrow_mut().prune(event, &once_ids) {
      pending.release();
    }
  }
}

// Emits an event and caches its value as the latest for replay to future
// subscribers (see sticky_cached). Used for events describing current state
// (window size, theme) rather than occurrences; everything else dispatches via
// emit_event directly.
pub fn emit_sticky<'js, D: IntoJs<'js>>(ctx: &Ctx<'js>, event: &str, data: D) {
  let arg = data.into_js(ctx).unwrap_or_else(|_| Value::new_undefined(ctx.clone()));
  {
    let store = ctx.userdata::<StickyMap>().expect("sticky cache userdata");
    store.0.borrow_mut().insert(event.to_string(), Persistent::save(ctx, arg.clone()));
  }
  emit_event(ctx, event, arg);
}

// The cached value of a sticky event restored into the current context, if any
// has been emitted. A subscription surface replays this to a new subscriber so
// it observes the current state without waiting for the next natural emit.
pub fn sticky_cached<'js>(ctx: &Ctx<'js>, event: &str) -> Option<Value<'js>> {
  let store = ctx.userdata::<StickyMap>().expect("sticky cache userdata");
  let persistent = store.0.borrow().get(event).cloned()?;
  persistent.restore(ctx).ok()
}
