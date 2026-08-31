// The 2d camera vocabulary, pure: the one camera type both layers share
// and its world <-> screen mapping as plain functions - no GPU or GUI
// imports, so the checks rig (checks/camera-check.ts) exercises this
// module headless on the flux binary. Three consumers implement or undo
// exactly this mapping and must agree with it: the vertex stages
// (shaders.ts, uCamera + uCameraRot), the tile layer's composite
// transform (the <view> props in components.tsx) and the pointer inverse
// (spriteDispatch in layer.ts, which calls unprojectCamera directly). The
// check keeps the spellings from drifting.

/**
 * A camera over a 2d world - the sprite layer's `setCamera`/`camera` prop
 * and the tile layer's `TileCamera` alike. The world point (`x`, `y`) is
 * shown at the viewport point (`pivotX`, `pivotY`), the world scaled by
 * `zoom` and rotated by `rotation` ABOUT that pivot. Rotation turns the
 * world clockwise (y-down) about the pivot; to render a heading `h`
 * upward use `rotation = -h - Math.PI / 2`. The pivot defaults to (0, 0),
 * which makes `{ x, y, zoom }` mean "world x/y at the viewport top-left" -
 * the plain scrolling camera.
 *
 * As a `setCamera` update, absent keys keep their values (the params
 * rule); as an argument to projectCamera/unprojectCamera, absent keys
 * read as the defaults (x/y/rotation 0, zoom 1, pivot (0, 0)).
 */
export type CameraUpdate = {
  /** World pixel shown at the pivot (the viewport top-left by default). */
  x?: number
  y?: number
  /** World-to-screen scale; 1 is pixel-for-pixel. */
  zoom?: number
  /** Rotation, radians, clockwise (y-down), about the pivot. */
  rotation?: number
  /** Viewport point the camera's world point pins to; default (0, 0). */
  pivotX?: number
  pivotY?: number
}

const CAMERA_FIELDS = ["x", "y", "zoom", "rotation", "pivotX", "pivotY"] as const

/**
 * Validate a setCamera update (throws - the dev validation policy): every
 * present field must be finite, zoom additionally positive. A NaN rotation
 * or pivot would otherwise poison the uniforms and blank the layer with no
 * error. Internal - both layer kinds call it at the top of setCamera.
 */
export function checkCamera(update: CameraUpdate): void {
  for (let key of CAMERA_FIELDS) {
    let value = update[key]
    if (value !== undefined && !Number.isFinite(value)) {
      throw new Error(`setCamera: ${key} must be a finite number, got ${value}`)
    }
  }
  if (update.zoom !== undefined && update.zoom <= 0) {
    throw new Error(`setCamera: zoom must be positive, got ${update.zoom}`)
  }
}

/**
 * World -> screen under a camera:
 * `pivot + R(rotation) * zoom * (world - cam)`. Screen means the layer's
 * viewport pixels (the sprite layer's output, the tile world's container).
 */
export function projectCamera(camera: CameraUpdate, worldX: number, worldY: number): [number, number] {
  let zoom = camera.zoom ?? 1
  let rotation = camera.rotation ?? 0
  let c = Math.cos(rotation)
  let s = Math.sin(rotation)
  let dx = (worldX - (camera.x ?? 0)) * zoom
  let dy = (worldY - (camera.y ?? 0)) * zoom
  return [(camera.pivotX ?? 0) + dx * c - dy * s, (camera.pivotY ?? 0) + dx * s + dy * c]
}

/** Screen -> world: the exact inverse of projectCamera. */
export function unprojectCamera(camera: CameraUpdate, screenX: number, screenY: number): [number, number] {
  let zoom = camera.zoom ?? 1
  let rotation = camera.rotation ?? 0
  let c = Math.cos(rotation)
  let s = Math.sin(rotation)
  let dx = screenX - (camera.pivotX ?? 0)
  let dy = screenY - (camera.pivotY ?? 0)
  return [(camera.x ?? 0) + (dx * c + dy * s) / zoom, (camera.y ?? 0) + (dy * c - dx * s) / zoom]
}
