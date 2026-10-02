use crate::manifest::{AppFonts, Manifest};
use crate::merge_fonts;
use alloy::rendertree::FontPayload;
use std::borrow::Cow;
use std::path::PathBuf;

fn font(alias: &str, bytes: &'static [u8]) -> FontPayload {
  FontPayload { alias: Some(alias.to_string()), bytes: Cow::Borrowed(bytes) }
}

fn aliases(fonts: &[FontPayload]) -> Vec<&str> {
  fonts.iter().map(|f| f.alias.as_deref().unwrap_or("<unaliased>")).collect()
}

fn temp_dir(tag: &str) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("sol-fonts-test-{}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  std::fs::create_dir_all(dir.join("assets/fonts")).expect("create temp dir");
  dir
}

#[test]
fn merge_replaces_base_entries_under_app_aliases() {
  let base = [font("sans", b"noto-sans"), font("serif", b"noto-serif"), font("mono", b"noto-mono")];
  let app = AppFonts {
    fonts: vec![font("sans", b"custom-sans"), font("display", b"custom-display")],
    aliases: vec!["sans".to_string(), "display".to_string()],
  };
  let merged = merge_fonts(&base, app);
  // The app's sans replaces the base one instead of joining it under the
  // same alias; untouched roles keep the base font; added aliases append.
  assert_eq!(aliases(&merged), ["serif", "mono", "sans", "display"]);
  assert_eq!(merged[2].bytes.as_ref(), b"custom-sans");
}

#[test]
fn merge_drops_a_base_entry_the_app_binds_to_nothing() {
  let base = [font("sans", b"noto-sans"), font("mono", b"noto-mono")];
  let app = AppFonts { fonts: Vec::new(), aliases: vec!["mono".to_string()] };
  // A dropped default (`"mono": false`) claims the alias without a file: the
  // base mono goes and nothing takes its place, so the role falls through to
  // the system font.
  assert_eq!(aliases(&merge_fonts(&base, app)), ["sans"]);
}

#[test]
fn merge_without_app_fonts_is_the_base_set() {
  let base = [font("sans", b"noto-sans")];
  assert_eq!(aliases(&merge_fonts(&base, AppFonts::default())), ["sans"]);
}

#[test]
fn load_fonts_reads_bound_files_and_claims_every_alias() {
  let dir = temp_dir("load");
  std::fs::write(dir.join("assets/fonts/Custom.ttf"), b"custom-bytes").expect("write font");
  let manifest = Manifest::parse(
    r#"{"appId":"com.example.app","runtimeVersion":1,"bundle":{"sha256":"","size":0},
        "fonts":[{"alias":"sans","path":"assets/fonts/Custom.ttf"},{"alias":"mono"},
                 {"alias":"serif","path":"assets/fonts/Missing.ttf"},{"alias":"evil","path":"../etc/passwd"}]}"#,
  )
  .expect("manifest parses");
  let app = manifest.load_fonts(&dir);
  // Only the readable, in-tree file registers; the unbound role, the missing
  // file and the unsafe path register nothing but still claim their alias,
  // so a base font never fills a role the manifest bound.
  assert_eq!(aliases(&app.fonts), ["sans"]);
  assert_eq!(app.fonts[0].bytes.as_ref(), b"custom-bytes");
  assert_eq!(app.aliases, ["sans", "mono", "serif", "evil"]);
  let _ = std::fs::remove_dir_all(&dir);
}
