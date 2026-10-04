use crate::wasm::*;

/// This build's engine: the native lane with `wasm-native`, wasmi without.
/// Every test runs on whichever it is, so both backends meet the same
/// contract across the two CI configurations.
fn engine() -> WasmEngine {
  WasmEngine::new().expect("wasm engine")
}

fn no_host(_: usize, _: Vec<WasmValue>) -> Result<Vec<WasmValue>, String> {
  Err("unexpected host call".to_string())
}

#[test]
fn lane_matches_the_build() {
  #[cfg(feature = "wasm-native")]
  assert_eq!(engine().lane(), Lane::Native);
  #[cfg(not(feature = "wasm-native"))]
  assert_eq!(engine().lane(), Lane::Interpreter);
}

#[test]
fn plain_export_call() {
  let wat = r#"(module (func (export "add") (param i32 i32) (result i32)
    local.get 0 local.get 1 i32.add))"#;
  let inst = engine().parse(wat.as_bytes()).expect("parse").instantiate().expect("instantiate");
  let r = inst.call("add", vec![WasmValue::I32(3), WasmValue::I32(4)], &mut no_host).expect("call");
  assert_eq!(r, vec![WasmValue::I32(7)]);
}

#[test]
fn import_roundtrip() {
  let wat = r#"(module
    (import "env" "mul" (func $mul (param i32 i32) (result i32)))
    (func (export "run") (param i32) (result i32)
      local.get 0 i32.const 10 call $mul))"#;
  let module = engine().parse(wat.as_bytes()).expect("parse");
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
  let inst = engine().parse(wat.as_bytes()).expect("parse").instantiate().expect("instantiate");
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
fn memory_data_ptr_tracks_growth() {
  let wat = r#"(module
    (memory (export "memory") 1 4)
    (func (export "grow") (result i32) i32.const 1 memory.grow))"#;
  let inst = engine().parse(wat.as_bytes()).expect("parse").instantiate().expect("instantiate");
  let (ptr, len) = inst.memory_data_ptr().expect("memory exported");
  assert!(!ptr.is_null());
  assert_eq!(len, 65536);
  inst.call("grow", vec![], &mut no_host).expect("grow");
  let (_, len) = inst.memory_data_ptr().expect("memory exported");
  assert_eq!(len, 2 * 65536, "length reflects growth");

  let no_mem = engine().parse(br#"(module)"#).expect("parse").instantiate().expect("instantiate");
  assert!(no_mem.memory_data_ptr().is_none());
}

#[test]
fn memory_read_write() {
  let wat = r#"(module
    (memory (export "memory") 1)
    (func (export "peek") (param i32) (result i32) local.get 0 i32.load8_u))"#;
  let inst = engine().parse(wat.as_bytes()).expect("parse").instantiate().expect("instantiate");
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
  let inst = engine().parse(wat.as_bytes()).expect("parse").instantiate().expect("instantiate");
  let err = inst.call("boom", vec![], &mut no_host).expect_err("should trap");
  assert!(err.contains("wasm call to boom failed"), "unexpected error: {err}");
  assert!(err.contains("unreachable"), "unexpected error: {err}");
}

#[test]
fn indirect_mismatch_names_target_and_hints() {
  // The guest's own call_indirect hits a table entry with the wrong signature
  // (the call site expects (param i32), the entry takes f64). Neither engine
  // exposes the failing index, so the message names the outer call and points
  // at the stale-function-pointer cause.
  let wat = r#"(module
    (table 1 funcref)
    (func $wrong (param f64))
    (elem (i32.const 0) $wrong)
    (type $expected (func (param i32)))
    (func (export "run")
      i32.const 7 i32.const 0 call_indirect (type $expected)))"#;
  let inst = engine().parse(wat.as_bytes()).expect("parse").instantiate().expect("instantiate");
  let err = inst.call("run", vec![], &mut no_host).expect_err("should mismatch");
  assert!(err.contains("wasm call to run failed"), "unexpected error: {err}");
  assert!(err.contains("indirect call type mismatch"), "unexpected error: {err}");
  assert!(err.contains("stale function pointer"), "unexpected error: {err}");
}

