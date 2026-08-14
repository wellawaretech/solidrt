// The factory payload a packed distribution boots: either the single-file
// trailer (the pack folder in section form, appended to this binary by
// `srt pack`) or a folder next to the runner (`srt pack --folder`). One shape
// behind both (okf/plans/client-storage-updates.md, stage 3): the manifest
// defines the version's file set (the runner is deliberately unlisted) and
// carries the app identity; a .js bundle is JS source, anything else QuickJS
// bytecode; fonts come from the manifest annotations; assets resolve through
// the mount (a dir, or ranges inside this executable).
//
// Trailer layout, must match packages/cli/src/packer.ts: each section's bytes,
// then a table of section entries, then [table offset u64 LE][entry count
// u32 LE][magic]. Table entry: [kind u32 LE][offset u64 LE][len u64 LE]
// [name len u16 LE][name bytes]. Offsets are absolute file offsets. Sections:
// 1 = the canonical manifest JSON, 2 = a manifest-listed file (name = its
// manifest path), 3 = a GL library (name = its filename; extracted and
// preloaded before window setup, see gl_libs.rs). The CLI and runners ship
// pinned together, so there is no format version; every offset/length is
// bounds-checked and any mismatch degrades to "no payload" instead of
// misparsing.
#[cfg(not(feature = "go"))]
const EMBED_MAGIC: &[u8; 9] = b"SOLIDRT\x88\x44";
#[cfg(not(feature = "go"))]
const EMBED_TAIL_LEN: usize = 8 + 4 + EMBED_MAGIC.len(); // table offset + entry count + magic
#[cfg(not(feature = "go"))]
const SECTION_MANIFEST: u32 = 1;
#[cfg(not(feature = "go"))]
const SECTION_FILE: u32 = 2;
#[cfg(not(feature = "go"))]
const SECTION_GL_LIB: u32 = 3;

#[cfg(not(feature = "go"))]
struct FactoryPayload {
  app: lattice::AppSource,
  fonts: Vec<alloy::rendertree::FontPayload>,
  app_id: String,
  base: forge::fs::AssetsBase,
  // (filename, bytes) in section order; only the single-file trailer carries
  // these (a folder pack ships the libraries next to the runner instead).
  gl_libs: Vec<(String, Vec<u8>)>,
}

// A plain filename, as the manifest's bundle entry must be: it cannot reach
// outside its distribution.
#[cfg(not(feature = "go"))]
fn plain_bundle_name(manifest: &lattice::manifest::Manifest) -> Option<&str> {
  let name = manifest.bundle.path.as_deref().unwrap_or("bundle.js");
  (!name.contains('/') && !name.contains('\\') && !name.starts_with('.')).then_some(name)
}

#[cfg(not(feature = "go"))]
fn app_from_bundle(name: &str, bytes: Vec<u8>) -> Option<lattice::AppSource> {
  if name.ends_with(".js") {
    Some(lattice::AppSource::Text(String::from_utf8(bytes).ok()?))
  } else {
    Some(lattice::AppSource::Bytecode(bytes))
  }
}

// Read our own image and slice out the sections appended by `srt pack`, if any.
#[cfg(not(feature = "go"))]
fn load_embedded_payload() -> Option<FactoryPayload> {
  let exe = std::env::current_exe().ok()?;
  let data = std::fs::read(&exe).ok()?;
  if data.len() < EMBED_TAIL_LEN {
    return None;
  }
  if &data[data.len() - EMBED_MAGIC.len()..] != EMBED_MAGIC {
    return None;
  }
  let tail = data.len() - EMBED_TAIL_LEN;
  let table_offset = u64::from_le_bytes(data[tail..tail + 8].try_into().ok()?);
  let count = u32::from_le_bytes(data[tail + 8..tail + 12].try_into().ok()?);
  if table_offset >= tail as u64 || count == 0 {
    return None;
  }
  let mut cursor = table_offset as usize;
  let mut manifest: Option<lattice::manifest::Manifest> = None;
  let mut index: std::collections::HashMap<String, (u64, u64)> = std::collections::HashMap::new();
  let mut gl_libs: Vec<(String, (u64, u64))> = Vec::new();
  for _ in 0..count {
    if cursor + 22 > tail {
      return None;
    }
    let kind = u32::from_le_bytes(data[cursor..cursor + 4].try_into().ok()?);
    let offset = u64::from_le_bytes(data[cursor + 4..cursor + 12].try_into().ok()?);
    let len = u64::from_le_bytes(data[cursor + 12..cursor + 20].try_into().ok()?);
    let name_len = u16::from_le_bytes(data[cursor + 20..cursor + 22].try_into().ok()?) as usize;
    cursor += 22;
    if cursor + name_len > tail {
      return None;
    }
    let name = std::str::from_utf8(&data[cursor..cursor + name_len]).ok()?;
    cursor += name_len;
    // Sections precede the table.
    if offset.checked_add(len)? > table_offset {
      return None;
    }
    match kind {
      SECTION_MANIFEST => {
        let text = std::str::from_utf8(&data[offset as usize..(offset + len) as usize]).ok()?;
        manifest = lattice::manifest::Manifest::parse(text).ok();
      }
      SECTION_FILE if !name.is_empty() => {
        index.insert(name.to_string(), (offset, len));
      }
      SECTION_GL_LIB if !name.is_empty() => {
        gl_libs.push((name.to_string(), (offset, len)));
      }
      // Unknown kinds are skipped; pinned CLI/runner versions make this unreachable today.
      _ => {}
    }
  }
  // The entries must consume the table region exactly.
  if cursor != tail {
    return None;
  }
  let manifest = manifest?;
  let slice = |&(offset, len): &(u64, u64)| data[offset as usize..(offset + len) as usize].to_vec();
  let bundle_name = plain_bundle_name(&manifest)?;
  let app = app_from_bundle(bundle_name, slice(index.get(bundle_name)?))?;
  let fonts = manifest
    .fonts
    .iter()
    .filter_map(|font| {
      let bytes = slice(index.get(&font.path)?);
      Some(alloy::rendertree::FontPayload { alias: Some(font.alias.clone()), bytes: std::borrow::Cow::Owned(bytes) })
    })
    .collect();
  let gl_libs = gl_libs.into_iter().map(|(name, range)| (name, slice(&range))).collect();
  Some(FactoryPayload { app, fonts, app_id: manifest.app_id, base: forge::fs::AssetsBase::Packed { exe, index }, gl_libs })
}

