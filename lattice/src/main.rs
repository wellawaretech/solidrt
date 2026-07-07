// Trailer identifying a SolidRT payload appended to this binary by `srt pack`.
// Must match SOLID_MAGIC in packages/cli/src/pack.ts.
#[cfg(not(feature = "go"))]
const EMBED_MAGIC: &[u8; 9] = b"SOLIDRT\x88\x44";
#[cfg(not(feature = "go"))]
const EMBED_TRAILER_LEN: usize = 8 + EMBED_MAGIC.len(); // u64 offset + magic

// Read our own image and slice out bytecode appended by `srt pack`, if any.
#[cfg(not(feature = "go"))]
fn load_embedded_bytecode() -> Option<Vec<u8>> {
  let exe = std::env::current_exe().ok()?;
  let data = std::fs::read(&exe).ok()?;
  if data.len() < EMBED_TRAILER_LEN {
    return None;
  }
  let magic_start = data.len() - EMBED_MAGIC.len();
  if &data[magic_start..] != EMBED_MAGIC {
    return None;
  }
  let offset_start = data.len() - EMBED_TRAILER_LEN;
  let offset = u64::from_le_bytes(data[offset_start..offset_start + 8].try_into().ok()?) as usize;
  if offset >= offset_start {
    return None;
  }
  Some(data[offset..offset_start].to_vec())
}

fn main() {
  #[cfg(not(feature = "go"))]
  let bytecode = load_embedded_bytecode();
  #[cfg(feature = "go")]
  let bytecode: Option<Vec<u8>> = None;

  let mut args = std::env::args().skip(1);
  let mut playback = false;
  let mut record_out: Option<String> = None;
  let mut script_path: Option<String> = None;
  let mut fps: u32 = 60;
  let mut duration: u32 = 1;
  let mut size: (u32, u32) = (1280, 720);
  let mut stats = false;
  let mut dev_server: Option<String> = None;
  let mut source_path: Option<String> = None;
  while let Some(arg) = args.next() {
    if arg == "--playback" {
      playback = true;
    } else if arg == "--record" {
      record_out = Some(args.next().expect("--record requires an output file path"));
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
  let record_input = record_out.map(|path| lattice::RecordInputConfig { path: path.into() });
  let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("Failed to build Tokio runtime");
  lattice::start(&rt, app, mode, size, stats, dev_server, record_input);
}

// Parses a `--script` file (see `srt playback --script`) into a ScriptPlayer.
// Needs serde_json, only pulled in by the `go` feature (the dev client); the
// plain packed-app binary never replays a script.
#[cfg(feature = "go")]
fn load_script(path: String) -> alloy::ScriptPlayer {
  #[derive(serde::Deserialize)]
  struct ScriptStep {
    after: f64,
    #[serde(rename = "type")]
    kind: String,
    key: String,
  }

  let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read '{path}': {e}"));
  let raw: Vec<ScriptStep> = serde_json::from_str(&text).unwrap_or_else(|e| panic!("Failed to parse '{path}': {e}"));
  let mut at = 0.0;
  let actions = raw
    .into_iter()
    .map(|step| {
      at += step.after;
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
