use rquickjs::{function::MutFn, AsyncContext, Ctx, Function};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;
use tokio::sync::oneshot;

use crate::engine::PendingOps;

type ActiveMap = Rc<std::cell::RefCell<HashMap<u32, oneshot::Sender<()>>>>;

#[derive(Clone)]
pub(crate) struct Timers {
    next_id: Rc<Cell<u32>>,
    active: ActiveMap,
    pending: PendingOps,
}

impl Timers {
    pub fn new(ctx: &Ctx<'_>) -> Self {
        Self {
            next_id: Rc::new(Cell::new(1)),
            active: Rc::new(std::cell::RefCell::new(HashMap::new())),
            pending: ctx.userdata::<PendingOps>().unwrap().clone(),
        }
    }

    fn alloc_id(&self) -> u32 {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        id
    }

    fn remove(&self, id: u32) {
        self.active.borrow_mut().remove(&id);
        self.pending.release();
    }

    fn cancel<'js>(&self, ctx: &Ctx<'js>, id: u32) -> rquickjs::Result<()> {
        let tx = self.active.borrow_mut().remove(&id);
        match tx {
            Some(tx) => {
                let _ = tx.send(());
                self.pending.release();
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
        let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
        self.active.borrow_mut().insert(id, cancel_tx);
        self.pending.hold();
        let timers = self.clone();
        ctx.spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(ms)) => {
                    timers.remove(id);
                    let _ = cb.call::<(), ()>(());
                }
                _ = cancel_rx => {}
            }
        });
        id
    }

    fn set_interval<'js>(&self, ctx: &Ctx<'js>, cb: Function<'js>, ms: u64) -> u32 {
        let id = self.alloc_id();
        let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
        self.active.borrow_mut().insert(id, cancel_tx);
        self.pending.hold();
        ctx.spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(ms));
            interval.tick().await; // skip immediate first tick
            tokio::select! {
                _ = async {
                    loop {
                        interval.tick().await;
                        let _ = cb.call::<(), ()>(());
                    }
                } => {}
                _ = cancel_rx => {}
            }
        });
        id
    }
}

pub(crate) async fn init_timers(context: &AsyncContext) {
    context
        .with(|ctx| {
            let timers = Timers::new(&ctx);
            let globals = ctx.globals();

            let set_timeout = Function::new(
                ctx.clone(),
                MutFn::from({
                    let timers = timers.clone();
                    move |cb: Function<'_>, ms: u64| -> u32 {
                        let ctx = cb.ctx().clone();
                        timers.set_timeout(&ctx, cb, ms)
                    }
                }),
            )
            .unwrap();

            let clear_timeout = Function::new(
                ctx.clone(),
                MutFn::from({
                    let timers = timers.clone();
                    move |ctx: Ctx<'_>, id: u32| timers.cancel(&ctx, id)
                }),
            )
            .unwrap();

            let set_interval = Function::new(
                ctx.clone(),
                MutFn::from({
                    let timers = timers.clone();
                    move |cb: Function<'_>, ms: u64| -> u32 {
                        let ctx = cb.ctx().clone();
                        timers.set_interval(&ctx, cb, ms)
                    }
                }),
            )
            .unwrap();

            let clear_interval = Function::new(
                ctx.clone(),
                MutFn::from(move |ctx: Ctx<'_>, id: u32| timers.cancel(&ctx, id)),
            )
            .unwrap();

            globals.set("setTimeout", set_timeout).unwrap();
            globals.set("clearTimeout", clear_timeout).unwrap();
            globals.set("setInterval", set_interval).unwrap();
            globals.set("clearInterval", clear_interval).unwrap();
        })
        .await;
}
