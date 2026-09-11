//! Vertex normals from a triangle list: the recompute behind the 3d
//! package's `computeVertexNormals`, over the same interleaved float
//! vertex arrays the shapes read. Pure geometry, no arena state.

/// The unit normal of every vertex, 3 floats per vertex over `count`
/// vertices, from the triangles in `indices` over positions read from
/// `vertices` (`stride` floats per vertex, xyz at `offset`): each vertex
/// sums the unit normals of the faces that name it, each weighted by
/// the angle the face makes at that corner, then normalizes. Weighting
/// by the corner angle rather than by face area makes the result
/// independent of how a surface was triangulated (a quad's two
/// triangles contribute the same 90 degrees at each corner whichever
/// way its diagonal runs, so a smoothed cube gets exact corner
/// diagonals). A degenerate face counts for nothing and a vertex no face
/// names gets a zero normal. Errors on an offset outside the stride, a
/// vertex array that is not whole vertices, or an index past the last
/// vertex.
pub fn vertex_normals(vertices: &[f32], stride: usize, offset: usize, indices: &[u32]) -> Result<Vec<f32>, String> {
  if stride < 3 || offset + 3 > stride {
    return Err(format!("position offset {offset} does not fit a stride of {stride} floats"));
  }
  if vertices.len() % stride != 0 {
    return Err(format!("{} floats is not a whole number of {stride}-float vertices", vertices.len()));
  }
  let count = vertices.len() / stride;
  let at = |i: u32| -> Result<[f32; 3], String> {
    let i = i as usize;
    if i >= count {
      return Err(format!("index {i} is outside the {count} vertices"));
    }
    let base = i * stride + offset;
    Ok([vertices[base], vertices[base + 1], vertices[base + 2]])
  };
  let mut sums = vec![0.0f32; count * 3];
  for face in indices.chunks_exact(3) {
    let p = [at(face[0])?, at(face[1])?, at(face[2])?];
    let u = sub(p[1], p[0]);
    let w = sub(p[2], p[0]);
    let n = cross(u, w);
    let len = length(n);
    if len == 0.0 {
      continue;
    }
    let n = [n[0] / len, n[1] / len, n[2] / len];
    // Squared side lengths: a = p0-p1, b = p1-p2, c = p2-p0.
    let a2 = dot(u, u);
    let v = sub(p[2], p[1]);
    let b2 = dot(v, v);
    let c2 = dot(w, w);
    let angles = [corner(a2, b2, c2), corner(a2, c2, b2), corner(b2, a2, c2)];
    for k in 0..3 {
      let base = face[k] as usize * 3;
      for c in 0..3 {
        sums[base + c] += n[c] * angles[k];
      }
    }
  }
  for v in sums.chunks_exact_mut(3) {
    let len = length([v[0], v[1], v[2]]);
    if len > 0.0 {
      v[0] /= len;
      v[1] /= len;
      v[2] /= len;
    }
  }
  Ok(sums)
}

/// Write `values` (`components` floats per vertex) into an interleaved
/// vertex array at `offset` within each `stride`-float vertex: the
/// scatter that puts `vertex_normals` back into the vertex buffer. Errors
/// when the offset does not fit the stride or the counts disagree.
pub fn write_channel(
  values: &[f32],
  components: usize,
  target: &mut [f32],
  stride: usize,
  offset: usize,
) -> Result<(), String> {
  if offset + components > stride {
    return Err(format!("channel offset {offset} of {components} does not fit a stride of {stride} floats"));
  }
  let count = values.len() / components;
  if target.len() / stride != count || values.len() % components != 0 {
    return Err(format!(
      "{count} values of {components} do not match {} vertices of {stride} floats",
      target.len() / stride
    ));
  }
  for (i, v) in values.chunks_exact(components).enumerate() {
    let base = i * stride + offset;
    target[base..base + components].copy_from_slice(v);
  }
  Ok(())
}

/// The angle opposite side b of a triangle with squared side lengths
/// a2, b2, c2 (law of cosines); 0 at a corner with a zero-length side.
fn corner(a2: f32, b2: f32, c2: f32) -> f32 {
  let denom = 2.0 * (a2 * c2).sqrt();
  if denom == 0.0 {
    return 0.0;
  }
  ((a2 + c2 - b2) / denom).clamp(-1.0, 1.0).acos()
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
  [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
  [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
  a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(a: [f32; 3]) -> f32 {
  dot(a, a).sqrt()
}
