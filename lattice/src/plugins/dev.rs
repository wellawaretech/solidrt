use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use flux::rquickjs::function::MutFn;
use flux::rquickjs::module::{Declarations, Exports, ModuleDef};
use flux::rquickjs::{Array, Ctx, Exception, Function, JsLifetime, Null, Object, Persistent, Value};

// The `srt:dev` module: the dev-server control surface (connect / discover /
// stop) used by the default app's connection UI. The actual command plumbing
// lives in the go layer (connection.rs); here we only marshal it to JS.
//
// The module is registered unconditionally so the default app's static import
// resolves in every build, but the underlying control is installed only in go
// builds via `install`. When absent (record / non-go runtimes), `available` is
// false and the functions are no-ops, which is what the default app guards on.

// The dev control installed as context userdata. Holds engine-agnostic closures
// so this module never references the go-only command channel directly.
#[derive(Clone, JsLifetime)]
pub struct DevControl(#[qjs(skip_trace)] Rc<DevControlInner>);

pub struct DevControlInner {
  pub connect: Box<dyn Fn(String)>,
  pub discover: Box<dyn Fn()>,
  pub stop: Box<dyn Fn()>,
  pub can_discover: bool,
  pub recents: Vec<String>,
  // Dev-server address delivered at launch (srt client --android); the default
  // app auto-connects to it. None when launched without one.
  pub launch_address: Option<String>,
}

// Constructed and installed by go::control only (see the module doc above);
// other builds run the module with no userdata.
#[cfg_attr(not(feature = "go"), allow(dead_code))]
impl DevControl {
  pub fn new(inner: DevControlInner) -> Self {
    Self(Rc::new(inner))
  }
}

// Installs the dev control as userdata. Call from a go engine plugin before the
// default app imports `srt:dev`.
#[cfg_attr(not(feature = "go"), allow(dead_code))]
pub fn install(ctx: &Ctx<'_>, control: DevControl) {
  ctx.store_userdata(control).expect("store dev control");
}

/// Debug commands the app registered via `registerDebug`, listed and called
/// through the dev server (the list_debug / call_debug MCP tools). Context
/// userdata, so a hot reload's fresh engine starts empty and module init
/// re-registers. Registration works in every build; only go builds ever call
/// anything, so elsewhere the registry is a write nothing reads.
#[derive(Clone, Default, JsLifetime)]
pub struct DebugRegistry(#[qjs(skip_trace)] Rc<RefCell<HashMap<String, Persistent<Function<'static>>>>>);

// The readers live in go::connection; see the struct doc - elsewhere the
// registry is a write nothing reads.
#[cfg_attr(not(feature = "go"), allow(dead_code))]
impl DebugRegistry {
  /// Registered command names, sorted.
  pub fn names(&self) -> Vec<String> {
    let mut names: Vec<String> = self.0.borrow().keys().cloned().collect();
    names.sort();
    names
  }

  /// The command's function, if registered.
  pub fn get(&self, name: &str) -> Option<Persistent<Function<'static>>> {
    self.0.borrow().get(name).cloned()
  }
}

/// Where the app says it is: the string it last reported through
/// `reportLocation` (a router does it on every navigation; an app routing by
/// hand may too). The runtime never interprets it. Owned by the engine loop
/// and shared, not context userdata that dies with an engine: the dev
/// connection answers `GET /__control__/link` (the get_location MCP tool)
/// from it without a JS-thread round trip, and the loop reads it after an
/// engine ends to hand it to a reload of the same app as its launch link, so
/// a rebuild comes back to the screen it left. Installed into every engine's
/// context, cleared for each new one. Reporting works in every build; only
/// go builds read it.
#[derive(Clone, Default, JsLifetime)]
pub struct LocationSlot(#[qjs(skip_trace)] Arc<Mutex<Option<String>>>);

impl LocationSlot {
  /// The last reported location, if any.
  pub fn get(&self) -> Option<String> {
    self.0.lock().expect("location slot lock poisoned").clone()
  }

  pub fn set(&self, location: Option<String>) {
    *self.0.lock().expect("location slot lock poisoned") = location;
  }
}

/// `reportLocation(string | null)`: a later call replaces the value, null
/// withdraws it (a router unmounting).
fn report_location_impl<'js>(ctx: Ctx<'js>, location: Value<'js>) -> flux::rquickjs::Result<()> {
  let slot = ctx.userdata::<LocationSlot>().expect("location slot installed").clone();
  if location.is_null() || location.is_undefined() {
    slot.set(None);
    return Ok(());
  }
  let Some(text) = location.as_string() else {
    return Err(Exception::throw_type(&ctx, "reportLocation(location): expected a string or null"));
  };
  slot.set(Some(text.to_string()?));
  Ok(())
}

/// `registerDebug(name, fn)`: duplicate names replace. Fetches the registry
/// from userdata itself so the export needs no captured state.
fn register_debug_impl<'js>(ctx: Ctx<'js>, name: String, func: Function<'js>) -> flux::rquickjs::Result<()> {
  // A command's return value is JSON-encoded for the caller, so an async
  // function's Promise would encode as `{}` with nothing said: refuse it
  // here, where the throw names the registration.
  let is_async = func
    .as_object()
    .and_then(|f| f.get::<_, Object>("constructor").ok())
    .and_then(|c| c.get::<_, String>("name").ok())
    .is_some_and(|n| n == "AsyncFunction");
  if is_async {
    return Err(Exception::throw_type(
      &ctx,
      &format!("registerDebug(\"{name}\"): an async command is not supported (its promise would encode as {{}}); compute the result synchronously"),
    ));
  }
  let registry = ctx.userdata::<DebugRegistry>().expect("debug registry installed").clone();
  let persistent = Persistent::save(&ctx, func);
  registry.0.borrow_mut().insert(name, persistent);
  Ok(())
}

