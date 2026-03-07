use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Ctx, Function, Module, Persistent, Value, function::MutFn, promise::PromiseState};
use std::cell::Cell;
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};

use crate::timer::{self, Timers};

/// Tracks pending async operations (beyond timers) that should keep the engine alive.
#[derive(Clone)]
struct PendingOps {
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

    fn hold(&self) {
        self.count.set(self.count.get() + 1);
    }

    fn release(&self) {
        let n = self.count.get() - 1;
        self.count.set(n);
        if n == 0 {
            self.notify.notify_waiters();
        }
    }

    fn is_idle(&self) -> bool {
        self.count.get() == 0
    }

    async fn wait_idle(&self) {
        loop {
            if self.is_idle() {
                return;
            }
            self.notify.notified().await;
        }
    }
}

//make a generic fn out of this? 
async fn wait_all_idle(timers: &Timers, pending: &PendingOps) {
    loop {
        timers.wait_idle().await;
        pending.wait_idle().await;
        if timers.is_idle() && pending.is_idle() {
            return;
        }
    }
}

enum JsCommand {
    EvalScript {
        code: String,
        responder: oneshot::Sender<Result<String, String>>,
    },
    Eval {
        code: String,
        responder: oneshot::Sender<()>,
    },
    Shutdown,
}

pub struct JsEngine {
    tx: mpsc::Sender<JsCommand>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl JsEngine {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<JsCommand>(32);

        let handle = std::thread::spawn(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create tokio runtime");

            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, Self::event_loop(rx));
        });

        Self {
            tx,
            handle: Some(handle),
        }
    }

    async fn event_loop(mut rx: mpsc::Receiver<JsCommand>) {
        let runtime = AsyncRuntime::new().expect("failed to create JS runtime");
        let context = AsyncContext::full(&runtime)
            .await
            .expect("failed to create JS context");

        let timers = Timers::new();
        let pending = PendingOps::new();
        init_globals(&context, timers.clone(), pending.clone()).await;

        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    match cmd {
                        // script mode only
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
                                    let timers = timers.clone();
                                    let pending = pending.clone();
                                    let context = context.clone();
                                    tokio::task::spawn_local(async move {
                                        wait_all_idle(&timers, &pending).await;
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
                            let timers = timers.clone();
                            let pending = pending.clone();
                            tokio::task::spawn_local(async move {
                                wait_all_idle(&timers, &pending).await;
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

    // script mode only
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
// script mode only
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

async fn init_globals(context: &AsyncContext, timers: Timers, pending: PendingOps) {
    timer::init_timers(context, timers).await;

    context
        .with(|ctx| {
            let globals = ctx.globals();

            globals
                .set(
                    "print",
                    Function::new(ctx.clone(), |msg: String| {
                        println!("{msg}");
                    })
                    .unwrap(),
                )
                .unwrap();

            // _load(path, cb) — cb(err, data). Called by the JS `load` wrapper.
            globals
                .set(
                    "_load",
                    Function::new(
                        ctx.clone(),
                        MutFn::from({
                            move |cb: Function<'_>, path: String| {
                                let ctx = cb.ctx().clone();
                                let pending = pending.clone();
                                pending.hold();
                                ctx.spawn(async move {
                                    match tokio::fs::read(&path).await {
                                        Ok(data) => {
                                            let ctx = cb.ctx().clone();
                                            let ta = rquickjs::TypedArray::<u8>::new(ctx, data)
                                                .unwrap();
                                            let _ = cb.call::<_, ()>((
                                                Value::new_null(cb.ctx().clone()),
                                                ta.into_value(),
                                            ));
                                        }
                                        Err(e) => {
                                            let _ = cb.call::<_, ()>((format!(
                                                "load: {path}: {e}"
                                            ),));
                                        }
                                    }
                                    pending.release();
                                });
                            }
                        }),
                    )
                    .unwrap(),
                )
                .unwrap();

            ctx.eval::<(), _>(
                "globalThis.load = (path) => new Promise((resolve, reject) => _load((err, data) => err ? reject(err) : resolve(data), path));",
            )
            .unwrap();
        })
        .await;
}
