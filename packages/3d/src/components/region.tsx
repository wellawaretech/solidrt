// The region components: <Region> (createRegion) and <RegionGrid>
// (createRegionGrid) with the content as a RENDER PROP - `build` returns
// the JSX of a region's content, mounted under the region's content node
// once its data is loaded, and `proxy` the JSX drawn while the content
// is not shown. A JSX build is one step of the slice budget (the mount
// runs in the frame's flush), so the imperative generator form is the
// one for a region whose creates must spread over several frames.
import { callRef, createEffect, createSignal, For, onCleanup, Show, untrack, useContext } from "@solidrt/core"
import type { Element } from "@solidrt/core"
import { SceneContext } from "./context.tsx"
import type { SceneCtx } from "./context.tsx"
import { syncNode } from "./node-props.ts"
import type { TransformProps } from "./node-props.ts"
import { createRegion, createRegionGrid } from "../region.ts"
import { add } from "../node.ts"
import type { RegionGridNode, RegionGridOptions, RegionNode, RegionOptions, RegionState } from "../region.ts"
import type { SceneNode } from "../node.ts"

type ContentProps<T> = {
  /** The region's content as JSX, mounted under the region's content
   * node with the loaded data; unmounted at leave. */
  build: (region: RegionNode<T>, data: T) => Element
  /** The JSX drawn while the content is not shown (HLOD): a card, a
   * merged low-poly stand-in. */
  proxy?: (region: RegionNode<T>) => Element
  load?: RegionOptions<T>["load"]
  unload?: RegionOptions<T>["unload"]
  onStateChange?: (region: RegionNode<T>, state: RegionState) => void
  onError?: (region: RegionNode<T>, error: unknown) => void
}

export type RegionProps<T = undefined> = TransformProps &
  ContentProps<T> &
  Omit<RegionOptions<T>, "build" | "proxy" | "load" | "unload"> & {
    ref?: (node: RegionNode<T>) => void
  }

export type RegionGridProps<T = undefined> = TransformProps &
  ContentProps<T> &
  Omit<RegionGridOptions<T>, "build" | "proxy" | "load" | "unload"> & {
    ref?: (node: RegionGridNode<T>) => void
  }

// One held region's reactive half: its data once built (the content
// mounts on it) and whether it is shown (the proxy unmounts on it).
type Held<T> = {
  region: RegionNode<T>
  data: () => { value: T } | null
  setData: (d: { value: T } | null) => void
  shown: () => boolean
  setShown: (s: boolean) => void
}

function held<T>(region: RegionNode<T>): Held<T> {
  let [data, setData] = createSignal<{ value: T } | null>(null)
  let [shown, setShown] = createSignal(false)
  return { region, data, setData, shown, setShown }
}

// The JSX of one held region: the content under its content node once
// the data is there, measured again once mounted (the build step ran
// before the mount); the proxy under the region node while not shown.
function Content<T>(props: { ctx: SceneCtx; held: Held<T>; build: ContentProps<T>["build"]; proxy: ContentProps<T>["proxy"] }): Element {
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
  let scope = (parent: SceneNode, children: () => Element | undefined): Element => (
    <SceneContext value={{ scene: props.ctx.scene, parent, viewport: props.ctx.viewport, pointer: props.ctx.pointer, viewportSize: props.ctx.viewportSize, quality: props.ctx.quality }}>{children()}</SceneContext>
  )
  return (
    <>
      {scope(region.content, content)}
      <Show when={props.proxy !== undefined && !props.held.shown()}>{scope(region, () => props.proxy!(region))}</Show>
    </>
  )
}

/**
 * An explicit region (createRegion as a component): a bounded group
 * placed like any node, whose `build` JSX exists while a viewer is
 * within `enter` of its box. `load` prepares the data (async), `unload`
 * runs before the content goes, `proxy` is drawn while it is not shown.
 */
export function Region<T = undefined>(props: RegionProps<T>): Element {
  let ctx = useContext(SceneContext)
  let h: Held<T> = null!
  let node = untrack(() =>
    createRegion<T>({
      bounds: props.bounds,
      enter: props.enter,
      leave: props.leave,
      weight: props.weight,
      priority: props.priority,
      retain: props.retain,
      label: props.label,
      load: props.load === undefined ? undefined : (r, signal) => props.load!(r, signal),
      build: (_, data) => {
        h.setData({ value: data })
      },
      unload: props.unload === undefined ? undefined : (r, data) => props.unload!(r, data),
    }),
  )
  h = held(node)
  add(ctx.parent, node)
  node.onStateChange = (r, s) => {
    h.setShown(s === "shown")
    if (s === "unwanted" || s === "pending" || s === "stalled" || s === "loading") h.setData(null)
    props.onStateChange?.(r, s)
  }
  node.onError = (r, e) => props.onError?.(r, e)
  syncNode(node, props)
  callRef(() => props.ref, node)
  onCleanup(() => node.dispose())
  return <Content ctx={ctx} held={h} build={props.build} proxy={props.proxy} />
}

/**
 * A region grid (createRegionGrid as a component): every cell a viewer
 * wants mounts its `build` JSX under a child region at the cell's
 * corner, and unmounts it at leave.
 */
export function RegionGrid<T = undefined>(props: RegionGridProps<T>): Element {
  let ctx = useContext(SceneContext)
  let [cells, setCells] = createSignal<Held<T>[]>([])
  let byRegion = new Map<RegionNode<T>, Held<T>>()
  // A cell's reactive half exists from its first state on (loading, or
  // building when there is no load), so its proxy mounts while it loads.
  let ensure = (r: RegionNode<T>): Held<T> => {
    let h = byRegion.get(r)
    if (h === undefined) {
      h = held(r)
      byRegion.set(r, h)
      setCells(list => [...list, h!])
    }
    return h
  }
  let node = untrack(() =>
    createRegionGrid<T>({
      origin: props.origin,
      cell: props.cell,
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
  add(ctx.parent, node)
  node.onStateChange = (r, s) => {
    if (s === "unwanted") {
      let h = byRegion.get(r)
      if (h !== undefined) {
        byRegion.delete(r)
        setCells(list => list.filter(x => x !== h))
      }
    } else ensure(r).setShown(s === "shown")
    props.onStateChange?.(r, s)
  }
  node.onError = (r, e) => props.onError?.(r, e)
  syncNode(node, props)
  callRef(() => props.ref, node)
  onCleanup(() => node.dispose())
  return <For each={cells()}>{h => <Content ctx={ctx} held={h} build={props.build} proxy={props.proxy} />}</For>
}
