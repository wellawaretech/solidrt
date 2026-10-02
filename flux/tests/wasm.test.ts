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
