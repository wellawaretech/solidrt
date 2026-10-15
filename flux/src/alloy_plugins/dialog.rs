//! `flux:dialog`: the system file dialogs, a thin marshaling layer over
//! `alloy::dialog`. `openFiles`, `saveFile` and `openDirectory` each show
//! the platform's own picker and resolve to `flux:fs` handles (a cancel is
//! an empty list or null, never an error), through a command the platform
//! loop answers later, as `navigator.clipboard` does. The option words are
//! the web pickers' (`multiple`, `suggestedName`, `startIn`, `types`), the
//! result is the one file model the runtime has.
//!
//! One dialog at a time: the platform's are modal, and a second call while
//! one is open rejects. A call holds the engine as work in flight until it
//! is answered, so `settle` waits on it.
//!
//! Answers can come from outside the dialog: a test or an agent queues one
//! (`answer`), which the next call consumes instead of showing anything,
//! and under a test host (`set_answers_only`) a call never shows a dialog
//! at all but waits on the queue. An answer that arrives while a native
//! dialog is open settles the call at once; the platform's own answer, if
//! it still comes, is dropped (SDL cannot dismiss a dialog it showed). The
//! last request made and its answer are kept for the control API.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Array, Ctx, Exception, Function, JsLifetime, Object, Persistent, Promise, Value};
use tokio::sync::oneshot;

use alloy::{AlloyCommand, DialogFilter, DialogKind, DialogRequest, Picked};

use crate::plugins::marshal::OptArg;

/// Which function made a request: what its answer is built into.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
  OpenFiles,
  SaveFile,
  OpenDirectory,
}

impl Kind {
  fn name(self) -> &'static str {
    match self {
      Kind::OpenFiles => "openFiles",
      Kind::SaveFile => "saveFile",
      Kind::OpenDirectory => "openDirectory",
    }
  }
}

/// A request as the outside sees it (the control API, a test's assertion):
/// the function and its options.
#[derive(Clone)]
struct Info {
  kind: Kind,
  json: serde_json::Value,
}

/// An answer queued ahead of a call, with what to tell once a call consumed
/// it (the test verb's promise, fulfilled with the request).
struct Queued {
  picked: Vec<Picked>,
  consumed: Option<Persistent<Function<'static>>>,
}

/// The call waiting for an answer: parked on the queue (answers only), or
/// on the platform's dialog.
struct Waiting {
  id: u64,
  info: Info,
  resolve: Persistent<Function<'static>>,
  reject: Persistent<Function<'static>>,
  _hold: crate::Hold,
}

struct Inner {
  cmd_tx: Sender<AlloyCommand>,
  queue: RefCell<VecDeque<Queued>>,
  waiting: RefCell<Option<Waiting>>,
  next_id: Cell<u64>,
  answers_only: Cell<bool>,
  /// The last request made, with its answer once it has one.
  last: RefCell<Option<serde_json::Value>>,
}

#[derive(Clone, JsLifetime)]
struct DialogPluginState(#[qjs(skip_trace)] Rc<Inner>);

/// Store the dialog plugin state in userdata. Runs at engine init, before
/// any module import; the `flux:dialog` module surface reads it.
pub(crate) fn store_state(ctx: &Ctx<'_>, cmd_tx: Sender<AlloyCommand>) {
  ctx
    .store_userdata(DialogPluginState(Rc::new(Inner {
      cmd_tx,
      queue: RefCell::new(VecDeque::new()),
      waiting: RefCell::new(None),
      next_id: Cell::new(1),
      answers_only: Cell::new(false),
      last: RefCell::new(None),
    })))
    .expect("store dialog state");
}

fn state(ctx: &Ctx<'_>) -> Rc<Inner> {
  ctx.userdata::<DialogPluginState>().expect("dialog state").0.clone()
}

fn try_state(ctx: &Ctx<'_>) -> Option<Rc<Inner>> {
  ctx.userdata::<DialogPluginState>().map(|s| s.0.clone())
}

/// Never show a dialog in this engine: every call waits on the queue (a
/// test host's engine, which has no surface to show one on and whose test
/// answers through the harness).
pub fn set_answers_only(ctx: &Ctx<'_>) {
  if let Some(state) = try_state(ctx) {
    state.answers_only.set(true);
  }
}

/// The `flux:dialog` module.
pub struct DialogModule;

impl ModuleDef for DialogModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("openFiles")?;
    decl.declare("saveFile")?;
    decl.declare("openDirectory")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("openFiles", Function::new(ctx.clone(), open_files_impl)?)?;
    exports.export("saveFile", Function::new(ctx.clone(), save_file_impl)?)?;
    exports.export("openDirectory", Function::new(ctx.clone(), open_directory_impl)?)?;
    Ok(())
  }
}

