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
//! - The child runtime starts on the first call and lives until `terminate()`
//!   or the parent's end (exit, reload: dropping the parent context fires
//!   every child's kill switch, transitively). Module state in the child
//!   persists between calls; each `isolate()` call is its own instance.
//! - Calls run one at a time in call order (the child is single-threaded); an
//!   `async` export is awaited before the next call starts.
//! - A throw (or rejection) in the called export rejects that call with its
//!   message. An uncaught error elsewhere in the child (a failed module load,
//!   a throw out of a timer) is logged; when it ends the child, pending and
//!   later calls reject with a message naming it.
//! - `terminate()` kills the child now: busy JS is interrupted, the child
//!   runtime is dropped, pending calls reject.
//! - Reserved names: `terminate` (the method) and `then` (so a handle is not a
//!   thenable). Symbol properties are looked up on the handle itself.
//!
//! The child gets the parent's host config (`EngineConfig`, so it can spawn
//! isolates of its own) and `opts.args` as its `flux:process` argv.
//!
//! Marshalling only: the link, call protocol, and kill switch are
//! `forge::isolate`; this layer spawns the thread and engine, routes replies to
//! promises on the parent's end, and dispatches calls to the module namespace
//! on the child's end.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rquickjs::class::Trace;
use rquickjs::function::{Args, Constructor, Rest};
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::{MaybePromise, Promised};
use rquickjs::{Class, Ctx, Exception, Function, JsLifetime, Object, Value as JsValue};
use tokio::sync::oneshot;

use crate::engine::{EngineConfig, FluxEngineBuilder, ModuleCode};
use crate::logger::Logger;
use crate::plugins::js_error::JsResult;
use crate::plugins::marshal::{mark_observed, with_pending, OptArg};
use crate::plugins::value::{self, Neutral};
use forge::isolate::{Kill, Link, Msg};
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

type Reply = Result<Value, String>;

/// The parent's end of one running child.
struct Instance {
  id: String,
  link: Link,
  kill: Arc<Kill>,
  next_call: AtomicU64,
  pending: Mutex<HashMap<u64, oneshot::Sender<Reply>>>,
  /// Set once the child is gone (exited or terminated): the message every
  /// later call rejects with.
  exited: Mutex<Option<String>>,
  /// The child's last uncaught error, to name the cause when it exits.
  last_error: Mutex<Option<String>>,
}

impl Instance {
  fn deliver(&self, id: u64, result: Reply) {
    if let Some(tx) = self.pending.lock().expect("pending lock poisoned").remove(&id) {
      let _ = tx.send(result);
    }
  }

  fn exit(&self, reason: String) {
    *self.exited.lock().expect("exited lock poisoned") = Some(reason.clone());
    for (_, tx) in self.pending.lock().expect("pending lock poisoned").drain() {
      let _ = tx.send(Err(reason.clone()));
    }
  }

  fn exited(&self) -> Option<String> {
    self.exited.lock().expect("exited lock poisoned").clone()
  }

