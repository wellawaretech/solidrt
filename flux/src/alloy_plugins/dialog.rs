//! `flux:dialog`: the system file dialogs, a thin marshaling layer over
//! `alloy::Dialogs`, the session this engine keeps. `openFiles`, `saveFile`
//! and `openDirectory` each resolve to `flux:fs` handles (a cancel is an
//! empty list or null, never an error); the session decides whether the
//! platform's dialog shows (through `AlloyCommand::ShowDialog`, the
//! clipboard's round trip with a later answer) or an answer a test or an
//! agent gave ahead is taken. A call holds the engine as work in flight
//! until it is answered, so `settle` waits on it. The option words are the
//! web pickers' (`multiple`, `suggestedName`, `startIn`, `types`), the
//! result is the one file model the runtime has.

use std::future::Future;
use std::rc::Rc;
use std::sync::mpsc::{SendError, Sender};
use std::sync::Arc;

use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Array, Ctx, Exception, Function, IntoJs, JsLifetime, Object, Value};
use tokio::sync::oneshot;

use alloy::{
  AlloyCommand, DialogAnswer, DialogError, DialogFilter, DialogKind, DialogRequest, DialogResponder, Dialogs,
};

use crate::plugins::marshal::OptArg;

struct Inner {
  cmd_tx: Sender<AlloyCommand>,
  dialogs: Arc<Dialogs>,
}

#[derive(Clone, JsLifetime)]
struct DialogPluginState(#[qjs(skip_trace)] Rc<Inner>);

/// Store the dialog plugin state in userdata. Runs at engine init, before
/// any module import; the `flux:dialog` module surface reads it.
pub(crate) fn store_state(ctx: &Ctx<'_>, cmd_tx: Sender<AlloyCommand>) {
  ctx
    .store_userdata(DialogPluginState(Rc::new(Inner { cmd_tx, dialogs: Dialogs::new() })))
    .expect("store dialog state");
}

fn state(ctx: &Ctx<'_>) -> Rc<Inner> {
  ctx.userdata::<DialogPluginState>().expect("dialog state").0.clone()
}

/// This engine's dialog session, for the embedder's test verb and control
/// surface (an answer in the user's place, the state). None before the GUI
/// is installed.
pub fn dialogs(ctx: &Ctx<'_>) -> Option<Arc<Dialogs>> {
  ctx.userdata::<DialogPluginState>().map(|s| s.0.dialogs.clone())
}

/// Never show a dialog in this engine: every call waits on the session's
/// queue (a test host's engine, whose test answers through the harness).
/// Every call then resolves, so the engine lists the "dialog" capability
/// whether or not the platform could show one: an app that gates on it
/// takes its dialog path under test, where the test answers.
pub fn set_answers_only(ctx: &Ctx<'_>) {
  let Some(dialogs) = dialogs(ctx) else {
    return;
  };
  dialogs.set_answers_only();
  if !alloy::dialogs_available() {
    crate::add_capability(ctx, super::DIALOG_CAPABILITY);
  }
}

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
fn parse_types<'js>(ctx: &Ctx<'js>, api: &str, opts: &Object<'js>) -> rquickjs::Result<Vec<DialogFilter>> {
  let Some(types) = opts.get::<_, Option<Array<'js>>>("types")? else {
    return Ok(Vec::new());
  };
  let mut filters = Vec::new();
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
    filters.push(DialogFilter { name, extensions });
  }
  Ok(filters)
}

fn start_in_opt<'js>(ctx: &Ctx<'js>, api: &str, opts: &Object<'js>) -> rquickjs::Result<Option<String>> {
  no_nul(ctx, api, "startIn", crate::plugins::marshal::string_opt(ctx, opts, "startIn", api)?)
}

fn open_files_impl<'js>(
  ctx: Ctx<'js>,
  options: OptArg<Object<'js>>,
) -> rquickjs::Result<Promised<impl Future<Output = Settled>>> {
  const API: &str = "openFiles";
  let mut multiple = false;
  let mut start_in = None;
  let mut filters = Vec::new();
  if let Some(opts) = options.0 {
    multiple = opts.get::<_, Option<bool>>("multiple")?.unwrap_or(false);
    start_in = start_in_opt(&ctx, API, &opts)?;
    filters = parse_types(&ctx, API, &opts)?;
  }
  Ok(begin(&ctx, Kind::OpenFiles, DialogRequest { kind: DialogKind::OpenFile { multiple }, filters, start_in }))
}

