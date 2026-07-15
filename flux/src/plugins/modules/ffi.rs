//! The `flux:ffi` module: a generic native shared-library host for flux.
//!
//! Marshalling only: decode JS declarations and arguments into the native
//! types of the engine-free `forge::ffi` core (dlopen + libffi), drive its
//! `FfiLibrary` methods, and encode results back to JS. Loading, calling,
//! callback trampolines, and raw memory access live in `forge::ffi`.
//!
//! This is the native counterpart of `flux:wasm` and follows the same shape:
//! declare what you need up front, every declared symbol must resolve, host
//! functions are JS functions the guest code calls back into during a call.
//! Unlike wasm there is NO sandbox: a library runs with full process rights,
//! declared signatures must match the real ABI, and callbacks may only fire
//! while a call into the library is on the stack (see forge::ffi).
//!
//! JS surface:
//! ```js
//! import { Library } from "flux:ffi";
//!
//! let lib = new Library(bytesOrPath, {       // Uint8Array | ArrayBuffer | path string
//!   cart_new:  { args: [] },                 // returns defaults to "void"
//!   cart_eval: { args: ["ptr", "i32"], returns: "i32" },
//! });
//! let { cart_new, cart_eval } = lib.symbols; // bound JS functions
//! let cb = lib.callback((a, b) => a + b, { args: ["i32", "i32"], returns: "i32" });
//! lib.readMemory(ptr, len);                  // Uint8Array
//! lib.writeMemory(ptr, bytes);               // void
//! ```
//!
//! Value marshalling: i32/f32/f64 <-> JS number; i64 and ptr <-> JS BigInt (a
//! number is also accepted where one is expected). A callback's JS return
//! value is coerced to its declared result type; "void" ignores it. An error
//! thrown by a callback cannot abort the native frame - the callback returns
//! zeroes, the call runs to completion, and the exception is rethrown after.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rquickjs::class::Trace;
use rquickjs::function::Rest;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{
  ArrayBuffer, Class, Ctx, Exception, Function, IntoJs, JsLifetime, Object, Persistent, TypedArray, Value,
};

use forge::ffi::{FfiLibrary, FfiSig, FfiType, FfiValue};

