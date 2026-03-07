use rquickjs::{function::MutFn, AsyncContext, Ctx, Function, Persistent};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;
use tokio::task::AbortHandle;

#[derive(Clone)]
pub(crate) struct Timers {
    next_id: Rc<Cell<u32>>,
    handles: Rc<RefCell<HashMap<u32, AbortHandle>>>,
    context: AsyncContext,
    idle_notify: Rc<tokio::sync::Notify>,
}

impl Timers {
    pub fn new(context: &AsyncContext) -> Self {
        Self {
            next_id: Rc::new(Cell::new(1)),
            handles: Rc::new(RefCell::new(HashMap::new())),
            context: context.clone(),
            idle_notify: Rc::new(tokio::sync::Notify::new()),
        }
    }

    pub async fn wait_idle(&self) {
        loop {
            if self.handles.borrow().is_empty() {
                return;
            }
            self.idle_notify.notified().await;
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

pub(crate) async fn init_timers(context: &AsyncContext, timers: Timers) {
    context
        .with(|ctx| {
            let globals = ctx.globals();

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
