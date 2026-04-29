use rquickjs::Ctx;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

use crate::logger::{Logger, LogLevel, LogFn, default_logger};
use crate::plugins::events::EventChannels;
use crate::plugins::{self, PluginFn};

type TokioRuntime = Arc<tokio::runtime::Runtime>;

type JsTask = Box<dyn for<'js> FnOnce(Ctx<'js>) + Send>;

enum JsCommand {
    Exec {
        task: JsTask,
        responder: oneshot::Sender<()>,
    },
    Shutdown,
}

type ShutdownFn = Box<dyn FnOnce() + Send>;

pub struct JsEngineBuilder {
    runtime: TokioRuntime,
    plugins: Vec<PluginFn>,
    log_fn: Option<LogFn>,
    event_channels: Vec<(String, usize, bool)>,
    stack_size: Option<usize>,
    shutdown_hooks: Vec<ShutdownFn>,
}

impl JsEngineBuilder {
    pub fn plugin<F>(mut self, f: F) -> Self
    where
        F: for<'js> FnOnce(Ctx<'js>) + Send + 'static,
    {
        self.plugins.push(Box::new(f));
        self
    }

    pub fn log<F: Fn(LogLevel, &str) + Send + Sync + 'static>(mut self, f: F) -> Self {
        self.log_fn = Some(Box::new(f));
        self
    }

    pub fn event_channel(mut self, event: &str, capacity: usize) -> EventChannelConfig {
        self.event_channels.push((event.to_string(), capacity, false));
        EventChannelConfig { builder: self }
    }

    pub fn stack_size(mut self, limit: usize) -> Self {
        self.stack_size = Some(limit);
        self
    }

    pub fn on_shutdown<F: FnOnce() + Send + 'static>(mut self, f: F) -> Self {
        self.shutdown_hooks.push(Box::new(f));
        self
    }

    pub fn build(self) -> JsEngine {
        JsEngine::start(self.runtime, self.plugins, self.log_fn, self.event_channels, self.stack_size, self.shutdown_hooks)
    }
}

pub struct EventChannelConfig {
    builder: JsEngineBuilder,
}

impl EventChannelConfig {
    pub fn trace(mut self) -> JsEngineBuilder {
        self.builder.event_channels.last_mut().unwrap().2 = true;
        self.builder
    }

    pub fn plugin<F>(self, f: F) -> JsEngineBuilder
    where
        F: for<'js> FnOnce(Ctx<'js>) + Send + 'static,
    {
        self.builder.plugin(f)
    }

    pub fn event_channel(self, event: &str, capacity: usize) -> EventChannelConfig {
        self.builder.event_channel(event, capacity)
    }

    pub fn build(self) -> JsEngine {
        self.builder.build()
    }
}

pub struct JsEngine {
    tx: mpsc::Sender<JsCommand>,
    handle: Option<std::thread::JoinHandle<()>>,
    #[allow(dead_code)]
    logger: Logger,
    event_channels: Arc<EventChannels>,
}

impl JsEngine {
    pub fn builder(runtime: TokioRuntime) -> JsEngineBuilder {
        JsEngineBuilder { runtime, plugins: Vec::new(), log_fn: None, event_channels: Vec::new(), stack_size: None, shutdown_hooks: Vec::new() }
    }

    pub fn new(runtime: TokioRuntime) -> Self {
        Self::start(runtime, Vec::new(), None, Vec::new(), None, Vec::new())
    }

    fn start(runtime: TokioRuntime, setups: Vec<PluginFn>, log_fn: Option<LogFn>, event_channel_defs: Vec<(String, usize, bool)>, stack_size: Option<usize>, shutdown_hooks: Vec<ShutdownFn>) -> Self {
        let (tx, rx) = mpsc::channel::<JsCommand>(32);
        let logger = match log_fn {
            Some(f) => Logger(Arc::from(f)),
            None => default_logger(),
        };

        let event_channels = Arc::new(EventChannels::new(event_channel_defs));
        let loop_channels = event_channels.clone();

        let engine_logger = logger.clone();
        let handle = std::thread::spawn(move || {
            let local = tokio::task::LocalSet::new();
            local.block_on(&*runtime, Self::event_loop(rx, setups, engine_logger, loop_channels, stack_size));
            for hook in shutdown_hooks {
                hook();
            }
        });

        Self {
            tx,
            handle: Some(handle),
            logger,
            event_channels,
        }
    }

    async fn event_loop(mut rx: mpsc::Receiver<JsCommand>, setups: Vec<PluginFn>, logger: Logger, event_channels: Arc<EventChannels>, stack_size: Option<usize>) {
        let (runtime, context, pending) = plugins::init_context(setups, logger, stack_size).await;

        loop {
            // Drain dedicated event channels first (handles race between notify and select)
            context.with(|ctx| {
                crate::plugins::events::drain_and_dispatch(&ctx, &event_channels);
            }).await;

            tokio::select! {
                _ = event_channels.notified() => {}
                cmd = rx.recv() => {
                    match cmd {
                        Some(JsCommand::Exec { task, responder }) => {
                            context.with(|ctx| task(ctx)).await;
                            let pending = pending.clone();
                            tokio::task::spawn_local(async move {
                                pending.wait_idle().await;
                                let _ = responder.send(());
                            });
                        }
                        Some(JsCommand::Shutdown) | None => break,
                    }
                }
                _ = runtime.idle() => {
                    // yield to let spawn_local tasks (timers) make progress
                    tokio::task::yield_now().await;
                    tokio::time::sleep(std::time::Duration::from_micros(100)).await;
                }
            }
        }
    }

