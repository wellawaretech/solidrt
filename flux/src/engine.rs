use std::time::Duration;

use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Ctx, Function, Module, Persistent, Value, promise::PromiseState};
use tokio::sync::{mpsc, oneshot};

use crate::timer::{self, Timers};

enum JsCommand {
    EvalScript {
        code: String,
        timeout: Option<Duration>,
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

        let timers = Timers::new(&context);
        init_globals(&context, timers.clone()).await;

        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    match cmd {
                        Some(JsCommand::EvalScript { code, timeout, responder }) => {
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
                                    let context = context.clone();
                                    tokio::task::spawn_local(async move {
                                        match timeout {
                                            Some(d) => { let _ = tokio::time::timeout(d, timers.wait_idle()).await; }
                                            None => timers.wait_idle().await,
                                        }
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
                            tokio::task::spawn_local(async move {
                                timers.wait_idle().await;
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

    pub async fn eval_script(&self, code: &str, timeout: Option<Duration>) -> Result<String, String> {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(JsCommand::EvalScript {
                code: code.to_string(),
                timeout,
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

async fn init_globals(context: &AsyncContext, timers: Timers) {
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
        })
        .await;
}
