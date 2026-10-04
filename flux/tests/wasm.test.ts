// The JS surface of flux:wasm, focused on what the plugin adds over the
// forge core (which has its own tests): the `instance.memory` ArrayBuffer
// view with its detach-on-grow contract, and the JS-visible error
// messages. Modules are embedded as wat text (forge enables wasmi's `wat`
// feature).
import { expect, test } from "flux:test"
import { Module } from "flux:wasm"

let wat = (text: string) => new Module(new TextEncoder().encode(text))

test("memory is a view aliasing linear memory", () => {
  let instance = wat(`(module
    (memory (export "memory") 1)
    (func (export "peek") (param i32) (result i32) local.get 0 i32.load8_u))`).instantiate({})
  let mem = instance.memory!
  expect(mem.byteLength).toBe(65536)
  instance.writeMemory(64, new Uint8Array([1, 2, 3]))
  let view = new Uint8Array(mem)
  expect([view[64], view[65], view[66]]).toEqual([1, 2, 3])
  view[65] = 42
  expect(instance.call("peek", 65)).toBe(42)
  expect(instance.memory === mem).toBe(true)
})

test("an overlapping writeMemory is staged", () => {
  let instance = wat(`(module
    (memory (export "memory") 1))`).instantiate({})
  instance.writeMemory(64, new Uint8Array([1, 2, 3]))
  // The source is a view over this instance's own memory; the copy must be
  // staged, not aliased.
  instance.writeMemory(65, new Uint8Array(instance.memory!, 64, 2))
  let view = new Uint8Array(instance.memory!)
  expect([view[64], view[65], view[66]]).toEqual([1, 1, 2])
})

test("growth detaches the view", () => {
  let instance = wat(`(module
    (memory (export "memory") 1 4)
    (func (export "grow") (result i32) i32.const 1 memory.grow))`).instantiate({})
  let before = instance.memory!
  instance.call("grow")
  expect(() => new Uint8Array(before)).toThrow()
  expect(instance.memory!.byteLength).toBe(131072)
})

test("mid-call growth detaches before the host dispatch", () => {
  let before: ArrayBuffer
  let seen = "handler not called"
  let instance = wat(`(module
    (import "env" "check" (func $check))
    (memory (export "memory") 1 4)
    (func (export "run") i32.const 1 memory.grow drop call $check))`).instantiate({
    env: {
      check: () => {
        try {
          new Uint8Array(before)
          seen = "stale view alive"
        } catch {
          seen = "detached before handler"
        }
      },
    },
  })
  before = instance.memory!
  instance.call("run")
  expect(seen).toBe("detached before handler")
})

test("memory is undefined without the export", () => {
  let instance = wat(`(module (func (export "noop")))`).instantiate({})
  expect(instance.memory).toBe(undefined)
})

test("call errors name the target and its signature", () => {
  let instance = wat(`(module
    (func (export "add") (param i32 i32) (result i32) local.get 0 local.get 1 i32.add)
    (table 1 funcref)
    (func $wrong (param f64))
    (elem (i32.const 0) $wrong)
    (type $expected (func (param i32)))
    (func (export "bad") i32.const 7 i32.const 0 call_indirect (type $expected)))`).instantiate({})
  expect(() => instance.call("add", 1)).toThrow(/^add \(i32, i32\) -> i32 expects 2 argument\(s\), got 1$/)
  // @ts-expect-error a string is no wasm value; the call has to say so at runtime too
  expect(() => instance.call("add", 1, "x")).toThrow(/^add \(i32, i32\) -> i32: argument 1: expected a number \(i32\)$/)
  expect(() => instance.call("bad")).toThrow("wasm call to bad failed")
  expect(() => instance.call("bad")).toThrow("stale function pointer")
})

// A precompiled binary rather than wat: the 41-byte module of
// examples/wasm-call (one export, add(i32, i32) -> i32), section by section.
const ADD_WASM = new Uint8Array([
  0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // magic, version 1
  0x01, 0x07, 0x01, 0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7f, // type 0: (i32, i32) -> i32
  0x03, 0x02, 0x01, 0x00, // one function, of type 0
  0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x00, // export "add" = function 0
  0x0a, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b, // body: local.get 0, local.get 1, i32.add
])

test("loads a precompiled binary and calls its export", () => {
  let mod = new Module(ADD_WASM)
  expect(mod.imports).toEqual([])
  let instance = mod.instantiate({})
  expect(instance.exports).toEqual([{ name: "add", kind: "function", params: ["i32", "i32"], results: ["i32"] }])
  expect(instance.call("add", 2, 3)).toBe(5)
  expect(instance.call("add", 0x7fffffff, 1)).toBe(-2147483648)
})
