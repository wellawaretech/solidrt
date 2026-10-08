use rquickjs::{function::MutFn, Ctx, Exception, Function, JsLifetime, Object, Persistent, Value};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::Instant;

use crate::logger::report_uncaught;
use crate::pending::{Hold, PendingOps};
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
  // id -> callback and the timer's standing hold on the engine.
  // Cancellation removes the callback and leaves the queue entry stale
  // (lazy deletion); advance() skips ids with no callback.
  callbacks: RefCell<HashMap<u32, (Persistent<Function<'static>>, Hold)>>,
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
    let hold = PendingOps::of(ctx).standing(if period_ms.is_some() { "interval" } else { "timer" });
    self.0.callbacks.borrow_mut().insert(id, (Persistent::save(ctx, cb), hold));
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
  fn cancel(&self, id: u32) -> bool {
    self.0.callbacks.borrow_mut().remove(&id).is_some()
  }

  /// Whether a live timer is due at or before `now_ms`.
  fn due(&self, now_ms: f64) -> bool {
    let callbacks = self.0.callbacks.borrow();
    self
      .0
      .queue
      .borrow()
      .iter()
      .take_while(|((deadline, _), _)| f64::from_bits(*deadline) <= now_ms)
      .any(|(_, entry)| callbacks.contains_key(&entry.id))
  }
}

