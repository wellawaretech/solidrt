// The pure tile-grid math: cell and rect validation, a cell's chunk and
// record slot, and the walk of a rect chunk by chunk - plain arithmetic
// with no GPU imports, so the headless check (checks/tiles-check.ts)
// drives it directly; tiles.ts wraps it with the chunk textures.

/** Throw unless (col, row) is a cell of the cols x rows grid. */
export function checkCell(verb: string, col: number, row: number, cols: number, rows: number): void {
  if (!(Number.isInteger(col) && Number.isInteger(row) && col >= 0 && col < cols && row >= 0 && row < rows)) {
    throw new Error(`${verb}: cell ${col}, ${row} outside the ${cols} x ${rows} grid`)
  }
}

/** Throw unless the w x h rect at (col, row) is non-empty and lies inside
 * the cols x rows grid. */
export function checkRect(verb: string, col: number, row: number, w: number, h: number, cols: number, rows: number): void {
  if (!(Number.isInteger(col) && Number.isInteger(row) && Number.isInteger(w) && Number.isInteger(h) && w > 0 && h > 0 && col >= 0 && row >= 0 && col + w <= cols && row + h <= rows)) {
    throw new Error(`${verb}: rect ${col}, ${row} of ${w} x ${h} outside the ${cols} x ${rows} grid`)
  }
}

/** A cell's chunk index in a grid chunked chunkTiles x chunkTiles, with
 * chunkCols chunks across: chunks are numbered row-major. */
export function chunkOf(col: number, row: number, chunkTiles: number, chunkCols: number): number {
  return Math.floor(row / chunkTiles) * chunkCols + Math.floor(col / chunkTiles)
}

/** A cell's record offset inside its chunk's records: cells are row-major
 * within the chunk, `floatsPerCell` floats each. */
export function slotOf(col: number, row: number, chunkTiles: number, floatsPerCell: number): number {
  return ((row % chunkTiles) * chunkTiles + (col % chunkTiles)) * floatsPerCell
}

/**
 * Walk the w x h rect at (col, row) chunk by chunk: `visit` is called once
 * per chunk the rect touches with the chunk's index and the rect's slice
 * of it, the cells [colA, colB) x [rowA, rowB) - chunk rows outer, chunk
 * columns inner, so a caller writing records advances through each
 * chunk's slice row by row.
 */
export function eachChunkSlice(
  col: number,
  row: number,
  w: number,
  h: number,
  chunkTiles: number,
  chunkCols: number,
  visit: (index: number, colA: number, colB: number, rowA: number, rowB: number) => void,
): void {
  let cr0 = Math.floor(row / chunkTiles)
  let cr1 = Math.floor((row + h - 1) / chunkTiles)
  let cc0 = Math.floor(col / chunkTiles)
  let cc1 = Math.floor((col + w - 1) / chunkTiles)
  for (let cr = cr0; cr <= cr1; cr++) {
    let rowA = Math.max(row, cr * chunkTiles)
    let rowB = Math.min(row + h, (cr + 1) * chunkTiles)
    for (let cc = cc0; cc <= cc1; cc++) {
      let colA = Math.max(col, cc * chunkTiles)
      let colB = Math.min(col + w, (cc + 1) * chunkTiles)
      visit(cr * chunkCols + cc, colA, colB, rowA, rowB)
    }
  }
}
