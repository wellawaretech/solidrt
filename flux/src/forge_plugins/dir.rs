use rquickjs::function::{MutFn, This};
use rquickjs::{promise::Promised, Ctx, Exception, Function, JsLifetime, Object};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::Notify;

use super::events::{add_listener, remove_listener};
use crate::plugins::marshal::{with_pending, OptArg};
use crate::plugins::value::Neutral;
use forge::fs;
use forge::Value;

// Marshalling for the `dir()` reference: forward to the engine-free
// `forge::fs` directory operations and encode their results back to JS.

// The directory watches alive in this context, keyed by watch id, each with
// the wakeup that tells its delivery task to stop. A task removes its own
// entry when it ends. The callback is never captured in a native closure: it
// is a listener on the event bus under the watch's own event name, so the
// bus keeps the engine alive while the watch is on, and the unsubscribe
// closure captures only ids and strings (see events.rs on why).
#[derive(Clone, JsLifetime, Default)]
pub(crate) struct InstalledWatches(#[qjs(skip_trace)] Rc<RefCell<WatchTable>>);

#[derive(Default)]
struct WatchTable {
  next_id: u32,
  stops: HashMap<u32, Arc<Notify>>,
}

fn watch_event_name(id: u32) -> String {
  format!("fs:watch:{id}")
}

// dir(path).watch(callback, { recursive }) -> unsubscribe. Opens the OS
// watch synchronously (a missing directory throws: the path is the caller's),
// then a spawned task forwards each change to the callback as
// { kind, path } until the unsubscribe fires or the OS watch ends.
fn watch<'js>(
  ctx: Ctx<'js>,
  this: This<Object<'js>>,
  callback: Function<'js>,
  opts: OptArg<Object<'js>>,
) -> rquickjs::Result<Function<'js>> {
  let path: String = this.0.get("path")?;
  let recursive = match opts.0.as_ref() {
    Some(obj) => obj.get::<_, Option<bool>>("recursive")?.unwrap_or(false),
    None => false,
  };
  let mut watcher = fs::DirWatcher::open(&path, recursive).map_err(|m| Exception::throw_message(&ctx, &m))?;

  let stop = Arc::new(Notify::new());
  let id = {
    let installed = ctx.userdata::<InstalledWatches>().expect("installed watches userdata");
    let mut table = installed.0.borrow_mut();
    let id = table.next_id;
    table.next_id += 1;
    table.stops.insert(id, stop.clone());
    id
  };
  let event = watch_event_name(id);
  let listener = add_listener(&ctx, event.clone(), callback, false);

  let ctx_cb = ctx.clone();
  let name = event.clone();
  ctx.spawn(async move {
    loop {
      let next = tokio::select! {
        got = watcher.recv() => got,
        _ = stop.notified() => None,
      };
      let Some(change) = next else { break };
      let data = Value::map([
        ("kind".to_string(), Value::String(change.kind.as_str().to_string())),
        ("path".to_string(), Value::String(change.path)),
      ]);
      super::events::emit_event(&ctx_cb, &name, Neutral(data));
    }
    // Ended by the OS watch going away rather than the unsubscribe: drop the
    // listener too, or the bus would hold the engine alive for nothing.
    remove_listener(&ctx_cb, &name, listener);
    if let Some(installed) = ctx_cb.userdata::<InstalledWatches>() {
      installed.0.borrow_mut().stops.remove(&id);
    }
  });

  Function::new(
    ctx,
    MutFn::from(move |ctx: Ctx<'_>| {
      remove_listener(&ctx, &event, listener);
      let installed = ctx.userdata::<InstalledWatches>().expect("installed watches userdata");
      let stop = installed.0.borrow().stops.get(&id).cloned();
      if let Some(stop) = stop {
        stop.notify_one();
      }
    }),
  )
}

fn build_dir<'js>(ctx: Ctx<'js>, path: String) -> rquickjs::Result<Object<'js>> {
  let path = Rc::new(path);
  let obj = Object::new(ctx.clone())?;
  obj.set("path", path.as_ref().clone())?;

  let entries_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let path = path.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        let path = path.clone();
        Ok(with_pending(&ctx, async move { fs::read_dir(&path).await.map(|entries| Neutral(Value::list(entries))) }))
      }
    }),
  )
  .expect("create entries function");
  obj.set("entries", entries_fn)?;

  let exists_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let path = path.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        let path = path.clone();
        Ok(with_pending(&ctx, async move { Ok::<bool, String>(fs::dir_exists(&path).await) }))
      }
    }),
  )
  .expect("create exists function");
  obj.set("exists", exists_fn)?;

  let create_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let path = path.clone();
      move |ctx: Ctx<'_>| -> rquickjs::Result<Promised<_>> {
        let path = path.clone();
        Ok(with_pending(&ctx, async move { fs::create_dir(&path).await }))
      }
    }),
  )
  .expect("create create function");
  obj.set("create", create_fn)?;

  obj.set("watch", Function::new(ctx.clone(), watch)?)?;

  Ok(obj)
}

pub(crate) fn dir_fn<'js>(ctx: &Ctx<'js>) -> Function<'js> {
  Function::new(ctx.clone(), build_dir).expect("create dir function")
}
