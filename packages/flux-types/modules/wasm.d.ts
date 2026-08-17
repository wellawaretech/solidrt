// There is no `WebAssembly` global in flux; this module is the entire wasm
// surface. Modules run in a pure interpreter (wasmi, no JIT), so this is a
// portability tool - one compiled module runs on every flux target with no
// native binaries or dlopen - not a speed tool. Tight typed compute runs
// somewhat faster than the same loop in JavaScript (a small constant
// factor, nowhere near browser wasm speed), and every host call costs
// extra marshalling, so call-heavy code can end up slower.
// Imports must be scalar-signature functions only (no imported
// memory, globals or tables), which constrains the toolchain on the other
// side: default emscripten output imports its memory and is rejected, while
// `emcc -sSTANDALONE_WASM=1 --no-entry` produces a module that fits.
declare module "flux:wasm" {
  /**
   * A scalar wasm value. i32/f32/f64 marshal as number; i64 marshals as BigInt
   * (an i64 does not fit a JS number without precision loss). A number is also
   * accepted where an i64 is expected.
   */
  type WasmValue = number | bigint
  type WasmTypeName = "i32" | "i64" | "f32" | "f64"

  /** A function import a module requires, `{ module, name }`-keyed like the standard. */
  type ImportInfo = {
    module: string
    name: string
    params: WasmTypeName[]
    results: WasmTypeName[]
  }

  /**
   * An instance export. `params`/`results` are present only for functions with
   * all-scalar signatures.
   */
  type ExportInfo =
    | { name: string; kind: "function"; params: WasmTypeName[]; results: WasmTypeName[] }
    | { name: string; kind: "memory" | "other" }

  /**
   * A host function backing a guest import. Called synchronously during an
   * export call; must return values matching the import's declared results:
   * nothing for zero results, a single value for one, an array for several.
   * A throw aborts the wasm call and propagates. May re-enter the instance
   * (e.g. via {@link Instance.callIndirect}).
   */
  type HostFunction = (...args: WasmValue[]) => WasmValue | WasmValue[] | void

  /** Host functions keyed by import module then name, e.g. `{ env: { mul } }`. */
  type Imports = Record<string, Record<string, HostFunction>>

  export class Module {
    /**
     * Parse and validate a wasm binary (wat text bytes are also accepted).
     * Throws on invalid input or on an unsupported import (non-function, or
     * non-scalar signature) - see the module note above for the emscripten
     * flags that produce a compatible binary.
     */
    constructor(bytes: Uint8Array | ArrayBuffer)
    /**
     * The function imports this module requires, in the order the host
     * functions are indexed.
     */
    readonly imports: ImportInfo[]
    /**
     * Instantiate with host functions. Every listed import must resolve to a
     * function; a missing or non-function entry throws.
     */
    instantiate(imports: Imports): Instance
  }

  /** An instantiated module. Created with {@link Module.instantiate}. */
  export class Instance {
    /** The module's exports. */
    readonly exports: ExportInfo[]
    /**
     * Call an exported function. Arguments are coerced to the export's declared
     * parameter types. Returns `undefined` for no results, the single value for
     * one, or an array for several. Host imports hit during the call dispatch
     * to the functions passed to `instantiate`; a throw from one aborts the
     * call.
     */
    call(name: string, ...args: WasmValue[]): WasmValue | WasmValue[] | undefined
    /**
     * Call a function by its index in the module's exported function table:
     * `table[index](...args)`. This is how a host function invokes a guest
     * function pointer it received as an integer (e.g. a C callback). Same
     * coercion and host-import dispatch rules as {@link call}; safe to use
     * from within a host function (re-entrant).
     */
    callIndirect(index: number, ...args: WasmValue[]): WasmValue | WasmValue[] | undefined
    /**
     * The exported linear memory as an `ArrayBuffer` aliasing the instance's
     * live bytes, or `undefined` if the module exports no memory. Reads and
     * writes go straight to guest memory - no copy; a `Uint8Array` over it
     * (`new Uint8Array(instance.memory, ptr, len)`) is the zero-copy way to
     * hand guest bytes to e.g. `uploadTexture`. Follows the web's
     * `WebAssembly.Memory.buffer` contract: the buffer stays valid until the
     * guest grows its memory, which detaches it; read `memory` again for a
     * fresh buffer over the moved storage.
     */
    readonly memory: ArrayBuffer | undefined
    /**
     * Copy `len` bytes out of the exported memory at `ptr`, as a fresh
     * `Uint8Array`. One-shot convenience; for repeated or large reads use
     * {@link memory} directly.
     */
    readMemory(ptr: number, len: number): Uint8Array
    /**
     * Copy `bytes` into the exported memory at `ptr`. The source may itself
     * be a view over this instance's memory; overlapping ranges copy
     * correctly.
     */
    writeMemory(ptr: number, bytes: Uint8Array | ArrayBuffer): void
  }
}
