//! The native backend: wasmtime with Cranelift, compiling a module to machine
//! code for the host. The lane wherever a backend exists and runtime codegen
//! is allowed (desktop, 64-bit Android); the `wasm-native` feature.
//!
//! Host imports: wasmtime runs a host function synchronously inside the guest
//! call, holding the store for its whole duration and handing the function a
//! `Caller` as its only way to the store. There is no suspension to hand back,
//! so re-entrancy works the other way round from wasmi's: while a host
//! function runs, the instance publishes that activation's `Caller` as the
//! active store context (`Shared::active`), and every entry point goes
//! through `with_store`, which uses the active caller when there is one and
//! the owned store otherwise. Nested host calls stack (each restores the
//! previous caller on return), and the handler for the call in flight is
//! published the same way (`Shared::handler`), since the closure registered
//! with the linker has to be `'static` and cannot capture it. Both are raw
//! pointers that live exactly as long as the frame that set them, published
//! through a guard that restores the previous value when the frame ends, by
//! return or by an unwinding panic; see the SAFETY notes at the two
//! dereferences.
//!
//! The compiled-artifact cache lives at `<cache dir>/wasm/<engine hash>/<module
//! sha256>.cwasm`. The engine hash digests wasmtime's own compatibility hash
//! (version, target, tunables, features), so an artifact is only ever offered
//! to the engine configuration that produced it; a different build gets a new
//! directory and the old one is pruned. Artifacts are written beside their
//! final name and renamed into place, so a crash never leaves a half-written
//! file where a load would find it.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use sha2::{Digest, Sha256};

use wasmtime::{
  AsContextMut, Caller, Config, ExternType, Func, FuncType, Linker, Memory, Store, StoreContextMut, Table, Trap, Val,
  ValType,
};

use super::{
  memory_window, ExportInfo, FuncSig, HostHandler, ImportInfo, Lane, WasmType, WasmValue, BAD_SIGNATURE_HINT,
};

pub const LANE: Lane = Lane::Native;

/// Subdirectory of an app's cache dir holding compiled modules.
const CACHE_SUBDIR: &str = "wasm";
/// Extension of a cached artifact (wasmtime's own name for its serialized form).
const ARTIFACT_EXT: &str = "cwasm";
/// How many bytes of the engine-compatibility digest name its cache directory:
/// 16 (32 hex characters) keeps the path short, and only distinctness between
/// engine configurations on one machine matters.
const ENGINE_HASH_BYTES: usize = 16;

/// One wasmtime engine, shared by every module compiled through it. Cloning
/// shares it (wasmtime's engine is a reference-counted handle).
#[derive(Clone)]
pub struct Engine {
  inner: wasmtime::Engine,
  /// Names this engine configuration's cache directory.
  compat: String,
}

impl Engine {
  pub fn new() -> Result<Engine, String> {
    let mut config = Config::new();
    // One proposal set across lanes: what wasmi validates (see interp.rs), so
    // a module that runs on one lane runs on every lane. Threads, exceptions
    // and function references are off by construction: their cargo features
    // (`threads`, `gc`) are not compiled in, and neither are their setters.
    config.wasm_simd(true);
    config.wasm_relaxed_simd(true);
    config.wasm_multi_memory(true);
    config.wasm_memory64(true);
    config.wasm_tail_call(true);
    config.wasm_extended_const(true);
    config.wasm_gc(false);
    config.wasm_stack_switching(false);
    config.wasm_custom_page_sizes(false);
    config.wasm_wide_arithmetic(false);
    // Errors reach JS as one line; never collect a wasm backtrace for them.
    config.wasm_backtrace_max_frames(None);
    let inner = wasmtime::Engine::new(&config).map_err(|e| format!("cannot create wasmtime engine: {e}"))?;
    let compat = engine_hash(&inner);
    Ok(Engine { inner, compat })
  }

