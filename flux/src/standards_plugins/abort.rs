//! The web-standard `AbortController` / `AbortSignal` globals, through the
//! solidrt lens: standard names and shapes, simplified semantics. One writer
//! (the controller's `abort`), state readable at any time (`aborted`,
//! `reason`), an `onabort` handler property (no `addEventListener`, like the
//! WebSocket client) fired once with a plain `{ type: "abort" }` event, and
//! `throwIfAborted()`. `AbortSignal.abort(reason?)` builds an already-aborted
//! signal; `AbortSignal.timeout`/`any` wait for a consumer. Without a
//! `DOMException` type, the default abort reason is an `Error` whose `name`
//! is `"AbortError"`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rquickjs::class::{Trace, Tracer};
use rquickjs::{Class, Ctx, Exception, Function, JsLifetime, Object, Value};
use tokio::sync::oneshot;

use crate::logger::report_uncaught;
use crate::plugins::marshal::OptArg;

/// One signal's state, shared by the signal and its controller.
#[derive(Default)]
struct SignalShared<'js> {
  aborted: Cell<bool>,
  /// Set exactly once, when the signal aborts.
  reason: RefCell<Option<Value<'js>>>,
  onabort: RefCell<Option<Function<'js>>>,
  /// Native consumers (fetch, isolate calls) run once on abort. Rust-only
  /// closures: they are dropped whenever the signal is collected, far too
  /// late for a captured JS value (see the Persistent trap in flux/CLAUDE.md).
  callbacks: RefCell<Vec<Box<dyn FnOnce()>>>,
}

#[rquickjs::class(rename = "AbortSignal")]
pub(crate) struct AbortSignal<'js> {
  shared: Rc<SignalShared<'js>>,
}

unsafe impl<'js> JsLifetime<'js> for AbortSignal<'js> {
  type Changed<'to> = AbortSignal<'to>;
}

impl<'js> Trace<'js> for AbortSignal<'js> {
  fn trace<'a>(&self, tracer: Tracer<'a, 'js>) {
    if let Some(r) = &*self.shared.reason.borrow() {
      r.trace(tracer);
    }
    if let Some(f) = &*self.shared.onabort.borrow() {
      f.trace(tracer);
    }
  }
}

/// The default abort reason: an `Error` named "AbortError" (there is no
/// `DOMException` here).
fn default_reason<'js>(ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
  let err = Exception::from_message(ctx.clone(), "The operation was aborted")?.into_value();
  if let Some(obj) = err.as_object() {
    obj.set("name", "AbortError")?;
  }
  Ok(err)
}

impl<'js> AbortSignal<'js> {
  fn fresh(ctx: Ctx<'js>) -> rquickjs::Result<Class<'js, AbortSignal<'js>>> {
    Class::instance(ctx, AbortSignal { shared: Rc::default() })
  }

  /// Run `f` once the signal aborts; immediately when it already has. `f`
  /// must capture no JS values (see `callbacks`). For native consumers that
  /// react to the abort without parking a task on the signal.
  pub(crate) fn on_abort(&self, f: impl FnOnce() + 'static) {
    if self.shared.aborted.get() {
      f();
    } else {
      self.shared.callbacks.borrow_mut().push(Box::new(f));
    }
  }

  /// A receiver that fires once the signal aborts; immediately when it
  /// already has. For native consumers racing work against the signal; the
  /// reason is read from the signal after the fire.
  pub(crate) fn subscribe(&self) -> oneshot::Receiver<()> {
    let (tx, rx) = oneshot::channel();
    self.on_abort(move || {
      let _ = tx.send(());
    });
    rx
  }

