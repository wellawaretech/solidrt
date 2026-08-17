// A dynamic bounding-volume hierarchy - the physics-broadphase AABB tree
// (Box2D/Bullet lineage): a binary tree over the items' world boxes, NOT a
// subdivision of space. Leaves store FAT boxes (a margin around the tight
// bounds) so an item moving a little refits nothing; an item escaping its
// fat box is removed and re-inserted along the cheapest path (least area
// growth). A ray query walks O(log n) nodes instead of testing every item,
// which is what keeps per-pointer-move picking off the O(meshes) path.
//
// The scene keeps the tree current from its own sync walk: transforms that
// changed are exactly the leaves to update, so maintenance is O(changed)
// per frame - the same delta discipline as rendering, and a static scene
// pays nothing. Storage is flat parallel arrays indexed by node id (no
// per-node objects, no allocation at steady state past tree growth).
//
// Pure module by design: no GUI imports, so the differential check rig
// (checks/pick-check.ts) runs it headless on flux against a linear oracle.

/** Fat-margin fraction of a leaf's largest extent. Bigger = fewer
 * re-inserts while moving, worse query pruning; 5% is the usual trade. */
const MARGIN = 0.05

type Visit<T> = (item: T) => void

export type Bvh<T> = {
  /** Insert an item with its tight world box; returns the leaf handle. */
  insert(item: T, minX: number, minY: number, minZ: number, maxX: number, maxY: number, maxZ: number): number
  /** Update a leaf's tight box. Free while it stays inside the fat box;
   * otherwise the leaf re-inserts. Returns true when it moved. */
  update(leaf: number, minX: number, minY: number, minZ: number, maxX: number, maxY: number, maxZ: number): boolean
  /** Remove a leaf (the handle is dead afterwards). */
  remove(leaf: number): void
  /** Visit every item whose FAT box the ray hits (t >= 0). Broadphase
   * only: the caller narrowphases against its own tight volumes. */
  raycast(ox: number, oy: number, oz: number, dx: number, dy: number, dz: number, visit: Visit<T>): void
}

/**
 * Entry distance of a ray against a box: the smallest t >= 0 with
 * origin + t * direction inside [min, max] (0 when the origin starts
 * inside), or -1 for a miss. The direction need not be normalized - t is
 * in units of its length, which is what keeps a ray transformed into a
 * mesh's local space reporting world distances. Shared by the tree's
 * broadphase and the scene's narrowphase.
 */
export function rayBoxDistance(
  ox: number, oy: number, oz: number,
  dx: number, dy: number, dz: number,
  minX: number, minY: number, minZ: number,
  maxX: number, maxY: number, maxZ: number,
): number {
  let tNear = 0
  let tFar = Infinity
  // Per axis: a zero direction component never crosses the slab, so the
  // origin must already be inside it (the multiply-by-inverse shortcut
  // turns that case into NaN, hence the explicit branch).
  if (dx === 0) {
    if (ox < minX || ox > maxX) return -1
  } else {
    let inv = 1 / dx
    let t1 = (minX - ox) * inv
    let t2 = (maxX - ox) * inv
    if (t1 > t2) { let t = t1; t1 = t2; t2 = t }
    if (t1 > tNear) tNear = t1
    if (t2 < tFar) tFar = t2
  }
  if (dy === 0) {
    if (oy < minY || oy > maxY) return -1
  } else {
    let inv = 1 / dy
    let t1 = (minY - oy) * inv
    let t2 = (maxY - oy) * inv
    if (t1 > t2) { let t = t1; t1 = t2; t2 = t }
    if (t1 > tNear) tNear = t1
    if (t2 < tFar) tFar = t2
  }
  if (dz === 0) {
    if (oz < minZ || oz > maxZ) return -1
  } else {
    let inv = 1 / dz
    let t1 = (minZ - oz) * inv
    let t2 = (maxZ - oz) * inv
    if (t1 > t2) { let t = t1; t1 = t2; t2 = t }
    if (t1 > tNear) tNear = t1
    if (t2 < tFar) tFar = t2
  }
  return tFar >= tNear ? tNear : -1
}

