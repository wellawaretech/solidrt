// A dynamic bounding-volume hierarchy - the physics-broadphase AABB tree
// (Box2D/Bullet lineage): a binary tree over the items' world boxes, NOT a
// subdivision of space. Leaves store FAT boxes (a margin around the tight
// bounds) so an item moving a little refits nothing; an item escaping its
// fat box is removed and re-inserted along the cheapest path (least area
// growth). A ray query walks O(log n) nodes instead of testing every item.
// Flat parallel storage indexed by tree node; the item is the caller's
// u32 (a spatial node index).

/// Fat-margin fraction of a leaf's largest extent. Bigger = fewer
/// re-inserts while moving, worse query pruning; 5% is the usual trade.
const MARGIN: f32 = 0.05;

pub type Box3 = [f32; 6];

pub struct Bvh {
  bounds: Vec<Box3>,
  parent: Vec<i32>,
  child1: Vec<i32>,
  child2: Vec<i32>,
  items: Vec<u32>,
  free: Vec<i32>,
  root: i32,
  stack: Vec<i32>,
}

/// Entry distance of a ray against a box: the smallest t >= 0 with
/// origin + t * direction inside the box (0 when the origin starts
/// inside), or None for a miss. The direction need not be normalized - t
/// is in units of its length, which is what keeps a ray transformed into
/// a mesh's local space reporting world distances.
pub fn ray_box_distance(o: [f32; 3], d: [f32; 3], b: &Box3) -> Option<f32> {
  let mut t_near = 0.0f32;
  let mut t_far = f32::INFINITY;
  for axis in 0..3 {
    // A zero direction component never crosses the slab, so the origin
    // must already be inside it (the inverse shortcut would yield NaN).
    if d[axis] == 0.0 {
      if o[axis] < b[axis] || o[axis] > b[axis + 3] {
        return None;
      }
    } else {
      let inv = 1.0 / d[axis];
      let mut t1 = (b[axis] - o[axis]) * inv;
      let mut t2 = (b[axis + 3] - o[axis]) * inv;
      if t1 > t2 {
        std::mem::swap(&mut t1, &mut t2);
      }
      t_near = t_near.max(t1);
      t_far = t_far.min(t2);
    }
  }
  if t_far >= t_near {
    Some(t_near)
  } else {
    None
  }
}

fn half_area(b: &Box3) -> f32 {
  let w = b[3] - b[0];
  let h = b[4] - b[1];
  let d = b[5] - b[2];
  w * (h + d) + h * d
}

fn union(a: &Box3, b: &Box3) -> Box3 {
  [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2]), a[3].max(b[3]), a[4].max(b[4]), a[5].max(b[5])]
}

fn fatten(b: &Box3) -> Box3 {
  let m = MARGIN * (b[3] - b[0]).max(b[4] - b[1]).max(b[5] - b[2]);
  [b[0] - m, b[1] - m, b[2] - m, b[3] + m, b[4] + m, b[5] + m]
}

impl Default for Bvh {
  fn default() -> Self {
    Bvh {
      bounds: Vec::new(),
      parent: Vec::new(),
      child1: Vec::new(),
      child2: Vec::new(),
      items: Vec::new(),
      free: Vec::new(),
      root: -1,
      stack: Vec::new(),
    }
  }
}

impl Bvh {
  fn allocate(&mut self) -> i32 {
    if let Some(n) = self.free.pop() {
      return n;
    }
    self.bounds.push([0.0; 6]);
    self.parent.push(-1);
    self.child1.push(-1);
    self.child2.push(-1);
    self.items.push(0);
    (self.bounds.len() - 1) as i32
  }

  fn refit(&mut self, n: i32) {
    let a = self.bounds[self.child1[n as usize] as usize];
    let b = self.bounds[self.child2[n as usize] as usize];
    self.bounds[n as usize] = union(&a, &b);
  }

  fn insert_leaf(&mut self, leaf: i32) {
    if self.root == -1 {
      self.root = leaf;
      self.parent[leaf as usize] = -1;
      return;
    }
    let lb = self.bounds[leaf as usize];
    // Descend toward the sibling that grows the least (the classic
    // incremental surface-area heuristic).
    let mut sibling = self.root;
    while self.child1[sibling as usize] != -1 {
      let a = self.child1[sibling as usize];
      let b = self.child2[sibling as usize];
      let ba = &self.bounds[a as usize];
      let bb = &self.bounds[b as usize];
      let cost_a = half_area(&union(ba, &lb)) - half_area(ba);
      let cost_b = half_area(&union(bb, &lb)) - half_area(bb);
      sibling = if cost_a < cost_b { a } else { b };
    }
    let old_parent = self.parent[sibling as usize];
    let new_parent = self.allocate();
    self.parent[new_parent as usize] = old_parent;
    self.child1[new_parent as usize] = sibling;
    self.child2[new_parent as usize] = leaf;
    self.parent[sibling as usize] = new_parent;
    self.parent[leaf as usize] = new_parent;
    if old_parent == -1 {
      self.root = new_parent;
    } else if self.child1[old_parent as usize] == sibling {
      self.child1[old_parent as usize] = new_parent;
    } else {
      self.child2[old_parent as usize] = new_parent;
    }
    let mut n = new_parent;
    while n != -1 {
      self.refit(n);
      self.rotate(n);
      n = self.parent[n as usize];
    }
  }