// A folder distribution next to this runner: manifest.json + the bundle it
// names + assets/. Absent or unreadable pieces degrade to "no folder payload".
#[cfg(not(feature = "go"))]
fn load_adjacent_folder() -> Option<FactoryPayload> {
  let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
  let manifest = lattice::manifest::Manifest::load(&dir)?;
  let bundle_name = plain_bundle_name(&manifest)?;
  let app = app_from_bundle(bundle_name, std::fs::read(dir.join(bundle_name)).ok()?)?;
  let fonts = manifest
    .load_fonts(&dir)
    .into_iter()
    .map(|(alias, bytes)| alloy::rendertree::FontPayload { alias: Some(alias), bytes: std::borrow::Cow::Owned(bytes) })
    .collect();
  Some(FactoryPayload { app, fonts, app_id: manifest.app_id, base: forge::fs::AssetsBase::Dir(dir), gl_libs: Vec::new() })
}

// Flag mistakes are usage errors: report and exit instead of panicking with a
// backtrace note. Exit code 2 matches the "no app to run" path below.
fn usage(msg: &str) -> ! {
  eprintln!("{msg}");
  std::process::exit(2);
}

fn main() {
  // A distribution owns its entire command line (fluxrt parity): when this
  // binary carries a packed payload - embedded trailer or adjacent folder -
  // everything after the executable is the app's argument vector, and none of
  // the runner flags below apply. Those are dev tooling for the source-path
  // shape.
  #[cfg(not(feature = "go"))]
  if let Some(payload) = load_embedded_payload().or_else(load_adjacent_folder) {
    // Before lattice::start: alloy's window setup runs inside it, and the GL
    // libraries must be loaded by then.
    lattice::gl_libs::provision(&payload.app_id, &payload.gl_libs);
    forge::fs::set_assets_base(Some(payload.base));
    let app_args: Vec<String> = std::env::args().skip(1).collect();
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("Failed to build Tokio runtime");
    let storage = lattice::storage::StorageSpec { data_root: None, client: None, app_id: Some(payload.app_id) };
    lattice::start(&rt, Some(payload.app), alloy::Mode::Run, (1280, 720), false, None, payload.fonts, storage, app_args);
    return;
  }

  let mut args = std::env::args().skip(1);
  let mut playback = false;
  let mut script_path: Option<String> = None;
  let mut fps: u32 = 60;
  let mut duration: f64 = 1.0;
  let mut size: (u32, u32) = (1280, 720);
  let mut stats = false;
  let mut out: Option<String> = None;
  let mut dev_server: Option<String> = None;
  let mut data_root: Option<String> = None;
  let mut client: Option<u32> = None;
  let mut source_path: Option<String> = None;
  let mut app_args: Vec<String> = Vec::new();
  while let Some(arg) = args.next() {
    if arg == "--playback" {
      playback = true;
    } else if arg == "--data-root" {
      data_root = Some(args.next().unwrap_or_else(|| usage("--data-root requires a directory path")));
    } else if arg == "--client" {
      client = Some(
        args
          .next()
          .unwrap_or_else(|| usage("--client requires a number"))
          .parse()
          .unwrap_or_else(|_| usage("--client value must be a non-negative integer")),
      );
    } else if arg == "--script" {
      script_path = Some(args.next().unwrap_or_else(|| usage("--script requires a file path")));
    } else if arg == "--stats" {
      stats = true;
    } else if arg == "--out" {
      out = Some(args.next().unwrap_or_else(|| usage("--out requires a directory or path prefix")));
    } else if arg == "--dev-server" {
      dev_server = Some(args.next().unwrap_or_else(|| usage("--dev-server requires a value")));
    } else if arg == "--fps" {
      fps = args
        .next()
        .unwrap_or_else(|| usage("--fps requires a value"))
        .parse()
        .ok()
        .filter(|&n| n > 0)
        .unwrap_or_else(|| usage("--fps value must be a positive integer"));
    } else if arg == "--duration" {
      duration = args
        .next()
        .unwrap_or_else(|| usage("--duration requires a value"))
        .parse()
        .ok()
        .filter(|d: &f64| d.is_finite() && *d > 0.0)
        .unwrap_or_else(|| usage("--duration value must be a positive number of seconds"));
    } else if arg == "--size" {
      let val = args.next().unwrap_or_else(|| usage("--size requires a value"));
      let (w, h) = val.split_once('x').unwrap_or_else(|| usage("--size must be in WxH format, e.g. 1920x1080"));
      size = (
        w.parse().unwrap_or_else(|_| usage("--size width must be a positive integer")),
        h.parse().unwrap_or_else(|_| usage("--size height must be a positive integer")),
      );
    } else {
      // The first non-flag argument is the source path; everything after it
      // is the app's argument vector, verbatim, so a stray runner flag can
      // neither select the app nor leak into its arguments.
      source_path = Some(arg);
      app_args.extend(args);
      break;
    }
  }
  let path_app = |path: String| {
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read '{path}': {e}"));
    lattice::AppSource::Text(src)
  };
  // Payloads were handled above, so this is the bare runtime: no fonts
  // either; text falls back to the platform font manager.
  #[cfg(not(feature = "go"))]
  let (app, fonts, app_id): (_, Vec<alloy::rendertree::FontPayload>, Option<String>) =
    (source_path.map(path_app), Vec::new(), None);
  // The runtime has no built-in screen to fall back to (the launcher is
  // go-only); without an app there is nothing to run.
  #[cfg(not(feature = "go"))]
  if app.is_none() {
    eprintln!("No app to run: expected a packed payload, an app folder, or a source path argument");
    std::process::exit(2);
  }
  #[cfg(feature = "go")]
  let (app, fonts, app_id): (_, _, Option<String>) = (source_path.map(path_app), lattice::embedded_fonts(), None);
  let mode = if playback {
    alloy::Mode::Playback(alloy::PlaybackConfig {
      fps,
      // Round to the nearest whole frame; any positive duration renders at
      // least one.
      frames: (duration * fps as f64).round().max(1.0) as u64,
      output_prefix: out.map(frame_prefix).unwrap_or_else(|| "frame".to_string()),
      script: script_path.map(load_script).unwrap_or_default(),
    })
  } else {
    alloy::Mode::Run
  };
  let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("Failed to build Tokio runtime");
  let storage = lattice::storage::StorageSpec { data_root: data_root.map(Into::into), client, app_id };
  lattice::start(&rt, app, mode, size, stats, dev_server, fonts, storage, app_args);
}

