declare module "flux:fs" {
  type DirEntry = {
    name: string
    type: "file" | "directory" | "symlink" | "other"
  }

  type FileStat = {
    size: number
    type: string
    mtime?: number
  }

  type FluxFile = {
    path: string
    /** Read the whole file as UTF-8 text. */
    text(): Promise<string>
    /** Read the whole file as raw bytes. */
    bytes(): Promise<Uint8Array>
    /** Read the whole file as an ArrayBuffer. */
    arrayBuffer(): Promise<ArrayBuffer>
    /** Read and parse the file as JSON. */
    json(): Promise<any>
    /** Resolve to whether the file exists. */
    exists(): Promise<boolean>
    /** Resolve to the file's metadata (size, type, mtime). */
    stat(): Promise<FileStat>
    /**
     * Read exactly `length` bytes starting at byte `offset`. A range extending
     * past end-of-file rejects rather than short-reading; clamp against
     * `stat()` size first.
     */
    read(offset: number, length: number): Promise<Uint8Array>
    /** Write `data`, replacing any existing contents. */
    write(data: string | Uint8Array): Promise<void>
    /** Append `data` to the end of the file, creating it if missing. */
    append(data: string | Uint8Array): Promise<void>
  }

  type FluxDir = {
    path: string
    /** List the directory's immediate entries (non-recursive). */
    entries(): Promise<DirEntry[]>
    /** Resolve to whether the directory exists. */
    exists(): Promise<boolean>
    /**
     * Create the directory, including any missing parents. Succeeds if it
     * already exists.
     */
    create(): Promise<void>
  }

  /**
   * Reference a file by path. Lazy: no I/O happens until a method is called.
   *
   * Relative paths resolve against the process cwd. Exception: in a SolidRT
   * app running an installed version, paths under `assets/` resolve read-only
   * into that version's immutable assets tree (writes there error).
   *
   * @param path  Path to the file.
   */
  export function file(path: string): FluxFile
  /**
   * Reference a directory by path. Lazy: no I/O happens until a method is
   * called.
   *
   * @param path  Path to the directory.
   */
  export function dir(path: string): FluxDir
}