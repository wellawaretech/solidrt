//! Engine-free native shared-library host: the FFI counterpart of `forge::wasm`.
//!
//! Loads a dynamic library (dlopen), calls its exported functions through
//! signatures declared at load time, and mints C function pointers ("host
//! callbacks") that dispatch back into a host-supplied closure - the same
//! import/export shape as the wasm core, over native code instead of a guest
//! module. Calls are constructed at runtime with libffi (no JIT: the callback
//! trampolines are libffi's prebuilt static ones).
//!
//! This is a GENERIC native host, not tailored to any one library. Only scalar
//! signatures are supported: i32/i64/f32/f64/ptr. Pointers are plain addresses
//! (u64); `memory_read`/`memory_write` are the raw-process-memory analog of the
//! wasm core's linear-memory access, used for the same
//! alloc-buffer/write/call protocol.
//!
//! Trust and safety model: loading a native library executes arbitrary code
//! with full process rights - there is no sandbox, unlike wasm. Everything
//! here is `unsafe` in nature even where the API is not marked so; the caller
//! decides which libraries are trusted. Two hard contracts:
//!
//! - Declared signatures must match the library's real ABI (wrong ones are
//!   undefined behavior, exactly as in any FFI).
//! - Host callbacks may only be invoked while a `call` into the library is on
//!   the stack, on the same thread. The dispatcher is installed for the
//!   duration of each call (re-entrant calls nest); a callback fired outside
//!   one (e.g. from a native background thread) gets no dispatcher and returns
//!   zeroes, recording an error.
//!
//! Like `forge::wasm`, the host dispatcher is threaded through every call as
//! `&mut FnMut(callback_index, args) -> Result<Option<value>, String>`, so this
//! crate names no scripting-engine types. A dispatcher error does NOT unwind
//! the native frame (native code cannot be aborted mid-call the way a wasm
//! trap can): the callback returns zeroes, the library call runs to
//! completion, and `call` then reports the recorded error.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;

use libffi::low;
use libffi::middle::{Arg, Cif, Closure, CodePtr, Ret, Type};

/// Scalar value types callable through the FFI host. `Ptr` is an address,
/// carried as u64.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FfiType {
  I32,
  I64,
  F32,
  F64,
  Ptr,
}

/// A scalar argument or result value.
#[derive(Clone, Copy, Debug)]
pub enum FfiValue {
  I32(i32),
  I64(i64),
  F32(f32),
  F64(f64),
  Ptr(u64),
}

/// A function signature: parameter types and an optional (void) result.
#[derive(Clone, Debug)]
pub struct FfiSig {
  pub params: Vec<FfiType>,
  pub result: Option<FfiType>,
}

/// A resolved exported function.
pub struct SymbolInfo {
  pub name: String,
  pub sig: FfiSig,
}

/// The host dispatcher: receives (callback index, decoded arguments), returns
/// the callback's result value (None for void). See the module doc for the
/// liveness contract.
pub type HostFn<'a> = dyn FnMut(usize, Vec<FfiValue>) -> Result<Option<FfiValue>, String> + 'a;

fn ffi_type(t: FfiType) -> Type {
  match t {
    FfiType::I32 => Type::i32(),
    FfiType::I64 => Type::i64(),
    FfiType::F32 => Type::f32(),
    FfiType::F64 => Type::f64(),
    FfiType::Ptr => Type::pointer(),
  }
}

fn make_cif(sig: &FfiSig) -> Cif {
  let params: Vec<Type> = sig.params.iter().map(|t| ffi_type(*t)).collect();
  let result = match sig.result {
    Some(t) => ffi_type(t),
    None => Type::void(),
  };
  Cif::new(params, result)
}

/// One argument/result slot: 8 bytes that libffi reads or writes as the
/// declared scalar type. Reading a narrower variant than was written relies on
/// little-endian layout (all supported targets are LE).
#[repr(C)]
#[derive(Clone, Copy)]
union Slot {
  i32_: i32,
  i64_: i64,
  f32_: f32,
  f64_: f64,
  ptr: *mut c_void,
}