    /// Execute a closure on the JS thread and wait for all pending ops to drain.
    async fn exec(&self, task: impl for<'js> FnOnce(Ctx<'js>) + Send + 'static) {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.send(JsCommand::Exec {
            task: Box::new(task),
            responder: tx,
        }).await;
        let _ = rx.await;
    }

    /// Execute a closure on the JS thread without waiting (fire-and-forget).
    fn exec_detached(&self, task: impl for<'js> FnOnce(Ctx<'js>) + Send + 'static) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.try_send(JsCommand::Exec {
            task: Box::new(task),
            responder: tx,
        });
        rx
    }

    /// Evaluate JS source as a module and wait for completion.
    pub async fn eval(&self, code: &str) {
        let code = code.to_string();
        self.exec(move |ctx| {
            use rquickjs::{CatchResultExt, Module};
            match Module::evaluate(ctx.clone(), "main", code).catch(&ctx) {
                Ok(promise) => log_rejected(&ctx, promise.into_value()),
                Err(e) => {
                    if let Some(l) = ctx.userdata::<crate::logger::Logger>() {
                        l.error(&format!("module error: {e:?}"));
                    }
                }
            }
        }).await;
    }

    /// Evaluate pre-compiled bytecode as a module and wait for completion.
    pub async fn eval_bytecode(&self, bytecode: Vec<u8>) {
        self.exec(move |ctx| {
            use rquickjs::{CatchResultExt, Module};
            let loaded = unsafe { Module::load(ctx.clone(), &bytecode) };
            match loaded {
                Ok(module) => {
                    match module.eval().map(|(_, promise)| promise).catch(&ctx) {
                        Ok(promise) => log_rejected(&ctx, promise.into_value()),
                        Err(e) => {
                            if let Some(l) = ctx.userdata::<crate::logger::Logger>() {
                                l.error(&format!("module error: {e:?}"));
                            }
                        }
                    }
                }
                Err(e) => {
                    if let Some(l) = ctx.userdata::<crate::logger::Logger>() {
                        l.error(&format!("bytecode load error: {e}"));
                    }
                }
            }
        }).await;
    }

    /// Emit an event via a dedicated per-event channel registered with `event_channel()`.
    /// When the channel is full, the oldest event is dropped.
    /// Panics if `event` was not registered with `event_channel()`.
    pub fn emit(&self, event: &str, data: String) {
        if !self.event_channels.send(event, data, &self.logger) {
            panic!("emit: event \"{event}\" not registered with event_channel()");
        }
    }

    /// Evaluate JS source as a module without waiting (fire-and-forget).
    pub fn eval_detached(&self, code: &str) -> oneshot::Receiver<()> {
        let code = code.to_string();
        self.exec_detached(move |ctx| {
            use rquickjs::{CatchResultExt, Module};
            match Module::evaluate(ctx.clone(), "main", code).catch(&ctx) {
                Ok(promise) => log_rejected(&ctx, promise.into_value()),
                Err(e) => {
                    if let Some(l) = ctx.userdata::<crate::logger::Logger>() {
                        l.error(&format!("module error: {e:?}"));
                    }
                }
            }
        })
    }

    /// Evaluate pre-compiled bytecode without waiting (fire-and-forget).
    pub fn eval_bytecode_detached(&self, bytecode: Vec<u8>) -> oneshot::Receiver<()> {
        self.exec_detached(move |ctx| {
            use rquickjs::{CatchResultExt, Module};
            let loaded = unsafe { Module::load(ctx.clone(), &bytecode) };
            match loaded {
                Ok(module) => {
                    match module.eval().map(|(_, promise)| promise).catch(&ctx) {
                        Ok(promise) => log_rejected(&ctx, promise.into_value()),
                        Err(e) => {
                            if let Some(l) = ctx.userdata::<crate::logger::Logger>() {
                                l.error(&format!("module error: {e:?}"));
                            }
                        }
                    }
                }
                Err(e) => {
                    if let Some(l) = ctx.userdata::<crate::logger::Logger>() {
                        l.error(&format!("bytecode load error: {e}"));
                    }
                }
            }
        })
    }

    pub async fn shutdown(mut self) {
        let _ = self.tx.send(JsCommand::Shutdown).await;
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for JsEngine {
    fn drop(&mut self) {
        let _ = self.tx.try_send(JsCommand::Shutdown);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn log_rejected<'js>(ctx: &Ctx<'js>, val: rquickjs::Value<'js>) {
    use rquickjs::promise::PromiseState;
    use rquickjs::Value;
    if let Some(promise) = val.as_promise() {
        if let PromiseState::Rejected = promise.state() {
            let err: Value = promise.result().unwrap().unwrap_or_else(|_| ctx.catch());
            if let Some(l) = ctx.userdata::<crate::logger::Logger>() {
                if let Some(exc) = err.as_exception() {
                    l.error(&format!("{exc}"));
                } else {
                    l.error(&format!("{err:?}"));
                }
            }
        }
    }
}
