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
    /** Read and parse the file as JSON. */
    json(): Promise<any>
    /** Resolve to whether the file exists. */
    exists(): Promise<boolean>
    /** Resolve to the file's metadata (size, type, mtime). */
    stat(): Promise<FileStat>
    /** Write `data`, replacing any existing contents. */
    write(data: string | Uint8Array): Promise<void>
  }

  type FluxDir = {
    path: string
    /** List the directory's immediate entries (non-recursive). */
    entries(): Promise<DirEntry[]>
    /** Resolve to whether the directory exists. */
    exists(): Promise<boolean>
  }

  /**
   * Reference a file by path. Lazy: no I/O happens until a method is called.
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