const ZERO_SLOT: Slot = Slot { i64_: 0 };

fn slot_from(value: FfiValue) -> Slot {
  match value {
    FfiValue::I32(x) => Slot { i32_: x },
    FfiValue::I64(x) => Slot { i64_: x },
    FfiValue::F32(x) => Slot { f32_: x },
    FfiValue::F64(x) => Slot { f64_: x },
    FfiValue::Ptr(x) => Slot { ptr: x as *mut c_void },
  }
}

fn slot_read(slot: &Slot, t: FfiType) -> FfiValue {
  unsafe {
    match t {
      FfiType::I32 => FfiValue::I32(slot.i32_),
      FfiType::I64 => FfiValue::I64(slot.i64_),
      FfiType::F32 => FfiValue::F32(slot.f32_),
      FfiType::F64 => FfiValue::F64(slot.f64_),
      FfiType::Ptr => FfiValue::Ptr(slot.ptr as u64),
    }
  }
}

/// The per-library dispatcher slot: a raw pointer to the `HostFn` of whichever
/// `call` is currently on the stack. Only ever touched from the calling
/// thread; `DispatchGuard` saves/restores it so re-entrant calls nest.
type DispatchCell = Cell<Option<*mut HostFn<'static>>>;

struct DispatchGuard<'a> {
  cell: &'a DispatchCell,
  prev: Option<*mut HostFn<'static>>,
}

impl<'a> DispatchGuard<'a> {
  fn install(cell: &'a DispatchCell, host: &mut HostFn<'_>) -> Self {
    // Erase the borrow lifetime: the pointer lives in the cell only until this
    // guard drops, which is within the borrow of `host`.
    let raw = unsafe { std::mem::transmute::<*mut HostFn<'_>, *mut HostFn<'static>>(host as *mut HostFn<'_>) };
    let prev = cell.replace(Some(raw));
    DispatchGuard { cell, prev }
  }
}

impl Drop for DispatchGuard<'_> {
  fn drop(&mut self) {
    self.cell.set(self.prev);
  }
}

/// State shared between a library and its callback trampolines.
struct Shared {
  dispatch: DispatchCell,
  // First dispatcher/panic error recorded during the current call; reported
  // by `call` after the native frame returns.
  pending_error: RefCell<Option<String>>,
}

impl Shared {
  fn record(&self, msg: String) {
    let mut pending = self.pending_error.borrow_mut();
    if pending.is_none() {
      *pending = Some(msg);
    }
  }
}

/// Userdata for one minted callback: what the libffi trampoline needs to
/// decode arguments and reach the current dispatcher.
struct CallbackData {
  index: usize,
  sig: FfiSig,
  shared: *const Shared,
}

struct Symbol {
  info: SymbolInfo,
  cif: Cif,
  code: CodePtr,
}

struct CallbackEntry {
  // Declared before `_data` so the trampoline can never observe a dropped
  // CallbackData (fields drop in declaration order).
  _closure: Closure<'static>,
  _data: Box<CallbackData>,
}

pub struct FfiLibrary {
  // Boxed so `shared` has a stable address for the raw pointer in each
  // CallbackData; dropped last (declaration order) so trampolines minted from
  // this library never outlive it.
  symbols: Vec<Symbol>,
  callbacks: RefCell<Vec<CallbackEntry>>,
  shared: Box<Shared>,
  // Keeps the mapping alive; symbols/callbacks point into it.
  _lib: libloading::Library,
  // A library loaded from bytes lands in a temp file that is deleted on drop.
  temp_path: Option<std::path::PathBuf>,
}

impl Drop for FfiLibrary {
  fn drop(&mut self) {
    if let Some(path) = &self.temp_path {
      let _ = std::fs::remove_file(path);
    }
  }
}

impl FfiLibrary {
  /// Load a shared library from a path and resolve every declared symbol.
  /// Loading runs the library's constructors: only open trusted code.
  pub fn open(path: &str, decls: Vec<(String, FfiSig)>) -> Result<FfiLibrary, String> {
    let lib = unsafe { libloading::Library::new(path) }.map_err(|e| format!("dlopen {path}: {e}"))?;
    Self::resolve(lib, decls, None)
  }

