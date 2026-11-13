use crate::manifest::Manifest;

// A manifest with the required fields only, plus `extra` at the top level.
fn manifest(extra: &str) -> Manifest {
  Manifest::parse(&format!(r#"{{"appId":"com.example.app","bundle":{{"sha256":"","size":0}}{extra}}}"#))
    .expect("parse manifest")
}

#[test]
fn window_size_reads_the_declared_pair() {
  let manifest = manifest(r#","windowSize":{"width":900,"height":600}"#);
  assert_eq!(manifest.window_size(), Some((900, 600)));
}

#[test]
fn window_size_is_none_without_the_key() {
  assert_eq!(manifest("").window_size(), None);
}
