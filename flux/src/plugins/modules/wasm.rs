//! The `flux:wasm` module: a generic WebAssembly host for flux.
//!
//! Marshalling only: decode JS args into the native types of the engine-free
//! `forge::wasm` core (a wasmi interpreter), drive its `WasmModule` /
//! `WasmInstance` methods, and encode the results back to JS. All module
//! parsing, linking, execution, and the resumable-call host-import bridge live
//! in `forge::wasm`.
//!
//! This is a GENERIC wasm host, not tailored to any one module. It exposes the
//! primitives - parse a module, inspect its imports, instantiate it with host
//! functions, call exports, read/write its linear memory - and nothing about
//! any particular guest.
//!
//! Everything here is synchronous: wasmi is a pure interpreter with no I/O, and
//! it runs on the JS thread, so there is no `Promised`/async plumbing. A host
//! import calls straight back into the supplied JS function during the export
//! call; a throw from that function aborts the wasm call and propagates.
//!
//! JS surface:
//! ```js
//! import { Module } from "flux:wasm";
//!
//! let mod = new Module(bytes);          // Uint8Array | ArrayBuffer; wat text also accepted
//! mod.imports;                          // [{ module, name, params, results }]
//! let instance = mod.instantiate({      // host functions, keyed like the standard
//!   env: { mul: (a, b) => a * b },
//! });
//! instance.exports;                     // [{ name, kind, params?, results? }]
//! instance.call("run", 6);              // scalar / undefined / array by result count
//! instance.callIndirect(fp, 1, 2);      // call table[fp] via the exported function table
//! instance.memory;                      // ArrayBuffer over linear memory, or undefined
//! instance.readMemory(ptr, len);        // copy out as a fresh Uint8Array
//! instance.writeMemory(ptr, bytes);     // copy in
//! ```
//!
//! `instance.memory` aliases the guest's linear memory with the web's
//! `WebAssembly.Memory.buffer` contract: reads and writes are copy-free, the
//! buffer stays valid until the guest grows its memory, and growth detaches it
//! (read `memory` again for a fresh buffer). Growth can only happen while
//! guest code runs, so the plugin re-checks the storage location at every
//! point where JS regains control after guest execution (host-import dispatch
//! and call return) and detaches a stale buffer before any JS can read
//! through it.
//!
//! Value marshalling: i32/f32/f64 <-> JS number; i64 <-> JS BigInt (an i64 does
//! not fit a JS number without precision loss). A number is also accepted where
//! an i64 is expected.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rquickjs::class::Trace;
use rquickjs::function::Rest;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{
  qjs, ArrayBuffer, Class, Ctx, Exception, Function, IntoJs, JsLifetime, Object, Persistent, TypedArray, Value,
};

use forge::wasm::{ExportInfo, FuncSig, ImportInfo, WasmType, WasmValue};

