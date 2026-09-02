use rquickjs::class::{Trace, Tracer};
use rquickjs::{Class, Ctx, Exception, Function, JsLifetime, Object, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::logger::{CtxLogger, Logger};
use crate::pending::PendingOps;
use crate::plugins::marshal::OptArg;
use crate::forge_plugins::websocket::{call_callback, message_payload};
use crate::standards_plugins::body::JsBytes;
use forge::websocket::{
  parse_ws_url, run_client, ClientDispatch, ClientSocket, ClientWriter, CLOSED, CLOSING, CONNECTING, OPEN,
};

// Marshalling only: the URL parsing, connect, handshake, frame loop and
// writer live in `forge::websocket` (the client half); this layer holds the
// web-standard handler properties, builds the event objects, and forwards the
// driver's callbacks to them.

/// The web-standard event handler properties. Stored behind a RefCell so the
/// connection task reads whatever is assigned at the moment an event fires.
#[derive(Default)]
struct Handlers<'js> {
  open: Option<Function<'js>>,
  message: Option<Function<'js>>,
  error: Option<Function<'js>>,
  close: Option<Function<'js>>,
}

/// The web-standard `WebSocket` client global. Stage 1: `ws://` only, handler
/// properties (no addEventListener), plain-object events.
#[rquickjs::class(rename = "WebSocket")]
pub(crate) struct WebSocket<'js> {
  socket: Rc<ClientSocket>,
  handlers: Rc<RefCell<Handlers<'js>>>,
  url: String,
}

unsafe impl<'js> JsLifetime<'js> for WebSocket<'js> {
  type Changed<'to> = WebSocket<'to>;
}

impl<'js> Trace<'js> for WebSocket<'js> {
  fn trace<'a>(&self, tracer: Tracer<'a, 'js>) {
    let h = self.handlers.borrow();
    for f in [&h.open, &h.message, &h.error, &h.close].into_iter().flatten() {
      f.trace(tracer);
    }
  }
}

/// A handler property value for JS: the function, or null when unset.
fn handler_value<'js>(ctx: &Ctx<'js>, f: &Option<Function<'js>>) -> Value<'js> {
  f.clone().map_or_else(|| Value::new_null(ctx.clone()), Function::into_value)
}