  /// Parse, validate and compile a wasm binary (or wat text), through the
  /// artifact cache under `cache_dir` when one is given. Rejects modules with
  /// non-function imports or imports with non-scalar signatures.
  pub fn parse(&self, bytes: &[u8], cache_dir: Option<&Path>) -> Result<Module, String> {
    let module = match cache_dir {
      Some(dir) => self.load_or_compile(bytes, dir)?,
      None => compile(&self.inner, bytes)?,
    };
    let mut imports: Vec<ImportInfo> = Vec::new();
    for imp in module.imports() {
      match imp.ty() {
        ExternType::Func(ty) => {
          // The same (module, name) may be imported more than once; register it
          // once and let all occurrences share the handler.
          if imports.iter().any(|i| i.module == imp.module() && i.name == imp.name()) {
            continue;
          }
          let sig = func_sig(&ty)
            .map_err(|t| format!("import {}.{} has unsupported wasm type {t:?}", imp.module(), imp.name()))?;
          imports.push(ImportInfo { module: imp.module().to_string(), name: imp.name().to_string(), sig });
        }
        other => {
          return Err(format!(
            "unsupported non-function import {}.{} ({})",
            imp.module(),
            imp.name(),
            extern_kind(&other)
          ));
        }
      }
    }
    Ok(Module { engine: self.inner.clone(), module, imports })
  }

  /// Fill the cache slot for `bytes` without keeping the module.
  pub fn precompile(&self, bytes: &[u8], cache_dir: &Path) -> Result<(), String> {
    self.load_or_compile(bytes, cache_dir).map(|_| ())
  }

  /// This engine's cache directory under an app's cache dir.
  fn cache_root(&self, cache_dir: &Path) -> PathBuf {
    cache_dir.join(CACHE_SUBDIR).join(&self.compat)
  }

  /// Load the cached artifact for `bytes`, or compile and cache it. Every
  /// cache failure is logged and degrades to an in-memory compile.
  fn load_or_compile(&self, bytes: &[u8], cache_dir: &Path) -> Result<wasmtime::Module, String> {
    let path = self.cache_root(cache_dir).join(format!("{}.{ARTIFACT_EXT}", sha256_hex(bytes)));
    if path.is_file() {
      // SAFETY: `deserialize_file` trusts its input to be an artifact this
      // engine produced. The path is content-addressed under the app's own
      // cache dir in a directory named by this engine configuration, written
      // only by `store_artifact` below from a module this engine compiled, and
      // wasmtime still verifies the artifact's version and config header
      // before using it; a mismatch or a damaged file is an error we recover
      // from, not undefined behaviour.
      match unsafe { wasmtime::Module::deserialize_file(&self.inner, &path) } {
        Ok(module) => return Ok(module),
        Err(e) => log::warn!("[forge] cached wasm artifact {} unusable, recompiling: {e}", path.display()),
      }
    }
    let module = compile(&self.inner, bytes)?;
    store_artifact(&module, &path);
    Ok(module)
  }

  /// Remove artifacts of other engine configurations wholesale, and within this
  /// engine's directory every artifact whose module hash is not in `keep`.
  pub fn prune_cache(&self, cache_dir: &Path, keep: &HashSet<String>) {
    let root = cache_dir.join(CACHE_SUBDIR);
    let Ok(engines) = std::fs::read_dir(&root) else { return };
    for engine_dir in engines.flatten() {
      let path = engine_dir.path();
      if engine_dir.file_name() != self.compat.as_str() {
        remove(&path);
        continue;
      }
      let Ok(artifacts) = std::fs::read_dir(&path) else { continue };
      for artifact in artifacts.flatten() {
        let name = artifact.file_name();
        let stem = name.to_str().and_then(|n| n.strip_suffix(&format!(".{ARTIFACT_EXT}")));
        // Anything that is not a kept artifact goes, write leftovers included.
        if !stem.is_some_and(|hash| keep.contains(hash)) {
          remove(&artifact.path());
        }
      }
    }
  }
}

fn compile(engine: &wasmtime::Engine, bytes: &[u8]) -> Result<wasmtime::Module, String> {
  wasmtime::Module::new(engine, bytes).map_err(|e| format!("invalid wasm module: {e}"))
}

