#[cfg(feature = "script")]
use rquickjs::promise::PromiseState;
use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Ctx, JsLifetime, Module, Persistent, Value};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

use crate::console;
use crate::events;
use crate::io;
use crate::timer;

/// Log level passed to the logger callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Log,
    Warn,
    Error,
}

/// Shared log sink, stored as userdata in the JS context.
#[derive(Clone, JsLifetime)]
pub(crate) struct Logger(#[qjs(skip_trace)] pub(crate) Arc<dyn Fn(LogLevel, &str) + Send + Sync>);

impl Logger {
    #[allow(dead_code)]
    pub(crate) fn debug(&self, msg: &str) {
        (self.0)(LogLevel::Debug, msg);
    }

    pub(crate) fn log(&self, msg: &str) {
        (self.0)(LogLevel::Log, msg);
    }

    pub(crate) fn warn(&self, msg: &str) {
        (self.0)(LogLevel::Warn, msg);
    }

    pub(crate) fn error(&self, msg: &str) {
        (self.0)(LogLevel::Error, msg);
    }
}

fn default_logger() -> Logger {
    Logger(Arc::new(|level, msg| match level {
        LogLevel::Debug | LogLevel::Log => println!("{msg}"),
        LogLevel::Warn | LogLevel::Error => eprintln!("{msg}"),
    }))
}

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
    Emit {
        event: String,
        data: String,
    },
    Shutdown,
}

type PluginFn = Box<dyn for<'js> FnOnce(Ctx<'js>) + Send>;

/// Logging function type: receives a log level and message string.
pub type LogFn = Box<dyn Fn(LogLevel, &str) + Send + Sync>;

pub struct JsEngineBuilder {
    plugins: Vec<PluginFn>,
    log_fn: Option<LogFn>,
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

    pub fn build(self) -> JsEngine {
        JsEngine::start(self.plugins, self.log_fn)
    }
}

pub struct JsEngine {
    tx: mpsc::Sender<JsCommand>,
    handle: Option<std::thread::JoinHandle<()>>,
    logger: Logger,
}

impl JsEngine {
    pub fn builder() -> JsEngineBuilder {
        JsEngineBuilder { plugins: Vec::new(), log_fn: None }
    }

    pub fn new() -> Self {
        Self::start(Vec::new(), None)
    }

    fn start(setups: Vec<PluginFn>, log_fn: Option<LogFn>) -> Self {
        let (tx, rx) = mpsc::channel::<JsCommand>(32);
        let logger = match log_fn {
            Some(f) => Logger(Arc::from(f)),
            None => default_logger(),
        };

        let engine_logger = logger.clone();
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create tokio runtime");

            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, Self::event_loop(rx, setups, engine_logger));
        });

        Self {
            tx,
            handle: Some(handle),
            logger,
        }
    }

    async fn init_context(setups: Vec<PluginFn>, logger: Logger) -> (AsyncRuntime, AsyncContext, PendingOps) {
        let runtime = AsyncRuntime::new().expect("failed to create JS runtime");
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

    async fn event_loop(mut rx: mpsc::Receiver<JsCommand>, setups: Vec<PluginFn>, logger: Logger) {
        let (runtime, context, pending) = Self::init_context(setups, logger).await;

        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    match cmd {
                        Some(JsCommand::Emit { event, data }) => {
                            context.with(|ctx| {
                                events::emit_event(&ctx, &event, data);
                            }).await;
                        }
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
                                    if let Err(e) = Module::evaluate(ctx.clone(), "main", code).catch(&ctx) {
                                        if let Some(l) = ctx.userdata::<Logger>() { l.error(&format!("module error: {e:?}")); }
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
                                            if let Err(e) = module.eval().map(|(_, promise)| promise).catch(&ctx) {
                                                if let Some(l) = ctx.userdata::<Logger>() { l.error(&format!("module error: {e:?}")); }
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

    /// Emit an event to JS listeners registered via `on(event, callback)`.
    /// `data` must be a valid JSON string; it is parsed into a JS value on the engine thread.
    /// Non-blocking: drops the event if the channel is full.
    pub fn emit(&self, event: &str, data: String) {
        if let Err(_) = self.tx.try_send(JsCommand::Emit {
            event: event.to_string(),
            data,
        }) {
            self.logger.warn(&format!("event \"{event}\" dropped: channel full"));
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

