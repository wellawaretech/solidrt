//! The `flux:isolate` module: call a module's exports on another thread.
//!
//! `isolate(id, opts?)` returns a handle for the isolate module the embedder
//! resolves `id` to (`EngineConfig::isolate_resolver`; in a SolidRT project a
//! `"use isolate"` module, in standalone `flux` the file `<id>.js` next to the
//! entry). Any property of the handle is an async function: `handle.sum(n)`
//! runs the module's `sum` export in a second flux runtime on its own OS
//! thread (own heap, own event loop, the non-gui `flux:*` modules) and
//! resolves with its return value. Arguments and results are neutral values
//! (see `plugins/value.rs`): copied, shared-nothing; unsendable arguments throw
//! a `TypeError` at the call, an unsendable result rejects it.
//!
//! - The child runtime starts on first use (a call, or reading `exited`) and
//!   lives until `terminate()` or the parent's end (exit, reload: dropping the
//!   parent context fires every child's kill switch, transitively). Module
//!   state in the child persists between calls; each `isolate()` call is its
//!   own instance.
//! - Calls start in call order and run concurrently, as the same functions
//!   would in-process: the child is single-threaded, so a sync export runs to
//!   completion before anything else, while an `async` export lets other calls
//!   run at each `await`. An export that must not interleave with itself
//!   serialises inside the module.
//! - Streams: an export whose result is async-iterable (an `async function*`)
//!   is pulled item by item. What the call returns is still a Promise, but one
//!   that is also an async iterator: `for await (let x of handle.ticks())`
//!   sends `Next` per item and `Return` on break; awaiting it instead rejects.
//!   Items are values like results; a never-ending generator is a subscription
//!   (an open stream keeps both runtimes alive until it is closed).
//! - A throw (or rejection) in the called export rejects that call with the
//!   error rebuilt as data: `new globals[name](message)` when that global is
//!   an error constructor (so `instanceof RangeError` holds across the
//!   boundary), else an `Error` carrying the name; the child's stack is the
//!   rebuilt error's `stack`, and its `cause` chain crosses too - each cause
//!   another rebuilt error or a sendable value (an unsendable cause is
//!   dropped, and the chain is capped, which also ends a cyclic one). A
//!   thrown non-Error rejects with the value itself when it is sendable. An
//!   uncaught error elsewhere in the child (a failed module load, a throw out
//!   of a timer) is logged; when it ends the child, pending and later calls
//!   reject with a message naming it.
//! - `terminate()` kills the child now: busy JS is interrupted, the child
//!   runtime is dropped, pending calls reject and open streams end.
//! - `exited` is a promise settling once the child is gone: with the uncaught
//!   error that ended it, or `null` after `terminate()` or a clean end.
//!   Reading it is a first use (spawns the child) and keeps this runtime's
//!   loop open until the child exits, so an exit is noticed with no call in
//!   flight.
//! - Reserved names: `terminate` (the method), `exited` (the promise) and
//!   `then` (so a handle is not a thenable). Symbol properties are looked up
//!   on the handle itself.
//!
//! The child gets the parent's host config (`EngineConfig`, so it can spawn
//! isolates of its own) and `opts.args` as its `flux:process` argv.
//!
//! Marshalling only: the link, call protocol, and kill switch are
//! `forge::isolate`; this layer spawns the thread and engine, routes replies to
//! promises and iterators on the parent's end, and dispatches calls to the
//! module namespace on the child's end.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rquickjs::atom::PredefinedAtom;
use rquickjs::class::Trace;
use rquickjs::function::{Args, Constructor, Rest, This};
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::MaybePromise;
use rquickjs::{Class, Ctx, Exception, Function, IntoJs, JsLifetime, Object, Promise, Value as JsValue};
use tokio::sync::{mpsc, oneshot};

use crate::engine::{EngineConfig, FluxEngineBuilder, ModuleCode};
use crate::logger::Logger;
use crate::plugins::marshal::{mark_observed, with_pending, OptArg};
use crate::plugins::value::{self, Neutral};
use forge::isolate::{CallError, Kill, Link, Msg, Thrown};
use forge::Value;