// `--out` names where playback frames land: an existing directory (frames
// appear inside it as frame-NNNNNN.png) or a path prefix (<out>-NNNNNN.png).
// Absolutized here because the runtime chdirs into the app's data sandbox
// before frames are written; a relative value therefore means relative to the
// invoking directory, as a caller expects.
fn frame_prefix(out: String) -> String {
  let path = std::path::PathBuf::from(out);
  let path = if path.is_dir() { path.join("frame") } else { path };
  let abs = std::path::absolute(&path).unwrap_or_else(|e| panic!("--out path '{}' is unusable: {e}", path.display()));
  abs.to_string_lossy().into_owned()
}

// Parses a `--script` file (see `srt render --script`, written by `srt run
// --capture`) into a ScriptPlayer. One JSON object per line (JSON Lines), not
// a single JSON array -- matches dev-server.ts's streaming capture writer.
// Needs serde_json, only pulled in by the `go` feature (the dev client); the
// plain packed-app binary never replays a script.
#[cfg(feature = "go")]
fn load_script(path: String) -> alloy::ScriptPlayer {
  #[derive(serde::Deserialize)]
  struct ScriptStep {
    after: u64, // milliseconds
    #[serde(rename = "type")]
    kind: String,
    key: String,
  }

  let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read '{path}': {e}"));
  let mut at = 0.0;
  let actions = text
    .lines()
    .filter(|line| !line.trim().is_empty())
    .map(|line| {
      let step: ScriptStep = serde_json::from_str(line).unwrap_or_else(|e| panic!("Failed to parse '{path}': {e}"));
      at += step.after as f64 / 1000.0;
      // `key` is a W3C KeyboardEvent.key value ("Enter", "ArrowLeft", "a") and
      // replays verbatim; there is no key-name registry to validate against.
      let down = match step.kind.as_str() {
        "keydown" => true,
        "keyup" => false,
        other => panic!("Unknown script step type '{other}' in '{path}'"),
      };
      alloy::ScriptedAction { at, event: alloy::ScriptEvent { down, key: step.key } }
    })
    .collect();
  alloy::ScriptPlayer::new(actions)
}

#[cfg(not(feature = "go"))]
fn load_script(path: String) -> alloy::ScriptPlayer {
  panic!("--script requires the go client build (path: {path})");
}
