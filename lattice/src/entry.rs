//! The program over the runtime: the desktop command line and the Android
//! entry that `start` the app with, and `Modules`, what a custom runtime
//! adds beside the built-in modules. The stock binaries are one call into
//! here with no modules; a custom runtime (a cargo project over lattice,
//! see docs/runtime "Native code") is the same call with its own, and on
//! Android the `android_entry!` line in its cdylib.

use flux::rquickjs::module::ModuleDef;
use flux::FluxEngineBuilder;

/// The window a run opens when nothing sizes it (`--size`, a packed app).
const DEFAULT_WINDOW_SIZE: (u32, u32) = (1280, 720);

/// The `flux:*` modules a runtime adds beside the built-ins: what a custom
/// runtime registers in its `main`. Each is registered on every engine the
/// runtime builds (a reload builds a new one) and listed in
/// `Flux.capabilities` by its short name, so an app checks
/// `Flux.capabilities.includes("physics3d")` the way it checks a built-in,
/// and the dev client reports it beside flux's own. The stock binaries pass
/// `Modules::new()`.
#[derive(Default)]
pub struct Modules {
  installs: Vec<Box<dyn Fn(FluxEngineBuilder) -> FluxEngineBuilder + Send>>,
  capabilities: Vec<&'static str>,
}

impl Modules {
  pub fn new() -> Self {
    Self::default()
  }

  /// Register `def` as the module `name`, a `flux:*` name (the one
  /// namespace; anything else is a programming error and panics here).
  /// `def` is cloned per engine.
  pub fn add<D: ModuleDef + Clone + Send + 'static>(mut self, name: &'static str, def: D) -> Self {
    let Some(capability) = name.strip_prefix("flux:") else {
      panic!("a runtime module registers under a flux:* name, not {name}");
    };
    self.capabilities.push(capability);
    self.installs.push(Box::new(move |builder| {
      builder.module(name, def.clone()).plugin(move |ctx| flux::add_capability(&ctx, capability))
    }));
    self
  }

  /// The primitive under `add`: anything applied to every engine's builder
  /// (a plugin seeding state, a userdata value a module reads).
  pub fn engine(mut self, f: impl Fn(FluxEngineBuilder) -> FluxEngineBuilder + Send + 'static) -> Self {
    self.installs.push(Box::new(f));
    self
  }

  /// The capability names `add` registered, for the dev client's report.
  #[cfg_attr(not(feature = "go"), allow(dead_code))]
  pub(crate) fn capabilities(&self) -> Vec<&'static str> {
    self.capabilities.clone()
  }

  pub(crate) fn install(&self, builder: FluxEngineBuilder) -> FluxEngineBuilder {
    self.installs.iter().fold(builder, |builder, install| install(builder))
  }
}

/// The Android entry of a runtime's cdylib: expands to the `SDL_main` the
/// SDL shell calls, over `android_main` with the given modules. Nothing on
/// other targets, so a custom runtime's lib.rs carries the line
/// unconditionally. The stock cdylib's entry is this macro behind the
/// `android-entry` feature, which a custom runtime leaves off: a
/// dependency's `#[no_mangle]` symbols are exported from its cdylib too,
/// so the two cannot coexist.
#[macro_export]
macro_rules! android_entry {
  ($modules:expr) => {
    #[cfg(target_os = "android")]
    #[no_mangle]
    pub extern "C" fn SDL_main(argc: i32, argv: *mut *mut i8) -> i32 {
      $crate::android_main(argc, argv, $modules)
    }
  };
}

/// The Android entry, what a cdylib's `SDL_main` runs (see `android_entry!`):
/// the go dev client boots the player (no app source) and auto-dials a dev
/// server when the launch intent carries one.
#[cfg(all(target_os = "android", feature = "go"))]
pub fn android_main(argc: i32, argv: *mut *mut i8, modules: Modules) -> i32 {
  let args = android_args(argc, argv);
  let dev_server = dev_server_arg(&args);
  let launch = launch_arg(&args);
  let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("build tokio runtime");
  // Android resolves its own sandboxed root; no flags, no packed identity.
  // No app argument channel either: the activity is launched by intent, not
  // from a command line.
  let storage = crate::storage::StorageSpec { data_root: None, client: None, app_id: None };
  crate::start(
    &rt,
    None,
    launch,
    None,
    DEFAULT_WINDOW_SIZE,
    false,
    dev_server,
    crate::embedded_fonts(),
    storage,
    Vec::new(),
    modules,
  );
  0
}

