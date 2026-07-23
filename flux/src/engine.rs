use rquickjs::{Ctx, JsLifetime};
use std::any::Any;
use std::sync::{Arc, Mutex};

use rquickjs::module::ModuleDef;

use crate::logger::{default_logger, CtxLogger, LogFn, LogLevel, Logger};
use crate::plugins::{self, ModuleOverrideFn, PluginFn, UserdataFn};

type ShutdownFn = Box<dyn FnOnce(&Logger) + Send>;
pub(crate) type ExecFn = Box<dyn for<'js> FnOnce(Ctx<'js>) + Send>;

#[derive(Clone, JsLifetime)]
pub struct ShutdownHooks {
  #[qjs(skip_trace)]
  inner: Arc<Mutex<Vec<ShutdownFn>>>,
}

impl ShutdownHooks {
  fn new() -> Self {
    Self { inner: Arc::new(Mutex::new(Vec::new())) }
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
  module_overrides: Vec<ModuleOverrideFn>,
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

  pub fn module_override<D: ModuleDef + Send + 'static>(mut self, name: &'static str, def: D) -> Self {
    self.module_overrides.push(Box::new(move |resolver, loader| {
      resolver.add_module(name);
      loader.add_module(name, def);
    }));
    self
  }

  pub fn stack_size(mut self, limit: usize) -> Self {
    self.stack_size = Some(limit);
    self
  }

  /// Directory for the fetch disk cache (`fetch(url, { cache: "force-cache" })`).
  /// Created lazily on first cached write. Without it the `cache` option is
  /// accepted but every request goes to the network.
  pub fn cache_dir(self, dir: std::path::PathBuf) -> Self {
    self.userdata(crate::plugins::standards::fetch::FetchCacheDir(dir))
  }

  /// The `User-Agent` product token outgoing `fetch` requests carry. An
  /// embedder sets its own identity here (e.g. `SolidRT/<version>`); the
  /// default is the runtime's own, `FluxRT/<version>`.
  pub fn user_agent(self, agent: String) -> Self {
    self.userdata(crate::plugins::standards::http::UserAgent(agent))
  }

  /// `cache_dir` at the dev default: `.srt-data/cache` under the working
  /// directory (the project-local dev data root, see
  /// okf/research/update-mechanism.md). Interim policy until the
  /// update-mechanism data-root resolution replaces it; a no-op when the
  /// working directory is unavailable.
  pub fn dev_cache_dir(self) -> Self {
    match std::env::current_dir() {
      Ok(cwd) => self.cache_dir(cwd.join(".srt-data").join("cache")),
      Err(_) => self,
    }
  }

  pub fn build(self) -> FluxEngine {
    let logger = match self.logger {
      Some(f) => Logger::new(f),
      None => default_logger(),
    };
    let (exec_tx, exec_rx) = tokio::sync::mpsc::unbounded_channel();
    FluxEngine {
      setups: self.plugins,
      userdata: self.userdata,
      module_overrides: self.module_overrides,
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
  module_overrides: Vec<ModuleOverrideFn>,
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
      module_overrides: Vec::new(),
      logger: None,
      stack_size: None,
    }
  }

  pub fn new() -> Self {
    Self::builder().build()
  }

  /// Returns a Send-safe handle for pushing closures into the engine from other threads.
  pub fn exec_handle(&self) -> ExecHandle {
    ExecHandle { tx: self.exec_tx.clone() }
  }

  /// Evaluate pre-compiled bytecode as a module and run the event loop.
  pub async fn eval(self, bytecode: Vec<u8>) {
    self
      .run(|ctx| {
        use rquickjs::{CatchResultExt, Module};
        let loaded = unsafe { Module::load(ctx.clone(), &bytecode) };
        match loaded {
          Ok(module) => match module.eval().map(|(_, promise)| promise).catch(&ctx) {
            Ok(promise) => report_rejection(&ctx, promise),
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
          Ok(promise) => report_rejection(&ctx, promise),
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

    let (runtime, context, pending, rejections) = plugins::init_context(
      self.setups,
      self.userdata,
      self.module_overrides,
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
              // Job queue drained: this is the microtask checkpoint at which any
              // still-unhandled rejection is genuinely unhandled. Report them.
              flush_rejections(&rejections, &logger);
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

/// Report and clear the unhandled promise rejections recorded since the last
/// checkpoint. The tracker (see `plugins::init_context`) cannot report on the
/// spot because a rejection may be handled a microtask later; once the job queue
/// drains, whatever remains here is genuinely unhandled.
fn flush_rejections(rejections: &plugins::RejectionLog, logger: &Logger) {
  let mut pending = rejections.lock().expect("rejection log poisoned");
  for (_key, message) in pending.drain() {
    logger.error(&message);
  }
}

/// Attach a rejection handler to the entry module promise so a rejection is
/// reported even when it occurs after a top-level `await`. At that point the
/// promise is still pending when `eval`/`eval_source` returns, so inspecting its
/// state once is not enough: a later rejection (for example a synchronous throw
/// from `serve()` running after an awaited import) would otherwise be swallowed
/// and the process would exit cleanly with no diagnostic. The handler also fires
/// for an already-rejected promise, since `then` schedules it as a microtask the
/// run loop drains.
fn report_rejection<'js>(ctx: &Ctx<'js>, promise: rquickjs::Promise<'js>) {
  use rquickjs::function::{MutFn, This};
  use rquickjs::{Function, Undefined, Value};

  let logger = ctx.logger();
  let on_rejected = match Function::new(
    ctx.clone(),
    MutFn::from(move |err: Value<'_>| {
      if let Some(exc) = err.as_exception() {
        logger.error(&format!("{exc}"));
      } else {
        logger.error(&format!("{err:?}"));
      }
    }),
  ) {
    Ok(f) => f,
    Err(e) => {
      ctx.logger().error(&format!("failed to build rejection handler: {e}"));
      return;
    }
  };

  let then = match promise.then() {
    Ok(then) => then,
    Err(e) => {
      ctx.logger().error(&format!("failed to attach rejection handler: {e}"));
      return;
    }
  };
  if let Err(e) = then.call::<_, ()>((This(promise), Undefined, on_rejected)) {
    ctx.logger().error(&format!("failed to attach rejection handler: {e}"));
  }
}