// Per-library callback functions live in context userdata, NOT in the
// `Library` class, for the same reason as flux:wasm's WasmHandlers: a
// `Persistent` held inside a class instance is released only by the class
// finalizer, which runs too late and trips QuickJS's shutdown assertion.
// Userdata is dropped with the context, before the runtime is freed.
#[derive(Clone, JsLifetime, Default)]
struct FfiHandlers(#[qjs(skip_trace)] Rc<RefCell<HandlerStore>>);

#[derive(Default)]
struct HandlerStore {
  next_id: u64,
  by_id: HashMap<u64, Vec<Persistent<Function<'static>>>>,
}

/// Get the per-context handler registry, creating it on first use.
fn handler_store(ctx: &Ctx<'_>) -> FfiHandlers {
  if let Some(existing) = ctx.userdata::<FfiHandlers>() {
    return existing.clone();
  }
  let store = FfiHandlers::default();
  ctx.store_userdata(store.clone()).expect("store ffi handler registry");
  store
}

/// A declared symbol's name (signatures live in the forge library, indexed in
/// declaration order).
struct SymbolName(String);

#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Library")]
pub struct Library {
  #[qjs(skip_trace)]
  inner: Rc<FfiLibrary>,
  // Only the id keying this library's callback functions in the userdata
  // registry - never the registry Rc itself (see FfiHandlers above).
  #[qjs(skip_trace)]
  id: u64,
  #[qjs(skip_trace)]
  names: Rc<Vec<SymbolName>>,
  // Declared result type of each minted callback, by callback index, for
  // coercing the JS return value in the dispatcher.
  #[qjs(skip_trace)]
  cb_results: Rc<RefCell<Vec<Option<FfiType>>>>,
}

#[rquickjs::methods]
impl Library {
  /// Load a shared library from bytes (`Uint8Array`/`ArrayBuffer`, e.g. a
  /// bundled binary import) or a filesystem path, resolving every declared
  /// symbol: `{ name: { args: ["i32", ...], returns?: "i32" } }`. A missing
  /// symbol or an invalid declaration throws. Loading runs the library's
  /// constructors - only load trusted code.
  #[qjs(constructor)]
  pub fn new<'js>(ctx: Ctx<'js>, source: Value<'js>, decls: Object<'js>) -> rquickjs::Result<Library> {
    let mut declared: Vec<(String, FfiSig)> = Vec::new();
    for prop in decls.props::<String, Object>() {
      let (name, decl) = prop?;
      let sig = parse_sig(&ctx, &decl)?;
      declared.push((name, sig));
    }
    let names = declared.iter().map(|(n, _)| SymbolName(n.clone())).collect();

    let lib = if let Some(path) = source.as_string() {
      FfiLibrary::open(&path.to_string()?, declared)
    } else {
      FfiLibrary::open_bytes(&value_to_bytes(&ctx, &source)?, declared)
    };
    let inner = lib.map_err(|m| Exception::throw_message(&ctx, &m))?;

    let store = handler_store(&ctx);
    let id = {
      let mut s = store.0.borrow_mut();
      let id = s.next_id;
      s.next_id += 1;
      s.by_id.insert(id, Vec::new());
      id
    };

    Ok(Library { inner: Rc::new(inner), id, names: Rc::new(names), cb_results: Rc::new(RefCell::new(Vec::new())) })
  }

  /// The declared symbols as bound JS functions, keyed by name. Destructure
  /// once and reuse: the object is rebuilt on each access.
  #[qjs(get)]
  pub fn symbols<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Object<'js>> {
    let obj = Object::new(ctx.clone())?;
    for (index, name) in self.names.iter().enumerate() {
      let inner = self.inner.clone();
      let id = self.id;
      let cb_results = self.cb_results.clone();
      let f = Function::new(
        ctx.clone(),
        value_fn(move |ctx: Ctx<'_>, args: Rest<Value<'_>>| call_symbol(ctx, &inner, id, &cb_results, index, args)),
      )?;
      obj.set(name.0.as_str(), f)?;
    }
    Ok(obj)
  }

  /// Mint a C function pointer (returned as a BigInt address) that invokes
  /// `func` when the library calls it during a `symbols.*` call. The pointer
  /// stays valid for the lifetime of the library.
  pub fn callback<'js>(&self, ctx: Ctx<'js>, func: Function<'js>, decl: Object<'js>) -> rquickjs::Result<Value<'js>> {
    let sig = parse_sig(&ctx, &decl)?;
    let store = handler_store(&ctx);
    // Push to both index spaces in lockstep: the forge callback index must
    // match the positions in the handler registry and cb_results.
    self.cb_results.borrow_mut().push(sig.result);
    store.0.borrow_mut().by_id.entry(self.id).or_default().push(Persistent::save(&ctx, func));
    let code = self.inner.register_callback(sig);
    Value::new_big_int(ctx, code as i64)
  }

  /// Copy `len` bytes out of process memory at `ptr`, as a `Uint8Array`.
  /// No bounds checking is possible: a bad pointer is undefined behavior.
  #[qjs(rename = "readMemory")]
  pub fn read_memory<'js>(&self, ctx: Ctx<'js>, ptr: Value<'js>, len: usize) -> rquickjs::Result<Value<'js>> {
    let ptr = value_to_ptr(&ctx, &ptr)?;
    let bytes = self.inner.memory_read(ptr, len).map_err(|m| Exception::throw_message(&ctx, &m))?;
    Ok(TypedArray::new(ctx.clone(), bytes)?.into_value())
  }

  /// Copy `bytes` (a `Uint8Array` or `ArrayBuffer`) into process memory at
  /// `ptr`. Same caveat as `readMemory`.
  #[qjs(rename = "writeMemory")]
  pub fn write_memory<'js>(&self, ctx: Ctx<'js>, ptr: Value<'js>, bytes: Value<'js>) -> rquickjs::Result<()> {
    let ptr = value_to_ptr(&ctx, &ptr)?;
    let bytes = value_to_bytes(&ctx, &bytes)?;
    self.inner.memory_write(ptr, &bytes).map_err(|m| Exception::throw_message(&ctx, &m))
  }
}

