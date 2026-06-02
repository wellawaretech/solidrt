use std::cell::RefCell;
use std::rc::Rc;

use flux::report_uncaught;
use flux::rquickjs::{Ctx, Function, JsLifetime, Persistent};

// Per-engine requestAnimationFrame queue, stored in the JS context userdata so
// it is recreated on engine reload. Callbacks are one-shot; flush() swaps the
// queue out before dispatch, so a callback that re-registers runs next frame.
#[derive(Clone, JsLifetime, Default)]
struct RafCallbacks(#[qjs(skip_trace)] Rc<RefCell<RafInner>>);

#[derive(Default)]
struct RafInner {
  next_id: u32,
  pending: Vec<(u32, Persistent<Function<'static>>)>,
}

pub fn init(ctx: &Ctx<'_>) {
  ctx.store_userdata(RafCallbacks::default()).expect("store raf callbacks");

  let globals = ctx.globals();
  let raf = Function::new(ctx.clone(), request_animation_frame).expect("create requestAnimationFrame");
  let caf = Function::new(ctx.clone(), cancel_animation_frame).expect("create cancelAnimationFrame");
  globals.set("requestAnimationFrame", raf).expect("set requestAnimationFrame");
  globals.set("cancelAnimationFrame", caf).expect("set cancelAnimationFrame");
}

fn request_animation_frame<'js>(ctx: Ctx<'js>, callback: Function<'js>) -> u32 {
  let persistent = Persistent::save(&ctx, callback);
  let store = ctx.userdata::<RafCallbacks>().expect("raf callbacks userdata");
  let mut inner = store.0.borrow_mut();
  inner.next_id += 1;
  let id = inner.next_id;
  inner.pending.push((id, persistent));
  id
}

fn cancel_animation_frame(ctx: Ctx<'_>, id: u32) {
  let store = ctx.userdata::<RafCallbacks>().expect("raf callbacks userdata");
  store.0.borrow_mut().pending.retain(|(i, _)| *i != id);
}

// Run every callback registered before this frame, passing the timestamp in ms.
// The queue is taken first so re-registrations during dispatch land next frame.
pub fn flush(ctx: &Ctx<'_>, timestamp: f64) {
  let due = {
    let store = ctx.userdata::<RafCallbacks>().expect("raf callbacks userdata");
    let mut inner = store.0.borrow_mut();
    std::mem::take(&mut inner.pending)
  };
  for (_id, cb) in due {
    if let Ok(f) = cb.restore(ctx) {
      if let Err(e) = f.call::<_, ()>((timestamp,)) {
        report_uncaught(ctx, e, "requestAnimationFrame callback");
      }
    }
  }
}