/// Kill switches of every isolate this context spawned. Dropping the context
/// drops this, which fires them all: children never outlive their parent.
#[derive(Default, JsLifetime)]
struct Isolates(#[qjs(skip_trace)] Mutex<Vec<Arc<Kill>>>);

impl Drop for Isolates {
  fn drop(&mut self) {
    for kill in self.0.lock().expect("isolates lock poisoned").drain(..) {
      kill.fire();
    }
  }
}

type Reply = Result<Value, Thrown>;

/// What the child sends back about one call, routed to that call's slot.
enum Event {
  Stream,
  Yield(Value),
  Reply(Reply),
}

/// The parent's end of one running child.
struct Instance {
  id: String,
  link: Link,
  kill: Arc<Kill>,
  next_call: AtomicU64,
  pending: Mutex<HashMap<u64, mpsc::UnboundedSender<Event>>>,
  /// Set once the child is gone (exited or terminated): the message every
  /// later call rejects with.
  exited: Mutex<Option<String>>,
  /// The child's last uncaught error, to name the cause when it exits.
  last_error: Mutex<Option<String>>,
  /// Set alongside `exited`: what `exited` watchers settle with - the uncaught
  /// error that ended the child, `None` for a clean end.
  exit_cause: Mutex<Option<Option<String>>>,
  /// `exited` watchers not yet settled.
  watchers: Mutex<Vec<oneshot::Sender<Option<String>>>>,
  /// Whether the exit pump runs (one per instance, started on the first
  /// `exited` read).
  pumping: AtomicBool,
}

impl Instance {
  fn deliver(&self, id: u64, event: Event) {
    let mut pending = self.pending.lock().expect("pending lock poisoned");
    if matches!(event, Event::Reply(_)) {
      if let Some(tx) = pending.remove(&id) {
        let _ = tx.send(event);
      }
    } else if let Some(tx) = pending.get(&id) {
      let _ = tx.send(event);
    }
  }

  /// Record the child's end, once (the first reason wins): later calls
  /// reject with `reason`, pending calls reject now, watchers settle with
  /// `cause`.
  fn exit(&self, reason: String, cause: Option<String>) {
    {
      let mut exited = self.exited.lock().expect("exited lock poisoned");
      if exited.is_some() {
        return;
      }
      *exited = Some(reason.clone());
    }
    *self.exit_cause.lock().expect("exit cause lock poisoned") = Some(cause.clone());
    for w in self.watchers.lock().expect("watchers lock poisoned").drain(..) {
      let _ = w.send(cause.clone());
    }
    for (_, tx) in self.pending.lock().expect("pending lock poisoned").drain() {
      let _ = tx.send(Event::Reply(Err(reason.clone().into())));
    }
  }

  fn exited(&self) -> Option<String> {
    self.exited.lock().expect("exited lock poisoned").clone()
  }

  /// A receiver that settles with the exit cause; immediately when the child
  /// is already gone.
  fn watch(&self) -> oneshot::Receiver<Option<String>> {
    let (tx, rx) = oneshot::channel();
    match self.exit_cause.lock().expect("exit cause lock poisoned").as_ref() {
      Some(cause) => {
        let _ = tx.send(cause.clone());
      }
      None => self.watchers.lock().expect("watchers lock poisoned").push(tx),
    }
    rx
  }

  /// Route one link message to its call's slot; the link's end (`None`)
  /// records the exit. False once the link is closed.
  fn route(&self, msg: Option<Msg>) -> bool {
    match msg {
      Some(Msg::Reply { id, result }) => self.deliver(id, Event::Reply(result)),
      Some(Msg::Stream { id }) => self.deliver(id, Event::Stream),
      Some(Msg::Yield { id, value }) => self.deliver(id, Event::Yield(value)),
      Some(Msg::Error(e)) => *self.last_error.lock().expect("last error lock poisoned") = Some(e),
      Some(Msg::Call { .. } | Msg::Next { .. } | Msg::Return { .. }) => {}
      None => {
        let cause = self.last_error.lock().expect("last error lock poisoned").take();
        let reason = match &cause {
          Some(e) => format!("isolate '{}' exited: {e}", self.id),
          None => format!("isolate '{}' exited", self.id),
        };
        self.exit(reason, cause);
        return false;
      }
    }
    true
  }

  /// Wait for the next event of one call. Whoever holds the link reads for
  /// everyone: events are routed to their call's slot, so a caller may return
  /// because another caller delivered its event.
  async fn recv_event(&self, rx: &mut mpsc::UnboundedReceiver<Event>) -> Event {
    loop {
      tokio::select! {
        biased;
        e = rx.recv() => return e.unwrap_or_else(|| Event::Reply(Err(self.exited().unwrap_or_else(|| "isolate call dropped".to_string()).into()))),
        m = self.link.recv() => {
          self.route(m);
        },
      }
    }
  }
}

/// What a call turned out to be, as far as the parent knows.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
  /// No answer yet.
  Unknown,
  /// A plain value call (settled or about to settle the promise).
  Plain,
  /// A stream: items arrive per `next()`.
  Stream,
  /// A stream that has ended.
  Ended,
}

