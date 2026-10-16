// The pure tile-grid math: the unbounded cell addressing (any integer
// cell, negative included, wrapped into a torus's period when the layer
// wraps), a cell's chunk and record slot, the walk of a rect chunk by
// chunk, the index form's flag codec (Tiled's three bits) and the eight
// cell symmetries as the record encodes them - plain arithmetic with no
// GPU imports, so the headless check (tests/tiles.test.ts) drives it
// directly; tiles.ts wraps it with the chunk textures.

/** A rect of cells: the layer's `bounds` option and `usedRect()`. */
export type TileRect = { col: number; row: number; cols: number; rows: number }

/** Floor division: the chunk of a cell, negative cells included. */
export function floorDiv(a: number, n: number): number {
  return Math.floor(a / n)
}

/** Euclidean modulo: the cell's offset inside its chunk, or its canonical
 * index in a wrapping world; always in [0, n). */
export function mod(a: number, n: number): number {
  let r = a % n
  return r < 0 ? r + n : r
}

// Chunk indices are packed into one number as a map key: this many chunks
// each side of the origin along either axis, exact in a double - and, at
// half of it, exact in the float32 a view's entry carries them in.
const CHUNK_KEY_SPAN = 2 ** 25

/** One number keying the chunk at (chunkCol, chunkRow): the sparse chunk
 * map's key. Indices beyond 2^24 chunks from the origin throw (a world
 * 10^7 cells across stays far inside; 2^24 is where a float32 stops
 * holding every integer, and a view's chunk-quad stage reads the
 * indices as floats). */
export function chunkKey(chunkCol: number, chunkRow: number): number {
  let half = CHUNK_KEY_SPAN / 2
  if (!(chunkCol > -half && chunkCol < half && chunkRow > -half && chunkRow < half)) {
    throw new Error(`chunk ${chunkCol}, ${chunkRow} is beyond the addressable world (2^25 chunks from the origin)`)
  }
  return (chunkCol + half) * CHUNK_KEY_SPAN + (chunkRow + half)
}

/** A cell's record offset inside its chunk's records: cells are row-major
 * within the chunk, `floatsPerCell` floats each. */
export function slotOf(col: number, row: number, chunkTiles: number, floatsPerCell: number): number {
  return (mod(row, chunkTiles) * chunkTiles + mod(col, chunkTiles)) * floatsPerCell
}

/**
 * Walk the w x h rect at (col, row) chunk by chunk: `visit` is called once
 * per chunk the rect touches with the chunk's indices and the rect's slice
 * of it, the cells [colA, colB) x [rowA, rowB) - chunk rows outer, chunk
 * columns inner, so a caller writing records advances through each
 * chunk's slice row by row. Cells are unbounded, so a rect may start at a
 * negative cell.
 */
export function eachChunkSlice(
  col: number,
  row: number,
  w: number,
  h: number,
  chunkTiles: number,
  visit: (chunkCol: number, chunkRow: number, colA: number, colB: number, rowA: number, rowB: number) => void,
): void {
  let cr0 = floorDiv(row, chunkTiles)
  let cr1 = floorDiv(row + h - 1, chunkTiles)
  let cc0 = floorDiv(col, chunkTiles)
  let cc1 = floorDiv(col + w - 1, chunkTiles)
  for (let cr = cr0; cr <= cr1; cr++) {
    let rowA = Math.max(row, cr * chunkTiles)
    let rowB = Math.min(row + h, (cr + 1) * chunkTiles)
    for (let cc = cc0; cc <= cc1; cc++) {
      let colA = Math.max(col, cc * chunkTiles)
      let colB = Math.min(col + w, (cc + 1) * chunkTiles)
      visit(cc, cr, colA, colB, rowA, rowB)
    }
  }
}

/** Throw unless `rect` is a well-formed, non-empty rect of integer cells. */
export function checkTileRect(verb: string, rect: TileRect): void {
  if (!(Number.isInteger(rect.col) && Number.isInteger(rect.row) && Number.isInteger(rect.cols) && Number.isInteger(rect.rows) && rect.cols > 0 && rect.rows > 0)) {
    throw new Error(`${verb}: a rect takes integer col, row and positive integer cols, rows, got ${JSON.stringify(rect)}`)
  }
}

/** Throw unless (col, row) is an integer cell inside `bounds` (any integer
 * cell without bounds). */
