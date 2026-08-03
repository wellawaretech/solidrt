//! Engine-free WebAssembly hosting core, built on the wasmi interpreter.
//!
//! Hosts precompiled `.wasm` modules (or, with wasmi's `wat` feature, text
//! format sources): parse and validate a module, resolve its function imports
//! to host-provided handlers, instantiate it, call its exports (by name, or by
//! index through its exported function table), and access its exported linear
//! memory. Pure Rust, no JIT, so one `.wasm` artifact runs unmodified on every
//! target.
//!
//! Host imports are bridged with wasmi's resumable calls: every function
//! import is registered as a stub that records its arguments and suspends
//! execution with a sentinel host error. `WasmInstance::call` then invokes the
//! caller-supplied handler OUTSIDE any borrow of the wasmi store and resumes
//! the wasm frame with the handler's results. Because no borrow is held while
//! the handler runs, a handler may re-enter the instance (call another export,
//! read or write memory) freely.
//!
//! Scope (deliberate, current):
//! - Function imports only; a module that imports memories, tables, or
//!   globals is rejected at parse.
//! - Scalar values only (i32/i64/f32/f64) in bridged signatures; v128 and
//!   reference types are rejected where they would cross the host boundary.
//! - A start function must not call a host import (instantiation runs it
//!   non-resumably); none of the common toolchains emit one that does.
//! - The wasm exception-handling proposal is not supported (wasmi does not
//!   implement it yet); such modules fail validation.

use std::cell::RefCell;
use std::fmt;

use wasmi::errors::HostError;
use wasmi::{
  Engine, ExternType, Func, FuncType, Instance, Linker, Memory, Module, ResumableCall, Store, Table, TrapCode, Val,
  ValType,
};

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

/// One function import a module requires from the host. `index` is the handler
/// index passed back to the host handler on every call of this import.
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

/// A parsed, validated module plus its resolved import list. Instantiate it
/// once the host has a handler for every listed import.
pub struct WasmModule {
  engine: Engine,
  module: Module,
  imports: Vec<ImportInfo>,
}

impl WasmModule {
  /// Parse and validate a wasm binary (or wat text). Rejects modules with
  /// non-function imports or imports with non-scalar signatures.
  pub fn parse(bytes: &[u8]) -> Result<WasmModule, String> {
    let engine = Engine::default();
    let module = Module::new(&engine, bytes).map_err(|e| format!("invalid wasm module: {e}"))?;
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
    Ok(WasmModule { engine, module, imports })
  }

  /// The function imports the module requires, in handler-index order.
  pub fn imports(&self) -> &[ImportInfo] {
    &self.imports
  }

  /// Instantiate: register a suspending stub for every import, link, and run
  /// the start function (which must not call a host import). Takes `&self`, so
  /// one parsed module can back several independent instances.
  pub fn instantiate(&self) -> Result<WasmInstance, String> {
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

    Ok(WasmInstance { store: RefCell::new(store), instance, memory, table, exports })
  }
}

/// A live instance. Not `Send`: it lives on (and re-enters) the host's calling
/// thread. All entry points take `&self`; the store is borrowed only while
/// wasm actually executes, never across a host handler invocation, so handlers
/// may re-enter the instance.
pub struct WasmInstance {
  store: RefCell<Store<HostState>>,
  instance: Instance,
  memory: Option<Memory>,
  table: Option<Table>,
  exports: Vec<(String, ExportInfo)>,
}

impl WasmInstance {
  /// The module's exports (name, kind), in declaration order.
  pub fn exports(&self) -> &[(String, ExportInfo)] {
    &self.exports
  }

  /// The scalar signature of an exported function, for argument coercion.
  pub fn export_sig(&self, name: &str) -> Option<&FuncSig> {
    self.exports.iter().find_map(|(n, info)| match info {
      ExportInfo::Func(sig) if n == name => Some(sig),
      _ => None,
    })
  }

  /// Whether the module exports a linear memory.
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
          let hint = if e.as_trap_code() == Some(TrapCode::BadSignature) {
            " (an indirect call inside the guest hit a table entry whose signature does not match the call site; \
             a stale function pointer, e.g. one taken before a re-instantiation, fails this way)"
          } else {
            ""
          };
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
    let end = ptr.checked_add(len).filter(|end| *end <= data.len());
    let Some(end) = end else {
      return Err(format!("memory read out of bounds: {ptr}+{len} exceeds size {}", data.len()));
    };
    Ok(data[ptr..end].to_vec())
  }

  /// Copy `bytes` into the exported memory at `ptr`.
  pub fn memory_write(&self, ptr: usize, bytes: &[u8]) -> Result<(), String> {
    let Some(mem) = self.memory else {
      return Err("wasm module exports no memory".to_string());
    };
    let mut store = self.store.borrow_mut();
    let data = mem.data_mut(&mut *store);
    let end = ptr.checked_add(bytes.len()).filter(|end| *end <= data.len());
    let Some(end) = end else {
      return Err(format!("memory write out of bounds: {ptr}+{} exceeds size {}", bytes.len(), data.len()));
    };
    data[ptr..end].copy_from_slice(bytes);
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