/// One outstanding call on the parent, shared by whoever drives it: the
/// initial pump (until the child says plain-or-stream, then while readers are
/// queued) and each `next()`/`return()`. Nothing pumps a stream nobody is
/// reading, so a stream that is awaited and never iterated does not hold the
/// runtime open. Holds no JS values: the promise's iterator closures own this,
/// so a JS reference here would be an untraceable cycle.
struct CallState {
  instance: Arc<Instance>,
  id: u64,
  name: String,
  mode: Cell<Mode>,
  /// The call's event slot; taken (`None`) once the call is over.
  rx: tokio::sync::Mutex<Option<mpsc::UnboundedReceiver<Event>>>,
  /// `next()`/`return()` callers waiting for an item (`Some`), the end
  /// (`None`), or an error; answered in order, as the child answers.
  waiters: RefCell<VecDeque<oneshot::Sender<Result<Option<Value>, Thrown>>>>,
}

/// What one pumped event means for the call's promise (only the initial pump
/// acts on it; readers pump only once the call is known to be a stream).
enum Outcome {
  Continue,
  Settle(Reply),
  Stream,
  Over,
}

impl CallState {
  fn answer(&self, r: Result<Option<Value>, Thrown>) {
    if let Some(w) = self.waiters.borrow_mut().pop_front() {
      let _ = w.send(r);
    }
  }

  fn answer_all(&self, r: Result<Option<Value>, Thrown>) {
    let mut waiters = self.waiters.borrow_mut();
    let mut first = Some(r);
    for w in waiters.drain(..) {
      let _ = w.send(first.take().unwrap_or(Ok(None)));
    }
  }

  /// Receive and apply one event of this call.
  async fn pump_one(&self) -> Outcome {
    let mut slot = self.rx.lock().await;
    let Some(rx) = slot.as_mut() else { return Outcome::Over };
    match self.instance.recv_event(rx).await {
      Event::Stream => {
        self.mode.set(Mode::Stream);
        Outcome::Stream
      }
      Event::Yield(value) => {
        self.answer(Ok(Some(value)));
        Outcome::Continue
      }
      Event::Reply(result) => {
        *slot = None;
        if self.mode.get() == Mode::Stream {
          self.mode.set(Mode::Ended);
          self.answer_all(result.map(|_| None));
          Outcome::Over
        } else {
          self.mode.set(Mode::Plain);
          self.answer_all(Err(not_a_stream(&self.name).into()));
          Outcome::Settle(result)
        }
      }
    }
  }
}

/// The handle `isolate(id)` returns (behind the Proxy that turns property
/// access into calls). Spawns its child on the first call.
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Isolate")]
pub struct Isolate {
  #[qjs(skip_trace)]
  id: String,
  #[qjs(skip_trace)]
  args: Vec<String>,
  #[qjs(skip_trace)]
  instance: RefCell<Option<Arc<Instance>>>,
  #[qjs(skip_trace)]
  terminated: RefCell<bool>,
}

