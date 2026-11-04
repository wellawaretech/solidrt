// The application id rename (patchManifest calls it), in a module of its own
// so flux:test can load it: strings.ts works on node Buffers, which flux:test
// bundles do not have.

// The pool rewrites that move the runner's application id to the app's:
// the id itself (the package, the app's own link scheme and the share
// provider's authority, spelled identically so the pool holds them once)
// and every name a library's manifest derives from it with
// `${applicationId}.<suffix>` (androidx's startup provider authority, its
// receiver permission): left at the runner's, two packed apps would declare
// the same provider or permission and the second refuses to install.
export function applicationIdRenames(pool: string[], from: string, to: string): Map<number, string> {
  let renames = new Map<number, string>()
  pool.forEach((s, i) => {
    if (s === from) renames.set(i, to)
    else if (s.startsWith(from + ".")) renames.set(i, to + s.slice(from.length))
  })
  return renames
}