/// Serialize `module` to `path`, beside-then-rename so a crash mid-write never
/// leaves a half artifact at the final name. Failure is logged, not returned:
/// the module is already compiled and the cache is a convenience.
fn store_artifact(module: &wasmtime::Module, path: &Path) {
  let write = || -> Result<(), String> {
    let bytes = module.serialize().map_err(|e| e.to_string())?;
    let dir = path.parent().ok_or("artifact path has no directory")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let name = path.file_name().and_then(|n| n.to_str()).ok_or("artifact path has no file name")?;
    let tmp = dir.join(format!(".tmp-{}-{name}", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
  };
  if let Err(e) = write() {
    log::warn!("[forge] cannot cache compiled wasm at {}: {e}", path.display());
  }
}

fn remove(path: &Path) {
  let result = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
  if let Err(e) = result {
    log::warn!("[forge] cannot remove stale wasm artifact {}: {e}", path.display());
  }
}

fn sha256_hex(bytes: &[u8]) -> String {
  Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// A stable digest of wasmtime's precompile compatibility hash. It is only
/// exposed as a `Hash` impl; feeding it to sha256 instead of the std hasher
/// keeps the directory name independent of the Rust release's hash algorithm.
fn engine_hash(engine: &wasmtime::Engine) -> String {
  struct Digesting(Sha256);
  impl Hasher for Digesting {
    fn write(&mut self, bytes: &[u8]) {
      self.0.update(bytes);
    }
    fn finish(&self) -> u64 {
      0
    }
  }
  let mut hasher = Digesting(Sha256::new());
  engine.precompile_compatibility_hash().hash(&mut hasher);
  hasher.0.finalize().iter().take(ENGINE_HASH_BYTES).map(|b| format!("{b:02x}")).collect()
}

/// A compiled module plus its resolved import list.
pub struct Module {
  engine: wasmtime::Engine,
  module: wasmtime::Module,
  imports: Vec<ImportInfo>,
}

/// The handler of the call in flight, lifetime erased for the store data.
type HandlerPtr = *mut (dyn FnMut(usize, Vec<WasmValue>) -> Result<Vec<WasmValue>, String> + 'static);

/// Per-instance state shared between the instance and its store data: the
/// caller of the host function currently running (if any) and the handler
/// serving the call in flight (if any). Both are published for the dynamic
/// extent of one frame through `publish`, which restores the previous value
/// when the frame ends.
struct Shared {
  active: Cell<Option<*mut Caller<'static, HostState>>>,
  handler: Cell<Option<HandlerPtr>>,
}

/// A value published into a slot for the extent of a scope. Dropping it puts
/// the previous value back, so the slot never outlives its frame: not on
/// return, and not when a panic in a host handler unwinds through the frame,
/// which would otherwise leave a dangling pointer for the next re-entrant
/// call to dereference.
struct Published<'a, T: Copy> {
  slot: &'a Cell<Option<T>>,
  previous: Option<T>,
}

fn publish<T: Copy>(slot: &Cell<Option<T>>, value: T) -> Published<'_, T> {
  Published { slot, previous: slot.replace(Some(value)) }
}

impl<T: Copy> Drop for Published<'_, T> {
  fn drop(&mut self) {
    self.slot.set(self.previous);
  }
}

struct HostState {
  shared: Rc<Shared>,
}

/// A host handler aborted the call with this message; surfaces verbatim.
#[derive(Debug)]
struct HostAbort(String);

impl fmt::Display for HostAbort {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.0)
  }
}

impl std::error::Error for HostAbort {}

/// A host import was called with no handler published: only the start
/// function can do that.
#[derive(Debug)]
struct StartImport;

impl fmt::Display for StartImport {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str("host import called outside a bridged call")
  }
}

impl std::error::Error for StartImport {}

impl Module {
  pub fn imports(&self) -> &[ImportInfo] {
    &self.imports
  }