impl Isolate {
  /// Call export `name` in the child with `args`: what `handle.name(...args)`
  /// does. Returns the call's promise, which doubles as an async iterator for
  /// stream results (see `attach_iterator`).
  fn call<'js>(&self, ctx: Ctx<'js>, name: String, args: Vec<Neutral>) -> rquickjs::Result<JsValue<'js>> {
    let instance = self.instance(&ctx).map_err(|m| Exception::throw_message(&ctx, &m))?;
    let (tx, rx) = mpsc::unbounded_channel();
    let id = instance.next_call.fetch_add(1, Ordering::Relaxed);
    instance.pending.lock().expect("pending lock poisoned").insert(id, tx);
    let sent = instance.link.send(Msg::Call { id, name: name.clone(), args: args.into_iter().map(|a| a.0).collect() });
    if let Err(e) = sent {
      instance.deliver(id, Event::Reply(Err(e.into())));
    }
    let state = Rc::new(CallState {
      instance,
      id,
      name,
      mode: Cell::new(Mode::Unknown),
      rx: tokio::sync::Mutex::new(Some(rx)),
      waiters: RefCell::default(),
    });
    let (promise, resolve, reject) = Promise::new(&ctx)?;
    // The initial pump: drives the call until the child says what it is (a
    // plain call is settled here), then keeps going only while readers that
    // queued before that answer are waiting. Owns the resolvers, so they die
    // with the task, not with the (JS-owned) state.
    let (pump_ctx, pump_state, pump_promise) = (ctx.clone(), state.clone(), promise.clone());
    ctx.spawn(async move {
      let reject_with = |t: Thrown| {
        if let Ok(err) = build_thrown(&pump_ctx, t) {
          let _ = reject.call::<_, ()>((err,));
        }
      };
      loop {
        if pump_state.mode.get() != Mode::Unknown && pump_state.waiters.borrow().is_empty() {
          return;
        }
        match pump_state.pump_one().await {
          Outcome::Continue => {}
          Outcome::Over => return,
          Outcome::Settle(Ok(v)) => {
            let _ = resolve.call::<_, ()>((Neutral(v),));
            return;
          }
          Outcome::Settle(Err(e)) => {
            reject_with(e);
            return;
          }
          Outcome::Stream => {
            // Only a caller who awaits the stream sees this; a reader does not.
            mark_observed(pump_promise.as_value());
            reject_with(format!("export '{}' returned a stream: iterate it with for await", pump_state.name).into());
          }
        }
      }
    });
    attach_iterator(&ctx, &promise, state)?;
    Ok(promise.into_value())
  }

  /// Kill the child now. Later calls reject; a handle that never called
  /// anything just never spawns.
  fn terminate(&self) {
    *self.terminated.borrow_mut() = true;
    if let Some(instance) = self.instance.borrow().as_ref() {
      instance.link.close();
      instance.kill.fire();
      instance.exit(format!("isolate '{}' terminated", self.id), None);
    }
  }

  /// The `exited` promise: settles once the child is gone, with the uncaught
  /// error that ended it or `null` for a clean end. Reading it is a first
  /// use: the child spawns like it would for a call, and the exit pump keeps
  /// this runtime's loop open until the child exits, so the exit is noticed
  /// with no call in flight.
  fn exited_promise<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<JsValue<'js>> {
    let (promise, resolve, _reject) = Promise::new(&ctx)?;
    let existing = self.instance.borrow().clone();
    let instance = match existing {
      Some(instance) => instance,
      None => {
        if *self.terminated.borrow() {
          // Never spawned and never will: gone, cleanly.
          resolve.call::<_, ()>((JsValue::new_null(ctx.clone()),))?;
          return Ok(promise.into_value());
        }
        self.instance(&ctx).map_err(|m| Exception::throw_message(&ctx, &m))?
      }
    };
    let rx = instance.watch();
    if !instance.pumping.swap(true, Ordering::Relaxed) {
      let pump = instance.clone();
      ctx.spawn(async move { while pump.route(pump.link.recv().await) {} });
    }
    let settle_ctx = ctx.clone();
    ctx.spawn(async move {
      let cause = rx.await.unwrap_or(None);
      let value = match cause {
        Some(e) => e.into_js(&settle_ctx),
        None => Ok(JsValue::new_null(settle_ctx.clone())),
      };
      if let Ok(v) = value {
        let _ = resolve.call::<_, ()>((v,));
      }
    });
    Ok(promise.into_value())
  }

  /// The running child, spawned on first use.
  fn instance(&self, ctx: &Ctx<'_>) -> Result<Arc<Instance>, String> {
    if let Some(instance) = self.instance.borrow().as_ref() {
      return match instance.exited() {
        Some(reason) => Err(reason),
        None => Ok(instance.clone()),
      };
    }
    if *self.terminated.borrow() {
      return Err(format!("isolate '{}' terminated", self.id));
    }
    let config = crate::FluxEngine::config(ctx);
    let resolver = config.isolate_resolver.clone().ok_or_else(|| "this runtime cannot spawn isolates".to_string())?;
    let code = resolver(&self.id)?;

    let (parent_link, child_link) = Link::pair();
    let kill = Arc::new(Kill::default());
    ctx.userdata::<Isolates>().expect("isolates registry").0.lock().expect("isolates lock poisoned").push(kill.clone());
    spawn_thread(config, self.id.clone(), code, self.args.clone(), child_link, kill.clone())?;

    let instance = Arc::new(Instance {
      id: self.id.clone(),
      link: parent_link,
      kill,
      next_call: AtomicU64::new(0),
      pending: Mutex::new(HashMap::new()),
      exited: Mutex::new(None),
      last_error: Mutex::new(None),
      exit_cause: Mutex::new(None),
      watchers: Mutex::new(Vec::new()),
      pumping: AtomicBool::new(false),
    });
    *self.instance.borrow_mut() = Some(instance.clone());
    Ok(instance)
  }
}

