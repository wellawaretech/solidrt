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
}