// Per-instance JS state lives here, in context userdata, NOT in the
// `Instance` class. A `Persistent` held inside a class instance is a GC root
// released only by the class finalizer, which runs too late and trips QuickJS's
// shutdown assertion (`gc_obj_list` not empty). Userdata is dropped with the
// context, before the runtime is freed, so all Persistents release in order -
// the same pattern raf/events use. Entries linger until context teardown (or
// engine reload, which recreates the userdata); a GC'd instance leaves a small
// dead entry, released at teardown.
#[derive(Clone, JsLifetime, Default)]
struct WasmHandlers(#[qjs(skip_trace)] Rc<RefCell<HandlerStore>>);

#[derive(Default)]
struct HandlerStore {
  next_id: u64,
  by_id: HashMap<u64, InstanceEntry>,
}

/// One instance's JS-side state: the host functions backing its imports, and
/// the cached `instance.memory` buffer if one has been minted.
struct InstanceEntry {
  handlers: Vec<Persistent<Function<'static>>>,
  view: Option<MemoryView>,
}

/// A minted `instance.memory` ArrayBuffer plus the storage location it was
/// minted over, so guest growth (which moves the storage) can be detected and
/// the stale buffer detached. The instance `Rc` pins the wasmi store: QuickJS
/// holds no ownership of the buffer's bytes (see `array_buffer_over`), so the
/// registry keeps the memory alive for as long as the buffer can be reached -
/// even if the `Instance` object itself is collected first.
struct MemoryView {
  buffer: Persistent<ArrayBuffer<'static>>,
  ptr: usize,
  len: usize,
  _instance: Rc<forge::wasm::WasmInstance>,
}

/// Create an ArrayBuffer aliasing external bytes, with NO free callback:
/// QuickJS never frees or touches the bytes, on detach or at finalization.
///
/// Not `ArrayBuffer::from_source`, deliberately: its drop closure is unsound
/// against detach. `JS_DetachArrayBuffer` invokes the buffer's `free_func`
/// but does not clear it, so the finalizer invokes it AGAIN at teardown with
/// the same opaque pointer, and rquickjs's shim then double-drops its boxed
/// closure (double `Box::from_raw`) - a crash. With no callback registered,
/// both sites are no-ops and the bytes' lifetime is the caller's contract:
/// here, `MemoryView` pins the wasm instance in the registry.
fn array_buffer_over<'js>(ctx: &Ctx<'js>, ptr: *mut u8, len: usize) -> rquickjs::Result<ArrayBuffer<'js>> {
  let value = unsafe {
    let raw = qjs::JS_NewArrayBuffer(ctx.as_raw().as_ptr(), ptr, len as _, None, std::ptr::null_mut(), false);
    Value::from_raw(ctx.clone(), raw)
  };
  if value.is_exception() {
    return Err(rquickjs::Error::Exception);
  }
  ArrayBuffer::from_value(value).ok_or(rquickjs::Error::Unknown)
}

/// Detach the cached memory buffer if the instance's linear memory has moved
/// or resized, i.e. the guest ran `memory.grow`. Growth is only possible while
/// guest code executes, so calling this at every point where JS regains
/// control afterwards (host-import dispatch, call return, and the `memory`
/// getter itself) guarantees no JS ever reads through a stale pointer. This is
/// the web's `WebAssembly.Memory.buffer` contract: growth detaches the buffer.
fn detach_stale_view(ctx: &Ctx<'_>, store: &WasmHandlers, id: u64, inst: &forge::wasm::WasmInstance) {
  let stale = {
    let mut s = store.0.borrow_mut();
    let Some(entry) = s.by_id.get_mut(&id) else { return };
    let Some(view) = &entry.view else { return };
    let current = inst.memory_data_ptr().map(|(p, l)| (p as usize, l));
    if current == Some((view.ptr, view.len)) {
      return;
    }
    entry.view.take()
  };
  // Detaching frees the buffer's MemorySource (pure Rust, no registry
  // re-entry); the registry borrow is already released in case GC runs.
  if let Some(view) = stale {
    if let Ok(mut buffer) = view.buffer.restore(ctx) {
      buffer.detach();
    }
  }
}

/// Get the per-context handler registry, creating it on first use.
fn handler_store(ctx: &Ctx<'_>) -> WasmHandlers {
  if let Some(existing) = ctx.userdata::<WasmHandlers>() {
    return existing.clone();
  }
  let store = WasmHandlers::default();
  ctx.store_userdata(store.clone()).expect("store wasm handler registry");
  store
}

#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Module")]
pub struct Module {
  #[qjs(skip_trace)]
  inner: Rc<forge::wasm::WasmModule>,
}

