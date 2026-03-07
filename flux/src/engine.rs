use rquickjs::{function::MutFn, AsyncContext, AsyncRuntime, CatchResultExt, Ctx, Function, Persistent};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::AbortHandle;

enum JsCommand {
    Eval {
        code: String,
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
                                        .eval::<rquickjs::Value, _>(code)
                                        .catch(&ctx)
                                        .map(|val| {
                                            if val.is_undefined() {
                                                String::new()
                                            } else if let Some(s) = val.as_string() {
                                                s.to_string().unwrap_or_default()
                                            } else {
                                                ctx.json_stringify(val)
                                                    .ok()
                                                    .flatten()
                                                    .and_then(|s| s.to_string().ok())
                                                    .unwrap_or_default()
                                            }
                                        })
                                        .unwrap_or_else(|e| format!("error: {e:?}"));
                                    let _ = responder.send(result);
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

    pub async fn eval(&self, code: &str) -> String {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .tx
            .send(JsCommand::Eval {
                code: code.to_string(),
                responder: tx,
            })
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

#[derive(Clone)]
struct Timers {
    next_id: Rc<Cell<u32>>,
    handles: Rc<RefCell<HashMap<u32, AbortHandle>>>,
    context: AsyncContext,
    idle_notify: Rc<tokio::sync::Notify>,
}

impl Timers {
    fn new(context: &AsyncContext) -> Self {
        Self {
            next_id: Rc::new(Cell::new(1)),
            handles: Rc::new(RefCell::new(HashMap::new())),
            context: context.clone(),
            idle_notify: Rc::new(tokio::sync::Notify::new()),
        }
    }

    fn alloc_id(&self) -> u32 {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        id
    }

    fn track(&self, id: u32, handle: AbortHandle) {
        self.handles.borrow_mut().insert(id, handle);
    }

    fn remove_handle(&self, id: u32) {
        self.handles.borrow_mut().remove(&id);
        if self.handles.borrow().is_empty() {
            self.idle_notify.notify_waiters();
        }
    }

    async fn wait_idle(&self) {
        loop {
            if self.handles.borrow().is_empty() {
                return;
            }
            self.idle_notify.notified().await;
        }
    }

    fn cancel<'js>(&self, ctx: &Ctx<'js>, id: u32) -> rquickjs::Result<()> {
        let removed = self.handles.borrow_mut().remove(&id);
        match removed {
            Some(h) => {
                h.abort();
                if self.handles.borrow().is_empty() {
                    self.idle_notify.notify_waiters();
                }
                Ok(())
            }
            None => Err(ctx.throw(
                rquickjs::String::from_str(ctx.clone(), &format!("invalid timer id: {id}"))
                    .unwrap()
                    .into(),
            )),
        }
    }

    fn set_timeout<'js>(&self, ctx: &Ctx<'js>, cb: Function<'js>, ms: u64) -> u32 {
        let id = self.alloc_id();
        let cb = Persistent::save(ctx, cb);
        let timers = self.clone();
        let jh = tokio::task::spawn_local(async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            timers.remove_handle(id);
            timers
                .context
                .with(|ctx| {
                    if let Ok(cb) = cb.restore(&ctx) {
                        let _ = cb.call::<(), ()>(());
                    }
                })
                .await;
        });
        self.track(id, jh.abort_handle());
        id
    }

    fn set_interval<'js>(&self, ctx: &Ctx<'js>, cb: Function<'js>, ms: u64) -> u32 {
        let id = self.alloc_id();
        let mut cb = Persistent::save(ctx, cb);
        let timers = self.clone();
        let jh = tokio::task::spawn_local(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(ms));
            interval.tick().await; // skip immediate first tick
            loop {
                interval.tick().await;
                let next = timers
                    .context
                    .with(|ctx| {
                        let f = cb.restore(&ctx).ok()?;
                        let _ = f.call::<(), ()>(());
                        Some(Persistent::save(&ctx, f))
                    })
                    .await;
                match next {
                    Some(p) => cb = p,
                    None => {
                        timers.remove_handle(id);
                        break;
                    }
                }
            }
        });
        self.track(id, jh.abort_handle());
        id
    }
}

async fn init_globals(context: &AsyncContext, timers: Timers) {

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

            globals
                .set(
                    "setTimeout",
                    Function::new(
                        ctx.clone(),
                        MutFn::from({
                            let timers = timers.clone();
                            move |cb: Function<'_>, ms: u64| -> u32 {
                                let ctx = cb.ctx().clone();
                                timers.set_timeout(&ctx, cb, ms)
                            }
                        }),
                    )
                    .unwrap(),
                )
                .unwrap();

            globals
                .set(
                    "clearTimeout",
                    Function::new(
                        ctx.clone(),
                        MutFn::from({
                            let timers = timers.clone();
                            move |ctx: Ctx<'_>, id: u32| timers.cancel(&ctx, id)
                        }),
                    )
                    .unwrap(),
                )
                .unwrap();

            globals
                .set(
                    "setInterval",
                    Function::new(
                        ctx.clone(),
                        MutFn::from({
                            let timers = timers.clone();
                            move |cb: Function<'_>, ms: u64| -> u32 {
                                let ctx = cb.ctx().clone();
                                timers.set_interval(&ctx, cb, ms)
                            }
                        }),
                    )
                    .unwrap(),
                )
                .unwrap();

            globals
                .set(
                    "clearInterval",
                    Function::new(
                        ctx.clone(),
                        MutFn::from(move |ctx: Ctx<'_>, id: u32| timers.cancel(&ctx, id)),
                    )
                    .unwrap(),
                )
                .unwrap();
        })
        .await;
}
