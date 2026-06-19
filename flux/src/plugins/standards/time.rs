use rquickjs::{
  function::{MutFn, This},
  Ctx, Function, JsLifetime, Object,
};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::Instant;

use crate::logger::report_uncaught;
use crate::pending::PendingOps;

// ----- Scheduling: setTimeout / setInterval / queueMicrotask -----

type ActiveMap = Rc<std::cell::RefCell<HashMap<u32, oneshot::Sender<()>>>>;

#[derive(Clone)]
pub(crate) struct Timers {
  next_id: Rc<Cell<u32>>,
  active: ActiveMap,
  pending: PendingOps,
}

impl Timers {
  pub fn new(ctx: &Ctx<'_>) -> Self {
    Self {
      next_id: Rc::new(Cell::new(1)),
      active: Rc::new(std::cell::RefCell::new(HashMap::new())),
      pending: ctx.userdata::<PendingOps>().unwrap().clone(),
    }
  }

  fn alloc_id(&self) -> u32 {
    let id = self.next_id.get();
    self.next_id.set(id + 1);
    id
  }

  fn remove(&self, id: u32) {
    self.active.borrow_mut().remove(&id);
    self.pending.release();
  }

  fn cancel<'js>(&self, ctx: &Ctx<'js>, id: u32) -> rquickjs::Result<()> {
    let tx = self.active.borrow_mut().remove(&id);
    match tx {
      Some(tx) => {
        let _ = tx.send(());
        self.pending.release();
        Ok(())
      }
      None => {
        Err(ctx.throw(rquickjs::String::from_str(ctx.clone(), &format!("invalid timer id: {id}")).unwrap().into()))
      }
    }
  }

  fn set_timeout<'js>(&self, ctx: &Ctx<'js>, cb: Function<'js>, ms: u64) -> u32 {
    let id = self.alloc_id();
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    self.active.borrow_mut().insert(id, cancel_tx);
    self.pending.hold();
    let timers = self.clone();
    ctx.spawn(async move {
      tokio::select! {
          _ = tokio::time::sleep(Duration::from_millis(ms)) => {
              timers.remove(id);
              if let Err(e) = cb.call::<(), ()>(()) {
                report_uncaught(cb.ctx(), e, "setTimeout callback");
              }
          }
          _ = cancel_rx => {}
      }
    });
    id
  }

  fn set_interval<'js>(&self, ctx: &Ctx<'js>, cb: Function<'js>, ms: u64) -> u32 {
    let id = self.alloc_id();
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    self.active.borrow_mut().insert(id, cancel_tx);
    self.pending.hold();
    ctx.spawn(async move {
      let mut interval = tokio::time::interval(Duration::from_millis(ms));
      interval.tick().await; // skip immediate first tick
      tokio::select! {
          _ = async {
              loop {
                  interval.tick().await;
                  if let Err(e) = cb.call::<(), ()>(()) {
                    report_uncaught(cb.ctx(), e, "setInterval callback");
                  }
              }
          } => {}
          _ = cancel_rx => {}
      }
    });
    id
  }
}

// Schedule a callback as a microtask. An already-resolved promise's `then`
// reaction runs on the job (microtask) queue, which is the timing we want. The
// callback is wrapped so a throw is reported as an uncaught error, matching the
// behavior of setTimeout/setInterval rather than being swallowed into an
// unhandled promise rejection.
fn schedule_microtask<'js>(cb: Function<'js>) -> rquickjs::Result<()> {
  let ctx = cb.ctx().clone();
  let (promise, resolve, _reject) = ctx.promise()?;
  resolve.call::<_, ()>(())?;

  let wrapper = Function::new(
    ctx.clone(),
    MutFn::from(move || {
      if let Err(e) = cb.call::<(), ()>(()) {
        report_uncaught(cb.ctx(), e, "queueMicrotask callback");
      }
    }),
  )?;

  let then = promise.then()?;
  then.call::<_, ()>((This(promise), wrapper))?;
  Ok(())
}

fn init_timers(ctx: &Ctx<'_>) {
  let timers = Timers::new(ctx);
  let globals = ctx.globals();

  let set_timeout = Function::new(
    ctx.clone(),
    MutFn::from({
      let timers = timers.clone();
      move |cb: Function<'_>, ms: u64| -> u32 {
        let ctx = cb.ctx().clone();
        timers.set_timeout(&ctx, cb, ms)
      }
    }),
  )
  .unwrap();

  let clear_timeout = Function::new(
    ctx.clone(),
    MutFn::from({
      let timers = timers.clone();
      move |ctx: Ctx<'_>, id: u32| timers.cancel(&ctx, id)
    }),
  )
  .unwrap();

  let set_interval = Function::new(
    ctx.clone(),
    MutFn::from({
      let timers = timers.clone();
      move |cb: Function<'_>, ms: u64| -> u32 {
        let ctx = cb.ctx().clone();
        timers.set_interval(&ctx, cb, ms)
      }
    }),
  )
  .unwrap();

  let clear_interval =
    Function::new(ctx.clone(), MutFn::from(move |ctx: Ctx<'_>, id: u32| timers.cancel(&ctx, id))).unwrap();

  let queue_microtask = Function::new(ctx.clone(), MutFn::from(|cb: Function<'_>| schedule_microtask(cb))).unwrap();

  globals.set("setTimeout", set_timeout).unwrap();
  globals.set("clearTimeout", clear_timeout).unwrap();
  globals.set("setInterval", set_interval).unwrap();
  globals.set("clearInterval", clear_interval).unwrap();
  globals.set("queueMicrotask", queue_microtask).unwrap();
}

// ----- Clock: performance.now() -----

// Process-wide monotonic origin, used when no Clock is injected. Built on
// tokio's Instant so tokio's test clock (pause / advance) drives the timers
// above and performance.now() together as one swappable clock.
static ORIGIN: OnceLock<Instant> = OnceLock::new();

fn default_now_ms() -> f64 {
  ORIGIN.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

// A high-resolution clock an embedder can inject via the engine builder
// (`.userdata(clock)`), so performance.now() and other time sources (e.g. the
// requestAnimationFrame timestamp) report on one shared origin. Without one,
// performance.now() falls back to the process-wide monotonic origin above.
#[derive(Clone, JsLifetime)]
pub struct Clock {
  #[qjs(skip_trace)]
  now: Arc<dyn Fn() -> f64 + Send + Sync>,
}

impl Clock {
  pub fn new(f: impl Fn() -> f64 + Send + Sync + 'static) -> Self {
    Self { now: Arc::new(f) }
  }

  pub fn now_ms(&self) -> f64 {
    (self.now)()
  }
}

fn perf_now(ctx: Ctx<'_>) -> f64 {
  match ctx.userdata::<Clock>() {
    Some(clock) => clock.now_ms(),
    None => default_now_ms(),
  }
}

fn init_performance(ctx: &Ctx<'_>) {
  let performance = Object::new(ctx.clone()).expect("create performance object");
  let now = Function::new(ctx.clone(), perf_now).expect("create performance.now");
  performance.set("now", now).expect("set performance.now");
  ctx.globals().set("performance", performance).expect("set performance global");
}

pub(crate) fn init(ctx: &Ctx<'_>) {
  init_timers(ctx);
  init_performance(ctx);
}