#[rquickjs::methods]
impl Module {
  /// Parse and validate a wasm binary (or wat text). Throws on invalid input
  /// or on an unsupported import (non-function, or non-scalar signature).
  #[qjs(constructor)]
  pub fn new<'js>(ctx: Ctx<'js>, bytes: Value<'js>) -> rquickjs::Result<Module> {
    let bytes = value_to_bytes(&ctx, &bytes)?;
    let module = forge::wasm::WasmModule::parse(&bytes).map_err(|m| Exception::throw_message(&ctx, &m))?;
    Ok(Module { inner: Rc::new(module) })
  }

  /// The function imports this module requires, in the order the host functions
  /// are indexed: `[{ module, name, params, results }]`.
  #[qjs(get)]
  pub fn imports<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let arr = rquickjs::Array::new(ctx.clone())?;
    for (i, info) in self.inner.imports().iter().enumerate() {
      let obj = Object::new(ctx.clone())?;
      obj.set("module", info.module.clone())?;
      obj.set("name", info.name.clone())?;
      obj.set("params", type_names(&ctx, &info.sig.params)?)?;
      obj.set("results", type_names(&ctx, &info.sig.results)?)?;
      arr.set(i, obj)?;
    }
    Ok(arr.into_value())
  }

  /// Instantiate with host functions supplied as a nested import object
  /// (`{ env: { mul: fn } }`, matching the standard). Every listed import must
  /// resolve to a function; a missing or non-function entry throws.
  pub fn instantiate<'js>(&self, ctx: Ctx<'js>, imports: Object<'js>) -> rquickjs::Result<Class<'js, Instance>> {
    let infos = self.inner.imports().to_vec();
    let mut handlers: Vec<Persistent<Function<'static>>> = Vec::with_capacity(infos.len());
    for info in &infos {
      let module: Option<Object> = imports.get(info.module.as_str())?;
      let func: Option<Function> = match module {
        Some(m) => m.get(info.name.as_str())?,
        None => None,
      };
      let Some(func) = func else {
        return Err(Exception::throw_message(&ctx, &format!("missing import function {}.{}", info.module, info.name)));
      };
      handlers.push(Persistent::save(&ctx, func));
    }

    let inst = self.inner.instantiate().map_err(|m| Exception::throw_message(&ctx, &m))?;

    let store = handler_store(&ctx);
    let id = {
      let mut s = store.0.borrow_mut();
      let id = s.next_id;
      s.next_id += 1;
      s.by_id.insert(id, InstanceEntry { handlers, view: None });
      id
    };

    Class::instance(ctx, Instance { inner: Rc::new(inst), id, imports: Rc::new(infos) })
  }
}

#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Instance")]
pub struct Instance {
  #[qjs(skip_trace)]
  inner: Rc<forge::wasm::WasmInstance>,
  // Only the id keying this instance's host functions in the userdata registry.
  // The class must NOT hold the registry Rc: userdata is cleared before the
  // runtime is freed, so a clone kept here would pin the Persistents past that
  // point (until the class finalizer) and trip QuickJS's shutdown assertion.
  // `call` looks the registry up from `ctx` instead.
  #[qjs(skip_trace)]
  id: u64,
  #[qjs(skip_trace)]
  imports: Rc<Vec<ImportInfo>>,
}

