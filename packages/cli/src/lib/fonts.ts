// The `solidrt.fonts` map in package.json: what an app says about fonts on
// top of the ones its runtime carries (lattice::Fonts; the stock runtime
// binds the three roles below, a custom runtime whatever its lib.rs says).
// Each key is a registration alias:
//
//   "solidrt": {
//     "fonts": {
//       "sans": "./assets/fonts/Inter.ttf",  // the app's face under sans, replacing the runtime's
//       "serif": true,                       // the runtime's (the same as leaving the key out)
//       "mono": false,                       // drops the runtime's mono: the role resolves like sans
//       "display": "./assets/fonts/F.ttf"    // adds a face, selectable as fontFamily "display"
//     }
//   }
//
// The dev client, `sol render` and a pack apply the same map over the same
// runtime set (collectAssets turns it into the manifest's font annotations),
// and a pack ships only the files the app binds. Pure: the shape rules, with
// no file system, so they are testable on flux (tests/project-fonts.test.ts).

/**
 * The role aliases the stock runtime binds (Noto Sans, Noto Serif, Noto
 * Sans Mono): what a boolean in the map can toggle in a project without a
 * runtime of its own. A custom runtime's set is its lib.rs's, listed by
 * its build beside its binaries (project.ts runtimeFontAliases).
 */
export const STOCK_FONT_ROLES = ["sans", "serif", "mono"]

/** The longest alias the manifest and the trailer carry, in bytes. */
const MAX_ALIAS_BYTES = 255

/**
 * The first thing wrong with `fonts`, the map as package.json has it, or
 * null when it is well-formed: a map of alias to a file path, true or false,
 * where a boolean names a font of the runtime, so it must be one of
 * `runtimeAliases`, the set the runtime binds; null when that set is not
 * known (a custom runtime not built on this host), which leaves the boolean
 * to the runtime (lattice warns on a dropped alias it does not bind).
 */
export function checkFontsMap(fonts: unknown, runtimeAliases: string[] | null): string | null {
  if (typeof fonts !== "object" || fonts === null || Array.isArray(fonts)) {
    return 'The "solidrt.fonts" key in package.json must be a map of alias to font file path (or true/false)'
  }
  for (let [alias, value] of Object.entries(fonts)) {
    if (typeof value !== "boolean" && typeof value !== "string") {
      return `"solidrt.fonts": "${alias}" must be a font file path, true or false, got ${JSON.stringify(value)}`
    }
    if (new TextEncoder().encode(alias).length > MAX_ALIAS_BYTES) {
      return `"solidrt.fonts": alias "${alias}" is too long (max ${MAX_ALIAS_BYTES} bytes)`
    }
    if (typeof value === "boolean" && runtimeAliases !== null && !runtimeAliases.includes(alias)) {
      let binds = runtimeAliases.length ? `only ${runtimeAliases.join("/")}` : "none"
      return `"solidrt.fonts": "${alias}": ${value} toggles a font of the runtime, but it binds ${binds}`
    }
  }
  return null
}

/**
 * The hint for a project runtime that compiles in fonts its app never
 * draws, or null when there are none: each alias the app claims (`claimed`,
 * dropped with `false` or replaced with its own file) whose runtime face is
 * in `runtimeAliases`, the set the project's runtime binds. Those bytes stay
 * in the binary whatever package.json says; only the runtime's lib.rs can
 * leave them out. The stock runtime has no such hint: its set is fixed.
 */
export function compiledInUnusedHint(claimed: string[], runtimeAliases: string[] | null): string | null {
  let unused = claimed.filter((alias) => runtimeAliases?.includes(alias))
  if (unused.length === 0) return null
  let list = unused.join(", ")
  return `The app drops or replaces ${list}, which the runtime still compiles in: delete ${unused.length === 1 ? "its line" : "their lines"} in fonts() of the runtime's lib.rs to leave the bytes out`
}