  /// Load a shared library from bytes (written to a temp file first, so it
  /// can ride inside an app bundle the way a wasm module does).
  pub fn open_bytes(bytes: &[u8], decls: Vec<(String, FfiSig)>) -> Result<FfiLibrary, String> {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let path = std::env::temp_dir().join(format!("forge-ffi-{}-{}.so", std::process::id(), nanos));
    std::fs::write(&path, bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
    let lib = unsafe { libloading::Library::new(&path) };
    match lib {
      Ok(lib) => Self::resolve(lib, decls, Some(path)),
      Err(e) => {
        let _ = std::fs::remove_file(&path);
        Err(format!("dlopen library bytes: {e}"))
      }
    }
  }

  fn resolve(
    lib: libloading::Library,
    decls: Vec<(String, FfiSig)>,
    temp_path: Option<std::path::PathBuf>,
  ) -> Result<FfiLibrary, String> {
    let mut symbols = Vec::with_capacity(decls.len());
    for (name, sig) in decls {
      let addr = unsafe {
        lib
          .get::<unsafe extern "C" fn()>(name.as_bytes())
          .map(|s| *s as *mut c_void)
          .map_err(|e| format!("missing symbol {name}: {e}"))?
      };
      let cif = make_cif(&sig);
      symbols.push(Symbol { info: SymbolInfo { name, sig }, cif, code: CodePtr(addr) });
    }
    Ok(FfiLibrary {
      symbols,
      callbacks: RefCell::new(Vec::new()),
      shared: Box::new(Shared { dispatch: Cell::new(None), pending_error: RefCell::new(None) }),
      _lib: lib,
      temp_path,
    })
  }

  /// The declared symbols, in declaration order (the index `call` takes).
  pub fn symbols(&self) -> Vec<&SymbolInfo> {
    self.symbols.iter().map(|s| &s.info).collect()
  }

  /// The signature of symbol `index`.
  pub fn symbol_sig(&self, index: usize) -> Result<&FfiSig, String> {
    self.symbols.get(index).map(|s| &s.info.sig).ok_or_else(|| format!("no symbol at index {index}"))
  }

  /// Call declared symbol `index`. Arguments must match the declared parameter
  /// types exactly (the marshalling layer coerces). Host callbacks minted from
  /// this library dispatch to `host` while the call is on the stack; a
  /// dispatcher error is reported after the native frame returns (see the
  /// module doc).
  pub fn call(&self, index: usize, args: Vec<FfiValue>, host: &mut HostFn<'_>) -> Result<Option<FfiValue>, String> {
    let symbol = self.symbols.get(index).ok_or_else(|| format!("no symbol at index {index}"))?;
    let sig = &symbol.info.sig;
    if args.len() != sig.params.len() {
      return Err(format!("{} expects {} argument(s), got {}", symbol.info.name, sig.params.len(), args.len()));
    }
    let mut slots = Vec::with_capacity(args.len());
    for (value, ty) in args.iter().zip(sig.params.iter()) {
      if !value_matches(value, *ty) {
        return Err(format!("{}: argument type mismatch, expected {ty:?}", symbol.info.name));
      }
      slots.push(slot_from(*value));
    }
    let ffi_args: Vec<Arg> = slots.iter().map(Arg::new).collect();

    let guard = DispatchGuard::install(&self.shared.dispatch, host);
    let mut ret = ZERO_SLOT;
    unsafe {
      match sig.result {
        Some(_) => symbol.cif.call_return_into(symbol.code, &ffi_args, Ret::new(&mut ret)),
        None => symbol.cif.call_return_into(symbol.code, &ffi_args, Ret::void()),
      }
    }
    drop(guard);

    if let Some(msg) = self.shared.pending_error.borrow_mut().take() {
      return Err(msg);
    }
    Ok(sig.result.map(|t| slot_read(&ret, t)))
  }

  /// Mint a C function pointer with the given signature. When native code
  /// invokes it (during a `call`), the current host dispatcher runs with this
  /// callback's index. Returns the function pointer as an address; it stays
  /// valid for the lifetime of the library.
  pub fn register_callback(&self, sig: FfiSig) -> u64 {
    let mut callbacks = self.callbacks.borrow_mut();
    let data = Box::new(CallbackData { index: callbacks.len(), sig: sig.clone(), shared: &*self.shared });
    // The Box gives `data` a stable address; the entry keeps it alive as long
    // as the closure, and the library outlives both.
    let data_ref: &'static CallbackData = unsafe { &*(data.as_ref() as *const CallbackData) };
    let closure = Closure::new(make_cif(&sig), trampoline, data_ref);
    let code = (*closure.code_ptr()) as usize as u64;
    callbacks.push(CallbackEntry { _closure: closure, _data: data });
    code
  }

  /// The number of callbacks minted so far (the next callback's index).
  pub fn callback_count(&self) -> usize {
    self.callbacks.borrow().len()
  }

  /// Copy `len` bytes out of process memory at `ptr`. No bounds checking is
  /// possible: a bad pointer is undefined behavior, exactly as in the native
  /// code itself.
  pub fn memory_read(&self, ptr: u64, len: usize) -> Result<Vec<u8>, String> {
    if ptr == 0 {
      return Err("null pointer read".to_string());
    }
    let mut out = vec![0u8; len];
    unsafe { std::ptr::copy_nonoverlapping(ptr as *const u8, out.as_mut_ptr(), len) };
    Ok(out)
  }

  /// Copy `bytes` into process memory at `ptr`. Same caveat as `memory_read`.
  pub fn memory_write(&self, ptr: u64, bytes: &[u8]) -> Result<(), String> {
    if ptr == 0 {
      return Err("null pointer write".to_string());
    }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len()) };
    Ok(())
  }
}