export function checkCell(verb: string, col: number, row: number, bounds: TileRect | null): void {
  if (!(Number.isInteger(col) && Number.isInteger(row))) throw new Error(`${verb}: cell ${col}, ${row} is not an integer cell`)
  if (bounds !== null && !(col >= bounds.col && col < bounds.col + bounds.cols && row >= bounds.row && row < bounds.row + bounds.rows)) {
    throw new Error(`${verb}: cell ${col}, ${row} outside the layer's bounds ${bounds.cols} x ${bounds.rows} at ${bounds.col}, ${bounds.row}`)
  }
}

/** Throw unless the w x h rect at (col, row) is a non-empty integer rect
 * inside `bounds` (anywhere without bounds). */
export function checkRect(verb: string, col: number, row: number, w: number, h: number, bounds: TileRect | null): void {
  if (!(Number.isInteger(col) && Number.isInteger(row) && Number.isInteger(w) && Number.isInteger(h) && w > 0 && h > 0)) {
    throw new Error(`${verb}: rect ${col}, ${row} of ${w} x ${h} is not a non-empty integer rect`)
  }
  if (bounds !== null && !(col >= bounds.col && row >= bounds.row && col + w <= bounds.col + bounds.cols && row + h <= bounds.row + bounds.rows)) {
    throw new Error(`${verb}: rect ${col}, ${row} of ${w} x ${h} outside the layer's bounds ${bounds.cols} x ${bounds.rows} at ${bounds.col}, ${bounds.row}`)
  }
}

// --- the index form's flag codec: Tiled's gid layout, the index in the
// low 29 bits and the three flip flags in the top three ---

/** Flip the cell horizontally (mirror its columns), as a flag on an index
 * cell of setTiles: Tiled's "flipped horizontally" bit. */
export const TILE_FLIP_X = 0x80000000
/** Flip the cell vertically (mirror its rows): Tiled's "flipped vertically" bit. */
export const TILE_FLIP_Y = 0x40000000
/** Transpose the cell (reflect across its diagonal, applied before the
 * flips as in Tiled): Tiled's "flipped diagonally" bit. */
export const TILE_TRANSPOSE = 0x20000000
/** The index under the flags. */
export const TILE_INDEX_MASK = 0x1fffffff

/** Whether an index cell clears: -1 in any array, which a Uint16Array
 * holds as 0xffff and a Uint32Array as 0xffffffff (the same bits). */
export function isClearIndex(v: number): boolean {
  return v === -1 || v === 0xffff || v === 0xffffffff
}

/** The frames-table index an index cell names: the low 29 bits. */
export function indexOf(v: number): number {
  return (v >>> 0) & TILE_INDEX_MASK
}

/** The symmetry flags of an index cell, as the three booleans. */
export function flagsOf(v: number): { flipX: boolean; flipY: boolean; transpose: boolean } {
  let u = v >>> 0
  return { flipX: (u & TILE_FLIP_X) !== 0, flipY: (u & TILE_FLIP_Y) !== 0, transpose: (u & TILE_TRANSPOSE) !== 0 }
}

/** How the record encodes a cell's symmetry: which UV axes writeFrame
 * swaps and the record's rotation. */
export type CellSymmetry = { flipU: boolean; flipV: boolean; rot: number }

// A quarter turn: the transpose's rotation (the record's rot is clockwise
// in the y-down world, glsl.ts).
const QUARTER_TURN = Math.PI / 2

/**
 * The record encoding of the eight symmetries of a cell, with no record
 * layout change: the two flips are UV swaps, the transpose a quarter-turn
 * rotation with the flips remapped. Flags compose in Tiled's order
 * (transpose first, then the horizontal, then the vertical flip): a
 * texel at frame offset (x, y) from the center lands at (y, x) under the
 * transpose, then (-x, y) under flipX and (x, -y) under flipY. The
 * shader rotates the quad's local (x, y) to (-y, x) for a quarter turn,
 * so a transposed cell pre-flips V (to land (y, x) exactly) and takes
 * flipX as a V flip and flipY as a U flip. tests/tiles.test.ts holds this
 * against the composed oracle for all eight.
 */
export function cellSymmetry(flipX: boolean, flipY: boolean, transpose: boolean): CellSymmetry {
  if (!transpose) return { flipU: flipX, flipV: flipY, rot: 0 }
  return { flipU: flipY, flipV: !flipX, rot: QUARTER_TURN }
}

/** The footprint a frame of fw x fh pixels covers in the world: swapped
 * under a transpose. */
export function footprint(fw: number, fh: number, transpose: boolean): [number, number] {
  return transpose ? [fh, fw] : [fw, fh]
}
