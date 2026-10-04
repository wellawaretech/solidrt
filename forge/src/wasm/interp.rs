//! The interpreter backend: wasmi, a register-based interpreter with no JIT.
//! Pure Rust, so one `.wasm` runs unmodified on every target; the lane for
//! targets that cannot run generated code (the `wasm-native` feature off).
//!
//! Host imports are bridged with wasmi's resumable calls: every function
//! import is registered as a stub that records its arguments and suspends
//! execution with a sentinel host error. `Instance::call` then invokes the
//! caller-supplied handler OUTSIDE any borrow of the wasmi store and resumes
//! the wasm frame with the handler's results. Because no borrow is held while
//! the handler runs, a handler may re-enter the instance (call another export,
//! read or write memory) freely.

use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt;
use std::path::Path;

use wasmi::errors::HostError;
use wasmi::{ExternType, Func, FuncType, Linker, Memory, ResumableCall, Store, Table, TrapCode, Val, ValType};

use super::{
  memory_window, ExportInfo, FuncSig, HostHandler, ImportInfo, Lane, WasmType, WasmValue, BAD_SIGNATURE_HINT,
};

pub const LANE: Lane = Lane::Interpreter;

/// Sentinel host error a stubbed import raises to suspend execution; never
/// surfaces to callers.
#[derive(Debug)]
struct HostCallPending;

impl fmt::Display for HostCallPending {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "pending host call")
  }
}

impl std::error::Error for HostCallPending {}
impl HostError for HostCallPending {}

/// Store data: the stub import's recorded call, picked up by `call`'s resume
/// loop right after the suspension.
struct HostState {
  pending: Option<(usize, Vec<WasmValue>)>,
}

/// One wasmi engine, shared by every module parsed through it.
#[derive(Clone)]
pub struct Engine {
  inner: wasmi::Engine,
}

impl Engine {
  pub fn new() -> Result<Engine, String> {
    // The default config enables wasmi's whole proposal set, SIMD and relaxed
    // SIMD included once the cargo feature is on; the native backend mirrors it.
    Ok(Engine { inner: wasmi::Engine::default() })
  }

  /// Parse and validate a wasm binary (or wat text). Rejects modules with
  /// non-function imports or imports with non-scalar signatures. There is no
  /// compiled form, so the cache dir is unused: translation is lazy and takes
  /// milliseconds for a large module.
  pub fn parse(&self, bytes: &[u8], _cache_dir: Option<&Path>) -> Result<Module, String> {
    let engine = self.inner.clone();
    let module = wasmi::Module::new(&engine, bytes).map_err(|e| format!("invalid wasm module: {e}"))?;
    let mut imports: Vec<ImportInfo> = Vec::new();
    for imp in module.imports() {
      match imp.ty() {
        ExternType::Func(ty) => {
          // The same (module, name) may be imported more than once; register it
          // once and let all occurrences share the handler (wasmi type-checks
          // each occurrence against the one definition at instantiation).
          if imports.iter().any(|i| i.module == imp.module() && i.name == imp.name()) {
            continue;
          }
          let sig = func_sig(ty)
            .map_err(|t| format!("import {}.{} has unsupported wasm type {t:?}", imp.module(), imp.name()))?;
          imports.push(ImportInfo { module: imp.module().to_string(), name: imp.name().to_string(), sig });
        }
        other => {
          return Err(format!(
            "unsupported non-function import {}.{} ({})",
            imp.module(),
            imp.name(),
            extern_kind(other)
          ));
        }
      }
    }
    Ok(Module { engine, module, imports })
  }

  /// Nothing to precompile: see `parse`.
  pub fn precompile(&self, _bytes: &[u8], _cache_dir: &Path) -> Result<(), String> {
    Ok(())
  }

  /// Nothing cached, nothing to prune.
  pub fn prune_cache(&self, _cache_dir: &Path, _keep: &HashSet<String>) {}
}

/// A parsed, validated module plus its resolved import list.
pub struct Module {
  engine: wasmi::Engine,
  module: wasmi::Module,
  imports: Vec<ImportInfo>,
}

impl Module {
  pub fn imports(&self) -> &[ImportInfo] {
    &self.imports
  }