  /// Instantiate: register a bridging host function for every import, link,
  /// and run the start function (which must not call a host import).
  pub fn instantiate(&self) -> Result<Instance, String> {
    let shared = Rc::new(Shared { active: Cell::new(None), handler: Cell::new(None) });
    let mut store = Store::new(&self.engine, HostState { shared: shared.clone() });
    let mut linker = Linker::<HostState>::new(&self.engine);
    for (index, info) in self.imports.iter().enumerate() {
      let ty = FuncType::new(
        &self.engine,
        info.sig.params.iter().map(|t| val_type(*t)),
        info.sig.results.iter().map(|t| val_type(*t)),
      );
      linker
        .func_new(&info.module, &info.name, ty, move |mut caller: Caller<'_, HostState>, params, results| {
          let args = params.iter().map(from_val).collect::<Result<Vec<_>, _>>().map_err(wasmtime::Error::new)?;
          let shared = caller.data().shared.clone();
          let Some(handler) = shared.handler.get() else {
            return Err(wasmtime::Error::new(StartImport));
          };
          // Publish this activation's store to re-entrant calls for exactly
          // the handler's run; the guard puts the outer activation back.
          let caller_ptr = (&mut caller as *mut Caller<'_, HostState>).cast::<Caller<'static, HostState>>();
          let outcome = {
            let _active = publish(&shared.active, caller_ptr);
            // SAFETY: `handler` was published by `drive` on this thread for the
            // duration of the `Func::call` this host function runs inside of,
            // and stays published until that call returns or unwinds, so the
            // handler it points to is alive and not otherwise borrowed while
            // we run it (a nested `drive` publishes its own and restores ours).
            unsafe { (*handler)(index, args) }
          };
          let values = outcome.map_err(|m| wasmtime::Error::new(HostAbort(m)))?;
          if values.len() != results.len() {
            return Err(wasmtime::Error::new(HostAbort(format!(
              "host function returned {} values, import declares {}",
              values.len(),
              results.len()
            ))));
          }
          for (slot, value) in results.iter_mut().zip(values) {
            *slot = to_val(value);
          }
          Ok(())
        })
        .map_err(|e| format!("failed to define import {}.{}: {e}", info.module, info.name))?;
    }
    let instance = linker.instantiate(&mut store, &self.module).map_err(|e| {
      if e.downcast_ref::<StartImport>().is_some() {
        "wasm start function called a host import, which is not supported".to_string()
      } else {
        format!("failed to instantiate wasm module: {e}")
      }
    })?;

    let mut exports: Vec<(String, ExportInfo)> = Vec::new();
    let mut memory: Option<Memory> = None;
    let mut table: Option<Table> = None;
    for exp in self.module.exports() {
      let info = match exp.ty() {
        ExternType::Func(ty) => func_sig(&ty).map(ExportInfo::Func).unwrap_or(ExportInfo::Other),
        ExternType::Memory(_) => {
          if memory.is_none() {
            memory = instance.get_memory(&mut store, exp.name());
          }
          ExportInfo::Memory
        }
        ExternType::Table(_) => {
          // The first exported table backs call_indirect (C toolchains export
          // their function table as `__indirect_function_table`).
          if table.is_none() {
            table = instance.get_table(&mut store, exp.name());
          }
          ExportInfo::Other
        }
        _ => ExportInfo::Other,
      };
      exports.push((exp.name().to_string(), info));
    }

    Ok(Instance { store: RefCell::new(store), shared, instance, memory, table, exports })
  }
}

/// A live instance. The owned store is borrowed for a top-level call; a
/// re-entrant call from inside a host function uses that function's caller
/// instead (see the module docs).
pub struct Instance {
  store: RefCell<Store<HostState>>,
  shared: Rc<Shared>,
  instance: wasmtime::Instance,
  memory: Option<Memory>,
  table: Option<Table>,
  exports: Vec<(String, ExportInfo)>,
}

impl Instance {
  pub fn exports(&self) -> &[(String, ExportInfo)] {
    &self.exports
  }

  pub fn has_memory(&self) -> bool {
    self.memory.is_some()
  }