/// Call declared symbol `index`: coerce JS arguments to the declared types,
/// run the call with a dispatcher that routes callback invocations to their
/// JS functions, and encode the result. A JS throw from a callback is
/// captured and rethrown after the native call returns, preserving the
/// original exception (native code cannot be aborted mid-call).
fn call_symbol<'js>(
  ctx: Ctx<'js>,
  inner: &Rc<FfiLibrary>,
  id: u64,
  cb_results: &Rc<RefCell<Vec<Option<FfiType>>>>,
  index: usize,
  args: Rest<Value<'js>>,
) -> rquickjs::Result<Value<'js>> {
  let ffi_args = {
    let sig = inner.symbol_sig(index).map_err(|m| Exception::throw_message(&ctx, &m))?;
    if args.0.len() != sig.params.len() {
      return Err(Exception::throw_message(
        &ctx,
        &format!("expected {} argument(s), got {}", sig.params.len(), args.0.len()),
      ));
    }
    let mut out = Vec::with_capacity(args.0.len());
    for (value, ty) in args.0.iter().zip(sig.params.iter()) {
      out.push(js_to_ffi(value, *ty).map_err(|m| Exception::throw_message(&ctx, &m))?);
    }
    out
  };

  let handlers = handler_store(&ctx);
  let mut thrown: Option<rquickjs::Error> = None;
  let mut host = |cb_index: usize, host_args: Vec<FfiValue>| -> Result<Option<FfiValue>, String> {
    // Clone the Persistent out under a short borrow before calling into JS: a
    // re-entrant call borrows the same registry.
    let saved = handlers.0.borrow().by_id.get(&id).and_then(|v| v.get(cb_index)).cloned();
    let func = saved.ok_or_else(|| "ffi callback missing from registry".to_string())?;
    let func = func.restore(&ctx).map_err(|e| e.to_string())?;
    let js_args: Vec<Value> = host_args.into_iter().map(|v| ffi_to_js(&ctx, v)).collect::<Result<_, _>>()?;
    let ret: Value = match func.call((Rest(js_args),)) {
      Ok(v) => v,
      Err(e) => {
        // Stash the real JS exception; return a placeholder Err to record.
        thrown = Some(e);
        return Err("ffi callback threw".to_string());
      }
    };
    match cb_results.borrow().get(cb_index) {
      Some(Some(ty)) => Ok(Some(js_to_ffi(&ret, *ty)?)),
      Some(None) => Ok(None),
      None => Err("ffi callback result type missing".to_string()),
    }
  };

  let out = inner.call(index, ffi_args, &mut host);
  if let Some(e) = thrown {
    return Err(e);
  }
  match out.map_err(|m| Exception::throw_message(&ctx, &m))? {
    Some(v) => ffi_to_js(&ctx, v).map_err(|m| Exception::throw_message(&ctx, &m)),
    None => Ok(Value::new_undefined(ctx)),
  }
}

/// HRTB coercion helper: a capturing closure passed to `Function::new` does
/// not infer the `for<'js>` bound on its own (see flux/CLAUDE.md).
fn value_fn<F>(f: F) -> F
where
  F: for<'js> Fn(Ctx<'js>, Rest<Value<'js>>) -> rquickjs::Result<Value<'js>>,
{
  f
}

// ---- declaration parsing ----------------------------------------------------