// Where `sol pack --apk` stores the payload in the APK, relative to assets/.
#[cfg(all(target_os = "android", not(feature = "go")))]
const PACKED_PAYLOAD_ASSET: &str = "app.solapp";

/// The Android entry, what a cdylib's `SDL_main` runs (see `android_entry!`):
/// the production runtime boots the .solapp packed into the APK (`sol pack
/// --apk`), read in place at its offset inside the APK - no dev server, no
/// player, no extraction. A runner APK without a payload is a packaging
/// error, so there is no fallback screen; the failure line lands in logcat
/// via SDL's stderr redirect when it does at all - primarily this exit code
/// is for the packager's bring-up.
#[cfg(all(target_os = "android", not(feature = "go")))]
pub fn android_main(argc: i32, argv: *mut *mut i8, modules: Modules) -> i32 {
  let launch = launch_arg(&android_args(argc, argv));
  let Some((apk, offset, len)) = alloy::sdl_utils::packed_asset_location(PACKED_PAYLOAD_ASSET) else {
    eprintln!("[sol] no {PACKED_PAYLOAD_ASSET} asset in this APK; nothing to run");
    return 1;
  };
  let Some(payload) =
    forge::trailer::read_at(apk, offset, len, crate::payload::EMBED_MAGIC).and_then(crate::payload::load)
  else {
    eprintln!("[sol] {PACKED_PAYLOAD_ASSET} is not a SolidRT app pack; nothing to run");
    return 1;
  };
  forge::fs::set_assets_base(Some(payload.base));
  let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("build tokio runtime");
  // Storage anchors into the app's data sandbox under the Android-resolved
  // root, keyed by the packed identity like every packed distribution. No
  // argument channel: the activity is launched by intent.
  let storage = crate::storage::StorageSpec { data_root: None, client: None, app_id: Some(payload.app_id) };
  crate::start(
    &rt,
    Some(payload.app),
    launch,
    payload.display_name,
    DEFAULT_WINDOW_SIZE,
    false,
    None,
    payload.fonts,
    storage,
    Vec::new(),
    modules,
  );
  0
}

// The C argv SDL hands SDL_main, populated from the activity's
// getArguments(): the launch fact from SolidRTActivity, plus the go client's
// dev-server address.
#[cfg(target_os = "android")]
fn android_args(argc: i32, argv: *mut *mut i8) -> Vec<String> {
  if argv.is_null() || argc <= 0 {
    return Vec::new();
  }
  (0..argc as isize)
    .filter_map(|i| {
      let ptr = unsafe { *argv.offset(i) };
      if ptr.is_null() {
        return None;
      }
      // c_char is u8 on Android ARM, i8 elsewhere; cast so this builds on both.
      unsafe { std::ffi::CStr::from_ptr(ptr as *const std::ffi::c_char) }.to_str().ok().map(str::to_owned)
    })
    .collect()
}

// The launch facts SolidRTActivity.getArguments() passes: `--restored` (the
// activity was recreated from saved state) and `--link <link>` (the intent's
// data); see Launch.
#[cfg(target_os = "android")]
fn launch_arg(args: &[String]) -> crate::Launch {
  let restored = args.iter().any(|arg| arg == "--restored");
  let mut link = None;
  let mut it = args.iter();
  while let Some(arg) = it.next() {
    if arg == "--link" {
      link = it.next().cloned();
    }
  }
  crate::Launch { restored, link }
}

// `--dev-server <addr>`: the dev server the go client should auto-dial; None
// when launched without it (e.g. tapping the app icon).
#[cfg(all(target_os = "android", feature = "go"))]
fn dev_server_arg(args: &[String]) -> Option<String> {
  let mut it = args.iter();
  while let Some(arg) = it.next() {
    if arg == "--dev-server" {
      return it.next().cloned();
    }
  }
  None
}

// Flag mistakes are usage errors: report and exit instead of panicking with a
// backtrace note. Exit code 2 matches the "no app to run" path below.
fn usage(msg: &str) -> ! {
  eprintln!("{msg}");
  std::process::exit(2);
}

