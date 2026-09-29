declare module "flux:path" {
  /**
   * Resolves `path` against the trusted base directory `base`, returning the
   * absolute result only if it stays inside `base`; otherwise `null`. Fusing
   * normalization and containment means a `..`-laden or absolute `path` that
   * would escape `base` is rejected rather than silently resolved.
   *
   * Purely lexical: it does not resolve symlinks, so a symlink inside `base`
   * pointing out of it is not caught.
   *
   * @param base  Trusted root directory. Relative values resolve against cwd.
   * @param path  Untrusted path to place within `base`.
   * @returns The contained absolute path, or `null` if it would escape `base`.
   *
   * @example
   * let target = resolveWithin(".", req.params.page)
   * if (!target) return new Response("Not found", { status: 404 })
   */
  export function resolveWithin(base: string, path: string): string | null

  /**
   * Joins and normalizes path `segments`. Lexical only, with no containment
   * guarantee; use `resolveWithin` when a segment is untrusted.
   */
  export function join(...segments: string[]): string

  /**
   * The last component of `path`, trailing separators ignored: Node's
   * `path.basename`. Lexical. `basename("/a/model.gltf")` is "model.gltf",
   * `basename("/a/b/")` is "b".
   *
   * @param ext  A suffix to cut off the result, as in
   *             `basename("/a/model.gltf", ".gltf")`, which is "model". A
   *             name that is nothing but the suffix keeps it.
   */
  export function basename(path: string, ext?: string): string

  /**
   * `path` without its last component: Node's `path.dirname`. Lexical.
   * `dirname("/a/b.txt")` is "/a", and a bare name's is ".".
   */
  export function dirname(path: string): string

  /**
   * The extension of `path`'s last component, from its last dot on, or ""
   * when it has none: Node's `path.extname`. `extname("a.tar.gz")` is ".gz";
   * a leading dot names a hidden file, so `extname(".bashrc")` is "".
   */
  export function extname(path: string): string
}