fn not_a_stream(name: &str) -> String {
  format!("export '{name}' is not a stream")
}

/// One `{ value, done }` iterator result.
struct IterResult(Option<Value>);

impl<'js> IntoJs<'js> for IterResult {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<JsValue<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("done", self.0.is_none())?;
    match self.0 {
      Some(v) => obj.set("value", Neutral(v))?,
      None => obj.set("value", JsValue::new_undefined(ctx.clone()))?,
    }
    Ok(obj.into_value())
  }
}

/// Make the call's promise its own async iterator: `next()` pulls one item
/// from the child, `return()` ends the stream early, `[Symbol.asyncIterator]`
/// returns the promise itself. Iterating a plain call rejects. The closures
/// capture no JS values (a capture alive at teardown is released too late,
/// see the Persistent trap in flux/CLAUDE.md), which is also why there is no
/// separate iterator object.
fn attach_iterator<'js>(ctx: &Ctx<'js>, promise: &Promise<'js>, state: Rc<CallState>) -> rquickjs::Result<()> {
  let s = state.clone();
  promise.set(
    "next",
    Function::new(ctx.clone(), move |ctx: Ctx<'js>| -> rquickjs::Result<JsValue<'js>> {
      stream_step(ctx, &s, Msg::Next { id: s.id })
    })?,
  )?;
  let s = state;
  promise.set(
    "return",
    Function::new(ctx.clone(), move |ctx: Ctx<'js>| -> rquickjs::Result<JsValue<'js>> {
      stream_step(ctx, &s, Msg::Return { id: s.id })
    })?,
  )?;
  promise.set(PredefinedAtom::SymbolAsyncIterator, Function::new(ctx.clone(), |this: This<JsValue<'js>>| this.0)?)
}

/// One `next()`/`return()`: ask the child (unless the call is known not to be
/// a stream, or the stream is over), then wait for the answer, pumping the
/// call's events once it is known to be a stream (before that the initial
/// pump does, and it settles the promise). Steps are answered in order.
fn stream_step<'js>(ctx: Ctx<'js>, state: &Rc<CallState>, msg: Msg) -> rquickjs::Result<JsValue<'js>> {
  match state.mode.get() {
    Mode::Plain => return Err(Exception::throw_message(&ctx, &not_a_stream(&state.name))),
    Mode::Ended => return IterResult(None).into_js(&ctx),
    Mode::Unknown | Mode::Stream => {}
  }
  let (tx, mut rx) = oneshot::channel();
  state.waiters.borrow_mut().push_back(tx);
  if let Err(e) = state.instance.link.send(msg) {
    state.answer(Err(e.into()));
  }
  let state = state.clone();
  with_pending(&ctx, async move {
    let done = |r: Result<Result<Option<Value>, Thrown>, oneshot::error::RecvError>| match r {
      Ok(r) => StepResult(r),
      Err(_) => StepResult(Err(format!("stream '{}' dropped", state.name).into())),
    };
    loop {
      if state.mode.get() == Mode::Unknown {
        // The initial pump answers us (it runs until we are answered).
        return Ok(done(rx.await));
      }
      tokio::select! {
        biased;
        r = &mut rx => return Ok(done(r)),
        outcome = state.pump_one() => if matches!(outcome, Outcome::Over) {
          return Ok(done(rx.await));
        },
      }
    }
  })
  .into_js(&ctx)
}

/// A stream step's outcome, converted on the JS thread: an item or the end as
/// `{ value, done }`, or the structured throw the step rejects with (like
/// `JsResult`, but keeping the error's name, stack and cause).
struct StepResult(Result<Option<Value>, Thrown>);

impl<'js> IntoJs<'js> for StepResult {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<JsValue<'js>> {
    match self.0 {
      Ok(v) => IterResult(v).into_js(ctx),
      Err(t) => Err(ctx.throw(build_thrown(ctx, t)?)),
    }
  }
}

