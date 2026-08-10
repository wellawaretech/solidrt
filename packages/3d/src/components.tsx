// The Solid face: PascalCase components over context, syncing the retained
// scene (scene.ts) - no new intrinsic elements, no renderer changes. Props
// follow the Solid 2.0 model (reactive values, no destructuring); effects
// write into the retained nodes and the runtime's dirty flush renders.
// Anything moving at frame rate can bypass the declarative layer: grab the
// node with `ref` and call setTransform from onFrame - signals carry
// structure and slow state, per-frame motion goes straight to the scene.

import { createContext, createEffect, onCleanup, untrack, useContext } from "@solidrt/core"
import type { Element, ParentComponent, TextureId, VoidComponent } from "@solidrt/core"
import {
  add,
  createGroup,
  createMesh,
  createScene,
  remove,
  setGeometry,
  setMaterial,
  setTransform,
  setVisible,
} from "./scene.ts"
import type { Mesh as MeshNode, Scene as SceneHandle, SceneNode } from "./scene.ts"
import type { Geometry } from "./geometry.ts"
import type { Material } from "./material.ts"
import type { Vec3 } from "./math.ts"

type SceneCtx = { scene: SceneHandle; parent: SceneNode }
let SceneContext = createContext<SceneCtx>()

/**
 * The enclosing scene and parent node - the imperative escape hatch inside
 * a component subtree (throws outside a `<Scene>`).
 */
export function useScene(): SceneCtx {
  return useContext(SceneContext)
}

export type TransformProps = {
  position?: Vec3
  /** Euler radians, applied x then y then z. */
  rotation?: Vec3
  scale?: Vec3 | number
  visible?: boolean
}

function syncNode(node: SceneNode, props: TransformProps): void {
  createEffect(
    () => [props.position, props.rotation, props.scale, props.visible] as const,
    ([position, rotation, scale, visible]) => {
      setTransform(node, { position, rotation, scale })
      setVisible(node, visible !== false)
    },
  )
}

export type SceneProps = {
  /** Target pixels. With `output`, the leaf's own width/height are layout,
   * so render size and display size separate (supersampling). */
  width: number
  height: number
  clearColor?: [number, number, number, number]
  label?: string
  ref?: (scene: SceneHandle) => void
  /**
   * Compose the output yourself: called once (untracked) with the scene's
   * texture id, and its return renders in place of the built-in `<texture>`
   * leaf - a `<d-texture>`, a leaf carrying paint/pointer/layout props, or
   * a post-effect chain (a shader target sampling the id; created in the
   * callback it disposes with the Scene). Return null to render no leaf.
   */
  output?: (texture: TextureId) => Element
}

/**
 * Owns a draw target and composites it as an ordinary `<texture>` leaf, so
 * the output takes layout, transforms, blendMode, and pointer events like
 * any element - or hand `output` the texture id and compose it yourself.
 * Children (Mesh/Group/PerspectiveCamera) render nothing themselves - they
 * populate the retained scene through context.
 */
export let Scene: ParentComponent<SceneProps> = props => {
  let scene = untrack(() =>
    createScene(props.width, props.height, { clearColor: props.clearColor, label: props.label }),
  )
  createEffect(
    () => [props.width, props.height] as const,
    ([w, h]) => scene.setSize(w, h),
  )
  untrack(() => props.ref)?.(scene)
  let output = untrack(() => props.output)
  return (
    <SceneContext value={{ scene, parent: scene.root }}>
      {output ? (
        untrack(() => output(scene.texture))
      ) : (
        <texture src={scene.texture} width={props.width} height={props.height} />
      )}
      {props.children}
    </SceneContext>
  )
}

/** A transform node: children inherit its position/rotation/scale. */
export let Group: ParentComponent<TransformProps & { ref?: (node: SceneNode) => void }> = props => {
  let ctx = useContext(SceneContext)
  let node = createGroup()
  add(ctx.parent, node)
  syncNode(node, props)
  untrack(() => props.ref)?.(node)
  onCleanup(() => remove(node))
  return <SceneContext value={{ scene: ctx.scene, parent: node }}>{props.children}</SceneContext>
}

export type MeshProps = TransformProps & {
  geometry: Geometry
  material: Material
  ref?: (mesh: MeshNode) => void
}

/** One draw entry: geometry drawn with a material at a transform. */
export let Mesh: VoidComponent<MeshProps> = props => {
  let ctx = useContext(SceneContext)
  let mesh = untrack(() => createMesh(props.geometry, props.material))
  add(ctx.parent, mesh)
  createEffect(
    () => props.geometry,
    g => setGeometry(mesh, g),
    { defer: true },
  )
  createEffect(
    () => props.material,
    m => setMaterial(mesh, m),
    { defer: true },
  )
  syncNode(mesh, props)
  untrack(() => props.ref)?.(mesh)
  onCleanup(() => remove(mesh))
  return null
}

export type PerspectiveCameraProps = {
  /** Vertical field of view in DEGREES (default 60). */
  fov?: number
  near?: number
  far?: number
  position?: Vec3
  lookAt?: Vec3
  up?: Vec3
}

/**
 * Drives the scene's camera from props (the scene has a default camera, so
 * this component is optional). The camera is scene state, not a tree node:
 * to orbit it, update `position`/`lookAt`.
 */
export let PerspectiveCamera: VoidComponent<PerspectiveCameraProps> = props => {
  let ctx = useContext(SceneContext)
  createEffect(
    () => [props.fov, props.near, props.far, props.position, props.lookAt, props.up] as const,
    ([fov, near, far, position, lookAt, up]) =>
      ctx.scene.setCamera({ fov, near, far, position, target: lookAt, up }),
  )
  return null
}
