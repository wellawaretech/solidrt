use rquickjs::loader::{BuiltinResolver, ModuleLoader};
use rquickjs::promise::PromiseState;
use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Ctx, JsLifetime, Module, Persistent, Value};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::collections::{HashMap, VecDeque};
use tokio::sync::{mpsc, oneshot};

use crate::plugins::{console, events, io, timer, memory};
use crate::logger::{Logger, LogLevel, LogFn, default_logger};

/// Tracks pending async operations that should keep the engine alive.
#[derive(Clone, JsLifetime)]
pub(crate) struct PendingOps {
    count: Rc<Cell<u32>>,
    notify: Rc<tokio::sync::Notify>,
}


impl PendingOps {
    fn new() -> Self {
        Self {
            count: Rc::new(Cell::new(0)),
            notify: Rc::new(tokio::sync::Notify::new()),
        }
    }

    pub(crate) fn hold(&self) {
        self.count.set(self.count.get() + 1);
    }

    pub(crate) fn release(&self) {
        let n = self.count.get() - 1;
        self.count.set(n);
        if n == 0 {
            self.notify.notify_waiters();
        }
    }

    async fn wait_idle(&self) {
        loop {
            if self.count.get() == 0 {
                return;
            }
            self.notify.notified().await;
        }
    }
}

enum JsCommand {
    #[cfg(feature = "script")]
    EvalScript {
        code: String,
        responder: oneshot::Sender<Result<String, String>>,
    },
    Eval {
        code: String,
        responder: oneshot::Sender<()>,
    },
    EvalBytecode {
        bytecode: Vec<u8>,
        responder: oneshot::Sender<()>,
    },
    Shutdown,
}

type PluginFn = Box<dyn for<'js> FnOnce(Ctx<'js>) + Send>;

pub struct JsEngineBuilder {
    plugins: Vec<PluginFn>,
    log_fn: Option<LogFn>,
    event_channels: Vec<(String, usize, bool)>,
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

    pub fn build(self) -> JsEngine {
        JsEngine::start(self.plugins, self.log_fn, self.event_channels)
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

struct EventSlot {
    buf: std::sync::Mutex<VecDeque<String>>,
    capacity: usize,
    log: bool,
}

struct EventChannels {
    slots: HashMap<String, EventSlot>,
    notify: tokio::sync::Notify,
}

impl EventChannels {
    fn new(events: Vec<(String, usize, bool)>) -> Self {
        let mut slots = HashMap::new();
        for (name, capacity, log) in events {
            slots.insert(name, EventSlot {
                buf: std::sync::Mutex::new(VecDeque::with_capacity(capacity)),
                capacity,
                log,
            });
        }
        Self { slots, notify: tokio::sync::Notify::new() }
    }

    fn send(&self, event: &str, data: String, logger: &Logger) -> bool {
        if let Some(slot) = self.slots.get(event) {
            if slot.log {
                logger.debug(&format!("emit \"{event}\""));
            }
            let mut buf = slot.buf.lock().unwrap();
            if buf.len() >= slot.capacity {
                buf.pop_front();
            }
            buf.push_back(data);
            self.notify.notify_one();
            true
        } else {
            false
        }
    }

    fn drain_all(&self) -> Vec<(String, String)> {
        let mut events = Vec::new();
        for (name, slot) in &self.slots {
            let mut buf = slot.buf.lock().unwrap();
            while let Some(data) = buf.pop_front() {
                events.push((name.clone(), data));
            }
        }
        events
    }

    async fn notified(&self) {
        self.notify.notified().await;
    }
}

pub struct JsEngine {
    tx: mpsc::Sender<JsCommand>,
    handle: Option<std::thread::JoinHandle<()>>,
    #[allow(dead_code)]
    logger: Logger,
    event_channels: Arc<EventChannels>,
}

fn log_rejected<'js>(ctx: &Ctx<'js>, val: Value<'js>) {
    if let Some(promise) = val.as_promise() {
        if let PromiseState::Rejected = promise.state() {
            let err: Value = promise.result().unwrap().unwrap_or_else(|_| ctx.catch());
            if let Some(l) = ctx.userdata::<Logger>() {
                if let Some(exc) = err.as_exception() {
                    l.error(&format!("{exc}"));
                } else {
                    l.error(&format!("{err:?}"));
                }
            }
        }
    }
}


impl JsEngine {
    pub fn builder() -> JsEngineBuilder {
        JsEngineBuilder { plugins: Vec::new(), log_fn: None, event_channels: Vec::new() }
    }

