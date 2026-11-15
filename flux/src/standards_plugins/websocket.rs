use rquickjs::class::{Trace, Tracer};
use rquickjs::{Class, Ctx, Exception, Function, JsLifetime, Object, Value};
use std::cell::RefCell;
use std::rc::Rc;

use crate::forge_plugins::http_client::FetchClient;
use crate::forge_plugins::websocket::{call_callback, message_payload};
use crate::logger::{CtxLogger, Logger};
use crate::pending::PendingOps;
use crate::plugins::marshal::OptArg;
use crate::standards_plugins::body::JsBytes;
use forge::websocket::{
  check_protocols, parse_ws_url, run_client, ClientDispatch, ClientSocket, ClientWriter, CLOSED, CLOSING, CONNECTING,
  OPEN,
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

/// The web-standard `WebSocket` client global: `ws://` and `wss://`, handler
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

/// The subprotocols of a `protocols` value: a string, or an array of
/// strings. `what` names the value in the throw.
fn protocols_of<'js>(ctx: &Ctx<'js>, value: &Value<'js>, what: &str) -> rquickjs::Result<Vec<String>> {
  let not_strings =
    || Exception::throw_syntax(ctx, &format!("WebSocket: {what} must be a string or an array of strings"));
  if let Some(s) = value.as_string() {
    return Ok(vec![s.to_string()?]);
  }
  let Some(array) = value.as_array() else {
    return Err(not_strings());
  };
  let mut protocols = Vec::with_capacity(array.len());
  for item in array.iter::<Value>() {
    match item?.as_string() {
      Some(s) => protocols.push(s.to_string()?),
      None => return Err(not_strings()),
    }
  }
  Ok(protocols)
}

/// The second constructor argument: the standard `protocols` (a string or
/// an array), or an options object `{ protocols?, client? }`.
fn options_of<'js>(
  ctx: &Ctx<'js>,
  value: Option<Value<'js>>,
) -> rquickjs::Result<(Vec<String>, Option<Class<'js, FetchClient>>)> {
  let Some(value) = value else {
    return Ok((Vec::new(), None));
  };
  if value.is_string() || value.is_array() {
    return Ok((protocols_of(ctx, &value, "protocols")?, None));
  }
  // A plain options object only (not a function, a typed array or a
  // promise): those are a mistake, not an empty offer.
  let opts = match value.as_object() {
    Some(opts) if value.type_of() == rquickjs::Type::Object => opts,
    _ => {
      return Err(Exception::throw_syntax(
        ctx,
        "WebSocket: the second argument must be a string, an array of strings or an options object",
      ))
    }
  };
  let protocols: Value = opts.get("protocols")?;
  let protocols = match protocols.is_undefined() || protocols.is_null() {
    true => Vec::new(),
    false => protocols_of(ctx, &protocols, "protocols")?,
  };
  let client = opts
    .get::<_, Option<Class<FetchClient>>>("client")
    .map_err(|_| Exception::throw_type(ctx, "WebSocket: client must be an HttpClient from createHttpClient"))?;
  Ok((protocols, client))
}

/// A handler property value for JS: the function, or null when unset.
fn handler_value<'js>(ctx: &Ctx<'js>, f: &Option<Function<'js>>) -> Value<'js> {
  f.clone().map_or_else(|| Value::new_null(ctx.clone()), Function::into_value)
}

#[rquickjs::methods]
impl<'js> WebSocket<'js> {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'js>, url: String, options: OptArg<Value<'js>>) -> rquickjs::Result<Self> {
    let mut target = parse_ws_url(&url).map_err(|msg| Exception::throw_message(&ctx, &msg))?;
    let (protocols, client) = options_of(&ctx, options.0)?;
    check_protocols(&protocols).map_err(|msg| Exception::throw_syntax(&ctx, &format!("WebSocket: {msg}")))?;
    target.protocols = protocols;
    let socket = ClientSocket::new();
    let handlers: Rc<RefCell<Handlers<'js>>> = Rc::default();
    let pending = PendingOps::of(&ctx);
    let hold = pending.standing("websocket");
    let logger = ctx.logger();
    let dispatch = JsClientDispatch { ctx: ctx.clone(), handlers: handlers.clone(), logger: logger.clone() };
    // The writer is its own task, spawned here (spawning is host-specific)
    // with its own standing hold.
    let writer_ctx = ctx.clone();
    let writer_pending = pending.clone();
    let writer_logger = logger.clone();
    let spawn_writer = move |writer: ClientWriter| {
      let hold = writer_pending.standing("websocket");
      writer_ctx.spawn(async move {
        writer.run(&writer_logger).await;
        drop(hold);
      });
    };
    // wss:// verifies the server as the client's fetches do, or without a
    // client as fetch does, against the webpki roots. `Err` is a closed
    // client, reported as a failed handshake is. ws:// takes no config.
    let tls = match (target.tls, client) {
      (false, _) => Ok(None),
      (true, Some(client)) => client.borrow().tls().map(Some).ok_or("WebSocket: the client is closed"),
      (true, None) => Ok(Some(
        forge::tls::client_config(&forge::tls::ClientOptions::default())
          .map_err(|msg| Exception::throw_message(&ctx, &msg))?,
      )),
    };
    // A WebSocket takes no per-host slot of fetch's limits, as in browsers:
    // the connection is its own, outside the fetch clients' pools.
    let task_socket = socket.clone();
    ctx.spawn(async move {
      match tls {
        Ok(tls) => run_client(task_socket, target, tls, &dispatch, spawn_writer, &logger).await,
        Err(msg) => {
          logger.warn(&format!("[flux] websocket connect failed: {msg}"));
          task_socket.fail(&dispatch, msg.to_string())
        }
      }
      drop(hold);
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

  /// The subprotocol the server picked: `""` until open, and for none.
  #[qjs(get)]
  pub fn protocol(&self) -> String {
    self.socket.protocol()
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
  fn fire(
    &self,
    handler: &Option<Function<'js>>,
    what: &str,
    build: impl FnOnce(&Object<'js>) -> rquickjs::Result<()>,
  ) {
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