fn save_file_impl<'js>(
  ctx: Ctx<'js>,
  options: OptArg<Object<'js>>,
) -> rquickjs::Result<Promised<impl Future<Output = Settled>>> {
  const API: &str = "saveFile";
  let mut suggested_name = None;
  let mut start_in = None;
  let mut filters = Vec::new();
  if let Some(opts) = options.0 {
    suggested_name =
      no_nul(&ctx, API, "suggestedName", crate::plugins::marshal::string_opt(&ctx, &opts, "suggestedName", API)?)?;
    start_in = start_in_opt(&ctx, API, &opts)?;
    filters = parse_types(&ctx, API, &opts)?;
  }
  Ok(begin(&ctx, Kind::SaveFile, DialogRequest { kind: DialogKind::SaveFile { suggested_name }, filters, start_in }))
}

fn open_directory_impl<'js>(
  ctx: Ctx<'js>,
  options: OptArg<Object<'js>>,
) -> rquickjs::Result<Promised<impl Future<Output = Settled>>> {
  const API: &str = "openDirectory";
  let mut start_in = None;
  if let Some(opts) = options.0 {
    start_in = start_in_opt(&ctx, API, &opts)?;
  }
  Ok(begin(&ctx, Kind::OpenDirectory, DialogRequest { kind: DialogKind::OpenDirectory, filters: Vec::new(), start_in }))
}

/// Begin a request with the session and await its answer, holding the
/// engine meanwhile. The session shows the platform's dialog through the
/// command channel when it decides to.
fn begin<'js>(ctx: &Ctx<'js>, kind: Kind, request: DialogRequest) -> Promised<impl Future<Output = Settled>> {
  let state = state(ctx);
  let hold = crate::pending::PendingOps::of(ctx).in_flight("dialog");
  let (tx, rx) = oneshot::channel();
  let respond: DialogResponder = Box::new(move |answer| {
    tx.send(answer).ok();
  });
  let cmd_tx = state.cmd_tx.clone();
  state.dialogs.begin(request, respond, move |request, respond| {
    if let Err(SendError(AlloyCommand::ShowDialog { respond, .. })) =
      cmd_tx.send(AlloyCommand::ShowDialog { request, respond })
    {
      respond(Err(DialogError::Failed("dialog: platform loop is gone".to_string())));
    }
  });
  Promised(async move {
    let answer =
      rx.await.unwrap_or_else(|_| Err(DialogError::Failed("dialog: platform loop did not answer".to_string())));
    drop(hold);
    Settled { kind, answer }
  })
}

/// A settled call: its answer built for its kind (handles, a handle or
/// null), or a rejection named for why there are none (`InvalidStateError`
/// while another dialog waits, `NotSupportedError` on a platform without
/// one) or carrying the platform's failure.
struct Settled {
  kind: Kind,
  answer: DialogAnswer,
}

impl<'js> IntoJs<'js> for Settled {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let picked = match self.answer {
      Ok(picked) => picked,
      Err(error) => {
        let value = match &error {
          DialogError::Busy => named_error(ctx, "InvalidStateError", &format!("{}: {error}", self.kind.name()))?,
          DialogError::Unavailable => named_error(ctx, "NotSupportedError", &format!("{}: {error}", self.kind.name()))?,
          DialogError::Failed(message) => Exception::from_message(ctx.clone(), message)?.into_value(),
        };
        return Err(ctx.throw(value));
      }
    };
    match self.kind {
      Kind::OpenFiles => {
        let arr = Array::new(ctx.clone())?;
        for (i, p) in picked.into_iter().enumerate() {
          arr.set(i, crate::forge_plugins::file::build_file_named(ctx.clone(), p.location, Some(p.name))?)?;
        }
        Ok(arr.into_value())
      }
      Kind::SaveFile => match picked.into_iter().next() {
        Some(p) => {
          Ok(crate::forge_plugins::file::build_file_named(ctx.clone(), p.location, Some(p.name))?.into_value())
        }
        None => Ok(Value::new_null(ctx.clone())),
      },
      Kind::OpenDirectory => match picked.into_iter().next() {
        Some(p) => Ok(crate::forge_plugins::dir::build_dir_named(ctx.clone(), p.location, Some(p.name))?.into_value()),
        None => Ok(Value::new_null(ctx.clone())),
      },
    }
  }
}

/// An `Error` with a DOMException-style `name`, for a rejection code can
/// branch on (`e.name === "NotSupportedError"`).
fn named_error<'js>(ctx: &Ctx<'js>, name: &str, message: &str) -> rquickjs::Result<Value<'js>> {
  let error = Exception::from_message(ctx.clone(), message)?;
  error.set("name", name)?;
  Ok(error.into_value())
}
