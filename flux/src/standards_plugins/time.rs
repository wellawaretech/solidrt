use rquickjs::{
  function::MutFn,
  Ctx, Exception, Function, JsLifetime, Object, Persistent, Value,
};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::Instant;

use crate::logger::report_uncaught;
use crate::pending::PendingOps;
use crate::plugins::marshal::OptArg;

// ----- Virtual time: embedder-driven timers -----

/// Virtual timer queue, opted into per context by an embedder that drives
/// time explicitly (`install_virtual_time` once, then `advance_virtual_time`
/// per quantum). While installed, setTimeout/setInterval deadlines live on
/// the embedder's timeline instead of tokio's: nothing fires until an advance
/// moves virtual time past it, so pausing the drive pauses the timers, and a
/// frame-clock drive quantizes firing to frames. Two behavioral consequences,
/// both deliberate for a frame-paced app runtime: timer resolution is one
/// advance quantum (a `setTimeout(fn, 0)` runs on the NEXT advance, in
/// registration order, like a task queue turn), and an interval fires at most
/// once per advance (missed periods collapse instead of storming after a
/// pause). Without an install, timers keep the tokio wall-clock path -
/// headless flux is untouched.
///
/// Callbacks are held as Persistents in context userdata, dropped with the
/// context (the safe order; see the flux conventions on Persistent storage).
#[derive(Clone, JsLifetime)]
pub struct VirtualTime(#[qjs(skip_trace)] Rc<VirtualState>);

struct VirtualState {
  now: Cell<f64>,
  seq: Cell<u64>,
  // Firing order: (deadline ms bits, registration seq). Deadlines never go
  // negative, so the f64 bit pattern orders like the number.
  queue: RefCell<BTreeMap<(u64, u64), VirtualEntry>>,
  // id -> callback. Cancellation removes the callback and leaves the queue
  // entry stale (lazy deletion); advance() skips ids with no callback.
  callbacks: RefCell<HashMap<u32, Persistent<Function<'static>>>>,
  // Optional fresh reading of the same timeline the advances report,
  // sampled at schedule time (see set_virtual_now_source). Without one,
  // deadlines anchor to the last advance's reading, which is up to one
  // advance quantum stale at registration - so a timer can fire up to one
  // quantum early in wall terms. An embedder whose timeline has a
  // between-advances reading installs it to get at-least-delay firing.
  now_source: RefCell<Option<Box<dyn Fn() -> f64>>>,
}

struct VirtualEntry {
  id: u32,
  // Some(period): re-arm after firing (setInterval).
  period_ms: Option<f64>,
}

impl VirtualTime {
  fn insert(&self, deadline_ms: f64, id: u32, period_ms: Option<f64>) {
    let seq = self.0.seq.get();
    self.0.seq.set(seq + 1);
    self.0.queue.borrow_mut().insert((deadline_ms.max(0.0).to_bits(), seq), VirtualEntry { id, period_ms });
  }

  fn schedule<'js>(&self, ctx: &Ctx<'js>, cb: Function<'js>, id: u32, delay_ms: f64, period_ms: Option<f64>) {
    self.0.callbacks.borrow_mut().insert(id, Persistent::save(ctx, cb));
    pending(ctx).hold();
    // Deadline base: the fresh reading when a source is installed (never
    // behind the advance timeline - max keeps a lagging source from
    // scheduling into the past), the last advance's reading otherwise.
    let base = match self.0.now_source.borrow().as_ref() {
      Some(f) => f().max(self.0.now.get()),
      None => self.0.now.get(),
    };
    self.insert(base + delay_ms, id, period_ms);
  }

  /// Remove a live timer; false when the id is unknown or already fired.
  fn cancel(&self, ctx: &Ctx<'_>, id: u32) -> bool {
    if self.0.callbacks.borrow_mut().remove(&id).is_some() {
      pending(ctx).release();
      true
    } else {
      false
    }
  }
}

fn pending(ctx: &Ctx<'_>) -> PendingOps {
  ctx.userdata::<PendingOps>().expect("pending ops userdata").clone()
}

/// Put this context's timers on a virtual timeline, seeded at `now_ms` (the
/// same timeline later `advance_virtual_time` calls report). Install before
/// app code runs so every timer the app registers is virtual.
pub fn install_virtual_time(ctx: &Ctx<'_>, now_ms: f64) {
  let state = VirtualTime(Rc::new(VirtualState {
    now: Cell::new(now_ms),
    seq: Cell::new(0),
    queue: RefCell::new(BTreeMap::new()),
    callbacks: RefCell::new(HashMap::new()),
    now_source: RefCell::new(None),
  }));
  ctx.store_userdata(state).expect("store virtual time");
}