#[rquickjs::methods]
impl<'js> WebSocket<'js> {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'js>, url: String) -> rquickjs::Result<Self> {
    let target = parse_ws_url(&url).map_err(|msg| Exception::throw_message(&ctx, &msg))?;
    let socket = ClientSocket::new();
    let handlers: Rc<RefCell<Handlers<'js>>> = Rc::default();
    let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
    pending.hold();
    let logger = ctx.logger();
    let dispatch = JsClientDispatch { ctx: ctx.clone(), handlers: handlers.clone(), logger: logger.clone() };
    // The writer is its own task, spawned here (spawning is host-specific)
    // with its own liveness hold.
    let writer_ctx = ctx.clone();
    let writer_pending = pending.clone();
    let writer_logger = logger.clone();
    let spawn_writer = move |writer: ClientWriter| {
      writer_pending.hold();
      writer_ctx.spawn(async move {
        writer.run(&writer_logger).await;
        writer_pending.release();
      });
    };
    let task_socket = socket.clone();
    ctx.spawn(async move {
      run_client(task_socket, target, &dispatch, spawn_writer, &logger).await;
      pending.release();
    });
    Ok(WebSocket { socket, handlers, url })
  }

  /// Queue a message: a string sends a text frame, a Uint8Array a binary frame.
  /// Throws while CONNECTING; a send on a closing or closed socket is dropped.
  pub fn send(&self, ctx: Ctx<'js>, data: Value<'js>) -> rquickjs::Result<()> {
    if self.socket.state() == CONNECTING {
      return Err(Exception::throw_message(&ctx, "WebSocket is still in CONNECTING state"));
    }
    let (kind, payload) = message_payload(&data)?;
    self.socket.send(kind, payload);
    Ok(())
  }

  /// Start the closing handshake (default code 1000). During CONNECTING this
  /// aborts the attempt; the close event then reports an unclean 1006.
  pub fn close(&self, ctx: Ctx<'js>, code: OptArg<u16>, reason: OptArg<String>) -> rquickjs::Result<()> {
    self.socket.close(code.0, reason.0).map_err(|msg| Exception::throw_message(&ctx, &msg))
  }

  #[qjs(get, rename = "readyState")]
  pub fn ready_state(&self) -> u8 {
    self.socket.state()
  }

  #[qjs(get)]
  pub fn url(&self) -> String {
    self.url.clone()
  }

  #[qjs(get, rename = "onopen")]
  pub fn onopen(&self, ctx: Ctx<'js>) -> Value<'js> {
    handler_value(&ctx, &self.handlers.borrow().open)
  }

  #[qjs(set, rename = "onopen")]
  pub fn set_onopen(&self, value: Value<'js>) {
    self.handlers.borrow_mut().open = value.into_function();
  }

  #[qjs(get, rename = "onmessage")]
  pub fn onmessage(&self, ctx: Ctx<'js>) -> Value<'js> {
    handler_value(&ctx, &self.handlers.borrow().message)
  }

  #[qjs(set, rename = "onmessage")]
  pub fn set_onmessage(&self, value: Value<'js>) {
    self.handlers.borrow_mut().message = value.into_function();
  }

  #[qjs(get, rename = "onerror")]
  pub fn onerror(&self, ctx: Ctx<'js>) -> Value<'js> {
    handler_value(&ctx, &self.handlers.borrow().error)
  }

  #[qjs(set, rename = "onerror")]
  pub fn set_onerror(&self, value: Value<'js>) {
    self.handlers.borrow_mut().error = value.into_function();
  }

  #[qjs(get, rename = "onclose")]
  pub fn onclose(&self, ctx: Ctx<'js>) -> Value<'js> {
    handler_value(&ctx, &self.handlers.borrow().close)
  }

  #[qjs(set, rename = "onclose")]
  pub fn set_onclose(&self, value: Value<'js>) {
    self.handlers.borrow_mut().close = value.into_function();
  }
}

/// The marshalling `ClientDispatch`: builds the web-standard event objects and
/// invokes whichever handler property is assigned when the event fires.
struct JsClientDispatch<'js> {
  ctx: Ctx<'js>,
  handlers: Rc<RefCell<Handlers<'js>>>,
  logger: Logger,
}

impl<'js> JsClientDispatch<'js> {
  /// Build a `{ type }` event object and pass it to `build` for event-specific
  /// fields, then invoke the handler with it. Event-building failures are
  /// logged, not propagated (there is no JS caller).
  fn fire(&self, handler: &Option<Function<'js>>, what: &str, build: impl FnOnce(&Object<'js>) -> rquickjs::Result<()>) {
    if handler.is_none() {
      return;
    }
    let event = Object::new(self.ctx.clone()).and_then(|o| {
      o.set("type", what)?;
      build(&o)?;
      Ok(o)
    });
    match event {
      Ok(event) => call_callback(&self.ctx, handler, (event,), what, &self.logger),
      Err(e) => self.logger.warn(&format!("[flux] websocket: could not build {what} event: {e}")),
    }
  }
}

impl<'js> ClientDispatch for JsClientDispatch<'js> {
  fn on_open(&self) {
    let open = self.handlers.borrow().open.clone();
    self.fire(&open, "open", |_| Ok(()));
  }

  fn on_text(&self, text: String) {
    let message = self.handlers.borrow().message.clone();
    self.fire(&message, "message", |o| o.set("data", text));
  }

  fn on_binary(&self, bytes: Vec<u8>) {
    let message = self.handlers.borrow().message.clone();
    self.fire(&message, "message", |o| o.set("data", JsBytes(bytes)));
  }

  fn on_error(&self, message: String) {
    let error = self.handlers.borrow().error.clone();
    self.fire(&error, "error", |o| o.set("message", message));
  }

  fn on_close(&self, code: u16, reason: String, was_clean: bool) {
    let close = self.handlers.borrow().close.clone();
    self.fire(&close, "close", |o| {
      o.set("code", code)?;
      o.set("reason", reason)?;
      o.set("wasClean", was_clean)
    });
  }
}

pub(crate) fn init_websocket(ctx: &Ctx<'_>) {
  Class::<WebSocket>::define(&ctx.globals()).expect("define WebSocket class");
  let ctor: Object = ctx.globals().get("WebSocket").expect("WebSocket constructor");
  let proto: Object = ctor.get("prototype").expect("WebSocket prototype");
  for (name, value) in [("CONNECTING", CONNECTING), ("OPEN", OPEN), ("CLOSING", CLOSING), ("CLOSED", CLOSED)] {
    ctor.set(name, value).expect("set WebSocket constant");
    proto.set(name, value).expect("set WebSocket constant");
  }
}
