//! Engine-free WebAssembly hosting core: one API, one engine per build.
//!
//! Hosts precompiled `.wasm` modules (or text-format sources): parse and
//! validate a module, resolve its function imports to host-provided handlers,
//! instantiate it, call its exports (by name, or by index through its exported
//! function table), and access its exported linear memory.
//!
//! Two backends implement the same types, and a build carries exactly one
//! (okf/design/wasm.md, section 5):
//! - `native` (cargo feature `wasm-native`): wasmtime with Cranelift, compiling
//!   the module to machine code for the host. The lane wherever a backend
//!   exists and runtime codegen is allowed (desktop, 64-bit Android).
//! - `interp` (the feature off): wasmi, a register-based interpreter. The lane
//!   for targets that cannot run generated code (iOS, 32-bit ARM Android), and
//!   how a desktop runs that lane for testing: build with the feature off.
//! Both are held to one proposal set (wasmi's, SIMD included), so a module that
//! validates on one lane validates on every lane; the lanes differ in speed.
//!
//! A `WasmEngine` is cheap to clone and meant to be made once per process by
//! the embedder and handed to whoever parses modules. The native lane
//! compiles, which takes real time on a device, so it keeps a disk cache of
//! compiled artifacts (`parse_cached`, `precompile`, `prune_cache`) when the
//! embedder hands it a cache dir: content-addressed by the module's sha256
//! under a directory per engine version, fail-soft (an unusable cache means
//! compiling in memory), with the artifacts of modules an app no longer holds
//! pruned by the embedder's retention. No cache dir, no disk.
//!
//! Host imports are bridged so that a handler runs with NO engine borrow held
//! and may re-enter the instance (call another export, read or write memory)
//! freely; how each backend achieves that is its own business (wasmi suspends
//! the frame with a resumable call, wasmtime hands out the active caller).
//!
//! Scope (deliberate, current):
//! - Function imports only; a module that imports memories, tables, or
//!   globals is rejected at parse.
//! - Scalar values only (i32/i64/f32/f64) in bridged signatures; v128 and
//!   reference types are rejected where they would cross the host boundary.
//! - A start function must not call a host import (instantiation runs it
//!   non-resumably); none of the common toolchains emit one that does.
//! - The wasm exception-handling proposal is not supported (wasmi does not
//!   implement it); such modules fail validation on both lanes.

use std::collections::HashSet;
use std::fmt;
use std::path::Path;

#[cfg(feature = "wasm-native")]
mod native;
#[cfg(feature = "wasm-native")]
use native as backend;
#[cfg(not(feature = "wasm-native"))]
mod interp;
#[cfg(not(feature = "wasm-native"))]
use interp as backend;

/// A scalar wasm value crossing the host boundary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WasmValue {
  I32(i32),
  I64(i64),
  F32(f32),
  F64(f64),
}

/// A scalar wasm value type in a bridged function signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmType {
  I32,
  I64,
  F32,
  F64,
}

impl WasmType {
  /// The type's canonical wasm name ("i32", "i64", "f32", "f64").
  pub fn name(self) -> &'static str {
    match self {
      WasmType::I32 => "i32",
      WasmType::I64 => "i64",
      WasmType::F32 => "f32",
      WasmType::F64 => "f64",
    }
  }
}

/// A bridged function signature: scalar parameter and result types.
#[derive(Debug, Clone)]
pub struct FuncSig {
  pub params: Vec<WasmType>,
  pub results: Vec<WasmType>,
}

/// Renders as `(i32, i32) -> i32`; the arrow is omitted for no results and the
/// result list parenthesized when there are several.
impl fmt::Display for FuncSig {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let join = |types: &[WasmType]| types.iter().map(|t| t.name()).collect::<Vec<_>>().join(", ");
    write!(f, "({})", join(&self.params))?;
    match self.results.len() {
      0 => Ok(()),
      1 => write!(f, " -> {}", self.results[0].name()),
      _ => write!(f, " -> ({})", join(&self.results)),
    }
  }
}