  /// Instantiate: register a suspending stub for every import, link, and run
  /// the start function (which must not call a host import). Takes `&self`, so
  /// one parsed module can back several independent instances.
  pub fn instantiate(&self) -> Result<Instance, String> {
    let mut store = Store::new(&self.engine, HostState { pending: None });
    let mut linker = Linker::<HostState>::new(&self.engine);
    for (index, info) in self.imports.iter().enumerate() {
      let ty = FuncType::new(
        info.sig.params.iter().map(|t| val_type(*t)).collect::<Vec<_>>(),
        info.sig.results.iter().map(|t| val_type(*t)).collect::<Vec<_>>(),
      );
      linker
        .func_new(&info.module, &info.name, ty, move |mut caller, args, _results| {
          let args = args.iter().map(from_val).collect::<Result<Vec<_>, _>>().map_err(wasmi::Error::host)?;
          caller.data_mut().pending = Some((index, args));
          Err(wasmi::Error::host(HostCallPending))
        })
        .map_err(|e| format!("failed to define import {}.{}: {e}", info.module, info.name))?;
    }
    let instance = linker.instantiate_and_start(&mut store, &self.module).map_err(|e| {
      if e.downcast_ref::<HostCallPending>().is_some() {
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
        ExternType::Func(ty) => func_sig(ty).map(ExportInfo::Func).unwrap_or(ExportInfo::Other),
        ExternType::Memory(_) => {
          if memory.is_none() {
            memory = instance.get_memory(&store, exp.name());
          }
          ExportInfo::Memory
        }
        ExternType::Table(_) => {
          // The first exported table backs call_indirect (C toolchains export
          // their function table as `__indirect_function_table`).
          if table.is_none() {
            table = instance.get_table(&store, exp.name());
          }
          ExportInfo::Other
        }
        _ => ExportInfo::Other,
      };
      exports.push((exp.name().to_string(), info));
    }

    Ok(Instance { store: RefCell::new(store), instance, memory, table, exports })
  }
}

/// A live instance. The store is borrowed only while wasm actually executes,
/// never across a host handler invocation, so handlers may re-enter.
pub struct Instance {
  store: RefCell<Store<HostState>>,
  instance: wasmi::Instance,
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

  /// Call an exported function. `args` must already match the export's
  /// parameter types (wasmi verifies). Host imports hit during execution
  /// suspend the wasm frame and are dispatched to `host`; its results resume
  /// the frame. A `host` error aborts and unwinds the wasm call.
  pub fn call(&self, name: &str, args: Vec<WasmValue>, host: HostHandler<'_>) -> Result<Vec<WasmValue>, String> {
    let func = {
      let store = self.store.borrow();
      self.instance.get_func(&*store, name).ok_or_else(|| format!("no exported wasm function named {name}"))?
    };
    self.drive(name, func, args, host)
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
    let func = self.table_func(index)?;
    let what = {
      let store = self.store.borrow();
      match func_sig(&func.ty(&*store)) {
        Ok(sig) => format!("table[{index}] {sig}"),
        Err(_) => format!("table[{index}]"),
      }
    };
    self.drive(&what, func, args, host)
  }

  /// The scalar signature of the function at `index` in the exported table,
  /// for argument coercion.
  pub fn table_func_sig(&self, index: u32) -> Result<FuncSig, String> {
    let func = self.table_func(index)?;
    let store = self.store.borrow();
    func_sig(&func.ty(&*store)).map_err(|t| format!("table function {index} has unsupported wasm type {t:?}"))
  }

  /// Resolve a non-null funcref from the exported function table.
  fn table_func(&self, index: u32) -> Result<Func, String> {
    let Some(table) = self.table else {
      return Err("wasm module exports no function table".to_string());
    };
    let store = self.store.borrow();
    let entry =
      table.get(&*store, u64::from(index)).ok_or_else(|| format!("function table index {index} out of range"))?;
    let funcref = entry.as_func().ok_or_else(|| format!("function table entry {index} is not a funcref"))?;
    Option::<&Func>::from(funcref)
      .cloned()
      .ok_or_else(|| format!("function table entry {index} is a null function pointer"))
  }

  /// Run `func` to completion, bridging suspended host-import calls to `host`.
  /// `what` names the call target (an export name or `table[i] (sig)`) so
  /// failures say which call went wrong.
  fn drive(&self, what: &str, func: Func, args: Vec<WasmValue>, host: HostHandler<'_>) -> Result<Vec<WasmValue>, String> {
    let n_results = {
      let store = self.store.borrow();
      func.ty(&*store).results().len()
    };
    let inputs: Vec<Val> = args.iter().map(|v| to_val(*v)).collect();
    let mut outputs = vec![Val::I32(0); n_results];

    let mut state = {
      let mut store = self.store.borrow_mut();
      func.call_resumable(&mut *store, &inputs, &mut outputs)
    };
    loop {
      match state {
        Ok(ResumableCall::Finished) => {
          return outputs.iter().map(|v| from_val(v).map_err(String::from)).collect();
        }
        Ok(ResumableCall::HostTrap(trap)) => {
          if trap.host_error().downcast_ref::<HostCallPending>().is_none() {
            return Err(format!("wasm call to {what} failed: {}", trap.into_host_error()));
          }
          let (index, host_args) = self
            .store
            .borrow_mut()
            .data_mut()
            .pending
            .take()
            .ok_or_else(|| "wasm host call suspended without recorded arguments".to_string())?;
          // No store borrow is held here: the handler may re-enter freely.
          let results = host(index, host_args)?;
          let resume_inputs: Vec<Val> = results.iter().map(|v| to_val(*v)).collect();
          state = {
            let mut store = self.store.borrow_mut();
            trap.resume(&mut *store, &resume_inputs, &mut outputs)
          };
        }
        Ok(ResumableCall::OutOfFuel(_)) => {
          // Fuel metering is never enabled on this engine.
          return Err("wasm execution ran out of fuel".to_string());
        }
        Err(e) => {
          // A bad-signature trap comes from a call_indirect INSIDE the guest;
          // wasmi does not expose which table index was called, so the best we
          // can name is the outer call and the likely cause.
          let hint = if e.as_trap_code() == Some(TrapCode::BadSignature) { BAD_SIGNATURE_HINT } else { "" };
          return Err(format!("wasm call to {what} failed: {e}{hint}"));
        }
      }
    }
  }

