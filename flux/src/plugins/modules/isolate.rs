//! The `flux:isolate` module: run JS on another thread, talk over a port.
//!
//! `spawn(source, opts?)` starts a second flux runtime on its own OS thread
//! with its own heap and event loop, evaluates `source` as a module there, and
//! returns the parent's end of a port. Inside the child, `import { port } from
//! "flux:isolate"` is the other end (`port` is `undefined` in a runtime that
//! was not spawned). Messages are neutral values (see `plugins/value.rs`):
//! copied, shared-nothing; unsendable values throw a `TypeError` at `send`.
//!
//! Port surface (both ends alike): `send(value)`, `await recv()`, async
//! iteration (`for await (let m of port)`), `close()`. The parent's end adds
//! `terminate()`.
//!
//! - `recv()` resolves the next message; `undefined` once the peer has closed
//!   (or exited) and the queue is drained. It rejects with the peer's uncaught
//!   error (module throw, unhandled rejection, a throw out of a timer or
//!   callback) at the point that error happens; the child keeps running
//!   unless the error ended its module. Every such error is also logged.
//! - A pending `recv()` holds its own runtime's loop open. Two ends both
//!   awaiting `recv()` forever is a program bug, like Go's "all goroutines are
//!   asleep"; nothing detects it.
//! - `close()` stops sending from this end; the peer's `recv()` drains, then
//!   reports `undefined`. A child whose module has finished and whose port has
//!   been closed by the parent exits when nothing else keeps its loop alive.
//! - `terminate()` kills the child now: busy JS is interrupted, the child
//!   runtime is dropped, and this end's `recv()` reports `undefined`.
//! - Children die with their parent: dropping the parent context (exit,
//!   reload) terminates every isolate it spawned, transitively.
//! - `Promise.race([a.recv(), b.recv()])` is not `select`: the losing `recv()`
//!   already took its message. Await one port at a time or dedicate a loop per
//!   port.
//!
//! The child runtime gets the non-gui `flux:*` modules and the standard globals,
//! the parent's host config (`EngineConfig`: logger, fetch cache dir, user
//! agent, stack size), and `opts.args` as its `flux:process` argv.
//!
//! Marshalling only: the port link and kill switch are `forge::isolate`; this
//! layer spawns the thread and engine (host-specific) and binds the `Port`
//! class.

use std::future::Future;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use rquickjs::class::Trace;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Class, Ctx, Exception, Function, JsLifetime, Object, Value};

use crate::engine::{EngineConfig, FluxEngineBuilder};
use crate::logger::Logger;
use crate::plugins::js_error::JsResult;
use crate::plugins::marshal::{attach_async_iterator, with_pending, OptArg, Step};
use crate::plugins::value::Neutral;
use forge::isolate::{Kill, Link, Msg};

/// The child's end of its parent port, stored in the child's context userdata
/// by `spawn` so the module's `port` export can pick it up.
#[derive(Clone, JsLifetime)]
struct ChildLink(#[qjs(skip_trace)] Link);

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

#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Port")]
pub struct Port {
  #[qjs(skip_trace)]
  link: Link,
  /// The child's kill switch on the parent's end; `None` on the child's end.
  #[qjs(skip_trace)]
  kill: Option<Arc<Kill>>,
}

impl Port {
  fn create<'js>(ctx: &Ctx<'js>, link: Link, kill: Option<Arc<Kill>>) -> rquickjs::Result<Class<'js, Port>> {
    let instance = Class::instance(ctx.clone(), Port { link, kill })?;
    attach_async_iterator(ctx, &instance)?;
    Ok(instance)
  }
}

