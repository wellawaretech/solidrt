declare module "flux:ffi" {
  /**
   * A scalar ffi value. i32/f32/f64 marshal as number; i64 and ptr marshal as
   * BigInt (an address or i64 does not fit a JS number without precision
   * loss). A number is also accepted where an i64 or ptr is expected.
   */
  type FfiValue = number | bigint
  /**
   * What a `symbols.*` call accepts per argument. Where a `ptr` is declared,
   * an ArrayBuffer or typed array may be passed instead of an address: its
   * data pointer (view offset respected) is handed to the native call and the
   * buffer stays valid for the call's duration, so native code may read from
   * and write into it (out-parameters, result buffers). Do not keep such an
   * address past the call. A detached buffer throws.
   */
  type FfiArg = FfiValue | ArrayBuffer | ArrayBufferView
  type FfiTypeName = "i32" | "i64" | "f32" | "f64" | "ptr"

  /**
   * One symbol declaration. `returns` defaults to "void". A missing symbol
   * fails the load unless `optional` is set, in which case its `symbols`
   * entry is `undefined` (test for it before calling).
   */
  type SymbolDecl = {
    args: FfiTypeName[]
    returns?: FfiTypeName | "void"
    optional?: boolean
  }

  /** The symbols to resolve at load time, keyed by exported name. */
  type Symbols = Record<string, SymbolDecl>

  /**
   * A JS function backing a minted C function pointer. Called synchronously
   * while a `symbols.*` call is on the stack; its return value is coerced to
   * the callback's declared result type ("void" ignores it). Unlike a call
   * argument, a callback may not return a buffer as a ptr (the address would
   * dangle once the callback returns). A throw cannot
   * abort the native frame: the callback returns zeroes, the native call runs
   * to completion, and the exception is rethrown after it returns.
   */
  type CallbackFunction = (...args: FfiValue[]) => FfiValue | void

  /**
   * A native shared library. The native counterpart of `flux:wasm`'s Module:
   * declare what you need up front, every declared symbol must resolve
   * (unless marked optional).
   *
   * There is NO sandbox: the library runs with full process rights, loading
   * runs its constructors, and a declared signature that does not match the
   * real ABI is undefined behavior. Only load trusted code.
   */
  export class Library {
    /**
     * Load a shared library from bytes (e.g. a bundled binary import; written
     * to a temp file behind the scenes) or a filesystem path, and resolve
     * every declared symbol. A missing symbol throws unless declared
     * `optional`.
     */
    constructor(source: Uint8Array | ArrayBuffer | string, symbols: Symbols)
    /**
     * The declared symbols as bound JS functions, keyed by name. Destructure
     * once and reuse: the object is rebuilt on each access.
     */
    readonly symbols: Record<string, (...args: FfiArg[]) => FfiValue | undefined>
    /**
     * Mint a C function pointer (returned as a BigInt address) that invokes
     * `func` when the library calls it during a `symbols.*` call. Callbacks
     * may only fire while such a call is on the stack, on the same thread;
     * the pointer stays valid for the lifetime of the library.
     */
    callback(func: CallbackFunction, decl: SymbolDecl): bigint
    /**
     * Copy `count` elements out of process memory at `ptr`, in native byte
     * order. Without `type` (or with "u8") that is `count` bytes as a
     * Uint8Array; with an ffi type name it is `count` elements as the matching
     * typed array. No bounds checking is possible: a bad pointer is undefined
     * behavior.
     */
    readMemory(ptr: FfiValue, count: number, type?: "u8"): Uint8Array
    readMemory(ptr: FfiValue, count: number, type: "i32"): Int32Array
    readMemory(ptr: FfiValue, count: number, type: "i64"): BigInt64Array
    readMemory(ptr: FfiValue, count: number, type: "f32"): Float32Array
    readMemory(ptr: FfiValue, count: number, type: "f64"): Float64Array
    readMemory(ptr: FfiValue, count: number, type: "ptr"): BigUint64Array
    /**
     * Copy the bytes of a typed array or ArrayBuffer into process memory at
     * `ptr`. Same caveat as readMemory.
     */
    writeMemory(ptr: FfiValue, bytes: ArrayBufferView | ArrayBuffer): void
  }
}
