use rquickjs::{Ctx, JsLifetime};
use std::any::Any;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use rquickjs::module::ModuleDef;

use crate::logger::{default_logger, format_js_error, report_error, CtxLogger, LogLevel, Logger, UncaughtHook};
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

/// A JS module ready to evaluate: source text (dev, standalone `flux`) or
/// QuickJS bytecode (packed apps; the production runtime has no compiler).
#[derive(Clone)]
pub enum ModuleCode {
  Source(String),
  Bytecode(Vec<u8>),
}

/// The embedder's answer to "where is isolate `<id>`": the module a
/// `"use isolate"` entry compiles to, by the id `isolate(id)` was called
/// with. `Err` is the message the caller's promise rejects with. Standalone
/// `flux` reads `<entry dir>/isolates/<id>.js`; packed runners (solidrt,
/// fluxrt) use [`resolve_isolate_from_assets`].
pub type IsolateResolver = Arc<dyn Fn(&str) -> Result<ModuleCode, String> + Send + Sync>;

/// Resolve an isolate id through the forge assets mount: an app's isolate
/// modules travel as manifest assets under `isolates/` - packed as bytecode
/// (`isolates/<id>.bin`), pushed in dev as source (`isolates/<id>.js`). Both
/// read through the mount, so an installed version dir, a pack folder, and a
/// packed executable resolve alike; nothing mounted means no isolates.
pub fn resolve_isolate_from_assets(id: &str) -> Result<ModuleCode, String> {
  if id.is_empty() || id.starts_with('/') || id.split('/').any(|c| c.is_empty() || c == "." || c == "..") {
    return Err(format!("isolate '{id}': not a module id"));
  }
  if let Ok(bytes) = forge::fs::read_sync(&format!("isolates/{id}.bin")) {
    return Ok(ModuleCode::Bytecode(bytes));
  }
  match forge::fs::read_sync(&format!("isolates/{id}.js")) {
    Ok(bytes) => String::from_utf8(bytes).map(ModuleCode::Source).map_err(|_| format!("isolate '{id}': not UTF-8")),
    Err(_) => Err(format!("isolate '{id}': no such isolate module in this app")),
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
  #[qjs(skip_trace)]
  pub isolate_resolver: Option<IsolateResolver>,
}

pub struct FluxEngineBuilder {
  plugins: Vec<PluginFn>,
  userdata: Vec<UserdataFn>,
  module_overrides: Vec<ModuleOverrideFn>,
  logger: Option<Logger>,
  cache_dir: Option<PathBuf>,
  user_agent: Option<String>,
  stack_size: Option<usize>,
  memory_limit: Option<usize>,
  isolate_resolver: Option<IsolateResolver>,
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
    b.isolate_resolver = config.isolate_resolver;
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

  /// Heap limit in bytes for this engine's runtime (QuickJS's memory limit):
  /// once reached, allocations fail with an out-of-memory error where they
  /// happen instead of growing the process. Per engine, not part of the
  /// inherited `EngineConfig`: an isolate's limit does not cascade to the
  /// isolates it spawns.
  pub fn memory_limit(mut self, limit: usize) -> Self {
    self.memory_limit = Some(limit);
    self
  }

  /// How `flux:isolate` finds the module for `isolate(id)`. Without one every
  /// `isolate()` call rejects. Inherited by spawned runtimes, so isolates nest.
  pub fn isolate_resolver<F>(mut self, f: F) -> Self
  where
    F: Fn(&str) -> Result<ModuleCode, String> + Send + Sync + 'static,
  {
    self.isolate_resolver = Some(Arc::new(f));
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
      isolate_resolver: self.isolate_resolver,
    };
    // The config lands in userdata whole (for children) and split into the
    // per-plugin userdata fetch/http read.
    let mut userdata = self.userdata;
    if let Some(dir) = config.cache_dir.clone() {
      userdata.push(Box::new(move |ctx| {
        ctx.store_userdata(crate::standards_plugins::fetch::FetchCacheDir(dir)).expect("store cache dir");
      }));
    }
    if let Some(agent) = config.user_agent.clone() {
      userdata.push(Box::new(move |ctx| {
        ctx.store_userdata(crate::standards_plugins::http::UserAgent(agent)).expect("store user agent");
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
      memory_limit: self.memory_limit,
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
  memory_limit: Option<usize>,
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
      memory_limit: None,
      isolate_resolver: None,
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

  /// Evaluate a module and, once its top level has finished (including any
  /// top-level `await`), hand its namespace to `on_ready`; then run the event
  /// loop. A failed evaluation is reported like the entry module's; `on_ready`
  /// is not called. This is how an isolate serves its exports. `name` is the
  /// module name stack frames cite; bytecode carries its own, baked in at
  /// compile time.
  pub async fn eval_module<F>(self, name: String, code: ModuleCode, on_ready: F)
  where
    F: for<'js> FnOnce(Ctx<'js>, rquickjs::Object<'js>) + Send + 'static,
  {
    self
      .run(move |ctx| {
        use rquickjs::{CatchResultExt, Module};
        let declared = match code {
          ModuleCode::Bytecode(bytes) => {
            unsafe { Module::load(ctx.clone(), &bytes) }.map_err(|e| format!("bytecode load error: {e}"))
          }
          #[cfg(feature = "compile")]
          ModuleCode::Source(source) => {
            Module::declare(ctx.clone(), name, source).catch(&ctx).map_err(|e| format!("module error: {e:?}"))
          }
          #[cfg(not(feature = "compile"))]
          ModuleCode::Source(_) => Err(format!("this build cannot evaluate source module '{name}' (compile feature off)")),
        };
        let evaluated = declared.and_then(|m| m.eval().catch(&ctx).map_err(|e| format!("module error: {e:?}")));
        let (module, promise) = match evaluated {
          Ok(pair) => pair,
          Err(msg) => return report_error(&ctx, &msg),
        };
        report_rejection(&ctx, promise.clone());
        on_fulfilled(&ctx, promise, move |ctx| match module.namespace() {
          Ok(ns) => on_ready(ctx, ns),
          Err(e) => report_error(&ctx, &format!("module namespace error: {e}")),
        });
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
      self.memory_limit,
      self.interrupt,
      self.on_uncaught,
      shutdown_hooks.clone(),
    )
    .await;

    context.with(|ctx| task(ctx)).await;
    // Checkpoint after the entry evaluation too: exec closures queued during
    // startup must not race the module's own microtasks.
    drain_job_queue(&runtime).await;
    flush_rejections(&rejections, &logger, on_uncaught.as_ref());

    // Set once runtime.idle() completes (jobs drained, spawner empty) while
    // pending ops still hold the engine open. Re-polling idle() then resolves
    // immediately, so the arm is disabled and the loop parks on the exec
    // channel and the release notification; an exec closure re-arms it.
    let mut runtime_drained = false;
    loop {
      // Register for the release notification before re-checking the count:
      // Notify stores no permit, so a release landing between the check and
      // the await would otherwise be lost and park the loop for good.
      let notified = pending.notified();
      tokio::pin!(notified);
      notified.as_mut().enable();
      if runtime_drained && pending.is_idle() {
        break;
      }
      tokio::select! {
          Some(f) = exec_rx.recv() => {
              context.with(|ctx| f(ctx)).await;
              drain_job_queue(&runtime).await;
              flush_rejections(&rejections, &logger, on_uncaught.as_ref());
              runtime_drained = false;
          }
          _ = &mut notified => {}
          _ = runtime.idle(), if !runtime_drained => {
              // Job queue drained: this is the microtask checkpoint at which any
              // still-unhandled rejection is genuinely unhandled. Report them.
              flush_rejections(&rejections, &logger, on_uncaught.as_ref());
              if pending.is_idle() {
                  break;
              }
              runtime_drained = true;
          }
      }
    }

    shutdown_hooks.run(&logger);
  }
}

/// The microtask checkpoint after each JS entry (the initial evaluation, then
/// every exec closure): run the job queue dry so the next closure (the next
/// event dispatch) observes every signal write the previous one queued. Runs only ready work - unlike `idle()`, this never
/// waits on futures spawned inside the runtime (timers, serve loops).
async fn drain_job_queue(runtime: &rquickjs::AsyncRuntime) {
  loop {
    match runtime.execute_pending_job().await {
      Ok(true) => {}
      Ok(false) => break,
      Err(e) => {
        e.0
          .with(|ctx| {
            let msg = format_js_error(&ctx, rquickjs::Error::Exception);
            report_error(&ctx, &format!("job error: {msg}"));
          })
          .await;
      }
    }
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

/// Run `f` once `promise` fulfills (a rejection is somebody else's report).
fn on_fulfilled<'js, F>(ctx: &Ctx<'js>, promise: rquickjs::Promise<'js>, f: F)
where
  F: FnOnce(Ctx<'js>) + 'js,
{
  use rquickjs::function::{OnceFn, This};
  use rquickjs::Function;

  let handler = match Function::new(ctx.clone(), OnceFn::from(move |ctx: Ctx<'js>| f(ctx))) {
    Ok(f) => f,
    Err(e) => return ctx.logger().error(&format!("failed to build fulfillment handler: {e}")),
  };
  let then = match promise.then() {
    Ok(then) => then,
    Err(e) => return ctx.logger().error(&format!("failed to attach fulfillment handler: {e}")),
  };
  if let Err(e) = then.call::<_, ()>((This(promise), handler)) {
    ctx.logger().error(&format!("failed to attach fulfillment handler: {e}"));
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
