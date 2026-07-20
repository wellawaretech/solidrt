// Trailer identifying a SolidRT payload appended to this binary by `srt pack`.
// Must match the writer in packages/cli/src/packer.ts. Layout after the runner
// image: each section's bytes, then a table of section entries, then
// [table offset u64 LE][entry count u32 LE][magic]. Table entry:
// [kind u32 LE][offset u64 LE][len u64 LE][alias len u8][alias bytes].
// Offsets are absolute file offsets. The CLI and runners ship pinned together,
// so there is no format version; every offset/length is bounds-checked and any
// mismatch degrades to "no payload" instead of misparsing.
#[cfg(not(feature = "go"))]
const EMBED_MAGIC: &[u8; 9] = b"SOLIDRT\x88\x44";
#[cfg(not(feature = "go"))]
const EMBED_TAIL_LEN: usize = 8 + 4 + EMBED_MAGIC.len(); // table offset + entry count + magic
#[cfg(not(feature = "go"))]
const SECTION_BYTECODE: u32 = 1;
#[cfg(not(feature = "go"))]
const SECTION_FONT: u32 = 2;
#[cfg(not(feature = "go"))]
const SECTION_APP: u32 = 3;

#[cfg(not(feature = "go"))]
struct EmbeddedPayload {
  bytecode: Option<Vec<u8>>,
  fonts: Vec<alloy::rendertree::FontPayload>,
  identity: Option<lattice::storage::AppIdentity>,
}

// Read our own image and slice out the sections appended by `srt pack`, if any.
#[cfg(not(feature = "go"))]
fn load_embedded_payload() -> Option<EmbeddedPayload> {
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
  let mut payload = EmbeddedPayload { bytecode: None, fonts: Vec::new(), identity: None };
  for _ in 0..count {
    if cursor + 21 > tail {
      return None;
    }
    let kind = u32::from_le_bytes(data[cursor..cursor + 4].try_into().ok()?);
    let offset = u64::from_le_bytes(data[cursor + 4..cursor + 12].try_into().ok()?);
    let len = u64::from_le_bytes(data[cursor + 12..cursor + 20].try_into().ok()?);
    let alias_len = data[cursor + 20] as usize;
    cursor += 21;
    if cursor + alias_len > tail {
      return None;
    }
    let alias = match alias_len {
      0 => None,
      _ => Some(std::str::from_utf8(&data[cursor..cursor + alias_len]).ok()?.to_string()),
    };
    cursor += alias_len;
    // Sections precede the table.
    if offset.checked_add(len)? > table_offset {
      return None;
    }
    let bytes = data[offset as usize..(offset + len) as usize].to_vec();
    match kind {
      SECTION_BYTECODE => payload.bytecode = Some(bytes),
      SECTION_FONT => {
        payload.fonts.push(alloy::rendertree::FontPayload { alias, bytes: std::borrow::Cow::Owned(bytes) })
      }
      SECTION_APP => payload.identity = lattice::storage::decode_app_identity(&bytes),
      // Unknown kinds are skipped; pinned CLI/runner versions make this unreachable today.
      _ => {}
    }
  }
  // The entries must consume the table region exactly.
  if cursor != tail {
    return None;
  }
  Some(payload)
}

// A folder distribution next to this runner (okf/plans/client-storage-updates.md,
// stage 3b): manifest.json + the bundle it names + assets/. The manifest
// defines the version's file set (the runner is deliberately unlisted) and
// carries the app identity; a .js bundle is JS source, anything else QuickJS
// bytecode. Absent or unreadable pieces degrade to "no folder payload".
#[cfg(not(feature = "go"))]
struct FolderPayload {
  app: lattice::AppSource,
  fonts: Vec<alloy::rendertree::FontPayload>,
  identity: lattice::storage::AppIdentity,
  dir: std::path::PathBuf,
}

#[cfg(not(feature = "go"))]
fn load_adjacent_folder() -> Option<FolderPayload> {
  let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
  let manifest = lattice::manifest::Manifest::load(&dir)?;
  let bundle_path = manifest.bundle.path.as_deref().unwrap_or("bundle.js");
  // The bundle name comes from the manifest: a plain filename only, so it
  // cannot reach outside the folder.
  if bundle_path.contains('/') || bundle_path.contains('\\') || bundle_path.starts_with('.') {
    return None;
  }
  let bytes = std::fs::read(dir.join(bundle_path)).ok()?;
  let app = if bundle_path.ends_with(".js") {
    lattice::AppSource::Text(String::from_utf8(bytes).ok()?)
  } else {
    lattice::AppSource::Bytecode(bytes)
  };
  let fonts = manifest
    .load_fonts(&dir)
    .into_iter()
    .map(|(alias, bytes)| alloy::rendertree::FontPayload { alias: Some(alias), bytes: std::borrow::Cow::Owned(bytes) })
    .collect();
  // Pack manifests carry org/displayName for the storage pref path; a
  // hand-rolled folder from a dev manifest defaults them from the app id.
  let identity = lattice::storage::AppIdentity {
    org: manifest.org.clone().unwrap_or_else(|| manifest.app_id.clone()),
    display_name: manifest.display_name.clone().unwrap_or_else(|| manifest.app_id.clone()),
    app_id: manifest.app_id,
  };
  Some(FolderPayload { app, fonts, identity, dir })
}