/// Whether a timer of this context is due now and has not fired: on a
/// virtual timeline a timer fires with the next advance, so until that comes
/// the app has work waiting that no frame request stands for. "Now" is the
/// now-source's reading (the last advance's without one). False without
/// `install_virtual_time`: wall timers fire by themselves.
pub fn timer_due(ctx: &Ctx<'_>) -> bool {
  let Some(vt) = ctx.userdata::<VirtualTime>() else { return false };
  let now = match vt.0.now_source.borrow().as_ref() {
    Some(source) => source().max(vt.0.now.get()),
    None => vt.0.now.get(),
  };
  vt.due(now)
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

/// Advance virtual time to the now-source's current reading (see
/// `set_virtual_now_source`) and fire what is due: a task-queue turn for a
/// host whose frame clock has stopped driving `advance_virtual_time` (a
/// lifecycle hook running while the platform pump is blocked). No-op without
/// a source.
pub fn advance_virtual_time_to_now(ctx: &Ctx<'_>) {
  let Some(vt) = ctx.userdata::<VirtualTime>() else { return };
  let now = match vt.0.now_source.borrow().as_ref() {
    Some(source) => source(),
    None => return,
  };
  advance_virtual_time(ctx, now);
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
      // One-shot: consume the callback, and with it the timer's hold.
      None => match vt.0.callbacks.borrow_mut().remove(&entry.id) {
        Some((p, _hold)) => p,
        None => continue, // canceled; stale queue entry
      },
      // Interval: keep the callback and re-arm one period past now.
      Some(_) => match vt.0.callbacks.borrow().get(&entry.id) {
        Some((p, _hold)) => p.clone(),
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

// ----- Scheduling: setTimeout / setInterval / setImmediate / queueMicrotask -----

// id -> the cancel signal of a wall-clock timer or an immediate and its hold
// on the engine, released with the entry: on fire or cancel, whichever is
// first.
type ActiveMap = Rc<RefCell<HashMap<u32, (oneshot::Sender<()>, Hold)>>>;

// The scheduling state of a context, one id space for everything it hands
// out (timers, immediates, idle callbacks): an id lives in exactly one
// store, so a cancel can try each in turn. In context userdata so the frame
// protocol reaches the idle queue.
#[derive(Clone, JsLifetime)]
pub(crate) struct Timers {
  #[qjs(skip_trace)]
  next_id: Rc<Cell<u32>>,
  #[qjs(skip_trace)]
  active: ActiveMap,
  #[qjs(skip_trace)]
  idle: Rc<RefCell<IdleQueue>>,
  #[qjs(skip_trace)]
  pending: PendingOps,
}

impl Timers {
  pub fn new(ctx: &Ctx<'_>) -> Self {
    Self {
      next_id: Rc::new(Cell::new(1)),
      active: Rc::new(RefCell::new(HashMap::new())),
      idle: Rc::new(RefCell::new(IdleQueue::default())),
      pending: PendingOps::of(ctx),
    }
  }

  fn alloc_id(&self) -> u32 {
    let id = self.next_id.get();
    self.next_id.set(id + 1);
    id
  }

  fn remove(&self, id: u32) {
    self.active.borrow_mut().remove(&id);
  }

  fn cancel(&self, ctx: &Ctx<'_>, id: u32) {
    // Virtual mode owns every timer registered while it is installed; ids are
    // allocated from the same counter either way, so an id lives in exactly
    // one of the two stores.
    if let Some(vt) = ctx.userdata::<VirtualTime>() {
      if vt.clone().cancel(id) {
        return;
      }
    }
    if let Some((tx, _hold)) = self.active.borrow_mut().remove(&id) {
      let _ = tx.send(());
    }
    // Unknown or already-fired id: a no-op, matching Node and the browser, where
    // clearing a timer that never existed (or has already run) does nothing.
  }

  fn set_timeout<'js>(&self, ctx: &Ctx<'js>, cb: Function<'js>, ms: u64) -> u32 {
    let id = self.alloc_id();
    if let Some(vt) = ctx.userdata::<VirtualTime>() {
      vt.clone().schedule(ctx, cb, id, ms as f64, None);
      return id;
    }
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    self.active.borrow_mut().insert(id, (cancel_tx, self.pending.standing("timer")));
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

  // Run `cb` on the next turn of the engine loop: after the current task
  // and its microtasks, ahead of nothing in particular. Not a timer: it is
  // never on the virtual timeline, so in a frame-paced runtime it runs
  // between frames and a paused clock does not hold it. The task yields
  // once before running, which is what lets a chain of immediates
  // interleave with the loop's other work (a frame, an event): the
  // engine's scheduler hands control back when a task re-queues itself,
  // but never between a task that completes and the one it spawned.
  fn set_immediate<'js>(&self, ctx: &Ctx<'js>, cb: Function<'js>) -> u32 {
    let id = self.alloc_id();
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    // In flight, not standing: an immediate completes by itself, so what
    // waits for the app to come to rest waits for it.
    self.active.borrow_mut().insert(id, (cancel_tx, self.pending.in_flight("immediate")));
    let timers = self.clone();
    ctx.spawn(async move {
      tokio::select! {
          _ = tokio::task::yield_now() => {
              timers.remove(id);
              if let Err(e) = cb.call::<(), ()>(()) {
                report_uncaught(cb.ctx(), e, "setImmediate callback");
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
    self.active.borrow_mut().insert(id, (cancel_tx, self.pending.standing("interval")));
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

// ----- Idle callbacks: requestIdleCallback / cancelIdleCallback -----

// The longest idle period an idle callback is given, in ms: the Background
// Tasks spec's cap on timeRemaining(), and the whole budget of a period
// with no frame due (headless flux, a stepped host between its frames).
const IDLE_PERIOD_MAX_MS: f64 = 50.0;

struct IdleEntry {
  id: u32,
  callback: Persistent<Function<'static>>,
  // The timer that runs the callback with didTimeout if no idle period got
  // to it first (the `timeout` option); cancelled when a period does.
  timeout_timer: Option<u32>,
  // Standing: the callback waits for an idle period, which only a frame (or
  // the next turn, headless) brings; a settle steps that frame itself.
  _hold: Hold,
}

#[derive(Default)]
struct IdleQueue {
  // Registration order, which is the order the periods run them in.
  entries: Vec<IdleEntry>,
  // Whether a frame driver hands out the idle periods (see
  // `install_idle_periods`). Without one, a registration schedules its own
  // period on the next engine turn: headless flux has no frames to be idle
  // between, so the next turn is as idle as it gets.
  frame_driven: bool,
}

// The IdleDeadline handed to a callback: `timeRemaining()` counts down to
// the period's end, `didTimeout` says the timeout ran it instead.
fn idle_deadline<'js>(ctx: Ctx<'js>, until: Instant, did_timeout: bool) -> rquickjs::Result<Object<'js>> {
  let deadline = Object::new(ctx.clone())?;
  deadline.set("didTimeout", did_timeout)?;
  let time_remaining =
    Function::new(ctx, move || until.saturating_duration_since(Instant::now()).as_secs_f64() * 1000.0)?;
  deadline.set("timeRemaining", time_remaining)?;
  Ok(deadline)
}

impl Timers {
  fn request_idle_callback<'js>(
    &self,
    ctx: &Ctx<'js>,
    cb: Function<'js>,
    timeout_ms: Option<u64>,
  ) -> rquickjs::Result<u32> {
    let id = self.alloc_id();
    let timeout_timer = match timeout_ms {
      Some(ms) => {
        let timers = self.clone();
        let timed_out = Function::new(ctx.clone(), MutFn::from(move |ctx: Ctx<'_>| timers.idle_timed_out(&ctx, id)))?;
        Some(self.set_timeout(ctx, timed_out, ms))
      }
      None => None,
    };
    let hold = self.pending.standing("idle callback");
    let mut idle = self.idle.borrow_mut();
    idle.entries.push(IdleEntry { id, callback: Persistent::save(ctx, cb), timeout_timer, _hold: hold });
    if !idle.frame_driven {
      let ctx = ctx.clone();
      ctx.clone().spawn(async move {
        tokio::task::yield_now().await;
        run_idle_period(&ctx, None);
      });
    }
    Ok(id)
  }

  fn cancel_idle_callback(&self, ctx: &Ctx<'_>, id: u32) {
    let entry = self.take_idle_entry(id);
    if let Some(timer) = entry.and_then(|e| e.timeout_timer) {
      self.cancel(ctx, timer);
    }
  }

  fn take_idle_entry(&self, id: u32) -> Option<IdleEntry> {
    let mut idle = self.idle.borrow_mut();
    let at = idle.entries.iter().position(|e| e.id == id)?;
    Some(idle.entries.remove(at))
  }

  // The `timeout` ran out before an idle period got to the callback: run it
  // now, as the task the timer is, with nothing remaining.
  fn idle_timed_out(&self, ctx: &Ctx<'_>, id: u32) {
    if let Some(entry) = self.take_idle_entry(id) {
      self.run_idle_entry(ctx, entry, Instant::now(), true);
    }
  }

  fn run_idle_entry(&self, ctx: &Ctx<'_>, entry: IdleEntry, until: Instant, did_timeout: bool) {
    if let Some(timer) = entry.timeout_timer {
      self.cancel(ctx, timer);
    }
    let Ok(cb) = entry.callback.restore(ctx) else {
      return;
    };
    let deadline = match idle_deadline(ctx.clone(), until, did_timeout) {
      Ok(deadline) => deadline,
      Err(e) => {
        report_uncaught(ctx, e, "requestIdleCallback deadline");
        return;
      }
    };
    if let Err(e) = cb.call::<_, ()>((deadline,)) {
      report_uncaught(ctx, e, "requestIdleCallback callback");
    }
  }
}

/// Hand this context's idle periods to a frame driver: from here on an idle
/// callback waits for `run_idle_period`, which the driver calls after a
/// frame's work. Install with the driver, before app code runs.
pub fn install_idle_periods(ctx: &Ctx<'_>) {
  let timers = ctx.userdata::<Timers>().expect("timers installed");
  timers.idle.borrow_mut().frame_driven = true;
}

/// Whether an idle callback of this context waits for a frame-driven idle
/// period: work waiting that no frame request stands for, like a due
/// timer. False without `install_idle_periods`, where the next turn runs it.
pub fn idle_due(ctx: &Ctx<'_>) -> bool {
  let Some(timers) = ctx.userdata::<Timers>() else {
    return false;
  };
  let idle = timers.idle.borrow();
  idle.frame_driven && !idle.entries.is_empty()
}

/// An idle period: run the idle callbacks registered before this call,
/// oldest first, each with a deadline counting down to `until`, until it
/// passes or none is left. `None` is a period with nothing due; either way
/// the period is capped at the spec's 50 ms. A callback registered during
/// the period waits for the next one, and a period whose end has already
/// passed runs nothing.
pub fn run_idle_period(ctx: &Ctx<'_>, until: Option<Instant>) {
  let Some(timers) = ctx.userdata::<Timers>() else {
    return;
  };
  let timers = timers.clone();
  let Some(last_id) = timers.idle.borrow().entries.last().map(|e| e.id) else {
    return;
  };
  let cap = Instant::now() + Duration::from_secs_f64(IDLE_PERIOD_MAX_MS / 1000.0);
  let until = until.map_or(cap, |until| until.min(cap));
  while Instant::now() < until {
    let entry = {
      let mut idle = timers.idle.borrow_mut();
      match idle.entries.first() {
        Some(first) if first.id <= last_id => Some(idle.entries.remove(0)),
        _ => None,
      }
    };
    let Some(entry) = entry else {
      break;
    };
    timers.run_idle_entry(ctx, entry, until, false);
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

// The `timeout` of requestIdleCallback's options: a positive finite number
// of ms arms the timeout, anything else (no options, no timeout, 0) does not.
fn idle_timeout_ms(opts: OptArg<Value<'_>>) -> Option<u64> {
  let timeout: f64 = opts.0?.into_object()?.get::<_, Option<f64>>("timeout").ok()??;
  (timeout.is_finite() && timeout > 0.0).then(|| timeout as u64)
}

// The delay argument as the web reads it: omitted, undefined or null is 0
// (the next frame), a negative or NaN value is 0, a fraction truncates.
fn delay_ms(ms: OptArg<f64>) -> u64 {
  ms.0.filter(|v| v.is_finite()).unwrap_or(0.0).max(0.0) as u64
}

fn init_timers(ctx: &Ctx<'_>) {
  let timers = Timers::new(ctx);
  let globals = ctx.globals();

  let set_timeout = Function::new(
    ctx.clone(),
    MutFn::from({
      let timers = timers.clone();
      move |cb: Function<'_>, ms: OptArg<f64>| -> u32 {
        let ctx = cb.ctx().clone();
        timers.set_timeout(&ctx, cb, delay_ms(ms))
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
      move |cb: Function<'_>, ms: OptArg<f64>| -> u32 {
        let ctx = cb.ctx().clone();
        timers.set_interval(&ctx, cb, delay_ms(ms))
      }
    }),
  )
  .unwrap();

  let clear_interval = Function::new(
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

  let set_immediate = Function::new(
    ctx.clone(),
    MutFn::from({
      let timers = timers.clone();
      move |cb: Function<'_>| -> u32 {
        let ctx = cb.ctx().clone();
        timers.set_immediate(&ctx, cb)
      }
    }),
  )
  .unwrap();

  // clearImmediate: the one cancel serves every id the context hands out.
  let clear_immediate = Function::new(
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

  let request_idle_callback = Function::new(
    ctx.clone(),
    MutFn::from({
      let timers = timers.clone();
      move |cb: Function<'_>, opts: OptArg<Value<'_>>| -> rquickjs::Result<u32> {
        let ctx = cb.ctx().clone();
        timers.request_idle_callback(&ctx, cb, idle_timeout_ms(opts))
      }
    }),
  )
  .unwrap();

  let cancel_idle_callback = Function::new(
    ctx.clone(),
    MutFn::from({
      let timers = timers.clone();
      move |ctx: Ctx<'_>, id: OptArg<Value<'_>>| {
        if let Some(id) = timer_id(id) {
          timers.cancel_idle_callback(&ctx, id);
        }
      }
    }),
  )
  .unwrap();

  ctx.store_userdata(timers).expect("store timers");

  // Stash the engine's native queueMicrotask before the global is overwritten
  // below - reading it afterwards would recurse into our wrapper.
  let native_queue: Function = globals.get("queueMicrotask").expect("engine queueMicrotask");
  ctx.store_userdata(NativeQueueMicrotask(native_queue)).expect("store native queueMicrotask");
  let queue_microtask = Function::new(ctx.clone(), MutFn::from(|cb: Function<'_>| schedule_microtask(cb))).unwrap();

  globals.set("setTimeout", set_timeout).unwrap();
  globals.set("clearTimeout", clear_timeout).unwrap();
  globals.set("setInterval", set_interval).unwrap();
  globals.set("clearInterval", clear_interval).unwrap();
  globals.set("setImmediate", set_immediate).unwrap();
  globals.set("clearImmediate", clear_immediate).unwrap();
  globals.set("requestIdleCallback", request_idle_callback).unwrap();
  globals.set("cancelIdleCallback", cancel_idle_callback).unwrap();
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

/// The frame timeline reading for native consumers (video sync, the runner's
/// stepped clock): the injected Timeline when present, real elapsed time
/// otherwise (headless flux has no frames to be in phase with).
pub fn timeline_now_ms(ctx: &Ctx<'_>) -> f64 {
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

// What replaces `Date` in a context without a wall (see `freeze_wall`): the
// engine's own `Date` in everything but the current time, which `now`
// supplies to `Date.now()`, `new Date()` and `Date()`. Instances are the
// engine's, so `instanceof`, the prototype and every method are unchanged.
const FROZEN_DATE_SOURCE: &str = r#"(now) => {
  let RealDate = Date
  function FrozenDate(...args) {
    if (new.target === undefined) return new RealDate(now()).toString()
    return Reflect.construct(RealDate, args.length === 0 ? [now()] : args, new.target)
  }
  Object.defineProperty(FrozenDate, "name", { value: "Date", configurable: true })
  FrozenDate.prototype = RealDate.prototype
  FrozenDate.now = now
  FrozenDate.parse = RealDate.parse
  FrozenDate.UTC = RealDate.UTC
  Object.defineProperty(RealDate.prototype, "constructor", { value: FrozenDate, writable: true, configurable: true })
  globalThis.Date = FrozenDate
}"#;

/// Take the wall clock out of this context: `performance.now()` reads 0 and
/// the calendar reads `epoch_ms` plus the frame timeline (see `Timeline`),
/// through `Date.now()`, `new Date()` and `Date()`. For a host whose app
/// time is stepped by frames (a test, a headless render): what runs there
/// then depends on nothing but the frames it was given, and logic that
/// still measures with `performance.now()` fails the same way on every run
/// instead of passing within a tolerance. Beside `install_virtual_time`
/// (the timers) and `seed_random`: a host opts a context in.
pub fn freeze_wall(ctx: &Ctx<'_>, epoch_ms: f64) -> rquickjs::Result<()> {
  let performance: Object = ctx.globals().get("performance")?;
  performance.set("now", Function::new(ctx.clone(), || 0.0)?)?;
  performance.set("timeOrigin", epoch_ms)?;
  let now = Function::new(ctx.clone(), move |ctx: Ctx<'_>| epoch_ms + timeline_now_ms(&ctx))?;
  let install: Function = ctx.eval(FROZEN_DATE_SOURCE)?;
  install.call::<_, ()>((now,))
}

pub(crate) fn init(ctx: &Ctx<'_>) {
  init_timers(ctx);
  init_performance(ctx);
}
