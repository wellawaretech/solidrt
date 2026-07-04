use flux::report_uncaught;
use flux::rquickjs::module::{Declarations, Exports, ModuleDef};
use flux::rquickjs::{function::MutFn, Ctx, Function};

// The UI event surface: the `on` / `once` exports of the `srt:events` module.
// This sits on top of flux's event bus, which owns the mechanisms (listener
// registry, emit, the sticky cache); the surface's only policy is replaying a
// sticky event's cached value to a new subscriber. Which events are sticky is
// decided at the emit site (flux::emit_sticky vs flux::emit_event). A future
// flux:process would expose its own surface the same way.

pub struct SrtEventsModule;

impl ModuleDef for SrtEventsModule {
  fn declare<'js>(decl: &Declarations<'js>) -> flux::rquickjs::Result<()> {
    decl.declare("on")?;
    decl.declare("once")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> flux::rquickjs::Result<()> {
    exports.export("on", Function::new(ctx.clone(), on_impl)?)?;
    exports.export("once", Function::new(ctx.clone(), once_impl)?)?;
    Ok(())
  }
}

// on(event, callback) -> unsubscribe
// For a sticky event with a cached value, the callback fires synchronously with
// that value before the listener is registered, so the value is observed
// exactly once on subscribe and not duplicated by a later natural emit.
fn on_impl<'js>(event: String, callback: Function<'js>) -> flux::rquickjs::Result<Function<'js>> {
  let ctx = callback.ctx().clone();
  if let Some(value) = flux::sticky_cached(&ctx, &event) {
    if let Err(e) = callback.call::<_, ()>((value,)) {
      report_uncaught(&ctx, e, &format!("on(\"{event}\") listener"));
    }
  }
  flux::register_listener(&ctx, event, callback, false)
}

// once(event, callback) -> unsubscribe
// For a sticky event with a cached value, the callback fires synchronously and
// no persistent listener is registered; the returned unsubscribe is a no-op.
fn once_impl<'js>(event: String, callback: Function<'js>) -> flux::rquickjs::Result<Function<'js>> {
  let ctx = callback.ctx().clone();
  if let Some(value) = flux::sticky_cached(&ctx, &event) {
    if let Err(e) = callback.call::<_, ()>((value,)) {
      report_uncaught(&ctx, e, &format!("once(\"{event}\") listener"));
    }
    return Function::new(ctx, MutFn::from(|_: Ctx<'_>| {}));
  }
  flux::register_listener(&ctx, event, callback, true)
}