#[test]
fn host_side_indirect_failure_names_index_and_sig() {
  // A trap inside a host-driven call_indirect names the table slot and its
  // signature.
  let wat = r#"(module
    (table (export "__indirect_function_table") 1 funcref)
    (func $boom (param i32) (result i32) unreachable)
    (elem (i32.const 0) $boom))"#;
  let inst = engine().parse(wat.as_bytes()).expect("parse").instantiate().expect("instantiate");
  let err = inst.call_indirect(0, vec![WasmValue::I32(1)], &mut no_host).expect_err("should trap");
  assert!(err.contains("wasm call to table[0] (i32) -> i32 failed"), "unexpected error: {err}");
}

#[test]
fn sig_display() {
  let sig = FuncSig { params: vec![WasmType::I32, WasmType::F64], results: vec![WasmType::I64] };
  assert_eq!(sig.to_string(), "(i32, f64) -> i64");
  let none = FuncSig { params: vec![], results: vec![] };
  assert_eq!(none.to_string(), "()");
  let multi = FuncSig { params: vec![WasmType::I32], results: vec![WasmType::I32, WasmType::I32] };
  assert_eq!(multi.to_string(), "(i32) -> (i32, i32)");
}

#[test]
fn host_error_aborts_call() {
  let wat = r#"(module
    (import "env" "fail" (func $fail))
    (func (export "run") call $fail))"#;
  let inst = engine().parse(wat.as_bytes()).expect("parse").instantiate().expect("instantiate");
  let mut host = |_: usize, _: Vec<WasmValue>| Err("handler failed".to_string());
  let err = inst.call("run", vec![], &mut host).expect_err("should fail");
  assert_eq!(err, "handler failed");
}

#[test]
fn i64_and_f64_values() {
  let wat = r#"(module
    (func (export "big") (param i64) (result i64) local.get 0 i64.const 1 i64.add)
    (func (export "half") (param f64) (result f64) local.get 0 f64.const 2 f64.div))"#;
  let inst = engine().parse(wat.as_bytes()).expect("parse").instantiate().expect("instantiate");
  let r = inst.call("big", vec![WasmValue::I64(i64::MAX - 1)], &mut no_host).expect("call");
  assert_eq!(r, vec![WasmValue::I64(i64::MAX)]);
  let r = inst.call("half", vec![WasmValue::F64(5.0)], &mut no_host).expect("call");
  assert_eq!(r, vec![WasmValue::F64(2.5)]);
}

#[test]
fn non_function_import_rejected() {
  let wat = r#"(module (import "env" "mem" (memory 1)))"#;
  let err = engine().parse(wat.as_bytes()).map(|_| ()).expect_err("should reject");
  assert!(err.contains("non-function import"), "unexpected error: {err}");
}

#[test]
fn call_indirect_via_exported_table() {
  let wat = r#"(module
    (table (export "__indirect_function_table") 2 funcref)
    (func $inc (param i32) (result i32) local.get 0 i32.const 1 i32.add)
    (elem (i32.const 1) $inc))"#;
  let inst = engine().parse(wat.as_bytes()).expect("parse").instantiate().expect("instantiate");
  let sig = inst.table_func_sig(1).expect("sig");
  assert_eq!(sig.params, vec![WasmType::I32]);
  assert_eq!(sig.results, vec![WasmType::I32]);
  let r = inst.call_indirect(1, vec![WasmValue::I32(41)], &mut no_host).expect("call_indirect");
  assert_eq!(r, vec![WasmValue::I32(42)]);
  let err = inst.call_indirect(0, vec![], &mut no_host).expect_err("null entry");
  assert!(err.contains("null function pointer"), "unexpected error: {err}");
  let err = inst.call_indirect(9, vec![], &mut no_host).expect_err("out of range");
  assert!(err.contains("out of range"), "unexpected error: {err}");
}