#[rquickjs::methods]
impl Port {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'_>) -> rquickjs::Result<Port> {
    Err(Exception::throw_message(&ctx, "ports come from spawn(); they cannot be constructed"))
  }

  /// Copy `value` to the peer. Throws for values outside the neutral set and
  /// once this end is closed.
  pub fn send(&self, ctx: Ctx<'_>, value: Neutral) -> rquickjs::Result<()> {
    self.link.send(Msg::Value(value.0)).map_err(|m| Exception::throw_message(&ctx, &m))
  }

  /// The next message, or `undefined` once the peer is closed and drained.
  /// Rejects with the peer's uncaught error when one occurs.
  pub fn recv<'js>(
    &self,
    ctx: Ctx<'js>,
  ) -> rquickjs::Result<Promised<impl Future<Output = JsResult<Option<Neutral>>>>> {
    let link = self.link.clone();
    Ok(with_pending(&ctx, async move { receive(&link).await }))
  }

  /// Async-iterator step over `recv()`: `{ value, done: false }` per message,
  /// `{ done: true }` once closed. A peer error rejects the step.
  pub fn next<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Promised<impl Future<Output = JsResult<Step<Neutral>>>>> {
    let link = self.link.clone();
    Ok(with_pending(&ctx, async move { receive(&link).await.map(Step) }))
  }

  /// Stop sending from this end. Idempotent.
  pub fn close(&self) {
    self.link.close();
  }

  /// Kill the child now. Only meaningful on the parent's end; a no-op on the
  /// child's.
  pub fn terminate(&self) {
    if let Some(kill) = &self.kill {
      self.link.close();
      kill.fire();
    }
  }
}

async fn receive(link: &Link) -> Result<Option<Neutral>, String> {
  match link.recv().await {
    Some(Msg::Value(v)) => Ok(Some(Neutral(v))),
    Some(Msg::Error(e)) => Err(e),
    None => Ok(None),
  }
}

/// `spawn(source, { args? })`: start a child runtime evaluating `source` as a
/// module and return the parent's port end.
fn spawn<'js>(ctx: Ctx<'js>, source: String, opts: OptArg<Object<'js>>) -> rquickjs::Result<Class<'js, Port>> {
  let args: Vec<String> = match opts.0 {
    Some(o) => o.get::<_, Option<Vec<String>>>("args")?.unwrap_or_default(),
    None => Vec::new(),
  };
  let (parent_link, child_link) = Link::pair();
  let kill = Arc::new(Kill::default());
  ctx.userdata::<Isolates>().expect("isolates registry").0.lock().expect("isolates lock poisoned").push(kill.clone());

  spawn_thread(crate::FluxEngine::config(&ctx), source, args, child_link, kill.clone())
    .map_err(|m| Exception::throw_message(&ctx, &m))?;
  Port::create(&ctx, parent_link, Some(kill))
}

/// Run a child engine on its own thread until its loop goes idle or its kill
/// switch fires. The child inherits the parent's host config; its uncaught
/// errors are forwarded on the link.
#[cfg(feature = "compile")]
fn spawn_thread(
  mut config: EngineConfig,
  source: String,
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
    .name("flux-isolate".to_string())
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
          .plugin(move |ctx| {
            ctx.store_userdata(ChildLink(link)).expect("store child link");
          })
          .build();
        tokio::select! {
          _ = engine.eval_source(&source) => {}
          _ = kill.fired() => {}
        }
      }));
    })
    .map(|_| ())
    .map_err(|e| format!("isolate: failed to start thread: {e}"))
}

#[cfg(not(feature = "compile"))]
fn spawn_thread(
  _config: EngineConfig,
  _source: String,
  _args: Vec<String>,
  _link: Link,
  _kill: Arc<Kill>,
) -> Result<(), String> {
  Err("isolate: this build cannot evaluate source (compile feature off)".to_string())
}

pub struct IsolateModule;

impl ModuleDef for IsolateModule {
  fn declare(decl: &Declarations<'_>) -> rquickjs::Result<()> {
    decl.declare("spawn")?;
    decl.declare("port")?;
    decl.declare("Port")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    if ctx.userdata::<Isolates>().is_none() {
      ctx.store_userdata(Isolates::default()).expect("store isolates registry");
    }
    let port = match ctx.userdata::<ChildLink>() {
      Some(child) => Port::create(ctx, child.0.clone(), None)?.into_value(),
      None => Value::new_undefined(ctx.clone()),
    };
    let ctor = Class::<Port>::create_constructor(ctx)?.expect("Port class has a constructor");
    exports.export("Port", ctor)?;
    exports.export("port", port)?;
    exports.export("spawn", Function::new(ctx.clone(), spawn)?)?;
    Ok(())
  }
}
