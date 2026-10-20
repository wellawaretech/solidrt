"use isolate"
// The streaming example's data prep, off the JS thread: one call builds a
// terrain cell - its heightfield as a geometry in the base layout
// (position, normal, uv per vertex, interleaved) with its indices, the
// rock scatter over it, and the cell's mean height for the proxy slab.
// Pure arithmetic over a hashed value noise seeded by the cell index, so
// a cell comes back identical every visit; typed arrays cross to main as
// copies and become the Geometry of the cell's mesh as they are.

/** What one cell holds, as plain data. */
export type CellData = {
  /** Interleaved vertices: x, y, z, nx, ny, nz, u, v per vertex. */
  vertices: Float32Array
  indices: Uint16Array
  /** Rock placements: x, y, z, scale per rock. */
  rocks: Float32Array
  /** The mean height, for the proxy slab. */
  mean: number
}

// Floats per vertex in the base layout.
const VERTEX_FLOATS = 8
// Floats per rock placement.
const ROCK_FLOATS = 4
// Terrain shape: the noise's lateral period in world units, its height
// amplitude, and the octaves summed.
const NOISE_PERIOD = 90
const NOISE_HEIGHT = 9
const NOISE_OCTAVES = 3
// Rocks per cell, and the slope above which none sits.
const ROCKS_PER_CELL = 40
const ROCK_MAX_SLOPE = 0.6

// A hash of integer lattice coordinates to 0..1 (a 2d integer hash, the
// usual bit mixer), the noise's lattice values.
function hash(x: number, z: number, seed: number): number {
  let h = (x * 374761393 + z * 668265263 + seed * 2246822519) | 0
  h = Math.imul(h ^ (h >>> 13), 1274126177)
  h ^= h >>> 16
  return (h >>> 0) / 4294967296
}

function smooth(t: number): number {
  return t * t * (3 - 2 * t)
}

/** Value noise at a world point: lattice values blended with a smooth
 * step, octaves summed at halving amplitude. */
function noise(x: number, z: number, seed: number): number {
  let sum = 0
  let amplitude = 1
  let period = NOISE_PERIOD
  for (let o = 0; o < NOISE_OCTAVES; o++) {
    let fx = x / period
    let fz = z / period
    let x0 = Math.floor(fx)
    let z0 = Math.floor(fz)
    let tx = smooth(fx - x0)
    let tz = smooth(fz - z0)
    let a = hash(x0, z0, seed + o)
    let b = hash(x0 + 1, z0, seed + o)
    let c = hash(x0, z0 + 1, seed + o)
    let d = hash(x0 + 1, z0 + 1, seed + o)
    sum += amplitude * ((a + (b - a) * tx) * (1 - tz) + (c + (d - c) * tx) * tz)
    amplitude /= 2
    period /= 2
  }
  return sum
}

/** The terrain height at a world point. */
export function height(x: number, z: number, seed: number): number {
  return (noise(x, z, seed) - 0.875) * NOISE_HEIGHT
}

/**
 * Build cell (i, k) of a column grid with `size` world units per cell and
 * `resolution` quads per side: the heightfield in cell-local coordinates
 * (x and z from 0 to `size`, y the height), its rocks, its mean height.
 */
export function cell(i: number, k: number, size: number, resolution: number, seed: number): CellData {
  let side = resolution + 1
  let vertices = new Float32Array(side * side * VERTEX_FLOATS)
  let step = size / resolution
  let originX = i * size
  let originZ = k * size
  let mean = 0
  for (let row = 0; row < side; row++) {
    for (let col = 0; col < side; col++) {
      let x = col * step
      let z = row * step
      let wx = originX + x
      let wz = originZ + z
      let y = height(wx, wz, seed)
      mean += y
      // The normal from the height gradient, central differences.
      let dx = (height(wx + step, wz, seed) - height(wx - step, wz, seed)) / (2 * step)
      let dz = (height(wx, wz + step, seed) - height(wx, wz - step, seed)) / (2 * step)
      let len = Math.hypot(dx, 1, dz)
      let at = (row * side + col) * VERTEX_FLOATS
      vertices[at] = x
      vertices[at + 1] = y
      vertices[at + 2] = z
      vertices[at + 3] = -dx / len
      vertices[at + 4] = 1 / len
      vertices[at + 5] = -dz / len
      vertices[at + 6] = col / resolution
      vertices[at + 7] = row / resolution
    }
  }
  mean /= side * side
  let indices = new Uint16Array(resolution * resolution * 6)
  let n = 0
  for (let row = 0; row < resolution; row++) {
    for (let col = 0; col < resolution; col++) {
      let a = row * side + col
      let b = a + 1
      let c = a + side
      let d = c + 1
      // Counter-clockwise seen from above (+y).
      indices[n++] = a
      indices[n++] = c
      indices[n++] = b
      indices[n++] = b
      indices[n++] = c
      indices[n++] = d
    }
  }
  let rocks = new Float32Array(ROCKS_PER_CELL * ROCK_FLOATS)
  let placed = 0
  for (let r = 0; r < ROCKS_PER_CELL; r++) {
    let x = hash(r, 1, seed + i * 7919 + k * 104729) * size
    let z = hash(r, 2, seed + i * 7919 + k * 104729) * size
    let wx = originX + x
    let wz = originZ + z
    let slope = Math.hypot(height(wx + 1, wz, seed) - height(wx - 1, wz, seed), height(wx, wz + 1, seed) - height(wx, wz - 1, seed)) / 2
    if (slope > ROCK_MAX_SLOPE) continue
    let at = placed * ROCK_FLOATS
    rocks[at] = x
    rocks[at + 1] = height(wx, wz, seed)
    rocks[at + 2] = z
    rocks[at + 3] = 0.6 + hash(r, 3, seed) * 1.2
    placed++
  }
  return { vertices, indices, rocks: rocks.subarray(0, placed * ROCK_FLOATS), mean }
}