#[test]
fn unwind_through_nested_activation() {
  // The guest protected-call pattern (how a longjmp-free Lua build unwinds):
  // `run` asks the host to `try` the function at a table slot; that function
  // `throw`s back through the host. The `try` handler re-enters via
  // call_indirect, sees the inner activation abort with the matching tag,
  // and resumes the outer frame with a "caught" flag.
  let wat = r#"(module
    (import "env" "try" (func $try (param i32 i32) (result i32)))
    (import "env" "throw" (func $throw (param i32)))
    (table (export "__indirect_function_table") 1 funcref)
    (func $boom (param i32) local.get 0 call $throw)
    (elem (i32.const 0) $boom)
    (func (export "run") (param i32) (result i32)
      i32.const 0 local.get 0 call $try))"#;
  let module = engine().parse(wat.as_bytes()).expect("parse");
  let try_index = module.imports().iter().position(|i| i.name == "try").expect("try import");
  let throw_index = module.imports().iter().position(|i| i.name == "throw").expect("throw import");
  let inst = module.instantiate().expect("instantiate");
  let inst_ref = &inst;
  let mut host = move |index: usize, args: Vec<WasmValue>| -> Result<Vec<WasmValue>, String> {
    assert_eq!(index, try_index, "outer activation only calls try");
    let (WasmValue::I32(slot), WasmValue::I32(tag)) = (args[0], args[1]) else {
      return Err("bad try args".to_string());
    };
    let mut inner = |i: usize, a: Vec<WasmValue>| -> Result<Vec<WasmValue>, String> {
      assert_eq!(i, throw_index, "inner activation only calls throw");
      let WasmValue::I32(t) = a[0] else { return Err("bad throw arg".to_string()) };
      Err(format!("unwind:{t}"))
    };
    match inst_ref.call_indirect(slot as u32, vec![WasmValue::I32(tag)], &mut inner) {
      Ok(_) => Ok(vec![WasmValue::I32(0)]),
      Err(e) if e == format!("unwind:{tag}") => Ok(vec![WasmValue::I32(1)]),
      Err(e) => Err(e),
    }
  };
  let r = inst.call("run", vec![WasmValue::I32(7)], &mut host).expect("run");
  assert_eq!(r, vec![WasmValue::I32(1)], "unwind caught, outer frame resumed");
  // The store survived the discarded inner activation: run it again.
  let r = inst.call("run", vec![WasmValue::I32(9)], &mut host).expect("run again");
  assert_eq!(r, vec![WasmValue::I32(1)]);
}