/// One function import a module requires from the host. Its position in
/// `WasmModule::imports` is the handler index passed to the host handler on
/// every call of this import.
#[derive(Debug, Clone)]
pub struct ImportInfo {
  pub module: String,
  pub name: String,
  pub sig: FuncSig,
}

/// What an export is, as far as the host boundary cares.
#[derive(Debug, Clone)]
pub enum ExportInfo {
  /// A callable function with an all-scalar signature.
  Func(FuncSig),
  /// A linear memory (the first one becomes the instance's memory).
  Memory,
  /// Anything not bridged (globals, tables, functions with non-scalar types).
  /// The first exported table still backs `call_indirect`.
  Other,
}

/// Handler for host calls out of wasm: receives the import's index (position
/// in `WasmModule::imports`) and its arguments, returns the result values
/// (matching the import's declared result types) or an error message that
/// aborts the wasm call.
pub type HostHandler<'a> = &'a mut dyn FnMut(usize, Vec<WasmValue>) -> Result<Vec<WasmValue>, String>;

/// Which engine this build runs modules on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
  /// wasmi: the module is interpreted.
  Interpreter,
  /// wasmtime with Cranelift: the module is compiled to machine code.
  Native,
}

/// The engine modules are parsed and run with. Cheap to clone (a shared
/// handle); make one per process and hand it around.
#[derive(Clone)]
pub struct WasmEngine {
  inner: backend::Engine,
}

impl WasmEngine {
  /// This build's engine. Fails only if the native engine cannot be set up
  /// on this host, which an embedder reports rather than papers over.
  pub fn new() -> Result<WasmEngine, String> {
    Ok(WasmEngine { inner: backend::Engine::new()? })
  }

  /// Which lane this build is: a build fact, the same for every engine.
  pub fn lane(&self) -> Lane {
    backend::LANE
  }

  /// Parse and validate a wasm binary (or wat text). Rejects modules with
  /// non-function imports or imports with non-scalar signatures. Compiles in
  /// memory on the native lane; see `parse_cached` for the cache.
  pub fn parse(&self, bytes: &[u8]) -> Result<WasmModule, String> {
    self.parse_cached(bytes, None)
  }

  /// `parse`, with the compiled form cached under `<cache_dir>/wasm/` on the
  /// native lane: a hit loads the artifact instead of compiling, a miss
  /// compiles and stores one. The interpreter has no artifact worth keeping
  /// and ignores the dir. Fail-soft: an unreadable or unwritable cache is
  /// logged and the module is compiled in memory; a module that does not
  /// compile is an error either way.
  pub fn parse_cached(&self, bytes: &[u8], cache_dir: Option<&Path>) -> Result<WasmModule, String> {
    Ok(WasmModule { inner: self.inner.parse(bytes, cache_dir)? })
  }

  /// Fill the cache slot for `bytes` without keeping the module: the
  /// install-time pre-warm, so the first `parse_cached` of the same bytes is
  /// a load. A no-op on the interpreter lane.
  pub fn precompile(&self, bytes: &[u8], cache_dir: &Path) -> Result<(), String> {
    self.inner.precompile(bytes, cache_dir)
  }

  /// Drop every cached artifact under `<cache_dir>/wasm/` whose module hash
  /// (lowercase sha256 hex) is not in `keep`, and every artifact of another
  /// engine version. The embedder decides what is held; this applies it.
  pub fn prune_cache(&self, cache_dir: &Path, keep: &HashSet<String>) {
    self.inner.prune_cache(cache_dir, keep)
  }
}

/// A parsed, validated module plus its resolved import list. Instantiate it
/// once the host has a handler for every listed import.
pub struct WasmModule {
  inner: backend::Module,
}

impl WasmModule {
  /// The function imports the module requires, in handler-index order.
  pub fn imports(&self) -> &[ImportInfo] {
    self.inner.imports()
  }

  /// Instantiate: link every import to a bridging stub and run the start
  /// function (which must not call a host import). Takes `&self`, so one
  /// parsed module can back several independent instances.
  pub fn instantiate(&self) -> Result<WasmInstance, String> {
    Ok(WasmInstance { inner: self.inner.instantiate()? })
  }
}

