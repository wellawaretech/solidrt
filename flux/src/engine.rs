use rquickjs::{Ctx, JsLifetime};
use std::any::Any;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use rquickjs::module::ModuleDef;

use crate::logger::{default_logger, report_error, CtxLogger, LogLevel, Logger, UncaughtHook};
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

  /// Whether two handles drive the same engine instance. Closures queued via
  /// `exec` die with their engine, so state that tracks an in-flight closure
  /// (e.g. the runner's per-pointer move gate) must reset when the engine it
  /// was queued on is replaced.
  pub fn same_engine(&self, other: &ExecHandle) -> bool {
    self.tx.same_channel(&other.tx)
  }
}

/// The host-describing part of an engine's configuration: what a runtime
/// passes on unchanged to the runtimes it spawns (`flux:isolate`). Stored in
/// context userdata so a child can be built from `FluxEngine::config(&ctx)`.
/// App-specific setup (plugins, userdata, module overrides) is not part of it.
#[derive(Clone, JsLifetime)]
pub struct EngineConfig {
  #[qjs(skip_trace)]
  pub logger: Logger,
  #[qjs(skip_trace)]
  pub cache_dir: Option<PathBuf>,
  #[qjs(skip_trace)]
  pub user_agent: Option<String>,
  #[qjs(skip_trace)]
  pub stack_size: Option<usize>,
}

pub struct FluxEngineBuilder {
  plugins: Vec<PluginFn>,
  userdata: Vec<UserdataFn>,
  module_overrides: Vec<ModuleOverrideFn>,
  logger: Option<Logger>,
  cache_dir: Option<PathBuf>,
  user_agent: Option<String>,
  stack_size: Option<usize>,
  interrupt: Option<Arc<AtomicBool>>,
  on_uncaught: Option<UncaughtHook>,
}

impl FluxEngineBuilder {
  /// A builder pre-set from a config (see `FluxEngine::config`); the child of
  /// an isolate starts here.
  pub fn from_config(config: EngineConfig) -> Self {
    let mut b = FluxEngine::builder();
    b.logger = Some(config.logger);
    b.cache_dir = config.cache_dir;
    b.user_agent = config.user_agent;
    b.stack_size = config.stack_size;
    b
  }

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

