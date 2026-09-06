use crate::storage::{resolve, Storage, StorageSpec};

fn temp_root(tag: &str) -> std::path::PathBuf {
  let dir = std::env::temp_dir().join(format!("srt-storage-test-{}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  dir
}

#[test]
fn resolve_creates_tree_under_explicit_root() {
  let root = temp_root("tree");
  let spec = StorageSpec {
    data_root: Some(root.clone()),
    client: Some(3),
    app_id: Some("com.example.app".into()),
  };
  let store = resolve(&spec).expect("resolves");
  let client_dir = root.join("client3");
  assert_eq!(store.client_dir, client_dir);
  assert_eq!(store.data_dir, client_dir.join("apps").join("com.example.app").join("data"));
  assert_eq!(store.cache_dir("com.example.app"), client_dir.join("apps").join("com.example.app").join("cache"));
  for dir in [&store.data_dir, &client_dir.join("identity"), &client_dir.join("logs")] {
    assert!(dir.is_dir(), "missing {}", dir.display());
  }
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn resolve_defaults_client_and_app() {
  let root = temp_root("defaults");
  let spec = StorageSpec { data_root: Some(root.clone()), client: None, app_id: None };
  let store = resolve(&spec).expect("resolves");
  assert_eq!(store.data_dir, root.join("client0").join("apps").join("default").join("data"));
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn resolve_rejects_unsafe_components() {
  let root = temp_root("unsafe");
  // Traversal or separators in an app id must not escape the tree.
  let spec = StorageSpec { data_root: Some(root.clone()), client: None, app_id: Some("..".into()) };
  let store = resolve(&spec).expect("resolves");
  assert_eq!(store.data_dir, root.join("client0").join("apps").join("default").join("data"));
  let spec = StorageSpec { data_root: Some(root.clone()), client: None, app_id: Some("a/b".into()) };
  let store = resolve(&spec).expect("resolves");
  assert_eq!(store.data_dir, root.join("client0").join("apps").join("default").join("data"));
  let _ = std::fs::remove_dir_all(&root);
}

// The player layout resolves through the real pref path; pointing SDL's
// XDG base at a temp dir keeps the test out of the user's data dir. Linux
// only: the base env var is platform-specific. No other test reads it.
#[cfg(target_os = "linux")]
#[test]
fn player_tree_has_no_client_level() {
  let root = temp_root("go");
  std::env::set_var("XDG_DATA_HOME", &root);
  // --client is data-root-only: one install is one client, so the number is
  // ignored (with a warning) and no client<N> level appears.
  let spec = StorageSpec { data_root: None, client: Some(1), app_id: None };
  let store = resolve(&spec).expect("resolves");
  std::env::remove_var("XDG_DATA_HOME");
  let client_dir = root.join("SolidRT").join("go");
  assert_eq!(store.client_dir, client_dir);
  assert_eq!(store.data_dir, client_dir.join("apps").join("default").join("data"));
  let _ = std::fs::remove_dir_all(&root);
}

// The run marker warns about, but does not prevent, a second live client on
// the same tree; dropping the first (its lock dies with the process) frees
// the tree for the next claim.
#[test]
fn second_client_on_one_tree_is_detected() {
  let root = temp_root("marker");
  let spec = StorageSpec { data_root: Some(root.clone()), client: None, app_id: None };
  let first = resolve(&spec).expect("resolves");
  assert!(first.run_marker.is_some());
  let second = resolve(&spec).expect("a warning, not a lock: still resolves");
  assert!(second.run_marker.is_none());
  drop(first);
  let third = resolve(&spec).expect("resolves");
  assert!(third.run_marker.is_some());
  let _ = std::fs::remove_dir_all(&root);
}

// Restores the original cwd when dropped, so a chdir-ing test cannot leave
// the process elsewhere for the rest of the suite - including on a failed
// assert. The other tests here use absolute paths only, but keep it that way.
struct CwdGuard(std::path::PathBuf);
impl Drop for CwdGuard {
  fn drop(&mut self) {
    let _ = std::env::set_current_dir(&self.0);
  }
}

// The player can remove an app (or wipe its cache) while the client keeps
// running, unlinking the inode the cwd points at; getcwd then fails and every
// relative open ENOENTs. anchor_dir must recover from that, not just anchor
// once. Unix only: Windows refuses to delete the current directory, so the
// stranded-cwd state cannot exist there.
#[cfg(unix)]
#[test]
fn anchor_dir_recovers_from_deleted_cwd() {
  let _guard = CwdGuard(std::env::current_dir().expect("cwd"));
  let root = temp_root("anchor");
  let data_dir = root.join("data");

  // First anchor creates the sandbox and enters it.
  assert_eq!(crate::anchor_dir(&data_dir), Ok(true));
  assert_eq!(std::env::current_dir().expect("cwd"), data_dir.canonicalize().expect("canonical"));
  // Anchoring again is a no-op.
  assert_eq!(crate::anchor_dir(&data_dir), Ok(false));

  // Delete the sandbox out from under the anchor (the player's app remove).
  std::fs::remove_dir_all(&root).expect("remove root");
  assert!(std::env::current_dir().is_err(), "getcwd should fail on a deleted cwd");

  // Re-anchoring rebuilds the sandbox and relative writes work again - the
  // original failure mode was `file("atlas-font14.bin").write()` ENOENTing.
  assert_eq!(crate::anchor_dir(&data_dir), Ok(true));
  std::fs::write("probe.bin", b"ok").expect("relative write");
  assert!(data_dir.join("probe.bin").is_file());

  let _ = std::fs::remove_dir_all(&root);
}

// The packed flat layout (no --data-root, app id present) resolves through
// the real pref path, so its shape is asserted on a hand-built Storage
// instead of a resolve() that would write into the user's data dir.
#[test]
fn flat_app_dir_is_the_root() {
  let root = std::path::PathBuf::from("/packed-app");
  let store = Storage { client_dir: root.clone(), data_dir: root.join("data"), flat: true, run_marker: None };
  assert_eq!(store.app_dir("com.example.app"), root);
  assert_eq!(store.app_dir("anything-else"), root);
  // The single app's cache sits at the root, like every other app dir.
  assert_eq!(store.cache_dir("com.example.app"), root.join("cache"));
  assert_eq!(store.identity_dir(), root.join("identity"));
}
