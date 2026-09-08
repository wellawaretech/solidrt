// Modifier chords, the vocabulary two devices share: a spec names
// modifiers ahead of what it qualifies ("Shift+Tab" on the keyboard,
// "Ctrl" on a pointer gesture), the event must carry them, a bare spec
// ignores them (a shifted W still walks, a Ctrl-drag still drags), and
// among the specs one event satisfies the most specific wins - Unreal's
// chord blocker, Three's OrbitControls picking PAN over ROTATE at
// mousedown; Godot and Unity fire both by default and every app writes
// the exclusion itself. Which event resolves a chord is the device's
// call: the keyboard settles each key down, the pointer feed the event
// that opens a gesture bracket.

export type Modifier = "shiftKey" | "ctrlKey" | "altKey" | "metaKey"
/** The modifier flags every key, pointer and wheel event carries. */
export type Modified = Record<Modifier, boolean>

const MODIFIERS: Record<string, Modifier> = { Shift: "shiftKey", Ctrl: "ctrlKey", Control: "ctrlKey", Alt: "altKey", Meta: "metaKey" }
// The canonical order a chord is spelled in, so one modifier set has one name.
const ORDER: Modifier[] = ["shiftKey", "ctrlKey", "altKey", "metaKey"]
const NAMES: Record<Modifier, string> = { shiftKey: "Shift", ctrlKey: "Ctrl", altKey: "Alt", metaKey: "Meta" }

/** The modifiers `parts` name (["Ctrl", "Shift"]), deduplicated, in
 * canonical order; `what` and `text` name the caller and its spec in the
 * throw on an unknown name. */
export function parseModifiers(what: string, text: string, parts: string[]): Modifier[] {
  let mods = new Set<Modifier>()
  for (let part of parts) {
    let mod = MODIFIERS[part]
    if (!mod) throw new Error(`${what}: unknown modifier "${part}" in "${text}" (Shift, Ctrl, Alt or Meta)`)
    mods.add(mod)
  }
  return ORDER.filter(m => mods.has(m))
}

/** The canonical spelling of a modifier set: "Shift+Ctrl"; "" for none. */
export let chordName = (mods: Modifier[]): string => mods.map(m => NAMES[m]).join("+")

/** Of `items`, those whose modifiers the event carries, narrowed to the
 * most specific (the longest modifier lists, all of them on a tie);
 * empty when none match. */
export function mostSpecific<T>(items: T[], mods: (item: T) => Modifier[], event: Modified): T[] {
  let hits = items.filter(item => mods(item).every(m => event[m]))
  let most = hits.reduce((n, item) => Math.max(n, mods(item).length), 0)
  return hits.filter(item => mods(item).length === most)
}