/// `isolate(id, { args? })`: a handle whose properties call the exports of the
/// module `id` resolves to: a `Proxy` over the `Isolate` instance whose `get`
/// trap is `proxy_get`.
fn isolate<'js>(ctx: Ctx<'js>, id: String, opts: OptArg<Object<'js>>) -> rquickjs::Result<JsValue<'js>> {
  let args: Vec<String> = match opts.0 {
    Some(o) => o.get::<_, Option<Vec<String>>>("args")?.unwrap_or_default(),
    None => Vec::new(),
  };
  let handle =
    Class::instance(ctx.clone(), Isolate { id, args, instance: RefCell::new(None), terminated: RefCell::new(false) })?;
  let traps = Object::new(ctx.clone())?;
  traps.set("get", Function::new(ctx.clone(), proxy_get)?)?;
  let proxy: Constructor = ctx.globals().get("Proxy")?;
  proxy.construct((handle, traps))
}

/// The handle's `get` trap: a string property is a call to the export of that
/// name, except `terminate` (the method), `exited` (the exit promise) and
/// `then` (undefined, so awaiting or returning a handle from an async
/// function does not treat it as a thenable). Symbols are looked up on the
/// handle itself.
fn proxy_get<'js>(ctx: Ctx<'js>, handle: Class<'js, Isolate>, prop: JsValue<'js>) -> rquickjs::Result<JsValue<'js>> {
  let name = match prop.as_string() {
    Some(s) => s.to_string()?,
    None => return handle.as_inner().get(prop),
  };
  match name.as_str() {
    "then" => Ok(JsValue::new_undefined(ctx)),
    "terminate" => Function::new(ctx, move || handle.borrow().terminate()).map(|f| f.into_value()),
    "exited" => handle.borrow().exited_promise(ctx),
    _ => Function::new(ctx, move |ctx: Ctx<'js>, args: Rest<Neutral>| handle.borrow().call(ctx, name.clone(), args.0))
      .map(|f| f.into_value()),
  }
}

/// Run a child engine on its own thread until its loop goes idle or its kill
/// switch fires. The child inherits the parent's host config; its uncaught
/// errors are forwarded on the link; once its module has evaluated it serves
/// calls from the link against the module namespace.
fn spawn_thread(
  mut config: EngineConfig,
  id: String,
  code: ModuleCode,
  args: Vec<String>,
  link: Link,
  kill: Arc<Kill>,
) -> Result<(), String> {
  // Once terminated, the child's last words (the interruption error, a final
  // log line) are noise: the parent asked for silence.
  let parent_logger = config.logger.clone();
  let log_flag = kill.flag();
  config.logger = Logger::new(Box::new(move |level, msg| {
    if !log_flag.load(Ordering::Relaxed) {
      (parent_logger.0)(level, msg)
    }
  }));
  let error_link = link.clone();
  let error_flag = kill.flag();
  std::thread::Builder::new()
    .name(format!("isolate:{id}"))
    .spawn(move || {
      let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
          let _ = link.send(Msg::Error(format!("isolate: failed to start runtime: {e}")));
          return;
        }
      };
      let local = tokio::task::LocalSet::new();
      runtime.block_on(local.run_until(async move {
        let engine = FluxEngineBuilder::from_config(config)
          .interrupt_flag(kill.flag())
          .on_uncaught(move |msg| {
            if !error_flag.load(Ordering::Relaxed) {
              let _ = error_link.send(Msg::Error(msg.to_string()));
            }
          })
          .userdata(crate::ProcessArgs(args))
          .build();
        tokio::select! {
          _ = engine.eval_module(id, code, move |ctx, ns| serve(ctx, ns, link)) => {}
          _ = kill.fired() => {}
        }
      }));
    })
    .map(|_| ())
    .map_err(|e| format!("isolate: failed to start thread: {e}"))
}

