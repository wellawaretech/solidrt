// The region components: <Region> (createRegion) and <RegionGrid>
// (createRegionGrid) with the content as a RENDER PROP - `build` returns
// the JSX of a region's content (<Sprite>, <Group>, <Text2d>), mounted
// under the region's content group once its data is loaded, and `proxy`
// the JSX drawn while the content is not shown. A JSX build is one step
// of the slice budget (the mount runs in the frame's flush), so the
// imperative generator form is the one for a region whose creates must
// spread over several frames. The same shape as @solidrt/3d's
// components/region.tsx.
import { callRef, createEffect, createSignal, For, onCleanup, Show, untrack, useContext } from "@solidrt/core"
import type { Element } from "@solidrt/core"
import { GroupContext, LayerContext } from "./context.ts"
import { createRegion, createRegionGrid } from "../region.ts"
import type { Region as RegionHandle, RegionGrid as RegionGridHandle, RegionGridOptions, RegionOptions, RegionState } from "../region.ts"
import type { SpriteGroup } from "../layer.ts"

type ContentProps<T> = {
  /** The region's content as JSX, mounted under the region's content
   * group with the loaded data (positions cell-local); unmounted at
   * leave. */
  build: (region: RegionHandle<T>, data: T) => Element
  /** The JSX drawn under the region's group while the content is not
   * shown: a single stand-in sprite, a silhouette. */
  proxy?: (region: RegionHandle<T>) => Element
  load?: RegionOptions<T>["load"]
  unload?: RegionOptions<T>["unload"]
  onStateChange?: (region: RegionHandle<T>, state: RegionState) => void
  onError?: (region: RegionHandle<T>, error: unknown) => void
}

export type RegionProps<T = undefined> = ContentProps<T> &
  Omit<RegionOptions<T>, "build" | "proxy" | "load" | "unload" | "parent"> & {
    ref?: (region: RegionHandle<T> & { dispose(): void }) => void
  }

export type RegionGridProps<T = undefined> = ContentProps<T> &
  Omit<RegionGridOptions<T>, "build" | "proxy" | "load" | "unload"> & {
    ref?: (grid: RegionGridHandle<T>) => void
  }

// One held region's reactive half: its data once built (the content
// mounts on it) and whether it is shown (the proxy unmounts on it).
type Held<T> = {
  region: RegionHandle<T>
  data: () => { value: T } | null
  setData: (d: { value: T } | null) => void
  shown: () => boolean
  setShown: (s: boolean) => void
}

function held<T>(region: RegionHandle<T>): Held<T> {
  let [data, setData] = createSignal<{ value: T } | null>(null)
  let [shown, setShown] = createSignal(false)
  return { region, data, setData, shown, setShown }
}

// The JSX of one held region: the content under its content group once
// the data is there, measured again once mounted (the build step ran
// before the mount); the proxy under the region's group while not shown.
function Content<T>(props: { held: Held<T>; build: ContentProps<T>["build"]; proxy: ContentProps<T>["proxy"] }): Element {
  let region = props.held.region
  let content = (): Element | undefined => {
    let d = props.held.data()
    return d === null ? undefined : props.build(region, d.value)
  }
  createEffect(
    () => props.held.data(),
    d => {
      if (d !== null) region.measure()
    },
  )
  let scope = (parent: SpriteGroup, children: () => Element | undefined): Element => <GroupContext value={parent}>{children()}</GroupContext>
  return (
    <>
      {scope(region.content, content)}
      <Show when={props.proxy !== undefined && !props.held.shown()}>{scope(region.group, () => props.proxy!(region))}</Show>
    </>
  )
}

/**
 * An explicit region (createRegion as a component): a bounded group at
 * (`x`, `y`) under the enclosing `<Group>` (or the layer root), whose
 * `build` JSX exists while a viewer is within `enter` of its box. `load`
 * prepares the data (async), `unload` runs before the content goes,
 * `proxy` is drawn while it is not shown. The creation props are fixed
 * at mount; move the region through `ref` with setGroup on its group.
 */
export function Region<T = undefined>(props: RegionProps<T>): Element {
  let layer = useContext(LayerContext)
  let parent = useContext(GroupContext)
  let h: Held<T> = null!
  let region = untrack(() =>
    createRegion<T>(layer, {
      x: props.x,
      y: props.y,
      width: props.width,
      height: props.height,
      enter: props.enter,
      leave: props.leave,
      weight: props.weight,
      priority: props.priority,
      retain: props.retain,
      label: props.label,
      parent,
      load: props.load === undefined ? undefined : (r, signal) => props.load!(r, signal),
      build: (_, data) => {
        h.setData({ value: data })
      },
      unload: props.unload === undefined ? undefined : (r, data) => props.unload!(r, data),
    }),
  )
  h = held(region)
  region.onStateChange = (r, s) => {
    h.setShown(s === "shown")
    if (s === "unwanted" || s === "pending" || s === "stalled" || s === "loading") h.setData(null)
    props.onStateChange?.(r, s)
  }
  region.onError = (r, e) => props.onError?.(r, e)
  callRef(() => props.ref, region)
  onCleanup(() => region.dispose())
  return <Content held={h} build={props.build} proxy={props.proxy} />
}

/**
 * A region grid (createRegionGrid as a component): every cell a viewer
 * wants mounts its `build` JSX under a child region at the cell's
 * corner, and unmounts it at leave. The gate's grid is layer space, so
 * the component mounts at the layer root: inside a `<Group>` it throws.
 */
export function RegionGrid<T = undefined>(props: RegionGridProps<T>): Element {
  let layer = useContext(LayerContext)
  if (useContext(GroupContext) !== null) throw new Error("RegionGrid: a region grid is layer space; mount it at the layer root, not inside a <Group>")
  let [cells, setCells] = createSignal<Held<T>[]>([])
  let byRegion = new Map<RegionHandle<T>, Held<T>>()
  // A cell's reactive half exists from its first state on (loading, or
  // building when there is no load), so its proxy mounts while it loads.
  let ensure = (r: RegionHandle<T>): Held<T> => {
    let h = byRegion.get(r)
    if (h === undefined) {
      h = held(r)
      byRegion.set(r, h)
      setCells(list => [...list, h!])
    }
    return h
  }
  let grid = untrack(() =>
    createRegionGrid<T>(layer, {
      cell: props.cell,
      origin: props.origin,
      enter: props.enter,
      leave: props.leave,
      bounds: props.bounds,
      weight: props.weight,
      limit: props.limit,
      priority: props.priority,
      retain: props.retain,
      label: props.label,
      load: props.load === undefined ? undefined : (r, signal) => props.load!(r, signal),
      build: (r, data) => ensure(r).setData({ value: data }),
      unload: props.unload === undefined ? undefined : (r, data) => props.unload!(r, data),
    }),
  )
  grid.onStateChange = (r, s) => {
    if (s === "unwanted") {
      let h = byRegion.get(r)
      if (h !== undefined) {
        byRegion.delete(r)
        setCells(list => list.filter(x => x !== h))
      }
    } else ensure(r).setShown(s === "shown")
    props.onStateChange?.(r, s)
  }
  grid.onError = (r, e) => props.onError?.(r, e)
  callRef(() => props.ref, grid)
  onCleanup(() => grid.dispose())
  return <For each={cells()}>{h => <Content held={h} build={props.build} proxy={props.proxy} />}</For>
}
