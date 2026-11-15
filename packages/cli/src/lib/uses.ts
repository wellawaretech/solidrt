// The `solidrt.uses` key in package.json: what the app declares it uses, in
// platform-neutral names:
//
//   "solidrt": {
//     "uses": { "camera": true, "microphone": true, "haptics": true, "internet": true }
//   }
//
// Each packer maps a name to its platform's own declaration (an Android
// permission, an Apple usage description or entitlement, a Windows
// capability, a Flatpak finish-arg; pack/main.ts holds the Android column of
// the table in pack/docs.md). All are on by default, so a packed app can do
// what it did in the player; the scaffold lists them all so switching one off
// is a one-line edit. `false` drops a name: no packer declares it, and the
// manifest carries the declared set (manifestUses in project.ts), so the
// runtime leaves the name out of `Flux.capabilities` and the app sees the
// same on every platform. Pure: the rules, with no file system, so they are
// testable on flux (tests/project-uses.test.ts).

/** The declaration vocabulary; an unknown name fails. */
export const USES = ["camera", "microphone", "haptics", "internet"] as const
export type Use = (typeof USES)[number]

/** The `uses` map: a name to true/false; an absent name is on. */
export type UsesMap = Partial<Record<Use, boolean>>

/** The names a project declares it uses: every one not switched off. */
export function resolveUses(config: { uses?: UsesMap }): Use[] {
  return USES.filter((name) => config.uses?.[name] !== false)
}

/**
 * The manifest's `uses` entry: the declared set as a sorted array, so the
 * manifest's bytes (the version id) do not depend on the order package.json
 * lists the names in. Empty with no project (a file served on its own),
 * which the runtime reads as "everything is used".
 */
export function usesEntry(config: { uses?: UsesMap } | null): { uses?: Use[] } {
  return config ? { uses: resolveUses(config).sort() } : {}
}

/**
 * The first thing wrong with the declaration in `config`, the raw `solidrt`
 * object as package.json has it, or null when it is well-formed. Runs before
 * the unknown-key check, so the old key name gets the new one, not only an
 * "unknown key". A null `uses` is unset and passes.
 */
export function checkUses(config: Record<string, unknown>): string | null {
  // The key was named "capabilities" before; a config that still says so is
  // told the new name.
  if ("capabilities" in config) return '"solidrt": "capabilities" is now named "uses"; rename the key'
  if (!("uses" in config) || config.uses === null) return null
  let map = config.uses
  if (typeof map !== "object" || Array.isArray(map)) {
    return '"solidrt": "uses" must be a map of name to true/false'
  }
  for (let [name, value] of Object.entries(map as Record<string, unknown>)) {
    // The name "haptics" was "vibration" before; a config that still says so
    // is told the new name, not only the list.
    if (name === "vibration") return '"solidrt.uses": "vibration" is now named "haptics"; rename the key'
    if (!(USES as readonly string[]).includes(name)) return `"solidrt.uses": "${name}" is not one of ${USES.join(", ")}`
    if (typeof value !== "boolean") return `"solidrt.uses": "${name}" must be true or false`
  }
  return null
}
