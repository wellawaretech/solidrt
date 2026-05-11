use rquickjs::{Ctx, JsLifetime};
use std::any::Any;
use std::sync::{Arc, Mutex};

use crate::logger::{default_logger, CtxLogger, LogFn, LogLevel, Logger};
use crate::plugins::{self, PluginFn, UserdataFn};

type ShutdownFn = Box<dyn FnOnce(&Logger) + Send>;
pub(crate) type ExecFn = Box<dyn for<'js> FnOnce(Ctx<'js>) + Send>;

#[derive(Clone, JsLifetime)]
pub struct ShutdownHooks {
  #[qjs(skip_trace)]
  inner: Arc<Mutex<Vec<ShutdownFn>>>,
}

impl ShutdownHooks {
  fn new() -> Self {
    Self {
      inner: Arc::new(Mutex::new(Vec::new())),
    }
  }

  pub fn add<F: FnOnce(&Logger) + Send + 'static>(&self, f: F) {
    self.inner.lock().unwrap().push(Box::new(f));
  }

  fn run(self, logger: &Logger) {
    for hook in self.inner.lock().unwrap().drain(..) {
      hook(logger);
    }
  }
}

pub fn on_shutdown<F: FnOnce(&Logger) + Send + 'static>(ctx: &Ctx<'_>, f: F) {
  ctx.userdata::<ShutdownHooks>().unwrap().add(f);
}

/// Send-safe handle for pushing closures into the engine from other threads.
#[derive(Clone)]
pub struct ExecHandle {
  tx: tokio::sync::mpsc::UnboundedSender<ExecFn>,
}

impl ExecHandle {
  pub fn exec<F>(&self, f: F)
  where
    F: for<'js> FnOnce(Ctx<'js>) + Send + 'static,
  {
    let _ = self.tx.send(Box::new(f));
  }
}

pub struct FluxEngineBuilder {
  plugins: Vec<PluginFn>,
  userdata: Vec<UserdataFn>,
  logger: Option<LogFn>,
  stack_size: Option<usize>,
}

impl FluxEngineBuilder {
  pub fn plugin<F>(mut self, f: F) -> Self
  where
    F: for<'js> FnOnce(Ctx<'js>) + Send + 'static,
  {
    self.plugins.push(Box::new(f));
    self
  }

  /// Store a value in the JS context's userdata before any plugins run.
  /// Plugins can then retrieve it with `ctx.userdata::<T>()`.
  pub fn userdata<T>(mut self, value: T) -> Self
  where
    T: for<'js> JsLifetime<'js> + Send + 'static,
    for<'js> <T as JsLifetime<'js>>::Changed<'static>: Any,
  {
    self.userdata.push(Box::new(move |ctx| {
      ctx.store_userdata(value).expect("failed to store userdata");
    }));
    self
  }

  pub fn logger<F: Fn(LogLevel, &str) + Send + Sync + 'static>(mut self, f: F) -> Self {
    self.logger = Some(Box::new(f));
    self
  }

  pub fn stack_size(mut self, limit: usize) -> Self {
    self.stack_size = Some(limit);
    self
  }

  pub fn build(self) -> FluxEngine {
    let logger = match self.logger {
      Some(f) => Logger(Arc::from(f)),
      None => default_logger(),
    };
    let (exec_tx, exec_rx) = tokio::sync::mpsc::unbounded_channel();
    FluxEngine {
      setups: self.plugins,
      userdata: self.userdata,
      exec_tx,
      exec_rx,
      logger,
      stack_size: self.stack_size,
    }
  }
}

pub struct FluxEngine {
  setups: Vec<PluginFn>,
  userdata: Vec<UserdataFn>,
  exec_tx: tokio::sync::mpsc::UnboundedSender<ExecFn>,
  exec_rx: tokio::sync::mpsc::UnboundedReceiver<ExecFn>,
  logger: Logger,
  stack_size: Option<usize>,
}

impl FluxEngine {
  pub fn builder() -> FluxEngineBuilder {
    FluxEngineBuilder {
      plugins: Vec::new(),
      userdata: Vec::new(),
      logger: None,
      stack_size: None,
    }
  }

  pub fn new() -> Self {
    Self::builder().build()
  }

  /// Returns a Send-safe handle for pushing closures into the engine from other threads.
  pub fn exec_handle(&self) -> ExecHandle {
    ExecHandle {
      tx: self.exec_tx.clone(),
    }
  }

  /// Evaluate pre-compiled bytecode as a module and run the event loop.
  pub async fn eval(self, bytecode: Vec<u8>) {
    self
      .run(|ctx| {
        use rquickjs::{CatchResultExt, Module};
        let loaded = unsafe { Module::load(ctx.clone(), &bytecode) };
        match loaded {
          Ok(module) => match module.eval().map(|(_, promise)| promise).catch(&ctx) {
            Ok(promise) => log_rejected(&ctx, promise.into_value()),
            Err(e) => ctx.logger().error(&format!("module error: {e:?}")),
          },
          Err(e) => ctx.logger().error(&format!("bytecode load error: {e}")),
        }
      })
      .await;
  }

  /// Evaluate JS source as a module and run the event loop.
  #[cfg(feature = "compile")]
  pub async fn eval_source(self, code: &str) {
    let code = code.to_string();
    self
      .run(move |ctx| {
        use rquickjs::{CatchResultExt, Module};
        match Module::evaluate(ctx.clone(), "main", code).catch(&ctx) {
          Ok(promise) => log_rejected(&ctx, promise.into_value()),
          Err(e) => ctx.logger().error(&format!("module error: {e:?}")),
        }
      })
      .await;
  }

  async fn run<F>(self, task: F)
  where
    F: for<'js> FnOnce(Ctx<'js>) + Send,
  {
    let shutdown_hooks = ShutdownHooks::new();
    let logger = self.logger.clone();
    let mut exec_rx = self.exec_rx;

    let (runtime, context, pending) = plugins::init_context(
      self.setups,
      self.userdata,
      self.logger,
      self.stack_size,
      shutdown_hooks.clone(),
    )
    .await;

    context.with(|ctx| task(ctx)).await;

    loop {
      tokio::select! {
          Some(f) = exec_rx.recv() => {
              context.with(|ctx| f(ctx)).await;
          }
          _ = pending.notified() => {}
          _ = runtime.idle() => {
              if pending.is_idle() {
                  break;
              }
              tokio::task::yield_now().await;
              tokio::time::sleep(std::time::Duration::from_micros(1000)).await;
          }
      }
    }

    shutdown_hooks.run(&logger);
  }
}

fn log_rejected<'js>(ctx: &Ctx<'js>, val: rquickjs::Value<'js>) {
  use rquickjs::promise::PromiseState;
  use rquickjs::Value;
  if let Some(promise) = val.as_promise() {
    if let PromiseState::Rejected = promise.state() {
      let err: Value = promise.result().unwrap().unwrap_or_else(|_| ctx.catch());
      if let Some(exc) = err.as_exception() {
        ctx.logger().error(&format!("{exc}"));
      } else {
        ctx.logger().error(&format!("{err:?}"));
      }
    }
  }
}