  pub fn logger<F: Fn(LogLevel, &str) + Send + Sync + 'static>(self, f: F) -> Self {
    self.logger_sink(Logger::new(Box::new(f)))
  }

  /// An existing log sink (a parent's, a shared one) instead of a closure.
  pub fn logger_sink(mut self, logger: Logger) -> Self {
    self.logger = Some(logger);
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

  /// A kill switch: while `flag` is set, running JS is interrupted (an
  /// uncatchable error unwinds it) so the engine's loop regains control. Used
  /// by isolates so `terminate()` can stop a busy child; the flag alone does
  /// not end the loop, the owner drops the engine future once it yields.
  pub fn interrupt_flag(mut self, flag: Arc<AtomicBool>) -> Self {
    self.interrupt = Some(flag);
    self
  }

  /// Called with every uncaught error this engine reports (a module-level
  /// throw, an unhandled rejection, a throw out of a fire-and-forget callback),
  /// after it is logged. Isolates use it to forward the error to the parent's
  /// port; without a hook, logging is the only report.
  pub fn on_uncaught<F: Fn(&str) + Send + Sync + 'static>(mut self, f: F) -> Self {
    self.on_uncaught = Some(UncaughtHook(Arc::new(f)));
    self
  }

  /// Directory for the fetch disk cache (`fetch(url, { cache: "force-cache" })`).
  /// Created lazily on first cached write. Without it the `cache` option is
  /// accepted but every request goes to the network.
  pub fn cache_dir(mut self, dir: PathBuf) -> Self {
    self.cache_dir = Some(dir);
    self
  }

  /// The `User-Agent` product token outgoing `fetch` requests carry. An
  /// embedder sets its own identity here (e.g. `SolidRT/<version>`); the
  /// default is the runtime's own, `FluxRT/<version>`.
  pub fn user_agent(mut self, agent: String) -> Self {
    self.user_agent = Some(agent);
    self
  }

  pub fn build(self) -> FluxEngine {
    let config = EngineConfig {
      logger: self.logger.unwrap_or_else(default_logger),
      cache_dir: self.cache_dir,
      user_agent: self.user_agent,
      stack_size: self.stack_size,
    };
    // The config lands in userdata whole (for children) and split into the
    // per-plugin userdata fetch/http read.
    let mut userdata = self.userdata;
    if let Some(dir) = config.cache_dir.clone() {
      userdata.push(Box::new(move |ctx| {
        ctx.store_userdata(crate::plugins::standards::fetch::FetchCacheDir(dir)).expect("store cache dir");
      }));
    }
    if let Some(agent) = config.user_agent.clone() {
      userdata.push(Box::new(move |ctx| {
        ctx.store_userdata(crate::plugins::standards::http::UserAgent(agent)).expect("store user agent");
      }));
    }
    let stored = config.clone();
    userdata.push(Box::new(move |ctx| {
      ctx.store_userdata(stored).expect("store engine config");
    }));
    let (exec_tx, exec_rx) = tokio::sync::mpsc::unbounded_channel();
    FluxEngine {
      setups: self.plugins,
      userdata,
      module_overrides: self.module_overrides,
      exec_tx,
      exec_rx,
      logger: config.logger,
      stack_size: config.stack_size,
      interrupt: self.interrupt,
      on_uncaught: self.on_uncaught,
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
  interrupt: Option<Arc<AtomicBool>>,
  on_uncaught: Option<UncaughtHook>,
}

impl FluxEngine {
  pub fn builder() -> FluxEngineBuilder {
    FluxEngineBuilder {
      plugins: Vec::new(),
      userdata: Vec::new(),
      module_overrides: Vec::new(),
      logger: None,
      cache_dir: None,
      user_agent: None,
      stack_size: None,
      interrupt: None,
      on_uncaught: None,
    }
  }

  /// The running engine's host config, for building a child engine.
  pub fn config(ctx: &Ctx<'_>) -> EngineConfig {
    ctx.userdata::<EngineConfig>().expect("engine config userdata").clone()
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
            Err(e) => report_error(&ctx, &format!("module error: {e:?}")),
          },
          Err(e) => report_error(&ctx, &format!("bytecode load error: {e}")),
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
          Err(e) => report_error(&ctx, &format!("module error: {e:?}")),
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
    let on_uncaught = self.on_uncaught.clone();
    let mut exec_rx = self.exec_rx;

    let (runtime, context, pending, rejections) = plugins::init_context(
      self.setups,
      self.userdata,
      self.module_overrides,
      self.logger,
      self.stack_size,
      self.interrupt,
      self.on_uncaught,
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
              flush_rejections(&rejections, &logger, on_uncaught.as_ref());
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
fn flush_rejections(rejections: &plugins::RejectionLog, logger: &Logger, on_uncaught: Option<&UncaughtHook>) {
  let mut pending = rejections.0.lock().expect("rejection log poisoned");
  for (_key, message) in pending.drain() {
    logger.error(&message);
    if let Some(hook) = on_uncaught {
      hook.call(&message);
    }
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
///
/// A synchronous top-level throw makes QuickJS reject two promises: the one it
/// returns (handled here) and an internal one nothing can observe, which the
/// tracker records as unhandled. The handler drops that duplicate from the log
/// by message so one error is reported once.
fn report_rejection<'js>(ctx: &Ctx<'js>, promise: rquickjs::Promise<'js>) {
  use rquickjs::function::{MutFn, This};
  use rquickjs::{Function, Undefined, Value};

  let on_rejected = match Function::new(
    ctx.clone(),
    MutFn::from(move |ctx: Ctx<'_>, err: Value<'_>| {
      let message = match err.as_exception() {
        Some(exc) => format!("{exc}"),
        None => format!("{err:?}"),
      };
      let duplicate = format!("Uncaught (in promise) {message}");
      if let Some(log) = ctx.userdata::<plugins::RejectionLog>() {
        log.0.lock().expect("rejection log poisoned").retain(|_, m| *m != duplicate);
      }
      report_error(&ctx, &message);
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