fn throw(ctx: &Ctx<'_>, api: &str, message: &str) -> rquickjs::Error {
  Exception::throw_message(ctx, &format!("{api}: {message}"))
}

/// A string the platform receives as a C string: a NUL inside it could not
/// reach the dialog, and SDL would refuse the whole request after the
/// responder was handed over, so it is refused here, up front.
fn no_nul<'js>(ctx: &Ctx<'js>, api: &str, what: &str, value: Option<String>) -> rquickjs::Result<Option<String>> {
  match value {
    Some(s) if s.contains('\0') => Err(throw(ctx, api, &format!("{what} must not contain a NUL character"))),
    other => Ok(other),
  }
}

/// The `types` option: `[{ description, extensions }]`, extensions without
/// the dot (a leading dot is tolerated, as the web's `accept` spells them).
fn parse_types<'js>(
  ctx: &Ctx<'js>,
  api: &str,
  opts: &Object<'js>,
) -> rquickjs::Result<(Vec<DialogFilter>, serde_json::Value)> {
  let Some(types) = opts.get::<_, Option<Array<'js>>>("types")? else {
    return Ok((Vec::new(), serde_json::Value::Array(Vec::new())));
  };
  let mut filters = Vec::new();
  let mut json = Vec::new();
  for entry in types.iter::<Object<'js>>() {
    let entry = entry.map_err(|_| throw(ctx, api, "types must be an array of { description, extensions }"))?;
    let description: Option<String> = entry.get("description")?;
    let extensions: Vec<String> = entry
      .get::<_, Option<Vec<String>>>("extensions")?
      .ok_or_else(|| throw(ctx, api, "each type needs its extensions, an array of strings"))?
      .into_iter()
      .map(|e| e.trim_start_matches('.').to_string())
      .collect();
    if extensions.is_empty()
      || extensions.iter().any(|e| e.is_empty() || e.contains(';') || e.contains('*') || e.contains('\0'))
    {
      return Err(throw(ctx, api, "extensions must be non-empty names without wildcards, like [\"png\", \"jpg\"]"));
    }
    let name = no_nul(ctx, api, "a type's description", description)?.unwrap_or_else(|| extensions.join(", "));
    json.push(serde_json::json!({ "description": name, "extensions": extensions }));
    filters.push(DialogFilter { name, extensions });
  }
  Ok((filters, serde_json::Value::Array(json)))
}

fn open_files_impl<'js>(ctx: Ctx<'js>, options: OptArg<Object<'js>>) -> rquickjs::Result<Promise<'js>> {
  const API: &str = "openFiles";
  let mut multiple = false;
  let mut start_in = None;
  let mut filters = Vec::new();
  let mut types = serde_json::Value::Array(Vec::new());
  if let Some(opts) = options.0 {
    multiple = opts.get::<_, Option<bool>>("multiple")?.unwrap_or(false);
    start_in = no_nul(&ctx, API, "startIn", crate::plugins::marshal::string_opt(&ctx, &opts, "startIn", API)?)?;
    (filters, types) = parse_types(&ctx, API, &opts)?;
  }
  let info = Info {
    kind: Kind::OpenFiles,
    json: serde_json::json!({ "kind": "openFiles", "multiple": multiple, "types": types, "startIn": start_in }),
  };
  begin(ctx, info, DialogRequest { kind: DialogKind::OpenFile { multiple }, filters, start_in })
}

fn save_file_impl<'js>(ctx: Ctx<'js>, options: OptArg<Object<'js>>) -> rquickjs::Result<Promise<'js>> {
  const API: &str = "saveFile";
  let mut suggested_name = None;
  let mut start_in = None;
  let mut filters = Vec::new();
  let mut types = serde_json::Value::Array(Vec::new());
  if let Some(opts) = options.0 {
    suggested_name =
      no_nul(&ctx, API, "suggestedName", crate::plugins::marshal::string_opt(&ctx, &opts, "suggestedName", API)?)?;
    start_in = no_nul(&ctx, API, "startIn", crate::plugins::marshal::string_opt(&ctx, &opts, "startIn", API)?)?;
    (filters, types) = parse_types(&ctx, API, &opts)?;
  }
  let info = Info {
    kind: Kind::SaveFile,
    json: serde_json::json!({ "kind": "saveFile", "suggestedName": suggested_name, "types": types, "startIn": start_in }),
  };
  begin(ctx, info, DialogRequest { kind: DialogKind::SaveFile { suggested_name }, filters, start_in })
}