/// Give schedule-time deadlines a fresh reading of the virtual timeline (the
/// same one `advance_virtual_time` reports; see VirtualState::now_source).
/// The source must be cheap and must never run JS. No-op without
/// `install_virtual_time`.
pub fn set_virtual_now_source(ctx: &Ctx<'_>, f: impl Fn() -> f64 + 'static) {
  if let Some(vt) = ctx.userdata::<VirtualTime>() {
    *vt.0.now_source.borrow_mut() = Some(Box::new(f));
  }
}

/// Advance virtual time to `now_ms` and fire everything due at or before it,
/// in deadline order (ties in registration order). Timers registered by the
/// fired callbacks - including a re-armed interval - wait for the next
/// advance, so one call is one task-queue turn and can never loop. Time never
/// rewinds: a smaller reading than the current virtual now fires what is due
/// at the current now. No-op without `install_virtual_time`.
pub fn advance_virtual_time(ctx: &Ctx<'_>, now_ms: f64) {
  let Some(vt) = ctx.userdata::<VirtualTime>() else { return };
  let vt = vt.clone();
  if now_ms > vt.0.now.get() {
    vt.0.now.set(now_ms);
  }
  let now = vt.0.now.get();
  let end_seq = vt.0.seq.get();
  loop {
    // Pop one due entry under a short borrow: the callback may register or
    // cancel timers, which borrows the same cells.
    let entry = {
      let mut queue = vt.0.queue.borrow_mut();
      match queue.first_key_value() {
        Some((&(deadline, seq), _)) if f64::from_bits(deadline) <= now && seq < end_seq => {
          queue.pop_first().map(|(_, e)| e)
        }
        _ => None,
      }
    };
    let Some(entry) = entry else { break };
    let persistent = match entry.period_ms {
      // One-shot: consume the callback and its engine-liveness hold.
      None => match vt.0.callbacks.borrow_mut().remove(&entry.id) {
        Some(p) => {
          pending(ctx).release();
          p
        }
        None => continue, // canceled; stale queue entry
      },
      // Interval: keep the callback and re-arm one period past now.
      Some(_) => match vt.0.callbacks.borrow().get(&entry.id) {
        Some(p) => p.clone(),
        None => continue,
      },
    };
    if let Some(period) = entry.period_ms {
      vt.insert(now + period, entry.id, entry.period_ms);
    }
    let Ok(cb) = persistent.restore(ctx) else { continue };
    if let Err(e) = cb.call::<(), ()>(()) {
      report_uncaught(ctx, e, if entry.period_ms.is_some() { "setInterval callback" } else { "setTimeout callback" });
    }
  }
}

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

  fn cancel(&self, ctx: &Ctx<'_>, id: u32) {
    // Virtual mode owns every timer registered while it is installed; ids are
    // allocated from the same counter either way, so an id lives in exactly
    // one of the two stores.
    if let Some(vt) = ctx.userdata::<VirtualTime>() {
      if vt.clone().cancel(ctx, id) {
        return;
      }
    }
    if let Some(tx) = self.active.borrow_mut().remove(&id) {
      let _ = tx.send(());
      self.pending.release();
    }
    // Unknown or already-fired id: a no-op, matching Node and the browser, where
    // clearing a timer that never existed (or has already run) does nothing. The
    // pending op is released on whichever of fire/cancel happens first, so there
    // is nothing to release here.
  }

  fn set_timeout<'js>(&self, ctx: &Ctx<'js>, cb: Function<'js>, ms: u64) -> u32 {
    let id = self.alloc_id();
    if let Some(vt) = ctx.userdata::<VirtualTime>() {
      vt.clone().schedule(ctx, cb, id, ms as f64, None);
      return id;
    }
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
    if let Some(vt) = ctx.userdata::<VirtualTime>() {
      // First fire after one period, like the tokio path's skipped first tick.
      vt.clone().schedule(ctx, cb, id, ms as f64, Some(ms as f64));
      return id;
    }
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

// The engine's native queueMicrotask, stashed in context userdata by
// init_timers before the global is overwritten with the flux wrapper.
// (Userdata rather than a closure capture: rquickjs callback params are
// higher-ranked over 'js, so a captured Function can't unify with them.)
#[derive(JsLifetime)]
struct NativeQueueMicrotask<'js>(Function<'js>);

// Schedule a callback as a microtask by delegating to the engine's native
// queueMicrotask: one job record, no promise machinery. This is the reactive
// scheduler's per-flush path, so the enqueue cost matters. The callback is
// wrapped so a throw is reported as an uncaught error, matching the behavior
// of setTimeout/setInterval rather than falling into rquickjs's raw job-error
// fallback.
fn schedule_microtask<'js>(cb: Function<'js>) -> rquickjs::Result<()> {
  let ctx = cb.ctx().clone();
  let Some(native) = ctx.userdata::<NativeQueueMicrotask>() else {
    return Err(Exception::throw_message(&ctx, "queueMicrotask: timers not initialized"));
  };
  let wrapper = Function::new(
    ctx.clone(),
    MutFn::from(move || {
      if let Err(e) = cb.call::<(), ()>(()) {
        report_uncaught(cb.ctx(), e, "queueMicrotask callback");
      }
    }),
  )?;
  native.0.call::<_, ()>((wrapper,))
}

