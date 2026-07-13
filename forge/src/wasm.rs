//! Engine-free WebAssembly hosting core, built on the wasmi interpreter.
//!
//! Hosts precompiled `.wasm` modules (or, with wasmi's `wat` feature, text
//! format sources): parse and validate a module, resolve its function imports
//! to host-provided handlers, instantiate it, call its exports, and access its
//! exported linear memory. Pure Rust, no JIT, so one `.wasm` artifact runs
//! unmodified on every target.
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
use wasmi::{Engine, ExternType, FuncType, Instance, Linker, Memory, Module, ResumableCall, Store, Val, ValType};

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

/// A bridged function signature: scalar parameter and result types.
#[derive(Debug, Clone)]
pub struct FuncSig {
  pub params: Vec<WasmType>,
  pub results: Vec<WasmType>,
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
    for exp in self.module.exports() {
      let info = match exp.ty() {
        ExternType::Func(ty) => func_sig(ty).map(ExportInfo::Func).unwrap_or(ExportInfo::Other),
        ExternType::Memory(_) => {
          if memory.is_none() {
            memory = instance.get_memory(&store, exp.name());
          }
          ExportInfo::Memory
        }
        _ => ExportInfo::Other,
      };
      exports.push((exp.name().to_string(), info));
    }

    Ok(WasmInstance { store: RefCell::new(store), instance, memory, exports })
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
    let (func, n_results) = {
      let store = self.store.borrow();
      let func =
        self.instance.get_func(&*store, name).ok_or_else(|| format!("no exported wasm function named {name}"))?;
      let n_results = func.ty(&*store).results().len();
      (func, n_results)
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
            return Err(trap.into_host_error().to_string());
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
        Err(e) => return Err(format!("wasm call failed: {e}")),
      }
    }
  }

  /// The exported memory's current size in bytes, if the module exports one.
  pub fn memory_size(&self) -> Option<usize> {
    self.memory.map(|m| m.data(&*self.store.borrow()).len())
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

#[cfg(test)]
mod tests {
  use super::*;

  fn no_host(_: usize, _: Vec<WasmValue>) -> Result<Vec<WasmValue>, String> {
    Err("unexpected host call".to_string())
  }

  #[test]
  fn plain_export_call() {
    let wat = r#"(module (func (export "add") (param i32 i32) (result i32)
      local.get 0 local.get 1 i32.add))"#;
    let inst = WasmModule::parse(wat.as_bytes()).expect("parse").instantiate().expect("instantiate");
    let r = inst.call("add", vec![WasmValue::I32(3), WasmValue::I32(4)], &mut no_host).expect("call");
    assert_eq!(r, vec![WasmValue::I32(7)]);
  }

  #[test]
  fn import_roundtrip() {
    let wat = r#"(module
      (import "env" "mul" (func $mul (param i32 i32) (result i32)))
      (func (export "run") (param i32) (result i32)
        local.get 0 i32.const 10 call $mul))"#;
    let module = WasmModule::parse(wat.as_bytes()).expect("parse");
    assert_eq!(module.imports().len(), 1);
    assert_eq!(module.imports()[0].name, "mul");
    let inst = module.instantiate().expect("instantiate");
    let mut host = |index: usize, args: Vec<WasmValue>| {
      assert_eq!(index, 0);
      let (WasmValue::I32(a), WasmValue::I32(b)) = (args[0], args[1]) else {
        return Err("bad args".to_string());
      };
      Ok(vec![WasmValue::I32(a * b)])
    };
    let r = inst.call("run", vec![WasmValue::I32(6)], &mut host).expect("call");
    assert_eq!(r, vec![WasmValue::I32(60)]);
  }

  #[test]
  fn reentrant_host_handler() {
    // The handler for `outer`'s import re-enters the instance: it calls the
    // `double` export and reads memory while the outer frame is suspended.
    let wat = r#"(module
      (import "env" "host" (func $host (param i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 8) "\2a")
      (func (export "double") (param i32) (result i32) local.get 0 i32.const 2 i32.mul)
      (func (export "outer") (param i32) (result i32) local.get 0 call $host))"#;
    let inst = WasmModule::parse(wat.as_bytes()).expect("parse").instantiate().expect("instantiate");
    let inst_ref = &inst;
    let mut host = move |_index: usize, args: Vec<WasmValue>| {
      let doubled = inst_ref.call("double", args, &mut no_host)?;
      let WasmValue::I32(d) = doubled[0] else {
        return Err("bad double result".to_string());
      };
      let mem = inst_ref.memory_read(8, 1)?;
      Ok(vec![WasmValue::I32(d + mem[0] as i32)])
    };
    let r = inst.call("outer", vec![WasmValue::I32(5)], &mut host).expect("call");
    assert_eq!(r, vec![WasmValue::I32(52)]);
  }

  #[test]
  fn memory_read_write() {
    let wat = r#"(module
      (memory (export "memory") 1)
      (func (export "peek") (param i32) (result i32) local.get 0 i32.load8_u))"#;
    let inst = WasmModule::parse(wat.as_bytes()).expect("parse").instantiate().expect("instantiate");
    inst.memory_write(100, &[7, 8, 9]).expect("write");
    assert_eq!(inst.memory_read(100, 3).expect("read"), vec![7, 8, 9]);
    let r = inst.call("peek", vec![WasmValue::I32(101)], &mut no_host).expect("call");
    assert_eq!(r, vec![WasmValue::I32(8)]);
    assert_eq!(inst.memory_size(), Some(65536));
    assert!(inst.memory_read(65536, 1).is_err());
  }

  #[test]
  fn trap_is_error() {
    let wat = r#"(module (func (export "boom") unreachable))"#;
    let inst = WasmModule::parse(wat.as_bytes()).expect("parse").instantiate().expect("instantiate");
    let err = inst.call("boom", vec![], &mut no_host).expect_err("should trap");
    assert!(err.contains("unreachable"), "unexpected error: {err}");
  }

  #[test]
  fn host_error_aborts_call() {
    let wat = r#"(module
      (import "env" "fail" (func $fail))
      (func (export "run") call $fail))"#;
    let inst = WasmModule::parse(wat.as_bytes()).expect("parse").instantiate().expect("instantiate");
    let mut host = |_: usize, _: Vec<WasmValue>| Err("handler failed".to_string());
    let err = inst.call("run", vec![], &mut host).expect_err("should fail");
    assert_eq!(err, "handler failed");
  }

  #[test]
  fn i64_and_f64_values() {
    let wat = r#"(module
      (func (export "big") (param i64) (result i64) local.get 0 i64.const 1 i64.add)
      (func (export "half") (param f64) (result f64) local.get 0 f64.const 2 f64.div))"#;
    let inst = WasmModule::parse(wat.as_bytes()).expect("parse").instantiate().expect("instantiate");
    let r = inst.call("big", vec![WasmValue::I64(i64::MAX - 1)], &mut no_host).expect("call");
    assert_eq!(r, vec![WasmValue::I64(i64::MAX)]);
    let r = inst.call("half", vec![WasmValue::F64(5.0)], &mut no_host).expect("call");
    assert_eq!(r, vec![WasmValue::F64(2.5)]);
  }

  #[test]
  fn non_function_import_rejected() {
    let wat = r#"(module (import "env" "mem" (memory 1)))"#;
    let err = WasmModule::parse(wat.as_bytes()).map(|_| ()).expect_err("should reject");
    assert!(err.contains("non-function import"), "unexpected error: {err}");
  }
}