  // One SAH rotation at n (Box2D lineage): try swapping each child with a
  // grandchild from the other side and keep the swap that shrinks the
  // rotated child's box the most. Applied along every refit walk, this
  // keeps the tree shallow under adversarial insertion orders (a grid
  // inserted row by row degenerates a rotation-free SAH tree into deep
  // chains) at a constant cost per walked node.
  fn rotate(&mut self, n: i32) {
    let c1 = self.child1[n as usize];
    let c2 = self.child2[n as usize];
    if c1 == -1 {
      return;
    }
    // (gain, rotated child, its kept grandchild slot, the swapped-in node)
    let mut best: Option<(f32, i32, bool, i32)> = None;
    let mut consider = |rotated: i32, swapped_in: i32, bvh: &Bvh| {
      if bvh.child1[rotated as usize] == -1 {
        return;
      }
      let g1 = bvh.child1[rotated as usize];
      let g2 = bvh.child2[rotated as usize];
      let current = half_area(&bvh.bounds[rotated as usize]);
      let sb = &bvh.bounds[swapped_in as usize];
      // Swap `swapped_in` with g2 (keeping g1), then with g1 (keeping g2).
      for (keep_first, kept) in [(true, g1), (false, g2)] {
        let gain = current - half_area(&union(&bvh.bounds[kept as usize], sb));
        if gain > 0.0 && best.is_none_or(|(g, ..)| gain > g) {
          best = Some((gain, rotated, keep_first, swapped_in));
        }
      }
    };
    consider(c1, c2, self);
    consider(c2, c1, self);
    let Some((_, rotated, keep_first, swapped_in)) = best else {
      return;
    };
    let dropped = if keep_first { self.child2[rotated as usize] } else { self.child1[rotated as usize] };
    // `swapped_in` (the other child of n) takes the dropped grandchild's
    // slot; the dropped grandchild becomes n's direct child.
    if keep_first {
      self.child2[rotated as usize] = swapped_in;
    } else {
      self.child1[rotated as usize] = swapped_in;
    }
    self.parent[swapped_in as usize] = rotated;
    if self.child1[n as usize] == swapped_in {
      self.child1[n as usize] = dropped;
    } else {
      self.child2[n as usize] = dropped;
    }
    self.parent[dropped as usize] = n;
    self.refit(rotated);
    self.refit(n);
  }

  /// Longest root-to-leaf path (tests: tree-quality assertions).
  #[cfg(test)]
  pub(crate) fn depth(&self) -> usize {
    fn walk(bvh: &Bvh, n: i32) -> usize {
      if n == -1 || bvh.child1[n as usize] == -1 {
        return 0;
      }
      1 + walk(bvh, bvh.child1[n as usize]).max(walk(bvh, bvh.child2[n as usize]))
    }
    walk(self, self.root)
  }

  fn remove_leaf(&mut self, leaf: i32) {
    if leaf == self.root {
      self.root = -1;
      return;
    }
    let p = self.parent[leaf as usize];
    let sibling = if self.child1[p as usize] == leaf { self.child2[p as usize] } else { self.child1[p as usize] };
    let grand = self.parent[p as usize];
    self.parent[sibling as usize] = grand;
    if grand == -1 {
      self.root = sibling;
    } else {
      if self.child1[grand as usize] == p {
        self.child1[grand as usize] = sibling;
      } else {
        self.child2[grand as usize] = sibling;
      }
      let mut n = grand;
      while n != -1 {
        self.refit(n);
        self.rotate(n);
        n = self.parent[n as usize];
      }
    }
    self.child1[p as usize] = -1;
    self.free.push(p);
  }

  /// Insert an item with its tight world box; returns the leaf handle.
  pub fn insert(&mut self, item: u32, tight: &Box3) -> u32 {
    let leaf = self.allocate();
    self.items[leaf as usize] = item;
    self.child1[leaf as usize] = -1;
    self.bounds[leaf as usize] = fatten(tight);
    self.insert_leaf(leaf);
    leaf as u32
  }

  /// Update a leaf's tight box: free while it stays inside the fat box,
  /// otherwise the leaf re-inserts.
  pub fn update(&mut self, leaf: u32, tight: &Box3) {
    let fat = &self.bounds[leaf as usize];
    if fat[0] <= tight[0]
      && fat[1] <= tight[1]
      && fat[2] <= tight[2]
      && fat[3] >= tight[3]
      && fat[4] >= tight[4]
      && fat[5] >= tight[5]
    {
      return;
    }
    let leaf = leaf as i32;
    self.remove_leaf(leaf);
    self.bounds[leaf as usize] = fatten(tight);
    self.insert_leaf(leaf);
  }

  /// Remove a leaf (the handle is dead afterwards).
  pub fn remove(&mut self, leaf: u32) {
    self.remove_leaf(leaf as i32);
    self.free.push(leaf as i32);
  }

  /// Visit every item whose FAT box the ray hits. Broadphase only: the
  /// caller narrowphases against its own tight volumes.
  pub fn raycast(&mut self, o: [f32; 3], d: [f32; 3], visit: &mut dyn FnMut(u32)) {
    if self.root == -1 {
      return;
    }
    let mut stack = std::mem::take(&mut self.stack);
    stack.clear();
    stack.push(self.root);
    while let Some(n) = stack.pop() {
      if ray_box_distance(o, d, &self.bounds[n as usize]).is_none() {
        continue;
      }
      if self.child1[n as usize] == -1 {
        visit(self.items[n as usize]);
      } else {
        stack.push(self.child1[n as usize]);
        stack.push(self.child2[n as usize]);
      }
    }
    self.stack = stack;
  }
}