/// The child's dispatcher. Each message from the link is served without the
/// reader waiting on it: a call is looked up on the module namespace and
/// started in order, and settling its result (a promise) is its own task, so a
/// pending call holds up neither later calls nor stream steps. A call whose
/// result is async-iterable becomes a stream: its iterator is kept by call id
/// and each `Next`/`Return` is likewise served as its own task. Ends when the
/// parent closes the link, after which nothing holds the child's loop open.
fn serve<'js>(ctx: Ctx<'js>, ns: Object<'js>, link: Link) {
  let streams: Rc<RefCell<HashMap<u64, Object<'js>>>> = Rc::default();
  ctx.clone().spawn(async move {
    while let Some(msg) = link.recv().await {
      match msg {
        Msg::Call { id, name, args } => match start_call(&ctx, &ns, &name, args) {
          Ok(Started::Stream(iter)) => {
            streams.borrow_mut().insert(id, iter);
            let _ = link.send(Msg::Stream { id });
          }
          Ok(Started::Value(returned)) => {
            let (ctx, link) = (ctx.clone(), link.clone());
            ctx.clone().spawn(async move {
              let result = settle(&ctx, returned).await;
              let _ = link.send(Msg::Reply { id, result });
            });
          }
          Err(e) => {
            let _ = link.send(Msg::Reply { id, result: Err(e) });
          }
        },
        Msg::Next { id } => {
          let Some(iter) = streams.borrow().get(&id).cloned() else { continue };
          let (ctx, link, streams) = (ctx.clone(), link.clone(), streams.clone());
          ctx.clone().spawn(async move {
            match iter_step(&ctx, &iter, "next").await {
              Ok(Some(value)) => {
                let _ = link.send(Msg::Yield { id, value });
              }
              Ok(None) => {
                streams.borrow_mut().remove(&id);
                let _ = link.send(Msg::Reply { id, result: Ok(Value::Null) });
              }
              Err(e) => {
                streams.borrow_mut().remove(&id);
                let _ = link.send(Msg::Reply { id, result: Err(e) });
              }
            }
          });
        }
        Msg::Return { id } => {
          let Some(iter) = streams.borrow_mut().remove(&id) else { continue };
          let (ctx, link) = (ctx.clone(), link.clone());
          ctx.clone().spawn(async move {
            let result = iter_step(&ctx, &iter, "return").await.map(|_| Value::Null);
            let _ = link.send(Msg::Reply { id, result });
          });
        }
        Msg::Reply { .. } | Msg::Stream { .. } | Msg::Yield { .. } | Msg::Error(_) => {}
      }
    }
  });
}

/// A call's immediate outcome on the child: a stream's iterator, or the value
/// (possibly a promise) to settle and send back.
enum Started<'js> {
  Stream(Object<'js>),
  Value(JsValue<'js>),
}

fn start_call<'js>(ctx: &Ctx<'js>, ns: &Object<'js>, name: &str, args: Vec<Value>) -> Result<Started<'js>, Thrown> {
  let export: JsValue<'js> = ns.get(name).map_err(|e| call_error(ctx, e))?;
  let f = export.into_function().ok_or_else(|| Thrown::from(format!("no exported function '{name}'")))?;
  let mut call_args = Args::new(ctx.clone(), args.len());
  for arg in args {
    call_args.push_arg(Neutral(arg)).map_err(|e| call_error(ctx, e))?;
  }
  let returned: JsValue<'js> = f.call_arg(call_args).map_err(|e| call_error(ctx, e))?;
  // Async-iterable (an async generator object, or anything with the symbol):
  // a stream. A promise or plain value settles as before.
  if let Some(obj) = returned.as_object() {
    let factory: Option<Function<'js>> =
      obj.get(PredefinedAtom::SymbolAsyncIterator).map_err(|e| call_error(ctx, e))?;
    if let Some(factory) = factory {
      let iter: Object<'js> = factory.call((This(obj.clone()),)).map_err(|e| call_error(ctx, e))?;
      return Ok(Started::Stream(iter));
    }
  }
  Ok(Started::Value(returned))
}

async fn settle<'js>(ctx: &Ctx<'js>, returned: JsValue<'js>) -> Reply {
  mark_observed(&returned);
  let settled =
    MaybePromise::from_value(returned).into_future::<JsValue<'js>>().await.map_err(|e| call_error(ctx, e))?;
  value::from_js(ctx, settled).map_err(|e| call_error(ctx, e))
}

/// One iterator method call (`next`/`return`) on a stream's iterator, awaited:
/// `Some(value)` for an item, `None` when done. A missing `return` is done.
async fn iter_step<'js>(ctx: &Ctx<'js>, iter: &Object<'js>, method: &str) -> Result<Option<Value>, Thrown> {
  let f: Option<Function<'js>> = iter.get(method).map_err(|e| call_error(ctx, e))?;
  let Some(f) = f else { return Ok(None) };
  let returned: JsValue<'js> = f.call((This(iter.clone()),)).map_err(|e| call_error(ctx, e))?;
  mark_observed(&returned);
  let result: Object<'js> =
    MaybePromise::from_value(returned).into_future::<Object<'js>>().await.map_err(|e| call_error(ctx, e))?;
  let done: bool = result.get::<_, Option<bool>>("done").map_err(|e| call_error(ctx, e))?.unwrap_or(false);
  if done {
    return Ok(None);
  }
  let value: JsValue<'js> = result.get("value").map_err(|e| call_error(ctx, e))?;
  value::from_js(ctx, value).map(Some).map_err(|e| call_error(ctx, e))
}

