use crate::spatial::{vertex_normals, write_channel};

// The 8 corners of the unit cube, 8-float vertices (position, a zero
// normal slot, uv) so the test reads like the 3d package's standard
// layout; corner i has bit 0 for +x, bit 1 for +y, bit 2 for +z.
fn cube_vertices() -> Vec<f32> {
  let mut v = Vec::new();
  for i in 0..8u32 {
    let x = if i & 1 != 0 { 0.5 } else { -0.5 };
    let y = if i & 2 != 0 { 0.5 } else { -0.5 };
    let z = if i & 4 != 0 { 0.5 } else { -0.5 };
    v.extend_from_slice(&[x, y, z, 0.0, 0.0, 0.0, 0.0, 0.0]);
  }
  v
}

// The cube's twelve triangles, counter-clockwise from outside, each face
// split along one diagonal.
const CUBE_INDICES: [u32; 36] = [
  5, 1, 3, 5, 3, 7, // +x
  0, 4, 6, 0, 6, 2, // -x
  2, 6, 7, 2, 7, 3, // +y
  0, 1, 5, 0, 5, 4, // -y
  4, 5, 7, 4, 7, 6, // +z
  0, 2, 3, 0, 3, 1, // -z
];

fn near(a: f32, b: f32) -> bool {
  (a - b).abs() < 1e-5
}

#[test]
fn shared_cube_corners_get_exact_diagonals() {
  let normals = vertex_normals(&cube_vertices(), 8, 0, &CUBE_INDICES).expect("cube");
  let d = 1.0 / 3.0f32.sqrt();
  for i in 0..8usize {
    let want = [if i & 1 != 0 { d } else { -d }, if i & 2 != 0 { d } else { -d }, if i & 4 != 0 { d } else { -d }];
    for c in 0..3 {
      assert!(near(normals[i * 3 + c], want[c]), "corner {i} component {c}: {} vs {}", normals[i * 3 + c], want[c]);
    }
  }
}

#[test]
fn split_faces_get_face_normals_and_unreferenced_vertices_zero() {
  // One +z triangle over vertices 4, 5, 7; the other five corners are
  // named by no face.
  let normals = vertex_normals(&cube_vertices(), 8, 0, &[4, 5, 7]).expect("triangle");
  for i in [4usize, 5, 7] {
    assert!(
      near(normals[i * 3], 0.0) && near(normals[i * 3 + 1], 0.0) && near(normals[i * 3 + 2], 1.0),
      "vertex {i} faces +z"
    );
  }
  for i in [0usize, 1, 2, 3, 6] {
    assert_eq!(&normals[i * 3..i * 3 + 3], &[0.0, 0.0, 0.0], "vertex {i} is unreferenced");
  }
}

#[test]
fn degenerate_faces_count_for_nothing() {
  let normals = vertex_normals(&cube_vertices(), 8, 0, &[4, 5, 7, 4, 4, 5]).expect("with a sliver");
  assert!(near(normals[4 * 3 + 2], 1.0), "the zero-area face adds nothing");
}

#[test]
fn write_channel_scatters_at_the_offset_and_errors_are_named() {
  let mut vertices = cube_vertices();
  let normals = vertex_normals(&vertices, 8, 0, &CUBE_INDICES).expect("cube");
  write_channel(&normals, 3, &mut vertices, 8, 3).expect("write");
  for i in 0..8usize {
    assert_eq!(&vertices[i * 8 + 3..i * 8 + 6], &normals[i * 3..i * 3 + 3]);
    assert_eq!(&vertices[i * 8 + 6..i * 8 + 8], &[0.0, 0.0], "uv untouched");
  }
  assert!(write_channel(&normals, 3, &mut vertices, 8, 6).is_err(), "offset past the stride");
  assert!(write_channel(&normals[..9], 3, &mut vertices, 8, 3).is_err(), "count mismatch");
  assert!(vertex_normals(&vertices, 8, 0, &[0, 1, 8]).is_err(), "index past the last vertex");
  assert!(vertex_normals(&vertices[..7], 8, 0, &[]).is_err(), "not whole vertices");
  assert!(vertex_normals(&vertices, 8, 6, &[]).is_err(), "offset past the stride");
}
