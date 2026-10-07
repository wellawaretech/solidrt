use crate::path::{basename, dirname, extname, matches_glob, relative};

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

// Every row: the pattern, the paths it matches, the paths it does not.
fn check_glob(rows: &[(&str, &[&str], &[&str])]) {
  for (pattern, matching, others) in rows {
    for path in *matching {
      assert_eq!(matches_glob(path, pattern), Ok(true), "{pattern} matches {path}");
    }
    for path in *others {
      assert_eq!(matches_glob(path, pattern), Ok(false), "{pattern} does not match {path}");
    }
  }
}

#[test]
fn a_glob_without_wildcards_is_the_path_itself() {
  check_glob(&[
    (
      "assets/lion.png",
      &["assets/lion.png"],
      &["assets/lion.png.bak", "x/assets/lion.png", "assets/Lion.png", "assets/lionxpng"],
    ),
    ("a.b+c(d)$^|{e,f}", &["a.b+c(d)$^|{e,f}"], &["aXb+c(d)$^|{e,f}", "a.b+c(d)$^|e"]),
  ]);
}

#[test]
fn a_glob_star_stays_inside_its_segment() {
  check_glob(&[
    (
      "assets/*.png",
      &["assets/lion.png", "assets/.png", "assets/.hidden.png"],
      &["assets/sub/lion.png", "assets/lion.jpg", "lion.png"],
    ),
    ("assets/lion_*.png", &["assets/lion_.png", "assets/lion_head.png"], &["assets/lion.png", "assets/lion_a/b.png"]),
    ("a/*/c", &["a/b/c"], &["a/c", "a/b/b/c"]),
    ("tile_?.png", &["tile_1.png", "tile_a.png"], &["tile_.png", "tile_12.png", "tile_/.png"]),
  ]);
}

#[test]
fn a_glob_double_star_spans_segments() {
  check_glob(&[
    ("assets/**", &["assets/a.png", "assets/a/b/c.png"], &["assets", "assetsx/a.png", "x/assets/a.png"]),
    ("**/*.png", &["a.png", "a/b.png", "a/b/c.png"], &["a.jpg", "a/b.png/c"]),
    ("a/**/b", &["a/b", "a/x/b", "a/x/y/b"], &["ab", "a/xb", "a/b/c"]),
    ("**", &["a", "a/b/c"], &[]),
  ]);
}

#[test]
fn a_glob_set_is_one_character_and_never_the_separator() {
  check_glob(&[
    ("tile_[0-9].png", &["tile_0.png", "tile_9.png"], &["tile_a.png", "tile_10.png"]),
    ("tile_[!0-9].png", &["tile_a.png"], &["tile_0.png"]),
    ("a[!x]b", &["ayb"], &["axb", "a/b"]),
    ("[]]", &["]"], &["["]),
    ("a[*]b", &["a*b"], &["axb", "ab"]),
    ("hero.glb[#]lut", &["hero.glb#lut"], &["hero.glb"]),
  ]);
}

#[test]
fn a_malformed_glob_errs_naming_where() {
  assert_eq!(
    matches_glob("ab", "a**b"),
    Err(
      "the pattern \"a**b\" is malformed at character 0: recursive wildcards must form a single path component"
        .to_string()
    )
  );
  assert_eq!(
    matches_glob("ab", "a[b"),
    Err("the pattern \"a[b\" is malformed at character 1: invalid range pattern".to_string())
  );
  assert!(matches_glob("a", "a/***").is_err());
}

// Absolute paths, so the process cwd has no say; unix spelling.
#[cfg(unix)]
#[test]
fn relative_leads_from_one_place_to_another() {
  assert_eq!(relative("/a/b", "/a/b/c/d.txt"), "c/d.txt");
  assert_eq!(relative("/a/b", "/a/c/d.txt"), "../c/d.txt");
  assert_eq!(relative("/a/b/c", "/d"), "../../../d");
  assert_eq!(relative("/a/b", "/a/b"), "");
  assert_eq!(relative("/a/b/", "/a/b/../b/c"), "c");
  assert_eq!(relative("/", "/a"), "a");
}

// A relative path stands against the cwd, on both sides.
#[test]
fn relative_resolves_against_the_cwd() {
  let cwd = std::env::current_dir().expect("the process has a cwd");
  let inside = cwd.join("assets").join("a.png");
  assert_eq!(relative(".", &inside.to_string_lossy()), std::path::Path::new("assets").join("a.png").to_string_lossy());
  assert_eq!(
    relative("assets", "assets/textures/a.png"),
    std::path::Path::new("textures").join("a.png").to_string_lossy()
  );
}