  /// Run `f` against the store: the active host function's caller when one is
  /// running (the store is exclusively held by the call it sits in, and this
  /// is the only sanctioned way back into it), the owned store otherwise.
  fn with_store<R>(&self, f: impl FnOnce(StoreContextMut<'_, HostState>) -> R) -> R {
    match self.shared.active.get() {
      Some(active) => {
        // SAFETY: `active` points at the `Caller` local of the host function
        // currently executing on this thread (instances are not `Send`), set
        // right before it invoked the handler we are running inside of and
        // cleared right after. wasmtime hands that caller out as the one
        // mutable route to the store for the function's duration, which is
        // exactly our extent; a nested host function publishes its own
        // caller and restores this one, so no two uses overlap.
        let caller = unsafe { &mut *active };
        f(caller.as_context_mut())
      }
      None => {
        let mut store = self.store.borrow_mut();
        f(store.as_context_mut())
      }
    }
  }

  pub fn call(&self, name: &str, args: Vec<WasmValue>, host: HostHandler<'_>) -> Result<Vec<WasmValue>, String> {
    let func = self
      .with_store(|mut s| self.instance.get_func(&mut s, name))
      .ok_or_else(|| format!("no exported wasm function named {name}"))?;
    self.drive(name, func, args, host)
  }

  pub fn call_indirect(
    &self,
    index: u32,
    args: Vec<WasmValue>,
    host: HostHandler<'_>,
  ) -> Result<Vec<WasmValue>, String> {
    let func = self.table_func(index)?;
    let what = match self.with_store(|mut s| func_sig(&func.ty(&mut s))) {
      Ok(sig) => format!("table[{index}] {sig}"),
      Err(_) => format!("table[{index}]"),
    };
    self.drive(&what, func, args, host)
  }

  pub fn table_func_sig(&self, index: u32) -> Result<FuncSig, String> {
    let func = self.table_func(index)?;
    self
      .with_store(|mut s| func_sig(&func.ty(&mut s)))
      .map_err(|t| format!("table function {index} has unsupported wasm type {t:?}"))
  }

  /// Resolve a non-null funcref from the exported function table.
  fn table_func(&self, index: u32) -> Result<Func, String> {
    let Some(table) = self.table else {
      return Err("wasm module exports no function table".to_string());
    };
    let entry = self
      .with_store(|mut s| table.get(&mut s, u64::from(index)))
      .ok_or_else(|| format!("function table index {index} out of range"))?;
    let funcref = entry.as_func().ok_or_else(|| format!("function table entry {index} is not a funcref"))?;
    funcref.cloned().ok_or_else(|| format!("function table entry {index} is a null function pointer"))
  }

  /// Run `func` to completion with `host` serving its imports. `what` names
  /// the call target (an export name or `table[i] (sig)`) so failures say
  /// which call went wrong.
  fn drive(
    &self,
    what: &str,
    func: Func,
    args: Vec<WasmValue>,
    host: HostHandler<'_>,
  ) -> Result<Vec<WasmValue>, String> {
    let n_results = self.with_store(|mut s| func.ty(&mut s).results().len());
    let inputs: Vec<Val> = args.iter().map(|v| to_val(*v)).collect();
    let mut outputs = vec![Val::I32(0); n_results];

    // Publish the handler for this call's imports; a re-entrant call made by a
    // host function publishes its own and the guards restore in order.
    // SAFETY (of the transmute): only the trait object's lifetime bound is
    // erased; the pointer is published for this block's extent, which `host`
    // outlives, and withdrawn by the guard however the block ends.
    let handler: HandlerPtr = unsafe {
      std::mem::transmute(host as *mut (dyn FnMut(usize, Vec<WasmValue>) -> Result<Vec<WasmValue>, String> + '_))
    };
    let result = {
      let _handler = publish(&self.shared.handler, handler);
      self.with_store(|mut s| func.call(&mut s, &inputs, &mut outputs))
    };

    match result {
      Ok(()) => outputs.iter().map(|v| from_val(v).map_err(String::from)).collect(),
      Err(e) => {
        if let Some(abort) = e.downcast_ref::<HostAbort>() {
          return Err(abort.0.clone());
        }
        // A bad-signature trap comes from a call_indirect INSIDE the guest;
        // wasmtime does not expose which table index was called either.
        match e.downcast_ref::<Trap>() {
          Some(Trap::BadSignature) => {
            Err(format!("wasm call to {what} failed: {}{BAD_SIGNATURE_HINT}", Trap::BadSignature))
          }
          Some(trap) => Err(format!("wasm call to {what} failed: {trap}")),
          None => Err(format!("wasm call to {what} failed: {e}")),
        }
      }
    }
  }

  pub fn memory_size(&self) -> Option<usize> {
    let mem = self.memory?;
    Some(self.with_store(|s| mem.data_size(&s)))
  }

  pub fn memory_data_ptr(&self) -> Option<(*mut u8, usize)> {
    let mem = self.memory?;
    Some(self.with_store(|s| (mem.data_ptr(&s), mem.data_size(&s))))
  }

  pub fn memory_read(&self, ptr: usize, len: usize) -> Result<Vec<u8>, String> {
    let Some(mem) = self.memory else {
      return Err("wasm module exports no memory".to_string());
    };
    self.with_store(|s| {
      let data = mem.data(&s);
      Ok(data[memory_window("read", ptr, len, data.len())?].to_vec())
    })
  }

  pub fn memory_write(&self, ptr: usize, bytes: &[u8]) -> Result<(), String> {
    let Some(mem) = self.memory else {
      return Err("wasm module exports no memory".to_string());
    };
    self.with_store(|mut s| {
      let data = mem.data_mut(&mut s);
      let window = memory_window("write", ptr, bytes.len(), data.len())?;
      data[window].copy_from_slice(bytes);
      Ok(())
    })
  }
}

fn func_sig(ty: &FuncType) -> Result<FuncSig, ValType> {
  let scalar = |t: ValType| -> Result<WasmType, ValType> {
    match t {
      ValType::I32 => Ok(WasmType::I32),
      ValType::I64 => Ok(WasmType::I64),
      ValType::F32 => Ok(WasmType::F32),
      ValType::F64 => Ok(WasmType::F64),
      other => Err(other),
    }
  };
  Ok(FuncSig {
    params: ty.params().map(scalar).collect::<Result<_, _>>()?,
    results: ty.results().map(scalar).collect::<Result<_, _>>()?,
  })
}

fn val_type(t: WasmType) -> ValType {
  match t {
    WasmType::I32 => ValType::I32,
    WasmType::I64 => ValType::I64,
    WasmType::F32 => ValType::F32,
    WasmType::F64 => ValType::F64,
  }
}

fn to_val(v: WasmValue) -> Val {
  match v {
    WasmValue::I32(x) => Val::I32(x),
    WasmValue::I64(x) => Val::I64(x),
    WasmValue::F32(x) => Val::F32(x.to_bits()),
    WasmValue::F64(x) => Val::F64(x.to_bits()),
  }
}

fn from_val(v: &Val) -> Result<WasmValue, ValTypeError> {
  match v {
    Val::I32(x) => Ok(WasmValue::I32(*x)),
    Val::I64(x) => Ok(WasmValue::I64(*x)),
    Val::F32(bits) => Ok(WasmValue::F32(f32::from_bits(*bits))),
    Val::F64(bits) => Ok(WasmValue::F64(f64::from_bits(*bits))),
    other => Err(ValTypeError(other.ty_name())),
  }
}

/// A non-scalar value reached the host boundary (only possible through an
/// export we did not bridge; bridged signatures are validated at parse).
#[derive(Debug)]
struct ValTypeError(&'static str);

impl fmt::Display for ValTypeError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "unsupported wasm value type {} at the host boundary", self.0)
  }
}

impl std::error::Error for ValTypeError {}

impl From<ValTypeError> for String {
  fn from(e: ValTypeError) -> String {
    e.to_string()
  }
}

/// The kind of a non-scalar `Val`, for the error above (`Val::ty` needs a
/// store, which the boundary does not have at hand).
trait ValKind {
  fn ty_name(&self) -> &'static str;
}

impl ValKind for Val {
  fn ty_name(&self) -> &'static str {
    match self {
      Val::I32(_) => "i32",
      Val::I64(_) => "i64",
      Val::F32(_) => "f32",
      Val::F64(_) => "f64",
      Val::V128(_) => "v128",
      Val::FuncRef(_) => "funcref",
      Val::ExternRef(_) => "externref",
      Val::AnyRef(_) => "anyref",
      Val::ExnRef(_) => "exnref",
      Val::ContRef(_) => "contref",
    }
  }
}

fn extern_kind(ty: &ExternType) -> &'static str {
  match ty {
    ExternType::Func(_) => "function",
    ExternType::Global(_) => "global",
    ExternType::Table(_) => "table",
    ExternType::Memory(_) => "memory",
    ExternType::Tag(_) => "tag",
  }
}
