// Run the real Lua 5.5 cartridge under the generic forge host, from Rust (no
// JS layer). The cartridge is a standalone emcc build whose error unwinding is
// host-mediated: Lua's LUAI_TRY/LUAI_THROW are compiled into `cart_try` /
// `cart_throw` imports, serviced here with call_indirect plus a tagged error
// string. Proves cart_new + cart_eval end to end, including a Lua pcall
// catching an error and a top-level error returning LUA_ERRRUN instead of
// trapping.
use std::cell::RefCell;

use forge::wasm::{ExportInfo, ImportInfo, WasmInstance, WasmModule, WasmValue};

struct Host<'a> {
  inst: &'a WasmInstance,
  imports: Vec<ImportInfo>,
  grid: RefCell<Vec<i32>>,
}

fn i32_arg(args: &[WasmValue], i: usize) -> Result<i32, String> {
  match args.get(i) {
    Some(WasmValue::I32(x)) => Ok(*x),
    other => Err(format!("expected i32 argument at {i}, got {other:?}")),
  }
}

impl Host<'_> {
  fn dispatch(&self, index: usize, args: Vec<WasmValue>) -> Result<Vec<WasmValue>, String> {
    let info = &self.imports[index];
    match (info.module.as_str(), info.name.as_str()) {
      ("env", "host_print") => {
        let (ptr, len) = (i32_arg(&args, 0)?, i32_arg(&args, 1)?);
        let s = self.inst.memory_read(ptr as usize, len as usize)?;
        println!("[lua] {}", String::from_utf8_lossy(&s));
        Ok(vec![])
      }
      ("env", "host_poke") => {
        let (addr, val) = (i32_arg(&args, 0)?, i32_arg(&args, 1)?);
        if let Some(cell) = self.grid.borrow_mut().get_mut(addr as usize) {
          *cell = val;
        }
        Ok(vec![])
      }
      ("env", "host_peek") => {
        let addr = i32_arg(&args, 0)?;
        Ok(vec![WasmValue::I32(self.grid.borrow().get(addr as usize).copied().unwrap_or(0))])
      }
      // The Lua unwind protocol: cart_throw aborts the current activation with
      // a tagged error; the cart_try that owns the tag catches it and resumes
      // its own (outer) frame with 1. A mismatched tag propagates further out.
      ("env", "cart_throw") => {
        let tag = i32_arg(&args, 0)?;
        Err(format!("lua-unwind:{tag}"))
      }
      ("env", "cart_try") => {
        let (f, l, ud, tag) = (i32_arg(&args, 0)?, i32_arg(&args, 1)?, i32_arg(&args, 2)?, i32_arg(&args, 3)?);
        let mut inner = |i: usize, a: Vec<WasmValue>| self.dispatch(i, a);
        match self.inst.call_indirect(f as u32, vec![WasmValue::I32(l), WasmValue::I32(ud)], &mut inner) {
          Ok(_) => Ok(vec![WasmValue::I32(0)]),
          Err(e) if e == format!("lua-unwind:{tag}") => Ok(vec![WasmValue::I32(1)]),
          Err(e) => Err(e),
        }
      }
      ("wasi_snapshot_preview1", "fd_write") => self.fd_write(&args),
      ("wasi_snapshot_preview1", "clock_time_get") => {
        let time_ptr = i32_arg(&args, 2)? as usize;
        let ns =
          std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
        self.inst.memory_write(time_ptr, &ns.to_le_bytes())?;
        Ok(vec![WasmValue::I32(0)])
      }
      ("wasi_snapshot_preview1", "environ_sizes_get") => {
        for i in 0..2 {
          self.inst.memory_write(i32_arg(&args, i)? as usize, &0u32.to_le_bytes())?;
        }
        Ok(vec![WasmValue::I32(0)])
      }
      ("wasi_snapshot_preview1", "environ_get") => Ok(vec![WasmValue::I32(0)]),
      ("wasi_snapshot_preview1", "random_get") => {
        let (buf, len) = (i32_arg(&args, 0)?, i32_arg(&args, 1)?);
        self.inst.memory_write(buf as usize, &vec![4u8; len as usize])?; // chosen by fair dice roll
        Ok(vec![WasmValue::I32(0)])
      }
      ("wasi_snapshot_preview1", "proc_exit") => Err(format!("wasm module called proc_exit({})", i32_arg(&args, 0)?)),
      // Anything else WASI (fd_close, fd_seek, path_*, ...): fail with ENOSYS
      // so libc paths return an error instead of trapping.
      ("wasi_snapshot_preview1", _) => Ok(info.sig.results.iter().map(|_| WasmValue::I32(52)).collect()),
      _ => Err(format!("unhandled import {}.{}", info.module, info.name)),
    }
  }

  /// WASI fd_write: print the iovec bytes (fd 1/2) so stray libc output such
  /// as a Lua panic message is visible, and report all bytes written.
  fn fd_write(&self, args: &[WasmValue]) -> Result<Vec<WasmValue>, String> {
    let iovs = i32_arg(args, 1)? as usize;
    let count = i32_arg(args, 2)? as usize;
    let nwritten = i32_arg(args, 3)? as usize;
    let mut total: u32 = 0;
    for i in 0..count {
      let iov = self.inst.memory_read(iovs + i * 8, 8)?;
      let ptr = u32::from_le_bytes(iov[0..4].try_into().unwrap()) as usize;
      let len = u32::from_le_bytes(iov[4..8].try_into().unwrap());
      let bytes = self.inst.memory_read(ptr, len as usize)?;
      print!("{}", String::from_utf8_lossy(&bytes));
      total = total.saturating_add(len);
    }
    self.inst.memory_write(nwritten, &total.to_le_bytes())?;
    Ok(vec![WasmValue::I32(0)])
  }
}