fn temp_cache_dir(tag: &str) -> std::path::PathBuf {
  let dir = std::env::temp_dir().join(format!("forge-wasm-cache-{}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  dir
}

/// Every cached artifact under `dir`, sorted: `<dir>/wasm/<engine>/<hash>.cwasm`.
#[cfg(feature = "wasm-native")]
fn cached_artifacts(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
  let mut found = Vec::new();
  if let Ok(engines) = std::fs::read_dir(dir.join("wasm")) {
    for engine in engines.flatten() {
      if let Ok(artifacts) = std::fs::read_dir(engine.path()) {
        found.extend(artifacts.flatten().map(|a| a.path()).filter(|p| p.extension().is_some_and(|e| e == "cwasm")));
      }
    }
  }
  found.sort();
  found
}

#[cfg(not(feature = "wasm-native"))]
#[test]
fn interpreter_never_touches_the_cache_dir() {
  let dir = temp_cache_dir("interp");
  let wat = br#"(module (func (export "one") (result i32) i32.const 1))"#;
  let engine = engine();
  engine.parse_cached(wat, Some(&dir)).expect("parse");
  engine.precompile(wat, &dir).expect("precompile");
  assert!(!dir.exists(), "the interpreter has no artifact to cache");
}

#[cfg(feature = "wasm-native")]
#[test]
fn native_cache_round_trip() {
  use std::collections::HashSet;

  let dir = temp_cache_dir("native");
  let engine = engine();
  let wat = br#"(module (func (export "one") (result i32) i32.const 1))"#;
  let run = |module: WasmModule| {
    let inst = module.instantiate().expect("instantiate");
    inst.call("one", vec![], &mut no_host).expect("call")
  };

  // Miss: compiles and stores one artifact. Hit: loads and runs the same.
  assert_eq!(run(engine.parse_cached(wat, Some(&dir)).expect("parse")), vec![WasmValue::I32(1)]);
  let artifacts = cached_artifacts(&dir);
  assert_eq!(artifacts.len(), 1, "one artifact stored");
  let artifact = artifacts[0].clone();
  let compiled_len = std::fs::metadata(&artifact).expect("artifact").len();
  assert_eq!(run(engine.parse_cached(wat, Some(&dir)).expect("parse from cache")), vec![WasmValue::I32(1)]);

  // A damaged artifact is recompiled and rewritten, and the module still runs.
  std::fs::write(&artifact, b"not an artifact").expect("damage");
  assert_eq!(run(engine.parse_cached(wat, Some(&dir)).expect("parse past damage")), vec![WasmValue::I32(1)]);
  assert_eq!(std::fs::metadata(&artifact).expect("rewritten").len(), compiled_len, "artifact rewritten");

  // Precompile fills a slot without a module; prune keeps what is named.
  let other = br#"(module (func (export "two") (result i32) i32.const 2))"#;
  engine.precompile(other, &dir).expect("precompile");
  assert_eq!(cached_artifacts(&dir).len(), 2);
  let kept_hash = artifact.file_stem().and_then(|s| s.to_str()).expect("hash stem").to_string();
  let keep: HashSet<String> = [kept_hash].into_iter().collect();
  let stale_engine = dir.join("wasm").join("0123456789abcdef0123456789abcdef");
  std::fs::create_dir_all(&stale_engine).expect("stale engine dir");
  std::fs::write(stale_engine.join("deadbeef.cwasm"), b"old").expect("stale artifact");
  engine.prune_cache(&dir, &keep);
  assert_eq!(cached_artifacts(&dir), vec![artifact], "only the kept artifact survives");
  assert!(!stale_engine.exists(), "another engine's directory goes wholesale");

  // No cache dir: nothing is written anywhere.
  let bare = temp_cache_dir("bare");
  engine.parse_cached(wat, None).expect("parse without cache");
  assert!(!bare.exists());
  let _ = std::fs::remove_dir_all(&dir);
}

/// A panic in a host handler must not leave the instance holding stale
/// pointers: the next call after the unwind has to work like any other.
#[test]
fn panicking_handler_leaves_the_instance_usable() {
  let wat = r#"(module
    (import "env" "host" (func $host (param i32) (result i32)))
    (func (export "run") (param i32) (result i32) local.get 0 call $host)
    (func (export "plain") (result i32) i32.const 9))"#;
  let inst = engine().parse(wat.as_bytes()).expect("parse").instantiate().expect("instantiate");
  let mut boom = |_: usize, _: Vec<WasmValue>| -> Result<Vec<WasmValue>, String> { panic!("handler panicked") };
  let unwound =
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| inst.call("run", vec![WasmValue::I32(1)], &mut boom)));
  assert!(unwound.is_err(), "the panic propagates to the caller");
  let mut host = |_: usize, args: Vec<WasmValue>| Ok(args);
  assert_eq!(inst.call("run", vec![WasmValue::I32(4)], &mut host).expect("call after unwind"), vec![WasmValue::I32(4)]);
  assert_eq!(inst.call("plain", vec![], &mut no_host).expect("plain call"), vec![WasmValue::I32(9)]);
}