  /// Wait for call `id`'s reply. Whoever holds the link reads for everyone:
  /// replies are routed to their call's slot, so a caller may return because
  /// another caller delivered its reply.
  async fn await_reply(&self, mut rx: oneshot::Receiver<Reply>) -> Reply {
    loop {
      tokio::select! {
        biased;
        r = &mut rx => return r.unwrap_or_else(|_| Err(self.exited().unwrap_or_else(|| "isolate call dropped".to_string()))),
        m = self.link.recv() => match m {
          Some(Msg::Reply { id, result }) => self.deliver(id, result),
          Some(Msg::Error(e)) => *self.last_error.lock().expect("last error lock poisoned") = Some(e),
          Some(Msg::Call { .. }) => {}
          None => {
            let cause = self.last_error.lock().expect("last error lock poisoned").take();
            self.exit(match cause {
              Some(e) => format!("isolate '{}' exited: {e}", self.id),
              None => format!("isolate '{}' exited", self.id),
            });
          }
        },
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
  /// does.
  fn call<'js>(
    &self,
    ctx: Ctx<'js>,
    name: String,
    args: Vec<Neutral>,
  ) -> rquickjs::Result<Promised<impl Future<Output = JsResult<Neutral>>>> {
    let instance = self.instance(&ctx).map_err(|m| Exception::throw_message(&ctx, &m))?;
    let (tx, rx) = oneshot::channel();
    let id = instance.next_call.fetch_add(1, Ordering::Relaxed);
    instance.pending.lock().expect("pending lock poisoned").insert(id, tx);
    let sent = instance.link.send(Msg::Call { id, name, args: args.into_iter().map(|a| a.0).collect() });
    Ok(with_pending(&ctx, async move {
      if let Err(e) = sent {
        instance.deliver(id, Err(e));
      }
      instance.await_reply(rx).await.map(Neutral)
    }))
  }

  /// Kill the child now. Later calls reject; a handle that never called
  /// anything just never spawns.
  fn terminate(&self) {
    *self.terminated.borrow_mut() = true;
    if let Some(instance) = self.instance.borrow().as_ref() {
      instance.link.close();
      instance.kill.fire();
      instance.exit(format!("isolate '{}' terminated", self.id));
    }
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
    spawn_thread(config, code, self.args.clone(), child_link, kill.clone())?;

    let instance = Arc::new(Instance {
      id: self.id.clone(),
      link: parent_link,
      kill,
      next_call: AtomicU64::new(0),
      pending: Mutex::new(HashMap::new()),
      exited: Mutex::new(None),
      last_error: Mutex::new(None),
    });
    *self.instance.borrow_mut() = Some(instance.clone());
    Ok(instance)
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
/// name, except `terminate` (the method) and `then` (undefined, so awaiting or
/// returning a handle from an async function does not treat it as a
/// thenable). Symbols are looked up on the handle itself.
fn proxy_get<'js>(ctx: Ctx<'js>, handle: Class<'js, Isolate>, prop: JsValue<'js>) -> rquickjs::Result<JsValue<'js>> {
  let name = match prop.as_string() {
    Some(s) => s.to_string()?,
    None => return handle.as_inner().get(prop),
  };
  match name.as_str() {
    "then" => Ok(JsValue::new_undefined(ctx)),
    "terminate" => Function::new(ctx, move || handle.borrow().terminate()).map(|f| f.into_value()),
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
          .build();
        tokio::select! {
          _ = engine.eval_module(code, move |ctx, ns| serve(ctx, ns, link)) => {}
          _ = kill.fired() => {}
        }
      }));
    })
    .map(|_| ())
    .map_err(|e| format!("isolate: failed to start thread: {e}"))
}

/// The child's dispatcher: one call at a time from the link, each looked up
/// on the module namespace, awaited, and answered. Ends when the parent closes
/// the link, after which nothing holds the child's loop open.
fn serve<'js>(ctx: Ctx<'js>, ns: Object<'js>, link: Link) {
  ctx.clone().spawn(async move {
    while let Some(msg) = link.recv().await {
      let Msg::Call { id, name, args } = msg else { continue };
      let result = call_export(&ctx, &ns, &name, args).await;
      let _ = link.send(Msg::Reply { id, result });
    }
  });
}

async fn call_export<'js>(ctx: &Ctx<'js>, ns: &Object<'js>, name: &str, args: Vec<Value>) -> Reply {
  let export: JsValue<'js> = ns.get(name).map_err(|e| call_error(ctx, e))?;
  let f = export.into_function().ok_or_else(|| format!("no exported function '{name}'"))?;
  let mut call_args = Args::new(ctx.clone(), args.len());
  for arg in args {
    call_args.push_arg(Neutral(arg)).map_err(|e| call_error(ctx, e))?;
  }
  let returned: JsValue<'js> = f.call_arg(call_args).map_err(|e| call_error(ctx, e))?;
  mark_observed(&returned);
  let settled =
    MaybePromise::from_value(returned).into_future::<JsValue<'js>>().await.map_err(|e| call_error(ctx, e))?;
  value::from_js(ctx, settled).map_err(|e| call_error(ctx, e))
}

/// The rejection message for a failed call: the thrown exception's message
/// (it becomes the parent's `Error.message`, so no `Error:` prefix), with the
/// child's stack appended when there is one; a thrown non-Error as its debug
/// form.
fn call_error(ctx: &Ctx<'_>, err: rquickjs::Error) -> String {
  if !err.is_exception() {
    return err.to_string();
  }
  let caught = ctx.catch();
  let Some(exc) = caught.as_exception() else { return format!("{caught:?}") };
  let message = exc.message().unwrap_or_default();
  match exc.stack().filter(|s| !s.trim().is_empty()) {
    Some(stack) => format!("{message}\n{}", stack.trim_end()),
    None => message,
  }
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
