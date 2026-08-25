// What `srt bundle --json` (src/bundle/main.ts, a bun subprocess of the
// dev server) writes to stdout as one JSON object and the dev server
// (src/server/rebuild.ts) reads back. src/types/ holds the type-only contracts
// between the two programs: each has its own tsconfig (bun types on one
// side, flux types on the other), both include this folder, so nothing here
// may reference either runtime.

export type BundleOutput = {
  code: string
  /** Composed sourcemap JSON (bundle -> original .tsx sources), dev builds only. */
  map: string | null
  /** Version manifest JSON for this bundle; clients install pushes under its hash. */
  manifest: string
  /** The app's isolate bundles, one per "use isolate" module, in id order; maps dev builds only. */
  isolates: { id: string; code: string; map: string | null }[]
  /**
   * Every file this bundle was built from, absolute and sorted: the app's
   * modules, the dependency modules bundled in, files inlined by import
   * attribute, and the project's package.json / tsconfig.json. The assets/
   * tree is not listed (the manifest carries it). What the dev server
   * watches for reload-on-save.
   */
  inputs: string[]
}
