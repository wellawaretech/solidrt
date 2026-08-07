// Virtual time: setTimeout/setInterval on the embedder-driven timeline
// (install_virtual_time / advance_virtual_time), driven through the real JS
// globals. The tokio wall-clock path is untouched by design - virtual mode
// only exists where an embedder installed it - so these tests cover the
// virtual branch only.

use rquickjs::{Context, Ctx, Runtime};

use crate::pending::PendingOps;
use crate::plugins::standards::time::{advance_virtual_time, install_virtual_time};

fn with_virtual_ctx(f: impl FnOnce(&Ctx<'_>)) {
  let rt = Runtime::new().expect("js runtime");
  let context = Context::full(&rt).expect("js context");
  context.with(|ctx| {
    ctx.store_userdata(PendingOps::new()).expect("store pending ops");
    crate::plugins::standards::time::init(&ctx);
    install_virtual_time(&ctx, 0.0);
    ctx.eval::<(), _>("globalThis.log = []").expect("init log");
    f(&ctx);
  });
}

fn log(ctx: &Ctx<'_>) -> String {
  ctx.eval("log.join(',')").expect("read log")
}

#[test]
fn timeout_fires_at_deadline_once() {
  with_virtual_ctx(|ctx| {
    ctx.eval::<(), _>("setTimeout(() => log.push('a'), 100)").expect("register");
    advance_virtual_time(ctx, 99.0);
    assert_eq!(log(ctx), "");
    advance_virtual_time(ctx, 100.0);
    assert_eq!(log(ctx), "a");
    advance_virtual_time(ctx, 500.0);
    assert_eq!(log(ctx), "a");
  });
}

#[test]
fn due_timers_fire_in_deadline_order() {
  with_virtual_ctx(|ctx| {
    ctx
      .eval::<(), _>("setTimeout(() => log.push('late'), 100); setTimeout(() => log.push('early'), 60)")
      .expect("register");
    // One advance past both deadlines: deadline order, not registration order.
    advance_virtual_time(ctx, 100.0);
    assert_eq!(log(ctx), "early,late");
  });
}

#[test]
fn interval_fires_once_per_advance_and_rearms() {
  with_virtual_ctx(|ctx| {
    ctx.eval::<(), _>("setInterval(() => log.push('i'), 50)").expect("register");
    // Four periods elapse in one advance: missed periods collapse into one
    // firing instead of storming.
    advance_virtual_time(ctx, 200.0);
    assert_eq!(log(ctx), "i");
    // Re-armed one period past the advance that fired it.
    advance_virtual_time(ctx, 249.0);
    assert_eq!(log(ctx), "i");
    advance_virtual_time(ctx, 250.0);
    assert_eq!(log(ctx), "i,i");
  });
}

#[test]
fn clear_timeout_and_interval_cancel() {
  with_virtual_ctx(|ctx| {
    ctx
      .eval::<(), _>(
        "let t = setTimeout(() => log.push('t'), 50); let i = setInterval(() => log.push('i'), 50); \
         clearTimeout(t); clearInterval(i)",
      )
      .expect("register and cancel");
    advance_virtual_time(ctx, 1000.0);
    assert_eq!(log(ctx), "");
  });
}

#[test]
fn registration_during_callback_waits_for_next_advance() {
  with_virtual_ctx(|ctx| {
    // A zero-delay self-rescheduling timeout: one firing per advance, like
    // one task-queue turn - never a loop within a single advance.
    ctx
      .eval::<(), _>("function again() { log.push('x'); setTimeout(again, 0) } setTimeout(again, 0)")
      .expect("register");
    advance_virtual_time(ctx, 16.0);
    assert_eq!(log(ctx), "x");
    advance_virtual_time(ctx, 32.0);
    assert_eq!(log(ctx), "x,x");
  });
}

#[test]
fn cancel_inside_callback_stops_a_due_interval() {
  with_virtual_ctx(|ctx| {
    // Both due in the same advance; the first callback cancels the second
    // before it fires - the stale queue entry must be skipped.
    ctx
      .eval::<(), _>("globalThis.i = setInterval(() => log.push('i'), 60); setTimeout(() => { log.push('t'); clearInterval(i) }, 50)")
      .expect("register");
    advance_virtual_time(ctx, 100.0);
    assert_eq!(log(ctx), "t");
    advance_virtual_time(ctx, 1000.0);
    assert_eq!(log(ctx), "t");
  });
}

#[test]
fn time_never_rewinds() {
  with_virtual_ctx(|ctx| {
    advance_virtual_time(ctx, 100.0);
    ctx.eval::<(), _>("setTimeout(() => log.push('a'), 50)").expect("register");
    // A smaller reading than the current virtual now must not rewind the
    // timeline (deadline stays 150).
    advance_virtual_time(ctx, 40.0);
    assert_eq!(log(ctx), "");
    advance_virtual_time(ctx, 150.0);
    assert_eq!(log(ctx), "a");
  });
}
