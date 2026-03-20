use rquickjs::function::MutFn;
use rquickjs::{Ctx, Function, JsLifetime, Persistent, Value};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::engine::PendingOps;

type Listener = (u32, Persistent<Function<'static>>);

#[derive(Clone, JsLifetime)]
struct ListenerMap(#[qjs(skip_trace)] Rc<RefCell<HashMap<String, Vec<Listener>>>>);

impl Default for ListenerMap {
    fn default() -> Self {
        Self(Rc::new(RefCell::new(HashMap::new())))
    }
}

pub(crate) fn init_events(ctx: &Ctx<'_>) {
    ctx.store_userdata(ListenerMap::default()).unwrap();
    let next_id: Rc<Cell<u32>> = Rc::new(Cell::new(1));

    let on_fn = Function::new(
        ctx.clone(),
        MutFn::from(move |event: String, callback: Function<'_>| {
            let ctx = callback.ctx().clone();
            let listeners = ctx.userdata::<ListenerMap>().unwrap();
            let pending = ctx.userdata::<PendingOps>().unwrap();
            let persistent = Persistent::save(&ctx, callback);
            let id = next_id.get();
            next_id.set(id + 1);
            let mut map = listeners.0.borrow_mut();
            let is_new = !map.contains_key(&event);
            map.entry(event).or_default().push((id, persistent));
            if is_new {
                pending.hold();
            }
            id
        }),
    )
    .unwrap();

    let off_fn = Function::new(
        ctx.clone(),
        MutFn::from(move |event: String, id: u32, ctx: Ctx<'_>| {
            let listeners = ctx.userdata::<ListenerMap>().unwrap();
            let pending = ctx.userdata::<PendingOps>().unwrap();
            let mut map = listeners.0.borrow_mut();
            if let Some(cbs) = map.get_mut(&event) {
                cbs.retain(|(lid, _)| *lid != id);
                if cbs.is_empty() {
                    map.remove(&event);
                    pending.release();
                }
            }
        }),
    )
    .unwrap();

    let globals = ctx.globals();
    globals.set("on", on_fn).unwrap();
    globals.set("off", off_fn).unwrap();
}

pub(crate) fn emit_event(ctx: &Ctx<'_>, event: &str, data: String) {
    let listeners = ctx.userdata::<ListenerMap>().unwrap();
    let snapshot: Vec<_> = listeners
        .0
        .borrow()
        .get(event)
        .map(|cbs| cbs.iter().map(|(_, p)| p.clone()).collect())
        .unwrap_or_default();
    let arg = ctx
        .json_parse(data)
        .unwrap_or(Value::new_undefined(ctx.clone()));
    for p in snapshot {
        if let Ok(f) = p.restore(ctx) {
            let _ = f.call::<_, ()>((arg.clone(),));
        }
    }
}