/// A live instance. Not `Send`: it lives on (and re-enters) the host's calling
/// thread. All entry points take `&self`; the engine is borrowed only while
/// wasm actually executes, never across a host handler invocation, so handlers
/// may re-enter the instance.
pub struct WasmInstance {
  inner: backend::Instance,
}

impl WasmInstance {
  /// The module's exports (name, kind), in declaration order.
  pub fn exports(&self) -> &[(String, ExportInfo)] {
    self.inner.exports()
  }

  /// The scalar signature of an exported function, for argument coercion.
  pub fn export_sig(&self, name: &str) -> Option<&FuncSig> {
    self.exports().iter().find_map(|(n, info)| match info {
      ExportInfo::Func(sig) if n == name => Some(sig),
      _ => None,
    })
  }

  /// Whether the module exports a linear memory.
  pub fn has_memory(&self) -> bool {
    self.inner.has_memory()
  }

  /// Call an exported function. `args` must already match the export's
  /// parameter types (the engine verifies). Host imports hit during execution
  /// are dispatched to `host`, whose results continue the call. A `host` error
  /// aborts and unwinds the wasm call.
  pub fn call(&self, name: &str, args: Vec<WasmValue>, host: HostHandler<'_>) -> Result<Vec<WasmValue>, String> {
    self.inner.call(name, args, host)
  }

  /// Call a function by its index in the exported function table:
  /// `table[index](args)`. This is how the host invokes a guest function
  /// pointer it was handed as an integer. Same bridging rules as `call`.
  pub fn call_indirect(
    &self,
    index: u32,
    args: Vec<WasmValue>,
    host: HostHandler<'_>,
  ) -> Result<Vec<WasmValue>, String> {
    self.inner.call_indirect(index, args, host)
  }

  /// The scalar signature of the function at `index` in the exported table,
  /// for argument coercion.
  pub fn table_func_sig(&self, index: u32) -> Result<FuncSig, String> {
    self.inner.table_func_sig(index)
  }

  /// The exported memory's current size in bytes, if the module exports one.
  pub fn memory_size(&self) -> Option<usize> {
    self.inner.memory_size()
  }

  /// Base pointer and byte length of the exported memory, if the module
  /// exports one. The pointer aliases the live linear memory: growth
  /// (`memory.grow`, only possible while guest code runs) may move the
  /// storage and invalidate it, so a caller that hands the pointer out must
  /// re-check it after every guest call before letting anyone read through it.
  pub fn memory_data_ptr(&self) -> Option<(*mut u8, usize)> {
    self.inner.memory_data_ptr()
  }

  /// Copy `len` bytes out of the exported memory at `ptr`.
  pub fn memory_read(&self, ptr: usize, len: usize) -> Result<Vec<u8>, String> {
    self.inner.memory_read(ptr, len)
  }

  /// Copy `bytes` into the exported memory at `ptr`.
  pub fn memory_write(&self, ptr: usize, bytes: &[u8]) -> Result<(), String> {
    self.inner.memory_write(ptr, bytes)
  }
}

/// The hint appended to a bad-signature trap: the guest's own `call_indirect`
/// hit a table entry whose type does not match the call site, and neither
/// engine exposes which table index was called.
pub(crate) const BAD_SIGNATURE_HINT: &str =
  " (an indirect call inside the guest hit a table entry whose signature does not match the call site; \
   a stale function pointer, e.g. one taken before a re-instantiation, fails this way)";

/// Bounds-check a `ptr..ptr+len` window against a memory of `size` bytes.
pub(crate) fn memory_window(what: &str, ptr: usize, len: usize, size: usize) -> Result<std::ops::Range<usize>, String> {
  match ptr.checked_add(len).filter(|end| *end <= size) {
    Some(end) => Ok(ptr..end),
    None => Err(format!("memory {what} out of bounds: {ptr}+{len} exceeds size {size}")),
  }
}