fn main() {
  #[cfg(not(feature = "go"))]
  let (bytecode, fonts, identity) = match load_embedded_payload() {
    Some(payload) => (payload.bytecode, payload.fonts, payload.identity),
    // No trailer (bare runtime): no fonts either; text falls back to the
    // platform font manager.
    None => (None, Vec::new(), None),
  };
  #[cfg(feature = "go")]
  let (bytecode, fonts, identity): (Option<Vec<u8>>, Vec<alloy::rendertree::FontPayload>, Option<lattice::storage::AppIdentity>) =
    (None, lattice::embedded_fonts(), None);

  let mut args = std::env::args().skip(1);
  let mut playback = false;
  let mut script_path: Option<String> = None;
  let mut fps: u32 = 60;
  let mut duration: u32 = 1;
  let mut size: (u32, u32) = (1280, 720);
  let mut stats = false;
  let mut dev_server: Option<String> = None;
  let mut data_root: Option<String> = None;
  let mut client: Option<String> = None;
  let mut source_path: Option<String> = None;
  while let Some(arg) = args.next() {
    if arg == "--playback" {
      playback = true;
    } else if arg == "--data-root" {
      data_root = Some(args.next().expect("--data-root requires a directory path"));
    } else if arg == "--client" {
      client = Some(args.next().expect("--client requires a name"));
    } else if arg == "--script" {
      script_path = Some(args.next().expect("--script requires a file path"));
    } else if arg == "--stats" {
      stats = true;
    } else if arg == "--dev-server" {
      dev_server = Some(args.next().expect("--dev-server requires a value"));
    } else if arg == "--fps" {
      fps = args.next().expect("--fps requires a value").parse().expect("--fps value must be a positive integer");
    } else if arg == "--duration" {
      duration =
        args.next().expect("--duration requires a value").parse().expect("--duration value must be a positive integer");
    } else if arg == "--size" {
      let val = args.next().expect("--size requires a value");
      let (w, h) = val.split_once('x').expect("--size must be in WxH format, e.g. 1920x1080");
      size = (
        w.parse().expect("--size width must be a positive integer"),
        h.parse().expect("--size height must be a positive integer"),
      );
    } else {
      source_path = Some(arg);
    }
  }
  // An embedded payload (packed binary) takes precedence over a path argument.
  let app = match bytecode {
    Some(bytes) => Some(lattice::AppSource::Bytecode(bytes)),
    None => source_path.map(|path| {
      let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read '{path}': {e}"));
      lattice::AppSource::Text(src)
    }),
  };
  // With neither, a folder distribution adjacent to the runner boots: its
  // assets/ tree becomes the assets mount (reads under assets/ resolve there).
  #[cfg(not(feature = "go"))]
  let (app, fonts, identity) = match app {
    None => match load_adjacent_folder() {
      Some(folder) => {
        forge::fs::set_assets_base(Some(folder.dir));
        (Some(folder.app), folder.fonts, Some(folder.identity))
      }
      None => (None, fonts, identity),
    },
    app => (app, fonts, identity),
  };
  let mode = if playback {
    alloy::Mode::Playback(alloy::PlaybackConfig {
      fps,
      frames: (duration * fps) as u64,
      output_prefix: "frame".to_string(),
      script: script_path.map(load_script).unwrap_or_default(),
    })
  } else {
    alloy::Mode::Run
  };
  let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("Failed to build Tokio runtime");
  let storage = lattice::storage::StorageSpec { data_root: data_root.map(Into::into), client, identity };
  lattice::start(&rt, app, mode, size, stats, dev_server, fonts, storage);
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
      let keycode = alloy::sdl3::keyboard::Keycode::from_name(&step.key)
        .unwrap_or_else(|| panic!("Unknown key name '{}' in '{path}'", step.key));
      let event = match step.kind.as_str() {
        "keydown" => alloy::ScriptEvent::KeyDown(keycode),
        "keyup" => alloy::ScriptEvent::KeyUp(keycode),
        other => panic!("Unknown script step type '{other}' in '{path}'"),
      };
      alloy::ScriptedAction { at, event }
    })
    .collect();
  alloy::ScriptPlayer::new(actions)
}

#[cfg(not(feature = "go"))]
fn load_script(path: String) -> alloy::ScriptPlayer {
  panic!("--script requires the go client build (path: {path})");
}
