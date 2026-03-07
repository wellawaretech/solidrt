use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Ctx, Function, Value, promise::PromiseState};
use tokio::sync::{mpsc, oneshot};

use crate::timer::{self, Timers};

enum JsCommand {
    Eval {
        code: String,
        responder: oneshot::Sender<String>,
    },
    EvalModule {
        code: String,
    },
    Stringify {
        responder: oneshot::Sender<String>,
    },
    WaitIdle {
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
                        Some(JsCommand::Eval { code, responder }) => {
                            context
                                .with(|ctx| {
                                    let result = ctx
                                        .eval::<Value, _>(code)
                                        .catch(&ctx);
                                    match result {
                                        Ok(val) => {
                                            ctx.globals().set("__last", val).unwrap();
                                            let _ = responder.send(String::new());
                                        }
                                        Err(e) => {
                                            let _ = responder.send(format!("error: {e:?}"));
                                        }
                                    }
                                })
                                .await;
                        }
                        Some(JsCommand::Stringify { responder }) => {
                            context
                                .with(|ctx| {
                                    let val: Value = ctx.globals().get("__last").unwrap();
                                    let result = stringify_value(&ctx, val);
                                    let _ = responder.send(result);
                                })
                                .await;
                        }
                        Some(JsCommand::EvalModule { code }) => {
                            context
                                .with(|ctx| {
                                    if let Err(e) = ctx.eval::<Value, _>(code).catch(&ctx) {
                                        eprintln!("module error: {e:?}");
                                    }
                                })
                                .await;
                        }
                        Some(JsCommand::WaitIdle { responder }) => {
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

    pub async fn eval_module(&self, code: &str) {
        let _ = self
            .tx
            .send(JsCommand::EvalModule {
                code: code.to_string(),
            })
            .await;
    }

    pub async fn eval(&self, code: &str) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(JsCommand::Eval {
                code: code.to_string(),
                responder: tx,
            })
            .await;
        let result = rx.await.unwrap_or_else(|_| "engine dropped".into());
        if result.is_empty() { Ok(()) } else { Err(result) }
    }

    pub async fn stringify_last(&self) -> String {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(JsCommand::Stringify { responder: tx })
            .await;
        rx.await.unwrap_or_else(|_| "engine dropped".into())
    }

    pub async fn wait_idle(&self) {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(JsCommand::WaitIdle { responder: tx })
            .await;
        let _ = rx.await;
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
    if val.is_undefined() {
        "undefined".to_string()
    } else if val.is_null() {
        "null".to_string()
    } else if let Some(promise) = val.as_promise() {
        match promise.state() {
            PromiseState::Pending => "Promise { <pending> }".to_string(),
            PromiseState::Resolved => {
                let inner: Value = promise.result().unwrap().unwrap();
                format!("Promise {{ {} }}", stringify_value(ctx, inner))
            }
            PromiseState::Rejected => {
                match promise.result::<Value>() {
                    Some(Ok(inner)) => format!("Promise {{ <rejected> {} }}", stringify_value(ctx, inner)),
                    Some(Err(_)) => {
                        let caught = ctx.catch();
                        format!("Promise {{ <rejected> {} }}", stringify_value(ctx, caught))
                    }
                    None => "Promise { <rejected> }".to_string(),
                }
            }
        }
    } else if let Some(s) = val.as_string() {
        s.to_string().unwrap_or_default()
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
                        println!("[js] {msg}");
                    })
                    .unwrap(),
                )
                .unwrap();
        })
        .await;
}
