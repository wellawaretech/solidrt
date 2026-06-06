use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use flux::report_uncaught;
use flux::rquickjs::{function::MutFn, Ctx, Function, IntoJs, JsLifetime, Object, Persistent, Value};

// The UI event surface: the `srt.on` / `srt.once` globals plus sticky-event
// policy. This sits on top of flux's neutral event bus (register_listener +
// emit_event); flux itself has no notion of which events exist or of
// stickiness. A future flux:process would expose its own surface the same way.

// Sticky events cache their most recent value and replay it to any new
// subscriber on subscribe. resize and displayRefreshRate are sticky because a
// late subscriber (engine reload, top-level await before render(), late
// subscribe) still needs the current window state.
#[derive(Clone, JsLifetime, Default)]
struct StickyCache(#[qjs(skip_trace)] Rc<RefCell<HashMap<String, Persistent<Value<'static>>>>>);

pub fn init(ctx: &Ctx<'_>) {
  ctx.store_userdata(StickyCache::default()).expect("store sticky cache");

  let on = Function::new(ctx.clone(), on_impl).expect("create srt.on");
  let once = Function::new(ctx.clone(), once_impl).expect("create srt.once");

  let srt = Object::new(ctx.clone()).expect("create srt object");
  srt.set("on", on).expect("set srt.on");
  srt.set("once", once).expect("set srt.once");
  ctx.globals().set("srt", srt).expect("set srt global");
}

// Returns the cached sticky value for `event` restored into the current
// context, if any has been emitted.
fn cached<'js>(ctx: &Ctx<'js>, event: &str) -> Option<Value<'js>> {
  let store = ctx.userdata::<StickyCache>().expect("sticky cache userdata");
  let persistent = store.0.borrow().get(event).cloned()?;
  persistent.restore(ctx).ok()
}

// srt.on(event, callback) -> unsubscribe
// For a sticky event with a cached value, the callback fires synchronously with
// that value before the listener is registered, so the value is observed
// exactly once on subscribe and not duplicated by a later natural emit.
fn on_impl<'js>(event: String, callback: Function<'js>) -> flux::rquickjs::Result<Function<'js>> {
  let ctx = callback.ctx().clone();
  if let Some(value) = cached(&ctx, &event) {
    if let Err(e) = callback.call::<_, ()>((value,)) {
      report_uncaught(&ctx, e, &format!("srt.on(\"{event}\") listener"));
    }
  }
  flux::register_listener(&ctx, event, callback, false)
}

// srt.once(event, callback) -> unsubscribe
// For a sticky event with a cached value, the callback fires synchronously and
// no persistent listener is registered; the returned unsubscribe is a no-op.
fn once_impl<'js>(event: String, callback: Function<'js>) -> flux::rquickjs::Result<Function<'js>> {
  let ctx = callback.ctx().clone();
  if let Some(value) = cached(&ctx, &event) {
    if let Err(e) = callback.call::<_, ()>((value,)) {
      report_uncaught(&ctx, e, &format!("srt.once(\"{event}\") listener"));
    }
    return Function::new(ctx, MutFn::from(|_: Ctx<'_>| {}));
  }
  flux::register_listener(&ctx, event, callback, true)
}

// Emits an event and caches its value as the latest for replay to future
// subscribers. Used only for sticky UI events; everything else dispatches via
// flux::emit_event directly.
pub fn emit_sticky<'js, D: IntoJs<'js>>(ctx: &Ctx<'js>, event: &str, data: D) {
  let arg = data.into_js(ctx).unwrap_or_else(|_| Value::new_undefined(ctx.clone()));
  {
    let store = ctx.userdata::<StickyCache>().expect("sticky cache userdata");
    store.0.borrow_mut().insert(event.to_string(), Persistent::save(ctx, arg.clone()));
  }
  flux::emit_event(ctx, event, arg);
}