  /// Flip to aborted with `reason` (default: an "AbortError" `Error`) and
  /// fire `onabort` once; later calls are no-ops. A throw in the handler is
  /// reported, not propagated.
  fn do_abort(&self, ctx: &Ctx<'js>, reason: Option<Value<'js>>) -> rquickjs::Result<()> {
    if self.shared.aborted.get() {
      return Ok(());
    }
    self.shared.aborted.set(true);
    let reason = match reason {
      Some(r) if !r.is_undefined() => r,
      _ => default_reason(ctx)?,
    };
    *self.shared.reason.borrow_mut() = Some(reason);
    // Collected first: a callback registering another (`on_abort` runs it
    // immediately now) must not hit the RefCell reentrantly.
    let callbacks: Vec<_> = self.shared.callbacks.borrow_mut().drain(..).collect();
    for f in callbacks {
      f();
    }
    let handler = self.shared.onabort.borrow().clone();
    if let Some(f) = handler {
      let event = Object::new(ctx.clone())?;
      event.set("type", "abort")?;
      if let Err(e) = f.call::<_, ()>((event,)) {
        report_uncaught(ctx, e, "abort handler");
      }
    }
    Ok(())
  }
}

#[rquickjs::methods]
impl<'js> AbortSignal<'js> {
  /// `new AbortSignal()` is not a thing on the web either: signals come from
  /// a controller (or `AbortSignal.abort`).
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'js>) -> rquickjs::Result<Self> {
    Err(Exception::throw_type(&ctx, "Illegal constructor"))
  }

  /// `AbortSignal.abort(reason?)`: an already-aborted signal.
  #[qjs(static)]
  pub fn abort(ctx: Ctx<'js>, reason: OptArg<Value<'js>>) -> rquickjs::Result<Class<'js, AbortSignal<'js>>> {
    let signal = AbortSignal::fresh(ctx.clone())?;
    signal.borrow().do_abort(&ctx, reason.0)?;
    Ok(signal)
  }

  #[qjs(get)]
  pub fn aborted(&self) -> bool {
    self.shared.aborted.get()
  }

  /// The abort reason; `undefined` until aborted.
  #[qjs(get)]
  pub fn reason(&self, ctx: Ctx<'js>) -> Value<'js> {
    self.shared.reason.borrow().clone().unwrap_or_else(|| Value::new_undefined(ctx))
  }

  #[qjs(get, rename = "onabort")]
  pub fn onabort(&self, ctx: Ctx<'js>) -> Value<'js> {
    self.shared.onabort.borrow().clone().map_or_else(|| Value::new_null(ctx), Function::into_value)
  }

  #[qjs(set, rename = "onabort")]
  pub fn set_onabort(&self, value: Value<'js>) {
    *self.shared.onabort.borrow_mut() = value.into_function();
  }

  /// Throw `reason` if the signal is aborted; no-op otherwise.
  #[qjs(rename = "throwIfAborted")]
  pub fn throw_if_aborted(&self, ctx: Ctx<'js>) -> rquickjs::Result<()> {
    match self.shared.reason.borrow().clone() {
      Some(r) => Err(ctx.throw(r)),
      None => Ok(()),
    }
  }
}

#[rquickjs::class(rename = "AbortController")]
pub(crate) struct AbortController<'js> {
  signal: Class<'js, AbortSignal<'js>>,
}

unsafe impl<'js> JsLifetime<'js> for AbortController<'js> {
  type Changed<'to> = AbortController<'to>;
}

impl<'js> Trace<'js> for AbortController<'js> {
  fn trace<'a>(&self, tracer: Tracer<'a, 'js>) {
    self.signal.trace(tracer);
  }
}

#[rquickjs::methods]
impl<'js> AbortController<'js> {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'js>) -> rquickjs::Result<Self> {
    Ok(AbortController { signal: AbortSignal::fresh(ctx)? })
  }

  /// The controller's signal; the same object on every read.
  #[qjs(get)]
  pub fn signal(&self) -> Class<'js, AbortSignal<'js>> {
    self.signal.clone()
  }

  pub fn abort(&self, ctx: Ctx<'js>, reason: OptArg<Value<'js>>) -> rquickjs::Result<()> {
    self.signal.borrow().do_abort(&ctx, reason.0)
  }
}

pub(crate) fn init_abort(ctx: &Ctx<'_>) {
  Class::<AbortSignal>::define(&ctx.globals()).expect("define AbortSignal class");
  Class::<AbortController>::define(&ctx.globals()).expect("define AbortController class");
}