export function createBvh<T>(): Bvh<T> {
  // Node storage, 6 bounds floats per node; child1 === -1 marks a leaf.
  let bounds: number[] = []
  let parent: number[] = []
  let child1: number[] = []
  let child2: number[] = []
  let items: (T | undefined)[] = []
  let free: number[] = []
  let root = -1
  // Traversal stack, reused across queries.
  let stack: number[] = []

  let allocate = (): number => {
    let node = free.pop()
    if (node === undefined) {
      node = parent.length
      bounds.push(0, 0, 0, 0, 0, 0)
      parent.push(-1)
      child1.push(-1)
      child2.push(-1)
      items.push(undefined)
    }
    return node
  }

  // Half the surface area - the SAH cost of a node's box. Degenerate
  // (flat) boxes still cost via their other faces, which is what makes
  // the heuristic sane for planes.
  let area = (n: number): number => {
    let i = n * 6
    let w = bounds[i + 3]! - bounds[i]!
    let h = bounds[i + 4]! - bounds[i + 1]!
    let d = bounds[i + 5]! - bounds[i + 2]!
    return w * (h + d) + h * d
  }

  let unionArea = (n: number, minX: number, minY: number, minZ: number, maxX: number, maxY: number, maxZ: number): number => {
    let i = n * 6
    let w = Math.max(bounds[i + 3]!, maxX) - Math.min(bounds[i]!, minX)
    let h = Math.max(bounds[i + 4]!, maxY) - Math.min(bounds[i + 1]!, minY)
    let d = Math.max(bounds[i + 5]!, maxZ) - Math.min(bounds[i + 2]!, minZ)
    return w * (h + d) + h * d
  }

  // Recompute an internal node's box as the union of its children.
  let refit = (n: number): void => {
    let a = child1[n]! * 6
    let b = child2[n]! * 6
    let i = n * 6
    bounds[i] = Math.min(bounds[a]!, bounds[b]!)
    bounds[i + 1] = Math.min(bounds[a + 1]!, bounds[b + 1]!)
    bounds[i + 2] = Math.min(bounds[a + 2]!, bounds[b + 2]!)
    bounds[i + 3] = Math.max(bounds[a + 3]!, bounds[b + 3]!)
    bounds[i + 4] = Math.max(bounds[a + 4]!, bounds[b + 4]!)
    bounds[i + 5] = Math.max(bounds[a + 5]!, bounds[b + 5]!)
  }

  let insertLeaf = (leaf: number): void => {
    if (root === -1) {
      root = leaf
      parent[leaf] = -1
      return
    }
    let i = leaf * 6
    let minX = bounds[i]!, minY = bounds[i + 1]!, minZ = bounds[i + 2]!
    let maxX = bounds[i + 3]!, maxY = bounds[i + 4]!, maxZ = bounds[i + 5]!
    // Descend toward the sibling that grows the least (the classic
    // incremental surface-area heuristic).
    let sibling = root
    while (child1[sibling] !== -1) {
      let a = child1[sibling]!
      let b = child2[sibling]!
      let costA = unionArea(a, minX, minY, minZ, maxX, maxY, maxZ) - area(a)
      let costB = unionArea(b, minX, minY, minZ, maxX, maxY, maxZ) - area(b)
      sibling = costA < costB ? a : b
    }
    let oldParent = parent[sibling]!
    let newParent = allocate()
    parent[newParent] = oldParent
    child1[newParent] = sibling
    child2[newParent] = leaf
    items[newParent] = undefined
    parent[sibling] = newParent
    parent[leaf] = newParent
    if (oldParent === -1) {
      root = newParent
    } else if (child1[oldParent] === sibling) {
      child1[oldParent] = newParent
    } else {
      child2[oldParent] = newParent
    }
    for (let n = newParent; n !== -1; n = parent[n]!) refit(n)
  }

  let removeLeaf = (leaf: number): void => {
    if (leaf === root) {
      root = -1
      return
    }
    let p = parent[leaf]!
    let sibling = child1[p] === leaf ? child2[p]! : child1[p]!
    let grand = parent[p]!
    parent[sibling] = grand
    if (grand === -1) {
      root = sibling
    } else {
      if (child1[grand] === p) child1[grand] = sibling
      else child2[grand] = sibling
      for (let n = grand; n !== -1; n = parent[n]!) refit(n)
    }
    child1[p] = -1
    items[p] = undefined
    free.push(p)
  }

  let setFat = (leaf: number, minX: number, minY: number, minZ: number, maxX: number, maxY: number, maxZ: number): void => {
    let m = MARGIN * Math.max(maxX - minX, maxY - minY, maxZ - minZ)
    let i = leaf * 6
    bounds[i] = minX - m
    bounds[i + 1] = minY - m
    bounds[i + 2] = minZ - m
    bounds[i + 3] = maxX + m
    bounds[i + 4] = maxY + m
    bounds[i + 5] = maxZ + m
  }

  return {
    insert(item, minX, minY, minZ, maxX, maxY, maxZ) {
      let leaf = allocate()
      items[leaf] = item
      setFat(leaf, minX, minY, minZ, maxX, maxY, maxZ)
      insertLeaf(leaf)
      return leaf
    },
    update(leaf, minX, minY, minZ, maxX, maxY, maxZ) {
      let i = leaf * 6
      if (
        bounds[i]! <= minX && bounds[i + 1]! <= minY && bounds[i + 2]! <= minZ &&
        bounds[i + 3]! >= maxX && bounds[i + 4]! >= maxY && bounds[i + 5]! >= maxZ
      ) {
        return false
      }
      removeLeaf(leaf)
      setFat(leaf, minX, minY, minZ, maxX, maxY, maxZ)
      insertLeaf(leaf)
      return true
    },
    remove(leaf) {
      removeLeaf(leaf)
      items[leaf] = undefined
      free.push(leaf)
    },
    raycast(ox, oy, oz, dx, dy, dz, visit) {
      if (root === -1) return
      stack.length = 0
      stack.push(root)
      while (stack.length > 0) {
        let n = stack.pop()!
        let i = n * 6
        let t = rayBoxDistance(ox, oy, oz, dx, dy, dz, bounds[i]!, bounds[i + 1]!, bounds[i + 2]!, bounds[i + 3]!, bounds[i + 4]!, bounds[i + 5]!)
        if (t < 0) continue
        if (child1[n] === -1) {
          visit(items[n] as T)
        } else {
          stack.push(child1[n]!, child2[n]!)
        }
      }
    },
  }
}