fn value_matches(value: &FfiValue, ty: FfiType) -> bool {
  matches!(
    (value, ty),
    (FfiValue::I32(_), FfiType::I32)
      | (FfiValue::I64(_), FfiType::I64)
      | (FfiValue::F32(_), FfiType::F32)
      | (FfiValue::F64(_), FfiType::F64)
      | (FfiValue::Ptr(_), FfiType::Ptr)
  )
}

/// The libffi trampoline every minted callback lands in: decode the raw
/// argument slots per the callback's declared types, run the currently
/// installed dispatcher, write the result slot. Errors (no dispatcher, a
/// dispatcher Err, a panic) are recorded on the library and zeroes returned -
/// the native caller keeps running until its own frame returns.
unsafe extern "C" fn trampoline(
  _cif: &low::ffi_cif,
  result: &mut Slot,
  args: *const *const c_void,
  data: &CallbackData,
) {
  let shared = &*data.shared;
  let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    let mut values = Vec::with_capacity(data.sig.params.len());
    for (i, ty) in data.sig.params.iter().enumerate() {
      let slot = *args.add(i) as *const Slot;
      values.push(slot_read(&*slot, *ty));
    }
    match shared.dispatch.get() {
      Some(host) => (*host)(data.index, values),
      None => Err("callback invoked with no library call on the stack".to_string()),
    }
  }));
  let value = match out {
    Ok(Ok(v)) => v,
    Ok(Err(msg)) => {
      shared.record(msg);
      None
    }
    Err(_) => {
      shared.record("host callback panicked".to_string());
      None
    }
  };
  if let Some(ty) = data.sig.result {
    *result = match value {
      Some(v) if value_matches(&v, ty) => slot_from(v),
      _ => ZERO_SLOT,
    };
  }
}