#[rquickjs::methods]
impl Instance {
  /// The module's exports: `[{ name, kind, params?, results? }]`. `kind` is
  /// `"function"`, `"memory"`, or `"other"`; `params`/`results` are present
  /// only for functions with all-scalar signatures.
  #[qjs(get)]
  pub fn exports<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let arr = rquickjs::Array::new(ctx.clone())?;
    for (i, (name, info)) in self.inner.exports().iter().enumerate() {
      let obj = Object::new(ctx.clone())?;
      obj.set("name", name.clone())?;
      match info {
        ExportInfo::Func(sig) => {
          obj.set("kind", "function")?;
          obj.set("params", type_names(&ctx, &sig.params)?)?;
          obj.set("results", type_names(&ctx, &sig.results)?)?;
        }
        ExportInfo::Memory => obj.set("kind", "memory")?,
        ExportInfo::Other => obj.set("kind", "other")?,
      }
      arr.set(i, obj)?;
    }
    Ok(arr.into_value())
  }

  /// Call an exported function. Arguments are coerced to the export's declared
  /// parameter types. Returns `undefined` for no results, the single value for
  /// one, or an array for several. Host imports hit during the call dispatch to
  /// the functions passed to `instantiate`; a throw from one aborts the call.
  pub fn call<'js>(&self, ctx: Ctx<'js>, name: String, args: Rest<Value<'js>>) -> rquickjs::Result<Value<'js>> {
    let wasm_args = {
      let Some(sig) = self.inner.export_sig(&name) else {
        return Err(Exception::throw_message(&ctx, &format!("no exported function named {name}")));
      };
      coerce_args(&ctx, &name, sig, &args)?
    };
    self.invoke(ctx, Target::Export(&name), wasm_args)
  }

  /// Call a function by its index in the module's exported function table:
  /// `table[index](...args)`. This is how a host function invokes a guest
  /// function pointer it received as an integer (e.g. a C callback). Same
  /// coercion and host-import dispatch rules as `call`; safe to use from
  /// within a host function (re-entrant).
  #[qjs(rename = "callIndirect")]
  pub fn call_indirect<'js>(&self, ctx: Ctx<'js>, index: u32, args: Rest<Value<'js>>) -> rquickjs::Result<Value<'js>> {
    let sig = self.inner.table_func_sig(index).map_err(|m| Exception::throw_message(&ctx, &m))?;
    let wasm_args = coerce_args(&ctx, &format!("table[{index}]"), &sig, &args)?;
    self.invoke(ctx, Target::Table(index), wasm_args)
  }

  /// The exported linear memory as an `ArrayBuffer` aliasing the instance's
  /// live bytes, or `undefined` if the module exports no memory. Reads and
  /// writes go straight to guest memory - no copy; a `Uint8Array` over it is
  /// the zero-copy way to hand guest bytes to e.g. `uploadTexture`. Follows
  /// the web's `WebAssembly.Memory.buffer` contract: the buffer stays valid
  /// until the guest grows its memory, which detaches it; read `memory` again
  /// for a fresh buffer over the moved storage.
  #[qjs(get)]
  pub fn memory<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let Some((ptr, len)) = self.inner.memory_data_ptr() else {
      return Ok(Value::new_undefined(ctx));
    };
    let store = handler_store(&ctx);
    {
      let s = store.0.borrow();
      if let Some(view) = s.by_id.get(&self.id).and_then(|e| e.view.as_ref()) {
        if (view.ptr, view.len) == (ptr as usize, len) {
          let buffer = view.buffer.clone().restore(&ctx)?;
          return Ok(buffer.into_value());
        }
      }
    }
    detach_stale_view(&ctx, &store, self.id, &self.inner);
    let buffer = array_buffer_over(&ctx, ptr, len)?;
    let mut s = store.0.borrow_mut();
    let Some(entry) = s.by_id.get_mut(&self.id) else {
      // Unreachable: entries live until context teardown, and an Instance
      // cannot outlive its context. Fail closed rather than hand out a buffer
      // growth could never detach.
      return Err(Exception::throw_message(&ctx, "wasm instance state is gone"));
    };
    entry.view = Some(MemoryView {
      buffer: Persistent::save(&ctx, buffer.clone()),
      ptr: ptr as usize,
      len,
      _instance: self.inner.clone(),
    });
    Ok(buffer.into_value())
  }

  /// Copy `len` bytes out of the exported memory at `ptr`, as a fresh
  /// `Uint8Array`. One-shot convenience; for repeated or large reads use
  /// `memory` directly.
  #[qjs(rename = "readMemory")]
  pub fn read_memory<'js>(&self, ctx: Ctx<'js>, ptr: usize, len: usize) -> rquickjs::Result<Value<'js>> {
    let bytes = self.inner.memory_read(ptr, len).map_err(|m| Exception::throw_message(&ctx, &m))?;
    Ok(TypedArray::new(ctx.clone(), bytes)?.into_value())
  }

  /// Copy `bytes` (a `Uint8Array` or `ArrayBuffer`) into the exported memory
  /// at `ptr`. The source is read in place; a source that is itself a view
  /// over this instance's memory is staged through a copy first so the two
  /// ranges never alias.
  #[qjs(rename = "writeMemory")]
  pub fn write_memory<'js>(&self, ctx: Ctx<'js>, ptr: usize, bytes: Value<'js>) -> rquickjs::Result<()> {
    let (src, src_len) = value_raw_bytes(&ctx, &bytes)?;
    let overlaps = self.inner.memory_data_ptr().is_some_and(|(mem, mem_len)| {
      (src as usize) < mem as usize + mem_len && (mem as usize) < src as usize + src_len
    });
    let slice = unsafe { std::slice::from_raw_parts(src, src_len) };
    let result = if overlaps {
      let staged = slice.to_vec();
      self.inner.memory_write(ptr, &staged)
    } else {
      self.inner.memory_write(ptr, slice)
    };
    result.map_err(|m| Exception::throw_message(&ctx, &m))
  }
}

