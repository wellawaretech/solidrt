use crate::path::{basename, dirname, extname};

// The three splits are lexical and follow Node's `path` module, so each
// case here is one Node answers the same way.

#[test]
fn basename_is_the_last_component() {
  assert_eq!(basename("/a/b.txt", None), "b.txt");
  assert_eq!(basename("b.txt", None), "b.txt");
  assert_eq!(basename("/a/b/", None), "b");
  assert_eq!(basename("/a/b//", None), "b");
  assert_eq!(basename("/", None), "");
  assert_eq!(basename("", None), "");
}

#[test]
fn basename_cuts_a_matching_extension() {
  assert_eq!(basename("/a/model.gltf", Some(".gltf")), "model");
  assert_eq!(basename("/a/model.gltf", Some(".glb")), "model.gltf");
  assert_eq!(basename("/a/model.gltf/", Some(".gltf")), "model");
  // A name that is nothing but the suffix keeps it.
  assert_eq!(basename("/a/.gltf", Some(".gltf")), ".gltf");
  assert_eq!(basename("/a/model.gltf", Some("")), "model.gltf");
}

#[test]
fn dirname_is_everything_before_the_last_component() {
  assert_eq!(dirname("/a/b.txt"), "/a");
  assert_eq!(dirname("/a/b/"), "/a");
  assert_eq!(dirname("a/b"), "a");
  assert_eq!(dirname("a//b"), "a");
  assert_eq!(dirname("b.txt"), ".");
  assert_eq!(dirname(""), ".");
  assert_eq!(dirname("/b"), "/");
  assert_eq!(dirname("/"), "/");
}

#[test]
fn extname_is_the_last_dot_of_the_name() {
  assert_eq!(extname("/a/model.gltf"), ".gltf");
  assert_eq!(extname("a.tar.gz"), ".gz");
  assert_eq!(extname("a."), ".");
  assert_eq!(extname("a"), "");
  assert_eq!(extname(".bashrc"), "");
  assert_eq!(extname(".bashrc.bak"), ".bak");
  assert_eq!(extname("/a.b/c"), "");
  assert_eq!(extname("/a/b.txt/"), ".txt");
  assert_eq!(extname(".."), "");
  assert_eq!(extname(""), "");
}