    pub fn new() -> Self {
        Self::start(Vec::new(), None, Vec::new())
    }

    fn start(setups: Vec<PluginFn>, log_fn: Option<LogFn>, event_channel_defs: Vec<(String, usize, bool)>) -> Self {
        let (tx, rx) = mpsc::channel::<JsCommand>(32);
        let logger = match log_fn {
            Some(f) => Logger(Arc::from(f)),
            None => default_logger(),
        };

        let event_channels = Arc::new(EventChannels::new(event_channel_defs));
        let loop_channels = event_channels.clone();

        let engine_logger = logger.clone();
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create tokio runtime");

            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, Self::event_loop(rx, setups, engine_logger, loop_channels));
        });

        Self {
            tx,
            handle: Some(handle),
            logger,
            event_channels,
        }
    }

    async fn init_context(setups: Vec<PluginFn>, logger: Logger) -> (AsyncRuntime, AsyncContext, PendingOps) {
        let runtime = AsyncRuntime::new().expect("failed to create JS runtime");

        let mut resolver = BuiltinResolver::default();
        let mut loader = ModuleLoader::default();

        resolver
            .add_module("qjs:memory")
            .add_module("qjs:io");
        loader
            .add_module("qjs:memory", memory::MemoryModule)
            .add_module("qjs:io", io::IoModule);

        runtime.set_loader(resolver, loader).await;

        let context = AsyncContext::full(&runtime)
            .await
            .expect("failed to create JS context");

        let pending = PendingOps::new();

        context
            .with(|ctx| {
                ctx.store_userdata(pending.clone()).unwrap();
                ctx.store_userdata(logger).unwrap();
                timer::init_timers(&ctx);
                io::init_io(&ctx);
                console::init_console(&ctx);

                events::init_events(&ctx);
                for setup in setups {
                    setup(ctx.clone());
                }
            })
            .await;

        (runtime, context, pending)
    }

    async fn event_loop(mut rx: mpsc::Receiver<JsCommand>, setups: Vec<PluginFn>, logger: Logger, event_channels: Arc<EventChannels>) {
        let (runtime, context, pending) = Self::init_context(setups, logger).await;

        loop {
            // Drain dedicated event channels first (handles race between notify and select)
            for (event, data) in event_channels.drain_all() {
                context.with(|ctx| {
                    events::emit_event(&ctx, &event, data);
                }).await;
            }

            tokio::select! {
                _ = event_channels.notified() => {}
                cmd = rx.recv() => {
                    match cmd {
                        #[cfg(feature = "script")]
                        Some(JsCommand::EvalScript { code, responder }) => {
                            let persistent = context
                                .with(|ctx| {
                                    match ctx.eval::<Value, _>(code).catch(&ctx) {
                                        Ok(val) => Ok(Persistent::save(&ctx, val)),
                                        Err(e) => Err(format!("error: {e:?}")),
                                    }
                                })
                                .await;
                            match persistent {
                                Ok(persistent) => {
                                    let pending = pending.clone();
                                    let context = context.clone();
                                    tokio::task::spawn_local(async move {
                                        pending.wait_idle().await;
                                        let result = context.with(|ctx| {
                                            let val = persistent.restore(&ctx).unwrap();
                                            stringify_value(&ctx, val)
                                        }).await;
                                        let _ = responder.send(Ok(result));
                                    });
                                }
                                Err(e) => {
                                    let _ = responder.send(Err(e));
                                }
                            }
                        }
                        Some(JsCommand::Eval { code, responder }) => {
                            context
                                .with(|ctx| {
                                    match Module::evaluate(ctx.clone(), "main", code).catch(&ctx) {
                                        Ok(promise) => log_rejected(&ctx, promise.into_value()),
                                        Err(e) => {
                                            if let Some(l) = ctx.userdata::<Logger>() { l.error(&format!("module error: {e:?}")); }
                                        }
                                    }
                                })
                                .await;
                            let pending = pending.clone();
                            tokio::task::spawn_local(async move {
                                pending.wait_idle().await;
                                let _ = responder.send(());
                            });
                        }
                        Some(JsCommand::EvalBytecode { bytecode, responder }) => {
                            context
                                .with(|ctx| {
                                    let loaded = unsafe { Module::load(ctx.clone(), &bytecode) };
                                    match loaded {
                                        Ok(module) => {
                                            match module.eval().map(|(_, promise)| promise).catch(&ctx) {
                                                Ok(promise) => log_rejected(&ctx, promise.into_value()),
                                                Err(e) => {
                                                    if let Some(l) = ctx.userdata::<Logger>() { l.error(&format!("module error: {e:?}")); }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            if let Some(l) = ctx.userdata::<Logger>() { l.error(&format!("bytecode load error: {e}")); }
                                        }
                                    }
                                })
                                .await;
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

    pub async fn eval(&self, code: &str) {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(JsCommand::Eval {
                code: code.to_string(),
                responder: tx,
            })
            .await;
        let _ = rx.await;
    }

    pub async fn eval_bytecode(&self, bytecode: Vec<u8>) {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(JsCommand::EvalBytecode {
                bytecode,
                responder: tx,
            })
            .await;
        let _ = rx.await;
    }

    /// Emit an event via a dedicated per-event channel registered with `event_channel()`.
    /// When the channel is full, the oldest event is dropped.
    /// Panics if `event` was not registered with `event_channel()`.
    pub fn emit(&self, event: &str, data: String) {
        if !self.event_channels.send(event, data, &self.logger) {
            panic!("emit: event \"{event}\" not registered with event_channel()");
        }
    }

    pub fn eval_detached(&self, code: &str) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.try_send(JsCommand::Eval {
            code: code.to_string(),
            responder: tx,
        });
        rx
    }

    pub fn eval_bytecode_detached(&self, bytecode: Vec<u8>) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.try_send(JsCommand::EvalBytecode {
            bytecode,
            responder: tx,
        });
        rx
    }

    #[cfg(feature = "script")]
    pub async fn eval_script(&self, code: &str) -> Result<String, String> {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(JsCommand::EvalScript {
                code: code.to_string(),
                responder: tx,
            })
            .await;
        rx.await.unwrap_or_else(|_| Err("engine dropped".into()))
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
#[cfg(feature = "script")]
fn stringify_value<'js>(ctx: &Ctx<'js>, val: Value<'js>) -> String {
    if let Some(promise) = val.as_promise() {
        let (tag, inner) = match promise.state() {
            PromiseState::Pending => return "Promise { <pending> }".to_string(),
            PromiseState::Resolved => ("", promise.result::<Value>()),
            PromiseState::Rejected => ("<rejected> ", promise.result::<Value>()),
        };
        let inner = inner.and_then(Result::ok).unwrap_or_else(|| ctx.catch());
        return format!("Promise {{ {tag}{} }}", stringify_value(ctx, inner));
    }
    if val.is_undefined() {
        "undefined".to_string()
    } else if val.is_null() {
        "null".to_string()
    } else if let Some(s) = val.as_string() {
        format!("'{}'", s.to_string().unwrap_or_default())
    } else if val.is_array() {
        let arr = val.as_array().unwrap();
        let items: Vec<String> = (0..arr.len())
            .map(|i| {
                let item: Value = arr.get(i).unwrap();
                stringify_value(ctx, item)
            })
            .collect();
        format!("[ {} ]", items.join(", "))
    } else {
        ctx.json_stringify(val)
            .ok()
            .flatten()
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default()
    }
}