/// What `Instance::invoke` should run: a named export or a function-table slot.
enum Target<'a> {
  Export(&'a str),
  Table(u32),
}

impl Instance {
  /// Shared driver for `call`/`callIndirect`. The host handler dispatches a
  /// guest import call to its JS function. It runs with no borrow of the wasm
  /// store held (see forge::wasm), so a handler may freely re-enter this
  /// instance. A JS throw is captured and rethrown after the call unwinds,
  /// preserving the original exception.
  fn invoke<'js>(&self, ctx: Ctx<'js>, target: Target<'_>, wasm_args: Vec<WasmValue>) -> rquickjs::Result<Value<'js>> {
    let imports = self.imports.clone();
    let handlers = handler_store(&ctx);
    let id = self.id;
    let inner = self.inner.clone();
    let mut thrown: Option<rquickjs::Error> = None;
    let mut host = |index: usize, host_args: Vec<WasmValue>| -> Result<Vec<WasmValue>, String> {
      // The guest may have grown its memory since JS last ran; detach a stale
      // memory buffer before this handler can read through it.
      detach_stale_view(&ctx, &handlers, id, &inner);
      // Clone the Persistent out under a short borrow, then drop the borrow
      // before calling into JS: a re-entrant call/instantiate borrows the same
      // registry.
      let saved = handlers.0.borrow().by_id.get(&id).and_then(|e| e.handlers.get(index)).cloned();
      let func = saved.ok_or_else(|| "wasm host function missing from registry".to_string())?;
      let func = func.restore(&ctx).map_err(err_string)?;
      let js_args: Vec<Value> = host_args.into_iter().map(|v| wasm_to_js(&ctx, v)).collect::<Result<_, _>>()?;
      let ret: Value = match func.call((Rest(js_args),)) {
        Ok(v) => v,
        Err(e) => {
          // Stash the real JS exception; return a placeholder Err to unwind.
          thrown = Some(e);
          return Err("host function threw".to_string());
        }
      };
      results_from_js(&ctx, ret, &imports[index].sig.results)
    };

    let out = match target {
      Target::Export(name) => self.inner.call(name, wasm_args, &mut host),
      Target::Table(index) => self.inner.call_indirect(index, wasm_args, &mut host),
    };
    detach_stale_view(&ctx, &handlers, id, &inner);
    if let Some(e) = thrown {
      return Err(e);
    }
    let out = out.map_err(|m| Exception::throw_message(&ctx, &m))?;
    results_to_js(&ctx, out)
  }
}

// ---- value marshalling ------------------------------------------------------

/// Check arity and coerce JS arguments to a function's scalar parameter types.
/// `what` names the call target; errors carry it, the full signature, and the
/// offending argument's position.
fn coerce_args<'js>(
  ctx: &Ctx<'js>,
  what: &str,
  sig: &FuncSig,
  args: &Rest<Value<'js>>,
) -> rquickjs::Result<Vec<WasmValue>> {
  if args.0.len() != sig.params.len() {
    return Err(Exception::throw_message(
      ctx,
      &format!("{what} {sig} expects {} argument(s), got {}", sig.params.len(), args.0.len()),
    ));
  }
  let mut out = Vec::with_capacity(args.0.len());
  for (i, (value, ty)) in args.0.iter().zip(sig.params.iter()).enumerate() {
    out.push(
      js_to_wasm(ctx, value, *ty)
        .map_err(|m| Exception::throw_message(ctx, &format!("{what} {sig}: argument {i}: {m}")))?,
    );
  }
  Ok(out)
}

fn err_string<E: std::fmt::Display>(e: E) -> String {
  e.to_string()
}

fn type_names<'js>(ctx: &Ctx<'js>, types: &[WasmType]) -> rquickjs::Result<rquickjs::Array<'js>> {
  let arr = rquickjs::Array::new(ctx.clone())?;
  for (i, t) in types.iter().enumerate() {
    arr.set(i, t.name())?;
  }
  Ok(arr)
}

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