pub struct SrtDevModule;

impl ModuleDef for SrtDevModule {
  fn declare<'js>(decl: &Declarations<'js>) -> flux::rquickjs::Result<()> {
    decl.declare("available")?;
    decl.declare("connect")?;
    decl.declare("discover")?;
    decl.declare("stop")?;
    decl.declare("canDiscover")?;
    decl.declare("recents")?;
    decl.declare("launchAddress")?;
    decl.declare("registerDebug")?;
    decl.declare("reportLocation")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> flux::rquickjs::Result<()> {
    // The debug registry exists in every build (unlike the go-only control),
    // installed on first import of this module.
    if ctx.userdata::<DebugRegistry>().is_none() {
      ctx.store_userdata(DebugRegistry::default()).expect("store debug registry");
    }
    exports.export("registerDebug", Function::new(ctx.clone(), register_debug_impl)?)?;
    // The location slot is the engine loop's, installed at engine build.
    exports.export("reportLocation", Function::new(ctx.clone(), report_location_impl)?)?;

    match ctx.userdata::<DevControl>() {
      Some(control) => {
        let connect = control.clone();
        let discover = control.clone();
        let stop = control.clone();

        exports.export("available", true)?;
        exports
          .export("connect", Function::new(ctx.clone(), MutFn::from(move |addr: String| (connect.0.connect)(addr)))?)?;
        exports.export("discover", Function::new(ctx.clone(), MutFn::from(move || (discover.0.discover)()))?)?;
        exports.export("stop", Function::new(ctx.clone(), MutFn::from(move || (stop.0.stop)()))?)?;
        exports.export("canDiscover", control.0.can_discover)?;

        let recents = Array::new(ctx.clone())?;
        for (i, addr) in control.0.recents.iter().enumerate() {
          recents.set(i, addr.clone())?;
        }
        exports.export("recents", recents)?;
        match &control.0.launch_address {
          Some(addr) => exports.export("launchAddress", addr.clone())?,
          None => exports.export("launchAddress", Null)?,
        };
      }
      None => {
        exports.export("available", false)?;
        exports.export("connect", Function::new(ctx.clone(), MutFn::from(|_: String| {}))?)?;
        exports.export("discover", Function::new(ctx.clone(), MutFn::from(|| {}))?)?;
        exports.export("stop", Function::new(ctx.clone(), MutFn::from(|| {}))?)?;
        exports.export("canDiscover", false)?;
        exports.export("recents", Array::new(ctx.clone())?)?;
        exports.export("launchAddress", Null)?;
      }
    }
    Ok(())
  }
}
