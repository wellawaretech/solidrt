// What the bundle-cli entry (src/entries/bundle-cli.ts, a bun subprocess)
// writes to stdout as one JSON object and the dev server (server/rebuild.ts)
// reads back. See shared/config.ts for the folder.

export type BundleOutput = {
  code: string
  /** Composed sourcemap JSON (bundle -> original .tsx sources), dev builds only. */
  map: string | null
  /** Version manifest JSON for this bundle; clients install pushes under its hash. */
  manifest: string
  /** The app's isolate bundles, one per "use isolate" module, in id order; maps dev builds only. */
  isolates: { id: string; code: string; map: string | null }[]
}