/// Borrow the bytes of a JS `Uint8Array` or `ArrayBuffer` in place. The
/// returned pointer is only valid while no JS runs.
fn value_raw_bytes(ctx: &Ctx<'_>, value: &Value<'_>) -> rquickjs::Result<(*const u8, usize)> {
  let raw = if let Ok(ta) = TypedArray::<u8>::from_value(value.clone()) {
    ta.as_raw()
  } else if let Some(ab) = ArrayBuffer::from_value(value.clone()) {
    ab.as_raw()
  } else {
    return Err(Exception::throw_message(ctx, "expected a Uint8Array or ArrayBuffer"));
  };
  let raw = raw.ok_or_else(|| Exception::throw_message(ctx, "detached buffer"))?;
  Ok((raw.ptr.as_ptr(), raw.len))
}

/// Coerce a JS value to a scalar wasm value of the declared type.
fn js_to_wasm(_ctx: &Ctx<'_>, value: &Value<'_>, ty: WasmType) -> Result<WasmValue, String> {
  match ty {
    WasmType::I32 => number(value, ty).map(|n| WasmValue::I32(n as i32)),
    WasmType::F32 => number(value, ty).map(|n| WasmValue::F32(n as f32)),
    WasmType::F64 => number(value, ty).map(WasmValue::F64),
    WasmType::I64 => {
      if let Some(b) = value.as_big_int() {
        b.clone().to_i64().map(WasmValue::I64).map_err(|_| "BigInt out of range for i64".to_string())
      } else {
        number(value, ty).map(|n| WasmValue::I64(n as i64))
      }
    }
  }
}

fn number(value: &Value<'_>, ty: WasmType) -> Result<f64, String> {
  value.as_number().ok_or_else(|| format!("expected a number ({})", ty.name()))
}

/// Encode a scalar wasm value as a JS value (i64 -> BigInt, rest -> number).
fn wasm_to_js<'js>(ctx: &Ctx<'js>, v: WasmValue) -> Result<Value<'js>, String> {
  let r = match v {
    WasmValue::I32(x) => (x as f64).into_js(ctx),
    WasmValue::F32(x) => (x as f64).into_js(ctx),
    WasmValue::F64(x) => x.into_js(ctx),
    WasmValue::I64(x) => Value::new_big_int(ctx.clone(), x),
  };
  r.map_err(err_string)
}

/// Encode an export's result list: `undefined` / scalar / array by arity.
fn results_to_js<'js>(ctx: &Ctx<'js>, mut out: Vec<WasmValue>) -> rquickjs::Result<Value<'js>> {
  match out.len() {
    0 => Ok(Value::new_undefined(ctx.clone())),
    1 => wasm_to_js(ctx, out.remove(0)).map_err(|m| Exception::throw_message(ctx, &m)),
    _ => {
      let arr = rquickjs::Array::new(ctx.clone())?;
      for (i, v) in out.into_iter().enumerate() {
        let jv = wasm_to_js(ctx, v).map_err(|m| Exception::throw_message(ctx, &m))?;
        arr.set(i, jv)?;
      }
      Ok(arr.into_value())
    }
  }
}

/// Decode a host function's return value into result values matching the
/// import's declared result types. `undefined`/no-return is allowed only for a
/// zero-result import; a single value covers one result; an array covers many.
fn results_from_js(ctx: &Ctx<'_>, ret: Value<'_>, results: &[WasmType]) -> Result<Vec<WasmValue>, String> {
  if results.is_empty() {
    return Ok(Vec::new());
  }
  if results.len() == 1 {
    return Ok(vec![js_to_wasm(ctx, &ret, results[0])?]);
  }
  let Some(arr) = ret.into_array() else {
    return Err(format!("host function must return an array of {} results", results.len()));
  };
  if arr.len() != results.len() {
    return Err(format!("host function returned {} results, expected {}", arr.len(), results.len()));
  }
  let mut out = Vec::with_capacity(results.len());
  for (i, ty) in results.iter().enumerate() {
    let v: Value = arr.get(i).map_err(err_string)?;
    out.push(js_to_wasm(ctx, &v, *ty)?);
  }
  Ok(out)
}

pub struct WasmModuleDef;

impl ModuleDef for WasmModuleDef {
  fn declare(decl: &Declarations<'_>) -> rquickjs::Result<()> {
    decl.declare("Module")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    let ctor = Class::<Module>::create_constructor(ctx)?.expect("Module class has a constructor");
    exports.export("Module", ctor)?;
    Ok(())
  }
}