/// How deep a cause chain crosses; past this (or on a cycle) the rest is
/// dropped.
const MAX_CAUSE_DEPTH: usize = 8;

/// Discard a failed JS access, clearing the pending exception it may have
/// left on the context (a dropped `Err` would otherwise surface at the next
/// unrelated `ctx.catch()`).
fn quiet<T>(ctx: &Ctx<'_>, r: rquickjs::Result<T>) -> Option<T> {
  match r {
    Ok(v) => Some(v),
    Err(_) => {
      let _ = ctx.catch();
      None
    }
  }
}

/// The rejection data for a failed call: an error's name, message, stack and
/// cause chain; a thrown non-Error as the value itself when it is sendable,
/// else as an `Error` with its debug form.
fn call_error(ctx: &Ctx<'_>, err: rquickjs::Error) -> Thrown {
  if !err.is_exception() {
    return err.to_string().into();
  }
  thrown_from(ctx, ctx.catch(), 0)
}

fn thrown_from<'js>(ctx: &Ctx<'js>, value: JsValue<'js>, depth: usize) -> Thrown {
  let Some(exc) = value.as_exception() else {
    return match quiet(ctx, value::from_js(ctx, value.clone())) {
      Some(v) => Thrown::Value(v),
      None => format!("{value:?}").into(),
    };
  };
  let name = quiet(ctx, exc.get::<_, Option<String>>("name")).flatten().unwrap_or_else(|| "Error".to_string());
  let message = exc.message().unwrap_or_default();
  let stack = exc.stack().filter(|s| !s.trim().is_empty()).map(|s| s.trim_end().to_string());
  let cause = if depth < MAX_CAUSE_DEPTH {
    quiet(ctx, exc.get::<_, JsValue>("cause"))
      .filter(|v| !v.is_undefined())
      .and_then(|v| cause_from(ctx, v, depth + 1))
      .map(Box::new)
  } else {
    None
  };
  Thrown::Error(CallError { name, message, stack, cause })
}

/// A `cause` crossing the link: another error as data, or a sendable value;
/// an unsendable non-Error cause is dropped (unlike a top-level throw, there
/// is no need for a placeholder).
fn cause_from<'js>(ctx: &Ctx<'js>, value: JsValue<'js>, depth: usize) -> Option<Thrown> {
  if value.as_exception().is_some() {
    return Some(thrown_from(ctx, value, depth));
  }
  quiet(ctx, value::from_js(ctx, value)).map(Thrown::Value)
}

/// Rebuild what the child threw: an error from its data, or the thrown value
/// itself.
fn build_thrown<'js>(ctx: &Ctx<'js>, t: Thrown) -> rquickjs::Result<JsValue<'js>> {
  match t {
    Thrown::Error(e) => build_error(ctx, e),
    Thrown::Value(v) => Neutral(v).into_js(ctx),
  }
}

/// Rebuild a child's error on the parent: `new globals[name](message)` when
/// that yields an error (so `instanceof RangeError` holds for the standard
/// types), else a plain `Error` carrying the name; the child's stack and
/// (rebuilt) cause ride along as fields.
fn build_error<'js>(ctx: &Ctx<'js>, e: CallError) -> rquickjs::Result<JsValue<'js>> {
  let rebuilt = ctx
    .globals()
    .get::<_, Option<Constructor>>(e.name.as_str())
    .ok()
    .flatten()
    .and_then(|ctor| ctor.construct::<_, JsValue<'js>>((e.message.as_str(),)).ok())
    .filter(|v| v.as_exception().is_some());
  let err = match rebuilt {
    Some(v) => v,
    None => {
      let v = Exception::from_message(ctx.clone(), &e.message)?.into_value();
      if e.name != "Error" {
        if let Some(obj) = v.as_object() {
          obj.set("name", e.name.as_str())?;
        }
      }
      v
    }
  };
  if let Some(obj) = err.as_object() {
    if let Some(stack) = e.stack {
      obj.set("stack", stack)?;
    }
    if let Some(cause) = e.cause {
      obj.set("cause", build_thrown(ctx, *cause)?)?;
    }
  }
  Ok(err)
}

pub struct IsolateModule;

impl ModuleDef for IsolateModule {
  fn declare(decl: &Declarations<'_>) -> rquickjs::Result<()> {
    decl.declare("isolate")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    if ctx.userdata::<Isolates>().is_none() {
      ctx.store_userdata(Isolates::default()).expect("store isolates registry");
    }
    exports.export("isolate", Function::new(ctx.clone(), isolate)?)?;
    Ok(())
  }
}
