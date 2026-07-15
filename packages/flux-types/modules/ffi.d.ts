declare module "flux:ffi" {
  /**
   * A scalar ffi value. i32/f32/f64 marshal as number; i64 and ptr marshal as
   * BigInt (an address or i64 does not fit a JS number without precision
   * loss). A number is also accepted where an i64 or ptr is expected.
   */
  type FfiValue = number | bigint
  type FfiTypeName = "i32" | "i64" | "f32" | "f64" | "ptr"

  /** One symbol declaration. `returns` defaults to "void". */
  type SymbolDecl = {
    args: FfiTypeName[]
    returns?: FfiTypeName | "void"
  }

  /** The symbols to resolve at load time, keyed by exported name. */
  type Symbols = Record<string, SymbolDecl>

  /**
   * A JS function backing a minted C function pointer. Called synchronously
   * while a `symbols.*` call is on the stack; its return value is coerced to
   * the callback's declared result type ("void" ignores it). A throw cannot
   * abort the native frame: the callback returns zeroes, the native call runs
   * to completion, and the exception is rethrown after it returns.
   */
  type CallbackFunction = (...args: FfiValue[]) => FfiValue | void

  /**
   * A native shared library. The native counterpart of `flux:wasm`'s Module:
   * declare what you need up front, every declared symbol must resolve.
   *
   * There is NO sandbox: the library runs with full process rights, loading
   * runs its constructors, and a declared signature that does not match the
   * real ABI is undefined behavior. Only load trusted code.
   */
  export class Library {
    /**
     * Load a shared library from bytes (e.g. a bundled binary import; written
     * to a temp file behind the scenes) or a filesystem path, and resolve
     * every declared symbol. A missing symbol throws.
     */
    constructor(source: Uint8Array | ArrayBuffer | string, symbols: Symbols)
    /**
     * The declared symbols as bound JS functions, keyed by name. Destructure
     * once and reuse: the object is rebuilt on each access.
     */
    readonly symbols: Record<string, (...args: FfiValue[]) => FfiValue | undefined>
    /**
     * Mint a C function pointer (returned as a BigInt address) that invokes
     * `func` when the library calls it during a `symbols.*` call. Callbacks
     * may only fire while such a call is on the stack, on the same thread;
     * the pointer stays valid for the lifetime of the library.
     */
    callback(func: CallbackFunction, decl: SymbolDecl): bigint
    /**
     * Copy `len` bytes out of process memory at `ptr`. No bounds checking is
     * possible: a bad pointer is undefined behavior.
     */
    readMemory(ptr: FfiValue, len: number): Uint8Array
    /** Copy `bytes` into process memory at `ptr`. Same caveat as readMemory. */
    writeMemory(ptr: FfiValue, bytes: Uint8Array | ArrayBuffer): void
  }
}