// Extract a timer id from clearTimeout/clearInterval's argument. Node and the
// browser ignore anything that isn't a live id - a missing argument, undefined,
// null, a non-number, or a number that was never handed out - so any value we
// can't read as a positive integer yields None and the caller does nothing.
// Typing the argument as a raw Value (rather than u32) is what keeps a
// non-number argument from blowing up in conversion before we ever get to decide.
fn timer_id(arg: OptArg<Value<'_>>) -> Option<u32> {
  let v = arg.0?;
  let n = v.as_int().map(|i| i as f64).or_else(|| v.as_float())?;
  if n.is_finite() && n >= 1.0 && n <= u32::MAX as f64 {
    Some(n as u32)
  } else {
    None
  }
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
      move |ctx: Ctx<'_>, id: OptArg<Value<'_>>| {
        if let Some(id) = timer_id(id) {
          timers.cancel(&ctx, id);
        }
      }
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

  let clear_interval = Function::new(
    ctx.clone(),
    MutFn::from(move |ctx: Ctx<'_>, id: OptArg<Value<'_>>| {
      if let Some(id) = timer_id(id) {
        timers.cancel(&ctx, id);
      }
    }),
  )
  .unwrap();

  // Stash the engine's native queueMicrotask before the global is overwritten
  // below - reading it afterwards would recurse into our wrapper.
  let native_queue: Function = globals.get("queueMicrotask").expect("engine queueMicrotask");
  ctx.store_userdata(NativeQueueMicrotask(native_queue)).expect("store native queueMicrotask");
  let queue_microtask = Function::new(ctx.clone(), MutFn::from(|cb: Function<'_>| schedule_microtask(cb))).unwrap();

  globals.set("setTimeout", set_timeout).unwrap();
  globals.set("clearTimeout", clear_timeout).unwrap();
  globals.set("setInterval", set_interval).unwrap();
  globals.set("clearInterval", clear_interval).unwrap();
  globals.set("queueMicrotask", queue_microtask).unwrap();
}

// ----- performance.now() -----

// The process-wide time origin: performance.now() is elapsed ms since it and
// performance.timeOrigin is its wall-clock reading, so timeOrigin + now()
// tracks Date.now() like the browser. Built on tokio's Instant so tokio's
// test clock (pause / advance) drives it and the wall-clock timers above
// together. Process-wide, not per context: an embedder that reloads its app
// keeps one continuous timeline.
static ORIGIN: OnceLock<(Instant, f64)> = OnceLock::new();

fn origin() -> &'static (Instant, f64) {
  ORIGIN.get_or_init(|| {
    let wall = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .map(|d| d.as_secs_f64() * 1000.0)
      .unwrap_or(0.0);
    (Instant::now(), wall)
  })
}

fn perf_now() -> f64 {
  origin().0.elapsed().as_secs_f64() * 1000.0
}

// ----- Timeline: the embedder's frame timeline for native consumers -----

// The app's frame timeline in ms, injected by an embedder that paces frames
// (`.userdata(timeline)`): the timestamp its rAF/render callbacks and virtual
// timers march on. For native consumers that must stay in phase with the
// frames (video frame selection); it deliberately does NOT back
// performance.now(), which stays real elapsed time.
#[derive(Clone, JsLifetime)]
pub struct Timeline {
  #[qjs(skip_trace)]
  now: Arc<dyn Fn() -> f64 + Send + Sync>,
}

impl Timeline {
  pub fn new(f: impl Fn() -> f64 + Send + Sync + 'static) -> Self {
    Self { now: Arc::new(f) }
  }

  pub fn now_ms(&self) -> f64 {
    (self.now)()
  }
}

// The frame timeline reading for native consumers (video sync): the injected
// Timeline when present, real elapsed time otherwise (headless flux has no
// frames to be in phase with).
#[cfg(feature = "video-timeline-pacing")]
pub(crate) fn timeline_now_ms(ctx: &Ctx<'_>) -> f64 {
  match ctx.userdata::<Timeline>() {
    Some(t) => t.now_ms(),
    None => perf_now(),
  }
}

fn init_performance(ctx: &Ctx<'_>) {
  // The engine's performance object can't be patched in place (its
  // properties are non-configurable), so it is replaced wholesale with one
  // on the process-wide origin.
  let performance = Object::new(ctx.clone()).expect("create performance object");
  let now = Function::new(ctx.clone(), perf_now).expect("create performance.now");
  performance.set("now", now).expect("set performance.now");
  performance.set("timeOrigin", origin().1).expect("set performance.timeOrigin");
  ctx.globals().set("performance", performance).expect("set performance global");
}

pub(crate) fn init(ctx: &Ctx<'_>) {
  init_timers(ctx);
  init_performance(ctx);
}