fn open_directory_impl<'js>(ctx: Ctx<'js>, options: OptArg<Object<'js>>) -> rquickjs::Result<Promise<'js>> {
  const API: &str = "openDirectory";
  let mut start_in = None;
  if let Some(opts) = options.0 {
    start_in = no_nul(&ctx, API, "startIn", crate::plugins::marshal::string_opt(&ctx, &opts, "startIn", API)?)?;
  }
  let info =
    Info { kind: Kind::OpenDirectory, json: serde_json::json!({ "kind": "openDirectory", "startIn": start_in }) };
  begin(ctx, info, DialogRequest { kind: DialogKind::OpenDirectory, filters: Vec::new(), start_in })
}

/// An `Error` with a DOMException-style `name`, for a rejection code can
/// branch on (`e.name === "NotSupportedError"`).
fn named_error<'js>(ctx: &Ctx<'js>, name: &str, message: &str) -> rquickjs::Result<Value<'js>> {
  let error = Exception::from_message(ctx.clone(), message)?;
  error.set("name", name)?;
  Ok(error.into_value())
}

/// Start a request: answer it from the queue, park it for an answer, or
/// show the platform's dialog. Always a promise; a failure to even start
/// rejects it.
fn begin<'js>(ctx: Ctx<'js>, info: Info, request: DialogRequest) -> rquickjs::Result<Promise<'js>> {
  let state = state(&ctx);
  let (promise, resolve, reject) = Promise::new(&ctx)?;
  if state.waiting.borrow().is_some() {
    let error = named_error(&ctx, "InvalidStateError", &format!("{}: a dialog is already open", info.kind.name()))?;
    reject.call::<_, ()>((error,))?;
    return Ok(promise);
  }
  *state.last.borrow_mut() = Some(info.json.clone());

  if let Some(queued) = state.queue.borrow_mut().pop_front() {
    settle(&ctx, &state, &info, resolve, reject, Ok(queued.picked))?;
    if let Some(consumed) = queued.consumed {
      consumed.clone().restore(&ctx)?.call::<_, ()>((info.json.to_string(),))?;
    }
    return Ok(promise);
  }

  let show = !state.answers_only.get();
  if show && !alloy::dialogs_available() {
    let error =
      named_error(&ctx, "NotSupportedError", &format!("{}: no file dialog on this platform", info.kind.name()))?;
    reject.call::<_, ()>((error,))?;
    return Ok(promise);
  }

  let id = state.next_id.get();
  state.next_id.set(id + 1);
  let hold = crate::pending::PendingOps::of(&ctx).in_flight("dialog");
  *state.waiting.borrow_mut() = Some(Waiting {
    id,
    info: info.clone(),
    resolve: Persistent::save(&ctx, resolve),
    reject: Persistent::save(&ctx, reject),
    _hold: hold,
  });
  if !show {
    return Ok(promise);
  }

  let (tx, rx) = oneshot::channel();
  let respond = Box::new(move |result| {
    tx.send(result).ok();
  });
  if state.cmd_tx.send(AlloyCommand::ShowDialog { request, respond }).is_err() {
    settle_waiting(&ctx, &state, id, Err("dialog: platform loop is gone".to_string()))?;
    return Ok(promise);
  }
  let (task_ctx, task_state) = (ctx.clone(), state.clone());
  ctx.spawn(async move {
    let result = rx.await.unwrap_or_else(|_| Err("dialog: platform loop did not answer".to_string()));
    if let Err(e) = settle_waiting(&task_ctx, &task_state, id, result) {
      crate::logger::report_uncaught(&task_ctx, e, "dialog answer");
    }
  });
  Ok(promise)
}

/// Settle the waiting call `id`, if it is still the one waiting (an
/// outside answer may have settled it, and a later call may be waiting
/// under another id).
fn settle_waiting(ctx: &Ctx<'_>, state: &Inner, id: u64, result: Result<Vec<Picked>, String>) -> rquickjs::Result<()> {
  let taken = {
    let mut waiting = state.waiting.borrow_mut();
    match waiting.as_ref() {
      Some(w) if w.id == id => waiting.take(),
      _ => None,
    }
  };
  let Some(w) = taken else {
    return Ok(());
  };
  let resolve = w.resolve.clone().restore(ctx)?;
  let reject = w.reject.clone().restore(ctx)?;
  settle(ctx, state, &w.info, resolve, reject, result)
}