/// The stock program: the desktop command line (a source path, a .solapp,
/// `--render`, `--test` and their flags), or the packed payload this binary
/// carries. What the `solidrt` and `solidrt-go` binaries call with no modules,
/// and a custom runtime's `main` with its own.
pub fn main(modules: Modules) {
  // A distribution owns its entire command line (fluxrt parity): when this
  // binary carries a packed payload - embedded trailer or adjacent folder -
  // everything after the executable is the app's argument vector, and none of
  // the runner flags below apply. Those are dev tooling for the source-path
  // shape.
  #[cfg(not(feature = "go"))]
  if let Some(payload) =
    std::env::current_exe().ok().and_then(crate::payload::load_path).or_else(crate::payload::load_adjacent_folder)
  {
    // Before crate::start: alloy's window setup runs inside it, and the GL
    // libraries must be loaded by then.
    crate::gl_libs::provision(&payload.app_id, &payload.gl_libs);
    forge::fs::set_assets_base(Some(payload.base));
    let mut app_args: Vec<String> = std::env::args().skip(1).collect();
    // A link of the app's own scheme as the first argument is how the OS
    // hands a registered scheme to its handler (`"<exe>" "%1"`, `%u`): the
    // launch link, not an app argument. An instance already running takes
    // it instead, and this process ends without a window (links.rs).
    let link = match app_args.first() {
      Some(arg) if crate::links::own_link(&payload.app_id, arg) => Some(app_args.remove(0)),
      _ => None,
    };
    if let Some(link) = &link {
      if crate::links::hand_off(&payload.app_id, link) {
        return;
      }
    }
    let launch = crate::Launch { restored: false, link };
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("Failed to build Tokio runtime");
    let storage = crate::storage::StorageSpec { data_root: None, client: None, app_id: Some(payload.app_id) };
    crate::start(
      &rt,
      Some(payload.app),
      launch,
      payload.display_name,
      DEFAULT_WINDOW_SIZE,
      false,
      None,
      payload.fonts,
      storage,
      app_args,
      modules,
    );
    return;
  }

  let mut args = std::env::args().skip(1);
  // `--render`: write frames headless instead of running on a display
  // (lattice render host), with `--fps`, `--duration`, `--size`, `--out`,
  // `--script`, `--seed` for the run; `--settle` runs the app to rest
  // before the first frame; `--strict` fails the render on any error
  // logged (lattice ErrorTally). What `sol render` starts.
  let mut render = false;
  let mut script_path: Option<String> = None;
  let mut fps: u32 = 60;
  let mut duration: f64 = 1.0;
  let mut settle = false;
  let mut strict = false;
  let mut size: (u32, u32) = DEFAULT_WINDOW_SIZE;
  let mut stats = false;
  let mut out: Option<String> = None;
  let mut dev_server: Option<String> = None;
  let mut data_root: Option<String> = None;
  let mut client: Option<u32> = None;
  let mut assets: Option<String> = None;
  // `--link <link>`: the link the app is started with (env.launchLink), the
  // way an OS-routed link reaches a packaged app; `sol render --link` renders
  // a screen a link names.
  let mut link: Option<String> = None;
  // `--test`: run the source as a test file (lattice test mode), with
  // `--filter <text>` for the run and `--failures <dir>` for the snapshots
  // of failed tests. What `sol test` starts for a file that needs the app
  // runtime. `--seed <n>`: the Math.random sequence of a test or a render.
  let mut test = false;
  let mut filter: Option<String> = None;
  let mut seed: Option<u64> = None;
  let mut failures: Option<String> = None;
  let mut source_path: Option<String> = None;
  let mut app_args: Vec<String> = Vec::new();
  while let Some(arg) = args.next() {
    if arg == "--render" {
      render = true;
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
    } else if arg == "--assets" {
      assets = Some(args.next().unwrap_or_else(|| usage("--assets requires a directory path")));
    } else if arg == "--script" {
      script_path = Some(args.next().unwrap_or_else(|| usage("--script requires a file path")));
    } else if arg == "--link" {
      link = Some(args.next().unwrap_or_else(|| usage("--link requires a link")));
    } else if arg == "--test" {
      test = true;
    } else if arg == "--filter" {
      filter = Some(args.next().unwrap_or_else(|| usage("--filter requires a text")));
    } else if arg == "--failures" {
      failures = Some(args.next().unwrap_or_else(|| usage("--failures requires a directory path")));
    } else if arg == "--seed" {
      seed = Some(
        args
          .next()
          .unwrap_or_else(|| usage("--seed requires a number"))
          .parse()
          .unwrap_or_else(|_| usage("--seed value must be a non-negative integer")),
      );
    } else if arg == "--stats" {
      stats = true;
    } else if arg == "--strict" {
      strict = true;
    } else if arg == "--settle" {
      settle = true;
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
    crate::AppSource::Text(src)
  };
  // A source path ending in the pack magic is a .solapp (the extension is a
  // convention, the magic is the contract): identity, fonts and assets come
  // from the file, and the flags above still apply, so `sol console` runs a
  // packed app under the dev runner's controls. Anything else is JS source on
  // the bare runtime: no fonts (text falls back to the platform font
  // manager), no identity.
  #[cfg(not(feature = "go"))]
  let (app, mut fonts, mut app_id): (_, Vec<alloy::rendertree::FontPayload>, Option<String>) =
    match source_path.as_deref().and_then(|p| crate::payload::load_path(std::path::PathBuf::from(p))) {
      Some(payload) => {
        crate::gl_libs::provision(&payload.app_id, &payload.gl_libs);
        forge::fs::set_assets_base(Some(payload.base));
        (Some(payload.app), payload.fonts, Some(payload.app_id))
      }
      None => {
        if let Some(p) = source_path.as_deref().filter(|p| p.ends_with(".solapp")) {
          usage(&format!("'{p}' is not a SolidRT app pack (no valid payload at its end)"));
        }
        (source_path.map(path_app), Vec::new(), None)
      }
    };
  // The runtime has no built-in screen to fall back to (the player is
  // go-only); without an app there is nothing to run.
  #[cfg(not(feature = "go"))]
  if app.is_none() {
    eprintln!("No app to run: expected a packed payload, an app folder, or a .solapp or source path argument");
    std::process::exit(2);
  }
  #[cfg(feature = "go")]
  let (app, mut fonts, mut app_id): (_, _, Option<String>) = (source_path.map(path_app), crate::embedded_fonts(), None);
  // `--assets <dir>`: mount a directory holding an assets/ tree so
  // `assets/...` resolves through it instead of the data-sandbox cwd - what a
  // packed app or a go-installed version gets from its payload. `sol render`
  // passes the dir it stages the build into.
  let mut display_name = None;
  if let Some(dir) = assets {
    let dir = std::path::absolute(&dir).unwrap_or_else(|e| usage(&format!("--assets path '{dir}' is unusable: {e}")));
    if !dir.is_dir() {
      usage(&format!("--assets path '{}' is not a directory", dir.display()));
    }
    // A manifest in the mount (the staged dir `sol render` builds has the
    // installed-version shape) is the app's identity and font bindings, as
    // a version-store boot reads them: the app runs in its own sandbox with
    // its fonts registered over the base set.
    if let Some(manifest) = crate::manifest::Manifest::load(&dir) {
      fonts = crate::merge_fonts(&fonts, manifest.load_fonts(&dir));
      app_id = app_id.or(Some(manifest.app_id));
      display_name = manifest.display_name;
    }
    forge::fs::set_assets_base(Some(forge::fs::AssetsBase::Dir(dir)));
  }
  let storage = crate::storage::StorageSpec { data_root: data_root.map(Into::into), client, app_id };
  if test {
    run_tests(TestArgs { app, filter, seed, failures, size, fonts, storage, app_args, modules });
  }
  let launch = crate::Launch { restored: false, link };
  if render {
    run_render(RenderArgs {
      app,
      fps,
      duration,
      out,
      script_path,
      settle,
      strict,
      seed,
      launch,
      size,
      stats,
      fonts,
      storage,
      app_args,
      modules,
    });
  }
  let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("Failed to build Tokio runtime");
  crate::start(&rt, app, launch, display_name, size, stats, dev_server, fonts, storage, app_args, modules);
}

/// What `--test` runs with (see `run_tests`).
#[cfg_attr(not(feature = "test"), allow(dead_code))]
struct TestArgs {
  app: Option<crate::AppSource>,
  filter: Option<String>,
  seed: Option<u64>,
  failures: Option<String>,
  size: (u32, u32),
  fonts: Vec<alloy::rendertree::FontPayload>,
  storage: crate::storage::StorageSpec,
  app_args: Vec<String>,
  modules: Modules,
}

/// What `--render` runs with (see `run_render`).
#[cfg_attr(not(feature = "go"), allow(dead_code))]
struct RenderArgs {
  app: Option<crate::AppSource>,
  fps: u32,
  duration: f64,
  out: Option<String>,
  script_path: Option<String>,
  settle: bool,
  strict: bool,
  seed: Option<u64>,
  launch: crate::Launch,
  size: (u32, u32),
  stats: bool,
  fonts: Vec<alloy::rendertree::FontPayload>,
  storage: crate::storage::StorageSpec,
  app_args: Vec<String>,
  modules: Modules,
}

// `--render`: render the app headless and exit with the outcome. It exits
// hard, here in the binary: headless callers gate on the exit code (sol
// render verification), so an incomplete render must read nonzero - and a
// plain return would run the runtime's drop, which can block on a lingering
// blocking task and hang the render at the finish line.
#[cfg(feature = "go")]
fn run_render(args: RenderArgs) -> ! {
  let app = args.app.unwrap_or_else(|| usage("--render requires a source path"));
  let run = crate::RenderRun {
    fps: args.fps,
    // Round to the nearest whole frame; any positive duration renders at
    // least one.
    frames: (args.duration * args.fps as f64).round().max(1.0) as u64,
    output_prefix: args.out.map(frame_prefix).unwrap_or_else(|| "frame".to_string()),
    script: args.script_path.map(load_script).unwrap_or_default(),
    settle: args.settle,
    seed: args.seed.unwrap_or(flux::DEFAULT_SEED),
  };
  let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("Failed to build Tokio runtime");
  let outcome = crate::start_render(
    &rt,
    app,
    run,
    args.launch,
    args.strict,
    args.size,
    args.stats,
    args.fonts,
    args.storage,
    args.app_args,
    args.modules,
  );
  match outcome {
    Ok(()) => std::process::exit(0),
    Err(e) => {
      log::error!("[sol] {e}");
      std::process::exit(1);
    }
  }
}

#[cfg(not(feature = "go"))]
fn run_render(_args: RenderArgs) -> ! {
  usage("--render requires the dev client (solidrt-go)")
}

// `--test`: run the source as a test file and exit with its outcome, hard,
// here in the binary, for the reason `run_render` gives.
#[cfg(feature = "test")]
fn run_tests(args: TestArgs) -> ! {
  let app = args.app.unwrap_or_else(|| usage("--test requires a test bundle path"));
  let mut options = flux::test::RunOptions { filter: args.filter, ..Default::default() };
  if let Some(seed) = args.seed {
    options.seed = seed;
  }
  // Absolute, like `--out`: the runtime chdirs into the data sandbox.
  let failures = args.failures.map(|dir| {
    std::path::absolute(&dir).unwrap_or_else(|e| usage(&format!("--failures path '{dir}' is unusable: {e}")))
  });
  let run = crate::TestRun { options, failures };
  let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("Failed to build Tokio runtime");
  let passed =
    crate::start_tests(&rt, app, run, args.size, args.fonts, args.storage, args.app_args, args.modules).is_ok();
  std::process::exit(if passed { 0 } else { 1 })
}

#[cfg(not(feature = "test"))]
fn run_tests(_args: TestArgs) -> ! {
  usage("--test requires a client built with the test feature (make client)")
}

// `--out` names where the frames land: an existing directory (frames appear
// inside it as frame-NNNNNN.png) or a path prefix (<out>-NNNNNN.png).
#[cfg(feature = "go")]
// Absolutized here because the runtime chdirs into the app's data sandbox
// before frames are written; a relative value therefore means relative to the
// invoking directory, as a caller expects.
fn frame_prefix(out: String) -> String {
  let path = std::path::PathBuf::from(out);
  let path = if path.is_dir() { path.join("frame") } else { path };
  let abs = std::path::absolute(&path).unwrap_or_else(|e| panic!("--out path '{}' is unusable: {e}", path.display()));
  abs.to_string_lossy().into_owned()
}

// Parses a `--script` file (see `sol render --script`, written by `sol run
// --capture`) into a ScriptPlayer. One JSON object per line (JSON Lines), not
// a single JSON array -- matches dev-server.ts's streaming capture writer.
// Only the dev client renders, so only it replays a script.
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