fn eval(inst: &WasmInstance, host: &Host, script: &[u8]) -> i32 {
  let mut root = |i: usize, a: Vec<WasmValue>| host.dispatch(i, a);
  let ptr = match inst.call("cart_alloc", vec![WasmValue::I32(script.len() as i32)], &mut root).expect("cart_alloc")[0]
  {
    WasmValue::I32(p) => p,
    _ => panic!("cart_alloc did not return a pointer"),
  };
  inst.memory_write(ptr as usize, script).expect("write script into guest memory");
  let rc = inst
    .call("cart_eval", vec![WasmValue::I32(ptr), WasmValue::I32(script.len() as i32)], &mut root)
    .expect("cart_eval");
  inst.call("cart_free", vec![WasmValue::I32(ptr)], &mut root).expect("cart_free");
  match rc[0] {
    WasmValue::I32(code) => code,
    other => panic!("cart_eval returned {other:?}"),
  }
}

fn main() {
  let path = "/home/awel/solidrt/lua64/cartridge/dist/cartridge.wasm";
  let bytes = std::fs::read(path).expect("read cartridge.wasm");
  let module = WasmModule::parse(&bytes).expect("parse");
  for imp in module.imports() {
    println!("import: {}.{}", imp.module, imp.name);
  }
  let inst = module.instantiate().expect("instantiate");
  let host = Host { inst: &inst, imports: module.imports().to_vec(), grid: RefCell::new(vec![0; 65536]) };
  let mut root = |i: usize, a: Vec<WasmValue>| host.dispatch(i, a);

  // Reactor init: run C constructors before any cart_* export.
  for init in ["_initialize", "__wasm_call_ctors"] {
    if inst.exports().iter().any(|(n, e)| n == init && matches!(e, ExportInfo::Func(_))) {
      inst.call(init, vec![], &mut root).expect(init);
      break;
    }
  }

  inst.call("cart_new", vec![], &mut root).expect("cart_new");

  let rc = eval(
    &inst,
    &host,
    br#"
      print("hello from " .. _VERSION)
      poke(10, 42)
      print("peek(10) = " .. peek(10))
      local ok, err = pcall(function() error("boom") end)
      print("pcall ok = " .. tostring(ok) .. ", err = " .. tostring(err))
      assert(not ok)
    "#,
  );
  println!("cart_eval returned {rc} (0 = ok)");
  assert_eq!(rc, 0);

  // A top-level error must come back as LUA_ERRRUN (2), not a trap.
  let rc = eval(&inst, &host, br#"error("bang")"#);
  println!("cart_eval returned {rc} (2 = LUA_ERRRUN)");
  assert_eq!(rc, 2);
}
