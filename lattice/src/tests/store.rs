use crate::go::store::{app_info_at, install_at, list_installed_at, load_current_at, remove_app_at};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;

fn sha_hex(bytes: &[u8]) -> String {
  Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn manifest_for(code: &str) -> String {
  format!(
    r#"{{"appId":"com.example.app","runtimeVersion":1,"bundle":{{"path":"bundle.js","sha256":"{}","size":{}}}}}"#,
    sha_hex(code.as_bytes()),
    code.len()
  )
}

// A manifest with one assets entry, the shape buildManifest emits.
fn manifest_with_asset(code: &str, path: &str, asset: &[u8]) -> String {
  format!(
    r#"{{"appId":"com.example.app","runtimeVersion":1,"bundle":{{"path":"bundle.js","sha256":"{}","size":{}}},"assets":[{{"path":"{path}","sha256":"{}","size":{}}}]}}"#,
    sha_hex(code.as_bytes()),
    code.len(),
    sha_hex(asset),
    asset.len()
  )
}

fn temp_app_dir(tag: &str) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("srt-store-test-{}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  dir
}

#[test]
fn install_writes_version_and_state() {
  let app_dir = temp_app_dir("install");
  let code = "let a = 1";
  let manifest = manifest_for(code);
  let version = install_at(&app_dir, &manifest, code, &HashMap::new()).expect("installs");
  assert_eq!(version, sha_hex(manifest.as_bytes()));

  let version_dir = app_dir.join("versions").join(&version);
  assert_eq!(std::fs::read_to_string(version_dir.join("bundle.js")).expect("bundle written"), code);
  assert_eq!(std::fs::read_to_string(version_dir.join("manifest.json")).expect("manifest written"), manifest);
  assert!(app_dir.join("state.json").is_file());
  assert_eq!(load_current_at(&app_dir).expect("loads current"), code);
  let _ = std::fs::remove_dir_all(&app_dir);
}

#[test]
fn install_rejects_mismatched_bundle() {
  let app_dir = temp_app_dir("mismatch");
  let manifest = manifest_for("let a = 1");
  // Different code than the manifest hashes: nothing may be written.
  assert!(install_at(&app_dir, &manifest, "let a = 2", &HashMap::new()).is_err());
  assert!(load_current_at(&app_dir).is_none());
  assert!(!app_dir.join("state.json").exists());
  let _ = std::fs::remove_dir_all(&app_dir);
}

#[test]
fn reinstall_and_update_track_previous() {
  let app_dir = temp_app_dir("previous");
  let (code1, code2) = ("let v = 1", "let v = 2");
  let v1 = install_at(&app_dir, &manifest_for(code1), code1, &HashMap::new()).expect("installs v1");
  // Repush of the same version: current stays, no previous appears.
  install_at(&app_dir, &manifest_for(code1), code1, &HashMap::new()).expect("repush ok");
  let state: serde_json::Value =
    serde_json::from_slice(&std::fs::read(app_dir.join("state.json")).expect("state")).expect("state json");
  assert_eq!(state["current"], v1.as_str());
  assert!(state.get("previous").is_none());

  let v2 = install_at(&app_dir, &manifest_for(code2), code2, &HashMap::new()).expect("installs v2");
  let state: serde_json::Value =
    serde_json::from_slice(&std::fs::read(app_dir.join("state.json")).expect("state")).expect("state json");
  assert_eq!(state["current"], v2.as_str());
  assert_eq!(state["previous"], v1.as_str());
  assert_eq!(load_current_at(&app_dir).expect("loads current"), code2);
  let _ = std::fs::remove_dir_all(&app_dir);
}

#[test]
fn prune_keeps_five_versions() {
  let app_dir = temp_app_dir("prune");
  let mut versions = Vec::new();
  for i in 0..7 {
    let code = format!("let v = {i}");
    versions.push(install_at(&app_dir, &manifest_for(&code), &code, &HashMap::new()).expect("installs"));
  }
  let remaining: Vec<String> = std::fs::read_dir(app_dir.join("versions"))
    .expect("versions dir")
    .flatten()
    .map(|e| e.file_name().to_string_lossy().to_string())
    .collect();
  assert_eq!(remaining.len(), 5, "kept {remaining:?}");
  // Current and previous always survive a prune.
  assert!(remaining.contains(&versions[6]));
  assert!(remaining.contains(&versions[5]));
  let _ = std::fs::remove_dir_all(&app_dir);
}

#[test]
fn install_writes_fetched_assets_and_verifies() {
  let app_dir = temp_app_dir("assets");
  let code = "let a = 1";
  let asset = b"OggS fake sound";
  let manifest = manifest_with_asset(code, "assets/sounds/boing.ogg", asset);

  // Nothing held, nothing fetched: the install must refuse, not write a
  // version missing its files.
  assert!(install_at(&app_dir, &manifest, code, &HashMap::new()).is_err());
  assert!(!app_dir.join("state.json").exists());

  // Fetched bytes that do not match the manifest entry are refused too.
  let bad = HashMap::from([("assets/sounds/boing.ogg".to_string(), b"tampered".to_vec())]);
  assert!(install_at(&app_dir, &manifest, code, &bad).is_err());

  let fetched = HashMap::from([("assets/sounds/boing.ogg".to_string(), asset.to_vec())]);
  let version = install_at(&app_dir, &manifest, code, &fetched).expect("installs");
  let written = app_dir.join("versions").join(&version).join("assets/sounds/boing.ogg");
  assert_eq!(std::fs::read(written).expect("asset written"), asset);
  let _ = std::fs::remove_dir_all(&app_dir);
}

#[test]
fn update_reuses_held_assets() {
  let app_dir = temp_app_dir("hardlink");
  let asset = b"shared bytes";
  let (code1, code2) = ("let v = 1", "let v = 2");
  let fetched = HashMap::from([("assets/data.bin".to_string(), asset.to_vec())]);
  let v1 =
    install_at(&app_dir, &manifest_with_asset(code1, "assets/data.bin", asset), code1, &fetched).expect("installs v1");

  // Same asset in the next version: nothing fetched, the store links it from
  // the held version.
  let v2 = install_at(&app_dir, &manifest_with_asset(code2, "assets/data.bin", asset), code2, &HashMap::new())
    .expect("installs v2 from held assets");
  let file1 = app_dir.join("versions").join(&v1).join("assets/data.bin");
  let file2 = app_dir.join("versions").join(&v2).join("assets/data.bin");
  assert_eq!(std::fs::read(&file2).expect("linked asset"), asset);
  #[cfg(unix)]
  {
    use std::os::unix::fs::MetadataExt;
    let (m1, m2) = (std::fs::metadata(&file1).expect("v1 meta"), std::fs::metadata(&file2).expect("v2 meta"));
    assert_eq!(m1.ino(), m2.ino(), "same inode: hardlinked, not copied");
  }

  // A changed asset (same path, new hash) is NOT reused: without its bytes the
  // install refuses.
  let changed = manifest_with_asset("let v = 3", "assets/data.bin", b"different bytes");
  assert!(install_at(&app_dir, &changed, "let v = 3", &HashMap::new()).is_err());
  let _ = std::fs::remove_dir_all(&app_dir);
}

#[test]
fn list_skips_anchor_only_dirs_and_reads_display_names() {
  let apps = temp_app_dir("list");
  let code = "let a = 1";
  // One plain install (no displayName), one with a displayName, and one dir
  // that is only a data sandbox (anchored, never installed).
  install_at(&apps.join("com.example.app"), &manifest_for(code), code, &HashMap::new()).expect("installs");
  let named = format!(
    r#"{{"appId":"com.example.named","displayName":"Named App","runtimeVersion":1,"bundle":{{"path":"bundle.js","sha256":"{}","size":{}}}}}"#,
    sha_hex(code.as_bytes()),
    code.len()
  );
  install_at(&apps.join("com.example.named"), &named, code, &HashMap::new()).expect("installs named");
  std::fs::create_dir_all(apps.join("com.example.sandbox").join("data")).expect("sandbox dir");

  let listed = list_installed_at(&apps);
  assert_eq!(listed.len(), 2, "sandbox-only dir must not be listed");
  // Newest first, and the named app was installed last. Both installs can land
  // inside one mtime tick, but the name tie-break ("Named App" < ...) puts it
  // first either way.
  assert_eq!(listed[0].id, "com.example.named");
  assert_eq!(listed[0].name, "Named App");
  assert_eq!(listed[1].id, "com.example.app");
  assert_eq!(listed[1].name, "com.example.app");
  assert!(!listed[1].version.is_empty());
  // The declared size is the bundle's; neither app carries assets.
  assert!(listed.iter().all(|a| a.updated > 0));
  assert!(listed.iter().all(|a| a.size == code.len() as u64));
  let _ = std::fs::remove_dir_all(&apps);
}

#[test]
fn remove_deletes_only_valid_installed_ids() {
  let apps = temp_app_dir("remove");
  let code = "let a = 1";
  let app_dir = apps.join("com.example.app");
  install_at(&app_dir, &manifest_for(code), code, &HashMap::new()).expect("installs");

  // Traversal and unknown ids are refused without touching anything.
  assert!(remove_app_at(&apps, "../com.example.app").is_err());
  assert!(remove_app_at(&apps, "..").is_err());
  assert!(remove_app_at(&apps, "com.example.missing").is_err());
  assert!(app_dir.is_dir());

  remove_app_at(&apps, "com.example.app").expect("removes");
  assert!(!app_dir.exists());
  let _ = std::fs::remove_dir_all(&apps);
}

#[test]
fn app_info_reports_versions_files_and_data_usage() {
  let apps = temp_app_dir("info");
  let (code1, code2) = ("let v = 1", "let v = 22");
  let asset = b"jpg bytes";
  let app_dir = apps.join("com.example.app");
  install_at(&app_dir, &manifest_for(code1), code1, &HashMap::new()).expect("installs v1");
  let fetched = HashMap::from([("assets/hero.jpg".to_string(), asset.to_vec())]);
  let v2 =
    install_at(&app_dir, &manifest_with_asset(code2, "assets/hero.jpg", asset), code2, &fetched).expect("installs v2");
  // A data sandbox with one file and one subdir holding another file.
  std::fs::create_dir_all(app_dir.join("data/nested")).expect("data dirs");
  std::fs::write(app_dir.join("data/top.txt"), b"12345").expect("data file");
  std::fs::write(app_dir.join("data/nested/inner.bin"), b"123").expect("nested file");

  // Invalid and uninstalled ids are refused.
  assert!(app_info_at(&apps, "../com.example.app").is_err());
  assert!(app_info_at(&apps, "com.example.missing").is_err());

  let info = app_info_at(&apps, "com.example.app").expect("info");
  assert_eq!(info.id, "com.example.app");
  assert_eq!(info.version, v2);
  // Un-stamped manifests (pre-solidrtVersion CLIs) default to "unknown".
  assert!(info.versions.iter().all(|v| v.solidrt_version == "unknown"));
  assert_eq!(info.versions.len(), 2);
  // Current first, and every version's size covers its bundle + manifest.
  assert!(info.versions[0].current);
  assert_eq!(info.versions[0].id, v2);
  assert!(!info.versions[1].current);
  for v in &info.versions {
    assert!(v.size > 0, "version {} has a size", v.id);
  }
  assert_eq!(info.install_size, info.versions.iter().map(|v| v.size).sum::<u64>());

  // Files: disk walk of the current version dir (bundle + manifest + the
  // asset), sorted.
  let file_paths: Vec<&str> = info.version_files.iter().map(|f| f.path.as_str()).collect();
  assert_eq!(file_paths, ["assets/hero.jpg", "bundle.js", "manifest.json"]);

  // Data: recursive file walk, sorted by path.
  assert_eq!(info.data_size, 8);
  let data_paths: Vec<&str> = info.data_files.iter().map(|f| f.path.as_str()).collect();
  assert_eq!(data_paths, ["nested/inner.bin", "top.txt"]);
  assert_eq!(info.data_files[0].size, 3);
  assert_eq!(info.data_files[1].size, 5);

  // No cache dir yet: an empty cache (forge's scan tests cover a populated
  // one; the entry format belongs there).
  assert_eq!(info.cache_size, 0);
  assert!(info.cache.is_empty());
  let _ = std::fs::remove_dir_all(&apps);
}

#[test]
fn app_info_reports_solidrt_version() {
  let apps = temp_app_dir("solidrt-version");
  let code = "let s = 1";
  let manifest = format!(
    r#"{{"appId":"com.example.app","runtimeVersion":1,"solidrtVersion":"1.2.3","bundle":{{"path":"bundle.js","sha256":"{}","size":{}}}}}"#,
    sha_hex(code.as_bytes()),
    code.len()
  );
  install_at(&apps.join("com.example.app"), &manifest, code, &HashMap::new()).expect("installs");
  let info = app_info_at(&apps, "com.example.app").expect("info");
  assert_eq!(info.versions.len(), 1);
  assert_eq!(info.versions[0].solidrt_version, "1.2.3");
  let _ = std::fs::remove_dir_all(&apps);
}

#[test]
fn install_rejects_unsafe_asset_paths() {
  let app_dir = temp_app_dir("unsafe");
  let code = "let a = 1";
  for path in ["assets/../escape", "/etc/passwd", "assets/a\\b", "other/file", "assets//x"] {
    let asset = b"bytes";
    let manifest = manifest_with_asset(code, path, asset);
    let fetched = HashMap::from([(path.to_string(), asset.to_vec())]);
    assert!(install_at(&app_dir, &manifest, code, &fetched).is_err(), "accepted {path}");
  }
  assert!(!app_dir.join("state.json").exists());
  let _ = std::fs::remove_dir_all(&app_dir);
}