  /// The exported memory's current size in bytes, if the module exports one.
  pub fn memory_size(&self) -> Option<usize> {
    self.memory.map(|m| m.data(&*self.store.borrow()).len())
  }

  /// Base pointer and byte length of the exported memory, if the module
  /// exports one. The pointer aliases the live linear memory: growth
  /// (`memory.grow`, only possible while guest code runs) may move the
  /// storage and invalidate it, so a caller that hands the pointer out must
  /// re-check it after every guest call before letting anyone read through it.
  pub fn memory_data_ptr(&self) -> Option<(*mut u8, usize)> {
    let mem = self.memory?;
    let store = self.store.borrow();
    Some((mem.data_ptr(&*store), mem.data_size(&*store)))
  }

  /// Copy `len` bytes out of the exported memory at `ptr`.
  pub fn memory_read(&self, ptr: usize, len: usize) -> Result<Vec<u8>, String> {
    let Some(mem) = self.memory else {
      return Err("wasm module exports no memory".to_string());
    };
    let store = self.store.borrow();
    let data = mem.data(&*store);
    Ok(data[memory_window("read", ptr, len, data.len())?].to_vec())
  }

  /// Copy `bytes` into the exported memory at `ptr`.
  pub fn memory_write(&self, ptr: usize, bytes: &[u8]) -> Result<(), String> {
    let Some(mem) = self.memory else {
      return Err("wasm module exports no memory".to_string());
    };
    let mut store = self.store.borrow_mut();
    let data = mem.data_mut(&mut *store);
    let window = memory_window("write", ptr, bytes.len(), data.len())?;
    data[window].copy_from_slice(bytes);
    Ok(())
  }
}

fn func_sig(ty: &FuncType) -> Result<FuncSig, ValType> {
  let scalar = |t: &ValType| -> Result<WasmType, ValType> {
    match t {
      ValType::I32 => Ok(WasmType::I32),
      ValType::I64 => Ok(WasmType::I64),
      ValType::F32 => Ok(WasmType::F32),
      ValType::F64 => Ok(WasmType::F64),
      other => Err(*other),
    }
  };
  Ok(FuncSig {
    params: ty.params().iter().map(scalar).collect::<Result<_, _>>()?,
    results: ty.results().iter().map(scalar).collect::<Result<_, _>>()?,
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
    WasmValue::F32(x) => Val::F32(x.into()),
    WasmValue::F64(x) => Val::F64(x.into()),
  }
}

fn from_val(v: &Val) -> Result<WasmValue, ValTypeError> {
  match v {
    Val::I32(x) => Ok(WasmValue::I32(*x)),
    Val::I64(x) => Ok(WasmValue::I64(*x)),
    Val::F32(x) => Ok(WasmValue::F32((*x).into())),
    Val::F64(x) => Ok(WasmValue::F64((*x).into())),
    other => Err(ValTypeError(other.ty())),
  }
}

/// A non-scalar value reached the host boundary (only possible through an
/// export we did not bridge; bridged signatures are validated at parse).
#[derive(Debug)]
struct ValTypeError(ValType);

impl fmt::Display for ValTypeError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "unsupported wasm value type {:?} at the host boundary", self.0)
  }
}

impl std::error::Error for ValTypeError {}
impl HostError for ValTypeError {}

impl From<ValTypeError> for String {
  fn from(e: ValTypeError) -> String {
    e.to_string()
  }
}

fn extern_kind(ty: &ExternType) -> &'static str {
  match ty {
    ExternType::Func(_) => "function",
    ExternType::Global(_) => "global",
    ExternType::Table(_) => "table",
    ExternType::Memory(_) => "memory",
  }
}
