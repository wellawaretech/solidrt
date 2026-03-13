#[cfg(feature = "script")]
use rquickjs::promise::PromiseState;
use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Ctx, Function, JsLifetime, Module, Persistent, Value};
use std::cell::Cell;
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};

use crate::events;
use crate::io;
use crate::timer;

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
    Emit {
        event: String,
        data: String,
    },
    Shutdown,
}

type PluginFn = Box<dyn for<'js> FnOnce(Ctx<'js>) + Send>;

pub struct JsEngineBuilder {
    plugins: Vec<PluginFn>,
}

impl JsEngineBuilder {
    pub fn plugin<F>(mut self, f: F) -> Self
    where
        F: for<'js> FnOnce(Ctx<'js>) + Send + 'static,
    {
        self.plugins.push(Box::new(f));
        self
    }

    pub fn build(self) -> JsEngine {
        JsEngine::start(self.plugins)
    }
}

pub struct JsEngine {
    tx: mpsc::Sender<JsCommand>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl JsEngine {
    pub fn builder() -> JsEngineBuilder {
        JsEngineBuilder { plugins: Vec::new() }
    }

    pub fn new() -> Self {
        Self::start(Vec::new())
    }

    fn start(setups: Vec<PluginFn>) -> Self {
        let (tx, rx) = mpsc::channel::<JsCommand>(32);

        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create tokio runtime");

            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, Self::event_loop(rx, setups));
        });

        Self {
            tx,
            handle: Some(handle),
        }
    }

    async fn init_context(setups: Vec<PluginFn>) -> (AsyncRuntime, AsyncContext, PendingOps) {
        let runtime = AsyncRuntime::new().expect("failed to create JS runtime");
        let context = AsyncContext::full(&runtime)
            .await
            .expect("failed to create JS context");

        let pending = PendingOps::new();

        context
            .with(|ctx| {
                ctx.store_userdata(pending.clone()).unwrap();
                timer::init_timers(&ctx);
                io::init_io(&ctx);
                init_globals(&ctx);
                events::init_events(&ctx);
                for setup in setups {
                    setup(ctx.clone());
                }
            })
            .await;

        (runtime, context, pending)
    }

    async fn event_loop(mut rx: mpsc::Receiver<JsCommand>, setups: Vec<PluginFn>) {
        let (runtime, context, pending) = Self::init_context(setups).await;

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
                                        eprintln!("module error: {e:?}");
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
                    // idle() resolves immediately when no JS jobs are pending;
                    // yield to let spawn_local tasks (timers) make progress.
                    tokio::task::yield_now().await;
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

    /// Emit an event to JS listeners registered via `on(event, callback)`.
    /// `data` is a JSON-encoded string that gets parsed into a JS value on the engine thread.
    /// Non-blocking: drops the event if the channel is full.
    pub fn emit(&self, event: &str, data: String) {
        let _ = self.tx.try_send(JsCommand::Emit {
            event: event.to_string(),
            data,
        });
    }

    pub fn eval_detached(&self, code: &str) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.try_send(JsCommand::Eval {
            code: code.to_string(),
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

fn init_globals(ctx: &Ctx<'_>) {
    let globals = ctx.globals();

    let print = Function::new(ctx.clone(), |msg: String| {
        println!("{msg}");
    })
    .unwrap();

    globals.set("print", print).unwrap();
}