/// Parse `{ args: ["i32", ...], returns?: "i32" | "void" }`.
fn parse_sig<'js>(ctx: &Ctx<'js>, decl: &Object<'js>) -> rquickjs::Result<FfiSig> {
  let args: rquickjs::Array =
    decl.get("args").map_err(|_| Exception::throw_message(ctx, "declaration needs an args array"))?;
  let mut params = Vec::with_capacity(args.len());
  for entry in args.iter::<String>() {
    let name = entry?;
    let ty = parse_type(&name).ok_or_else(|| Exception::throw_message(ctx, &format!("unknown ffi type {name}")))?;
    params.push(ty);
  }
  let result = match decl.get::<_, Option<String>>("returns")? {
    None => None,
    Some(name) if name == "void" => None,
    Some(name) => {
      Some(parse_type(&name).ok_or_else(|| Exception::throw_message(ctx, &format!("unknown ffi type {name}")))?)
    }
  };
  Ok(FfiSig { params, result })
}

fn parse_type(name: &str) -> Option<FfiType> {
  match name {
    "i32" => Some(FfiType::I32),
    "i64" => Some(FfiType::I64),
    "f32" => Some(FfiType::F32),
    "f64" => Some(FfiType::F64),
    "ptr" => Some(FfiType::Ptr),
    _ => None,
  }
}

// ---- value marshalling ------------------------------------------------------

/// Decode a JS `Uint8Array` or `ArrayBuffer` into owned bytes.
fn value_to_bytes(ctx: &Ctx<'_>, value: &Value<'_>) -> rquickjs::Result<Vec<u8>> {
  if let Ok(ta) = TypedArray::<u8>::from_value(value.clone()) {
    Ok(ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default())
  } else if let Some(ab) = ArrayBuffer::from_value(value.clone()) {
    Ok(ab.as_bytes().map(|b| b.to_vec()).unwrap_or_default())
  } else {
    Err(Exception::throw_message(ctx, "expected a Uint8Array or ArrayBuffer"))
  }
}

/// Decode a pointer argument: BigInt or number.
fn value_to_ptr<'js>(ctx: &Ctx<'js>, value: &Value<'js>) -> rquickjs::Result<u64> {
  match js_to_ffi(value, FfiType::Ptr) {
    Ok(FfiValue::Ptr(p)) => Ok(p),
    _ => Err(Exception::throw_message(ctx, "expected a pointer (BigInt or number)")),
  }
}

/// Coerce a JS value to a scalar ffi value of the declared type.
fn js_to_ffi(value: &Value<'_>, ty: FfiType) -> Result<FfiValue, String> {
  let big = |value: &Value<'_>| -> Result<i64, String> {
    if let Some(b) = value.as_big_int() {
      b.clone().to_i64().map_err(|_| "invalid BigInt argument".to_string())
    } else {
      number(value).map(|n| n as i64)
    }
  };
  match ty {
    FfiType::I32 => number(value).map(|n| FfiValue::I32(n as i32)),
    FfiType::F32 => number(value).map(|n| FfiValue::F32(n as f32)),
    FfiType::F64 => number(value).map(FfiValue::F64),
    FfiType::I64 => big(value).map(FfiValue::I64),
    FfiType::Ptr => big(value).map(|n| FfiValue::Ptr(n as u64)),
  }
}

fn number(value: &Value<'_>) -> Result<f64, String> {
  value.as_number().ok_or_else(|| "expected a number argument".to_string())
}

/// Encode a scalar ffi value as a JS value (i64/ptr -> BigInt, rest -> number).
fn ffi_to_js<'js>(ctx: &Ctx<'js>, v: FfiValue) -> Result<Value<'js>, String> {
  let r = match v {
    FfiValue::I32(x) => (x as f64).into_js(ctx),
    FfiValue::F32(x) => (x as f64).into_js(ctx),
    FfiValue::F64(x) => x.into_js(ctx),
    FfiValue::I64(x) => Value::new_big_int(ctx.clone(), x),
    FfiValue::Ptr(x) => Value::new_big_int(ctx.clone(), x as i64),
  };
  r.map_err(|e| e.to_string())
}

pub struct FfiModuleDef;

impl ModuleDef for FfiModuleDef {
  fn declare(decl: &Declarations<'_>) -> rquickjs::Result<()> {
    decl.declare("Library")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    let ctor = Class::<Library>::create_constructor(ctx)?.expect("Library class has a constructor");
    exports.export("Library", ctor)?;
    Ok(())
  }
}