/// Resolve a call with its answer built for its kind (handles, a handle or
/// null, a list), or reject it with the platform's failure; record the
/// answer on the last request.
fn settle<'js>(
  ctx: &Ctx<'js>,
  state: &Inner,
  info: &Info,
  resolve: Function<'js>,
  reject: Function<'js>,
  result: Result<Vec<Picked>, String>,
) -> rquickjs::Result<()> {
  let answer = match &result {
    Ok(picked) if picked.is_empty() => serde_json::json!({ "cancelled": true }),
    Ok(picked) => serde_json::json!({
      "picked": picked.iter().map(|p| serde_json::json!({ "location": p.location, "name": p.name })).collect::<Vec<_>>()
    }),
    Err(e) => serde_json::json!({ "failed": e }),
  };
  if let Some(serde_json::Value::Object(last)) = state.last.borrow_mut().as_mut() {
    last.insert("answer".to_string(), answer);
  }
  match result {
    Ok(picked) => {
      let value = match info.kind {
        Kind::OpenFiles => {
          let arr = Array::new(ctx.clone())?;
          for (i, p) in picked.into_iter().enumerate() {
            arr.set(i, crate::forge_plugins::file::build_file_named(ctx.clone(), p.location, Some(p.name))?)?;
          }
          arr.into_value()
        }
        Kind::SaveFile => match picked.into_iter().next() {
          Some(p) => crate::forge_plugins::file::build_file_named(ctx.clone(), p.location, Some(p.name))?.into_value(),
          None => Value::new_null(ctx.clone()),
        },
        Kind::OpenDirectory => match picked.into_iter().next() {
          Some(p) => crate::forge_plugins::dir::build_dir_named(ctx.clone(), p.location, Some(p.name))?.into_value(),
          None => Value::new_null(ctx.clone()),
        },
      };
      resolve.call::<_, ()>((value,))
    }
    Err(e) => {
      let error = Exception::from_message(ctx.clone(), &e)?;
      reject.call::<_, ()>((error,))
    }
  }
}

/// Parse an answer as the harness and the control API spell it: `{ files:
/// [path, ...] }` (a string, or `{ path, name }` to name it), `{ directory:
/// path }`, or `{ cancel: true }`. A name not given is the path's file name.
pub fn parse_answer(json: &serde_json::Value) -> Result<Vec<Picked>, String> {
  const SHAPE: &str = "answer must be { files: [path, ...] }, { directory: path } or { cancel: true }";
  let Some(obj) = json.as_object() else {
    return Err(SHAPE.to_string());
  };
  if obj.get("cancel").and_then(|c| c.as_bool()) == Some(true) {
    return Ok(Vec::new());
  }
  if let Some(directory) = obj.get("directory") {
    let location = directory.as_str().ok_or_else(|| SHAPE.to_string())?.to_string();
    let name = name_of(&location);
    return Ok(vec![Picked { location, name }]);
  }
  let Some(files) = obj.get("files").and_then(|f| f.as_array()) else {
    return Err(SHAPE.to_string());
  };
  files
    .iter()
    .map(|f| {
      if let Some(path) = f.as_str() {
        return Ok(Picked { location: path.to_string(), name: name_of(path) });
      }
      let path = f.get("path").and_then(|p| p.as_str()).ok_or_else(|| SHAPE.to_string())?.to_string();
      let name = f.get("name").and_then(|n| n.as_str()).map(str::to_string).unwrap_or_else(|| name_of(&path));
      Ok(Picked { location: path, name })
    })
    .collect()
}

fn name_of(path: &str) -> String {
  std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string())
}

/// Answer the waiting call, or queue the answer for the next one. `consumed`
/// is called with the request's JSON once a call took the answer (at once
/// when one is waiting). Returns whether a call was waiting.
pub fn answer(
  ctx: &Ctx<'_>,
  picked: Vec<Picked>,
  consumed: Option<Persistent<Function<'static>>>,
) -> rquickjs::Result<bool> {
  let state = state(ctx);
  let waiting_id = state.waiting.borrow().as_ref().map(|w| w.id);
  match waiting_id {
    Some(id) => {
      let info = state.waiting.borrow().as_ref().map(|w| w.info.json.clone()).unwrap_or(serde_json::Value::Null);
      settle_waiting(ctx, &state, id, Ok(picked))?;
      if let Some(consumed) = consumed {
        consumed.clone().restore(ctx)?.call::<_, ()>((info.to_string(),))?;
      }
      Ok(true)
    }
    None => {
      state.queue.borrow_mut().push_back(Queued { picked, consumed });
      Ok(false)
    }
  }
}

/// What the control API reports: the request waiting for an answer (null
/// when none), the last request made with its answer, how many answers are
/// queued, and whether this platform can show a dialog.
pub fn status(ctx: &Ctx<'_>) -> serde_json::Value {
  let Some(state) = try_state(ctx) else {
    return serde_json::json!({ "pending": null, "last": null, "queued": 0, "available": false });
  };
  serde_json::json!({
    "pending": state.waiting.borrow().as_ref().map(|w| w.info.json.clone()),
    "last": state.last.borrow().clone(),
    "queued": state.queue.borrow().len(),
    "available": alloy::dialogs_available(),
  })
}
