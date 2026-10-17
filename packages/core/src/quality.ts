// The quality tier: the app's one statement of the device's rendering
// budget, which every package's quality table defaults from - the 3d
// package's rows for its shadows, LOD, multisampling, fill density, texture
// sampling and probes, a 2d package's for its own knobs. Unity's quality
// level and Godot's `.mobile` overrides are the same statement made
// global; here it is a reactive value the app sets once, from what it
// knows of the device or from a settings screen. Nothing picks it on its
// own: a GPU name is not a budget (vendor lists were tried and rejected),
// and a measured default waits on a startup cost probe. "medium" is the
// row an app that says nothing gets.
import { createSignal } from "@solidjs/signals"

/** The quality tier names, low to high. */
export type Quality = "low" | "medium" | "high"

/** The tier names in order, for a settings screen to list or a package to
 * validate against. */
export const QUALITIES: readonly Quality[] = ["low", "medium", "high"]

let [tier, setTier] = createSignal<Quality>("medium")

/** The current quality tier, as a reactive accessor: what a package's
 * quality table resolves its defaults from, and re-resolves when it
 * changes. */
export function quality(): Quality {
  return tier()
}

/** Set the quality tier for the whole app. Live: every scene, texture and
 * layer that follows the tier re-resolves its row. */
export function setQuality(quality: Quality): void {
  if (!QUALITIES.includes(quality)) throw new Error('setQuality: expected "low", "medium" or "high", got ' + quality)
  setTier(quality)
}
