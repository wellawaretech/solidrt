mod frame;
#[cfg_attr(not(feature = "go"), allow(dead_code))]
mod frame_history;
pub mod gl_libs;
#[cfg(feature = "go")]
mod go;
pub mod manifest;
mod overlay;
#[cfg(not(feature = "go"))]
pub mod payload;
mod stats;
mod paced_clock;
mod plugins;
mod runtime;
#[cfg(feature = "speech")]
pub mod speech;
pub mod storage;

#[cfg(test)]
mod tests;

#[cfg_attr(not(feature = "go"), allow(dead_code))]
enum EngineCmd {
  // Return to the launcher; only the dev session sends this.
  #[cfg(feature = "go")]
  Stop,
  // `app_id` names the app a dev push belongs to (from its installed
  // manifest); the runtime re-anchors into that app's data sandbox before the
  // reload applies. None for pushes without a manifest (bytecode one-shots,
  // the BSOD trigger), which keep the current sandbox. `args` is the app's
  // argument vector for this start (the dev session's configured args; empty
  // for a launcher launch), exposed as flux:process argv.
  Reload { code: String, app_id: Option<String>, args: Vec<String> },
}

// What "exit the current app" means, decided by host context (see
// okf/plans/exit-to-launcher.md): with the launcher hosting an app (or the
// BSOD), Stop returns to the launcher, dropping the dev connection on the
// way (see DevExitHandle); at the launcher root, and always in
// launcher-less runtime builds, the client quits - process exit on desktop,
// backgrounding the activity on Android (the platform's back-at-root
// convention). Backs the srt:app exit() verb, which is core's default action
// for an unprevented `back` event. In playback mode exit() ends the recording
// run instead: the frame budget is only an upper bound, and there is no
// launcher to return to.
#[derive(Clone)]
struct ExitPolicy {
  playback: bool,
  #[cfg(feature = "go")]
  launcher_active: Arc<std::sync::atomic::AtomicBool>,
  #[cfg(feature = "go")]
  engine_tx: tokio::sync::mpsc::UnboundedSender<EngineCmd>,
  #[cfg(feature = "go")]
  dev: Option<go::DevExitHandle>,
  alloy_cmd_tx: std::sync::mpsc::Sender<alloy::AlloyCommand>,
}

impl ExitPolicy {
  fn exit(&self) {
    if self.playback {
      std::process::exit(0);
    }
    #[cfg(feature = "go")]
    if !self.launcher_active.load(Ordering::Relaxed) {
      if let Some(dev) = &self.dev {
        dev.disconnect();
      }
      // A failed send means the engine loop is already shutting down; there
      // is nothing left to stop.
      let _ = self.engine_tx.send(EngineCmd::Stop);
      return;
    }
    if cfg!(target_os = "android") {
      let _ = self.alloy_cmd_tx.send(alloy::AlloyCommand::Background);
    } else {
      std::process::exit(0);
    }
  }
}

use alloy::impellers::ISize;
use alloy::rendertree::{FontPayload, PlatformContext, RenderTree};
use alloy::{AlloyEvent, InputState};
use flux::{ExecHandle, FluxEngine};
use runtime::UiRuntime;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

// --- Start Android entry point ------------------------------

// The go dev client: boots the launcher (no app source), auto-dials a dev
// server when the launch intent carries one.
#[cfg(all(target_os = "android", feature = "go"))]
#[no_mangle]
pub extern "C" fn SDL_main(argc: i32, argv: *mut *mut i8) -> i32 {
  let dev_server = parse_dev_server_arg(argc, argv);
  let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("build tokio runtime");
  // Android resolves its own sandboxed root; no flags, no packed identity.
  // No app argument channel either: the activity is launched by intent, not
  // from a command line.
  let storage = storage::StorageSpec { data_root: None, client: None, app_id: None };
  start(&rt, None, alloy::Mode::Run, (1280, 720), false, dev_server, embedded_fonts(), storage, Vec::new());
  0
}

// Where `srt pack --apk` stores the payload in the APK, relative to assets/.
#[cfg(all(target_os = "android", not(feature = "go")))]
const PACKED_PAYLOAD_ASSET: &str = "app.srtapp";

// The production Android runtime: boots the .srtapp packed into the APK
// (`srt pack --apk`), read in place at its offset inside the APK - no dev
// server, no launcher, no extraction. A runner APK without a payload is a
// packaging error, so there is no fallback screen; the failure line lands in
// logcat via SDL's stderr redirect when it does at all - primarily this exit
// code is for the packager's bring-up.
#[cfg(all(target_os = "android", not(feature = "go")))]
#[no_mangle]
pub extern "C" fn SDL_main(_argc: i32, _argv: *mut *mut i8) -> i32 {
  let Some((apk, offset, len)) = alloy::sdl_utils::packed_asset_location(PACKED_PAYLOAD_ASSET) else {
    eprintln!("[srt] no {PACKED_PAYLOAD_ASSET} asset in this APK; nothing to run");
    return 1;
  };
  let Some(payload) = forge::trailer::read_at(apk, offset, len, payload::EMBED_MAGIC).and_then(payload::load) else {
    eprintln!("[srt] {PACKED_PAYLOAD_ASSET} is not a SolidRT app pack; nothing to run");
    return 1;
  };
  forge::fs::set_assets_base(Some(payload.base));
  let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("build tokio runtime");
  // Storage anchors into the app's data sandbox under the Android-resolved
  // root, keyed by the packed identity like every packed distribution. No
  // argument channel: the activity is launched by intent.
  let storage = storage::StorageSpec { data_root: None, client: None, app_id: Some(payload.app_id) };
  start(&rt, Some(payload.app), alloy::Mode::Run, (1280, 720), false, None, payload.fonts, storage, Vec::new());
  0
}

// Pull `--dev-server <addr>` out of the C argv SDL hands SDL_main (populated from
// MainActivity.getArguments). The address is the dev server the go client should
// auto-dial; None when launched without it (e.g. tapping the app icon).
#[cfg(all(target_os = "android", feature = "go"))]
fn parse_dev_server_arg(argc: i32, argv: *mut *mut i8) -> Option<String> {
  if argv.is_null() || argc <= 0 {
    return None;
  }
  let args: Vec<String> = (0..argc as isize)
    .filter_map(|i| {
      let ptr = unsafe { *argv.offset(i) };
      if ptr.is_null() {
        return None;
      }
      // c_char is u8 on Android ARM, i8 elsewhere; cast so this builds on both.
      unsafe { std::ffi::CStr::from_ptr(ptr as *const std::ffi::c_char) }.to_str().ok().map(str::to_owned)
    })
    .collect();
  let mut it = args.iter();
  while let Some(arg) = it.next() {
    if arg == "--dev-server" {
      return it.next().cloned();
    }
  }
  None
}

// Receives the soft-keyboard (IME) inset height in pixels from
// SolidRTActivity.nativeKeyboardInset (Android UI thread) and stores it for
// the event loop to pick up. The export lives in the cdylib so the symbol
// lands in libmain.so; the env/class pointers are unused.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_com_solidrt_app_SolidRTActivity_nativeKeyboardInset(
  _env: *mut core::ffi::c_void,
  _class: *mut core::ffi::c_void,
  px: core::ffi::c_int,
) {
  alloy::set_keyboard_inset_px(px as i32);
}

// Receives hardware-keyboard presence from SolidRTActivity (initial device
// scan plus input-device hotplug). SDL's Android backend never registers
// keyboards, so this is the only source of the fact there.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn Java_com_solidrt_app_SolidRTActivity_nativeHardwareKeyboard(
  _env: *mut core::ffi::c_void,
  _class: *mut core::ffi::c_void,
  present: u8,
) {
  alloy::set_hardware_keyboard(present != 0);
}

// --- End Android entry point ------------------------------

// The launcher is the go client's home; the production runtime never
// shows it (it always boots a provided app source), so only go builds embed it.
#[cfg(feature = "go")]
const LAUNCHER_SOURCE: &str = include_str!("../resources/launcher/index.srt.js");
const BSOD_SOURCE: &str = include_str!("../resources/bsod/bsod.srt.js");

/// The dev client's built-in fonts: the three Noto role defaults, matching what
/// a default packed app carries in its trailer. The dev loop (launcher,
/// BSOD, HUD, `srt render` golden frames) needs deterministic text without a
/// packed payload, so these stay compiled in; the production runtime ships no
/// font data and registers whatever the trailer carries.
#[cfg(feature = "go")]
pub fn embedded_fonts() -> Vec<FontPayload> {
  use std::borrow::Cow;
  vec![
    FontPayload {
      alias: Some("sans".to_string()),
      bytes: Cow::Borrowed(include_bytes!("../../alloy/assets/fonts/NotoSans.ttf")),
    },
    FontPayload {
      alias: Some("serif".to_string()),
      bytes: Cow::Borrowed(include_bytes!("../../alloy/assets/fonts/NotoSerif.ttf")),
    },
    FontPayload {
      alias: Some("mono".to_string()),
      bytes: Cow::Borrowed(include_bytes!("../../alloy/assets/fonts/NotoSansMono.ttf")),
    },
  ]
}

pub const VERSION: &str = match option_env!("SOLIDRT_VERSION") {
  Some(v) => v,
  None => "0.0.0-dev",
};

/// Build profile reported to the dev server (list_clients), so "is this a
/// debug binary" is checkable without inspecting the file.
#[cfg_attr(not(feature = "go"), allow(dead_code))]
pub(crate) const PROFILE: &str = if cfg!(debug_assertions) { "debug" } else { "release" };

/// QuickJS call-stack soft limit. Sits below the UI thread's native stack (see
/// alloy gl::run_context) so deep recursion throws a clean "Maximum call stack
/// size exceeded" instead of overflowing the OS stack. Tunable down per-app later.
const JS_STACK_SIZE: usize = 64 * 1024 * 1024;

/// The app to run: either JS source (dev/default) or precompiled bytecode (packed binary).
pub enum AppSource {
  Text(String),
  Bytecode(Vec<u8>),
}

/// What to run, threaded from `start` into the UI thread (kept distinct from the
/// runtime plumbing it travels with: the tokio handle, alloy context, channels).
struct RunOptions {
  app: Option<AppSource>,
  playback_fps: Option<u32>,
  stats: bool,
  // Dev-server address to auto-connect on launch (go client only; see plugins::dev).
  dev_server: Option<String>,
  // Fonts to register at startup (see FontPayload): the go client's embedded
  // Notos, or a packed binary's trailer fonts.
  fonts: Vec<FontPayload>,
  // Data-root resolution inputs (see storage::resolve).
  storage: storage::StorageSpec,
  // The app's argument vector (everything after the source path or a bare
  // `--` on the runner command line), exposed as flux:process argv. Process-
  // level: a reload or launcher-launched app sees the same vector.
  args: Vec<String>,
}

// Point the assets mount (see forge::fs) at the app's current installed
// version, so reads under assets/ resolve into the immutable version dir while
// the app runs; an app with nothing installed clears it (assets then come over
// the dev proxy or plain cwd). Re-run on every named reload: a fresh install
// of the same app moves the current version dir.
#[cfg(feature = "go")]
fn mount_assets(app_id: &str) {
  forge::fs::set_assets_base(go::store::current_version_dir(app_id).map(forge::fs::AssetsBase::Dir));
}

// Register the font set for `app_id`: the client's base fonts plus the
// current installed version's manifest fonts, replacing whatever the previous
// app registered. Rebuilt from scratch on every app switch so fonts are
// per-app like the assets mount and the data sandbox - nothing accumulates
// across apps, and no alias is ever registered twice into one context.
#[cfg(feature = "go")]
fn apply_app_fonts(app_id: &str, platform: &PlatformContext, base_fonts: &[FontPayload]) {
  let mut fonts = base_fonts.to_vec();
  fonts.extend(go::store::app_fonts(app_id));
  platform.reset_fonts(fonts);
}

// The mechanics of anchoring, separated from storage resolution so tests can
// exercise deleted-cwd recovery without the process-wide storage global.
// Ok(true) means the cwd moved; Ok(false) that it already pointed at a live
// `data_dir`. A cwd whose inode was unlinked makes current_dir() itself fail,
// so that path re-anchors rather than erroring; canonicalize keeps the
// comparison honest when the storage tree sits behind a symlink.
fn anchor_dir(data_dir: &std::path::Path) -> Result<bool, String> {
  if let Ok(cwd) = std::env::current_dir() {
    if data_dir.canonicalize().is_ok_and(|dir| dir == cwd) {
      return Ok(false);
    }
  }
  std::fs::create_dir_all(data_dir).map_err(|e| format!("cannot create {}: {e}", data_dir.display()))?;
  std::env::set_current_dir(data_dir).map_err(|e| format!("cannot enter {}: {e}", data_dir.display()))?;
  Ok(true)
}

// Anchor the process into `app_id`'s data sandbox (see storage: the cwd is
// the app's persistent data dir). The sandbox can vanish while the client
// runs - the launcher removes an app or wipes its cache once it stopped -
// stranding the cwd on an unlinked inode where every relative open fails, so
// anchoring is re-checked before every engine spin instead of done once.
// Quiet when the cwd is already right; without writable storage the cwd is
// left alone (startup warned once).
fn ensure_anchored(app_id: &str) {
  let Some(store) = storage::get() else { return };
  let data_dir = store.app_dir(app_id).join("data");
  match anchor_dir(&data_dir) {
    Ok(true) => log::info!("[srt] working directory set to {}", data_dir.display()),
    Ok(false) => {}
    Err(e) => log::warn!("[srt] could not anchor working directory: {e}"),
  }
}

// Re-anchor into `app_id`'s data sandbox and record it as the app the process
// should be anchored to. `current` is intent, not observed state: if the
// anchor fails here, the loop-top ensure_anchored retries every engine spin
// until it holds.
fn anchor_app(app_id: &str, current: &mut Option<String>) {
  ensure_anchored(app_id);
  *current = Some(app_id.to_string());
}

fn ui_thread(
  handle: tokio::runtime::Handle,
  atx: Arc<alloy::Context>,
  alloy_cmd_tx: std::sync::mpsc::Sender<alloy::AlloyCommand>,
  event_rx: std::sync::mpsc::Receiver<alloy::AlloyEvent>,
  resampler: alloy::resample::SharedResampler,
  user_input_muted: Arc<AtomicBool>,
  opts: RunOptions,
) {
  let RunOptions { app, playback_fps, stats, dev_server, fonts, storage: storage_spec, args } = opts;
  // Only the go dev client consumes the launch dev-server address.
  #[cfg(not(feature = "go"))]
  let _ = dev_server;
  // Resolve the client storage tree, then anchor the process to the app's
  // data sandbox before any app code runs, so relative paths (e.g. a
  // flux:sqlite database) resolve to persistent per-app storage. The launch
  // cwd is unreliable: on Android it is "/" (read-only); on desktop it is
  // wherever the client was spawned. The dev server is a separate process
  // and unaffected.
  storage::init(&storage_spec);
  match storage::get() {
    Some(store) => match std::env::set_current_dir(&store.data_dir) {
      Ok(()) => log::info!("[srt] working directory set to {}", store.data_dir.display()),
      Err(e) => log::warn!("[srt] could not set working directory to {}: {e}", store.data_dir.display()),
    },
    None => log::warn!("[srt] no writable storage, leaving working directory unchanged"),
  }
  // The startup app id: the anchor before any named reload, and what Stop
  // re-anchors to so the launcher never squats in a stopped app's sandbox
  // (removing that app must not fight the cwd, and the loop-top guard must
  // not resurrect its data dir).
  let default_app_id = storage_spec.app_id.clone().unwrap_or_else(|| "default".to_string());
  // The app the process should be anchored to (whose data/ is the cwd). This
  // is intent, not observed state: the loop-top ensure_anchored re-checks it
  // every engine spin, so a sandbox deleted while the client runs (launcher
  // app remove / cache wipe) is rebuilt before the next app run. A dev push
  // naming a different app re-anchors (see anchor_app).
  let mut current_app_id: Option<String> = Some(default_app_id.clone());

  // The client's own fonts (embedded Notos or a packed trailer), kept so app
  // switches can rebuild the font set from scratch (see apply_app_fonts).
  // Embedded font bytes are borrowed Cows, so this clone copies no font data.
  #[cfg(feature = "go")]
  let base_fonts = fonts.clone();
  let platform = Arc::new(PlatformContext::new(fonts));
  // Hand alloy's loop the demand gate's latch so it can self-schedule the
  // repaints its surface lifecycle requires (expose, resize settling,
  // return to visibility); the rebind+repaint policy lives in alloy's
  // liveness.rs.
  alloy_cmd_tx.send(alloy::AlloyCommand::SetFrameRequestLatch(platform.frame_request_handle())).ok();
  // Playback mode renders every frame unconditionally: the lockstep capture
  // loop blocks waiting for each frame's display list, so a frame skipped by
  // the demand-driven gate would deadlock it.
  platform.set_always_render(matches!(playback_fps, Some(rfps) if rfps > 0));
  platform.set_stats_enabled(stats);
  let input_state = Arc::new(InputState::new());
  // The go client's boot rule: no app source means the launcher, always,
  // online or offline. Launched with a dev-server address, the launcher dials
  // it (srt:dev launchAddress) and the server's latched push provides the app;
  // installed apps are launched from the launcher's list, never auto-booted.
  #[cfg(feature = "go")]
  let mut current_app = app.unwrap_or_else(|| AppSource::Text(LAUNCHER_SOURCE.to_string()));
  #[cfg(not(feature = "go"))]
  let mut current_app = app.expect("runtime builds must provide an app source");
  // The running app's argument vector (flux:process argv), per app start:
  // the process tail for the app the process was started for, a push's args
  // for a dev reload, empty for the launcher and its launches.
  let mut current_args = args;
  let mut showing_bsod = false;

  // Bridge the synchronous Alloy event channel onto an async one: a blocking
  // recv on a dedicated thread forwards each event, so the event loop can await
  // events instead of polling on a timer.
  let (ev_tx, mut ev_rx) = tokio::sync::mpsc::unbounded_channel::<alloy::AlloyEvent>();
  // The dev connection's input-injection sender: synthetic downs/ups enter
  // the same batch loop as real SDL input (hit testing, focus, input
  // state); synthetic moves feed the resampler at the send site, following
  // the producer-side rule (see alloy's resample.rs).
  #[cfg(feature = "go")]
  let input_inject_tx = ev_tx.clone();
  std::thread::spawn(move || {
    while let Ok(event) = event_rx.recv() {
      if ev_tx.send(event).is_err() {
        break;
      }
    }
  });

  handle.block_on(async {
    let local = tokio::task::LocalSet::new();
    let current_exec: Rc<RefCell<Option<ExecHandle>>> = Rc::new(RefCell::new(None));
    let current_exec_events = current_exec.clone();
    // The back watchdog's probe handle and its engine-change guard: bumped on
    // every engine build so a watchdog armed against a dead app never fires
    // into its successor.
    let current_exec_probe = current_exec.clone();
    let engine_generation = Arc::new(AtomicU64::new(0));
    let engine_generation_events = engine_generation.clone();

    let platform_events = platform.clone();
    let input_state_events = input_state.clone();
    // Virtual present counter the playback-mode clock derives time from (frame/fps),
    // published by the frame verb. Unused in run mode.
    let playback_frame = Arc::new(AtomicU64::new(0));
    // Run-mode pacing for the animation timestamps (see paced_clock). None in
    // playback mode, which uses the deterministic frame/fps clock.
    let paced_clock = match playback_fps {
      Some(rfps) if rfps > 0 => None,
      _ => Some(paced_clock::PacedClock::new()),
    };
    // Dev-tool pause/step/scale state, shared with the dev connection (which
    // is the only writer); permanently scale 1 in builds without one.
    let clock_control = runtime::ClockControl::new();
    // Raw wall origin behind the paced clock, shared with the schedule-time
    // timer reading installed below (see set_virtual_now_source).
    let wall_start = tokio::time::Instant::now();
    let mut ui_runtime = runtime::FluxRuntime::new(
      current_exec_events,
      playback_frame.clone(),
      paced_clock.clone(),
      clock_control.clone(),
      wall_start,
      platform.clone(),
      resampler.clone(),
      input_state.clone(),
    );

    // Live input capture (see `--capture` in dev-server.ts): set from the
    // dev server's `welcome`/`capture` messages (see go::DevSession::start
    // below). Captured events are forwarded to the dev server over the
    // outbound channel, not written locally -- the server decides what to do
    // with them.
    let capture_enabled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let capture_enabled_events = capture_enabled.clone();
    // One outbound text channel to the dev server: capture events, forwarded
    // log lines, and query replies. The connection task drains it into the
    // websocket while connected.
    let (outbound_tx, outbound_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let capture_tx = outbound_tx.clone();
    // True while a dev-server connection is up; gates log forwarding so an
    // offline app never queues log lines (see go::dev_logger).
    let dev_connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
    // Latest stats figures, published by the draw loop every frame; the dev
    // connection answers stats queries from here without touching this thread.
    let stats_snapshot = Arc::new(std::sync::Mutex::new(stats::StatsSnapshot::default()));
    // Per-frame history behind the stats query's window summary; written by
    // the draw loop once per rebuild. Outlives engines so a reload keeps the
    // recent frames.
    let frame_history = Arc::new(std::sync::Mutex::new(frame_history::FrameHistory::new()));
    // Send-safe copy of the live engine's exec handle, refreshed on each engine
    // build; the dev connection uses it to snapshot the render tree on the JS
    // thread for tree queries.
    let query_exec: Arc<std::sync::Mutex<Option<ExecHandle>>> = Arc::new(std::sync::Mutex::new(None));
    #[cfg(not(feature = "go"))]
    let _ = (capture_enabled, outbound_rx, &dev_connected, &outbound_tx, &clock_control);

    // Pacing policy sender for the event loop below (see the InputDevices
    // arm); the original stays with this thread for later commands.
    let alloy_cmd_events = alloy_cmd_tx.clone();
    local.spawn_local(async move {
      loop {
        // One batch per cycle: block for the first event, then drain whatever
        // else is queued, with one coalescing rule: frame signals
        // (FrameRendered / Tick) collapse to the newest one, dispatched
        // after the batch's input. A frame signal triggers a full paint +
        // present on this thread; when presents stall (driver throttling,
        // an occluded window) the signals pile up in the unbounded queue,
        // and each stale one replays another paint into the saturated
        // swapchain - the loop falls arbitrarily far behind, re-animating
        // old hover states and starving input and dev queries. Only the
        // newest signal matters; the dropped ones are exactly the catch-up
        // frames a browser skips too. Playback mode is lockstep (one
        // FrameRendered in flight, no Ticks), so its captures never see a
        // collapse. Pointer moves never appear here: their producers
        // consume them into the resampler (see alloy's resample.rs) and the
        // frame verb samples one position per pointer per signal, so a
        // stalled drain replays no stale positions either.
        let Some(first) = ev_rx.recv().await else { break };
        let mut events: Vec<AlloyEvent> = Vec::new();
        let mut frame_signal: Option<AlloyEvent> = None;
        let mut incoming = Some(first);
        loop {
          let event = match incoming.take() {
            Some(event) => event,
            None => match ev_rx.try_recv() {
              Ok(event) => event,
              Err(_) => break,
            },
          };
          match event {
            signal @ (AlloyEvent::FrameRendered { .. } | AlloyEvent::Tick { .. }) => frame_signal = Some(signal),
            event => events.push(event),
          }
        }
        let batch = events.into_iter().chain(frame_signal);
        for event in batch {
          if capture_enabled_events.load(Ordering::Relaxed) {
            if let Some(script_event) = alloy::ScriptEvent::from_alloy_event(&event) {
              let kind = if script_event.down { "keydown" } else { "keyup" };
              // W3C key values include printables like `"` and `\`; escape them
              // so the hand-built JSON line stays valid.
              let key = script_event.key.replace('\\', "\\\\").replace('"', "\\\"");
              let _ = capture_tx.send(format!(r#"{{"type":"capture","kind":"{kind}","key":"{key}"}}"#));
            }
          }
          // Runner bookkeeping: device and window facts that outlive any single
          // engine (the pointer positions hover refresh reads, the platform's
          // window geometry, fps). Everything engine-facing happens behind the
          // UiRuntime verbs below.
          match &event {
            AlloyEvent::Quit => std::process::exit(0),
            AlloyEvent::Key { modifiers, .. } => {
              input_state_events.set_modifiers(*modifiers);
            }
            AlloyEvent::Resize { size, safe_area, display_scale } => {
              platform_events.set_window_size(size.width as f32, size.height as f32);
              platform_events.set_display_scale(*display_scale);
              platform_events.set_safe_area(*safe_area);
            }
            // Move positions are recorded by the frame verb from the
            // resampler's samples; only the arrival-dispatched events pass
            // through here.
            AlloyEvent::PointerDown { pointer_id, pointer_type, x, y, modifiers, .. } => {
              input_state_events.set_pointer_pos((*pointer_type, *pointer_id), *x, *y);
              input_state_events.set_modifiers(*modifiers);
            }
            AlloyEvent::PointerUp { pointer_id, pointer_type, x, y, modifiers, .. } => {
              input_state_events.set_pointer_pos((*pointer_type, *pointer_id), *x, *y);
              input_state_events.set_modifiers(*modifiers);
              // Touch pointers end at release; mouse pointers persist.
              if *pointer_type == alloy::PointerType::Touch {
                input_state_events.remove_pointer((*pointer_type, *pointer_id));
              }
            }
            AlloyEvent::Wheel { modifiers, .. } => input_state_events.set_modifiers(*modifiers),
            AlloyEvent::FrameRendered { fps, .. } | AlloyEvent::Tick { fps, .. } => {
              platform_events.set_fps(*fps);
            }
            // Frame-pacing policy from the input-modality fact, re-evaluated
            // on hotplug: touch devices get the latency-first vsync
            // phase-lock (finger-to-glass drag latency), everything else
            // (TV remotes, keyboards, pointers) gets fluency-first swap
            // pacing - a saturated buffer queue presents metronomically
            // where the vsync release chain's jitter drops latches (see
            // okf/backlog/frame-pacing-fluency.md; measured 0 drops vs
            // ~1.4 percent on a 50Hz Android TV).
            AlloyEvent::InputDevices { keyboard, mouse, touch, screen_keyboard } => {
              let pacing = if *touch { alloy::FramePacing::VsyncLocked } else { alloy::FramePacing::SwapPaced };
              log::info!(
                "[srt] input devices: keyboard={keyboard} mouse={mouse} touch={touch} screen_keyboard={screen_keyboard} -> pacing {pacing:?}"
              );
              alloy_cmd_events.send(alloy::AlloyCommand::SetFramePacing(pacing)).ok();
            }
            _ => {}
          }
          match event {
            // FrameRendered reports the frame native just finished drawing. JS
            // uses the "render" event to compute the NEXT frame's state, so
            // shift the field by +1. The JS-side bootstrap owns frame 0;
            // without the shift, playback mode re-runs frame 0 at tick 0 and
            // duplicates a PNG.
            AlloyEvent::FrameRendered { frame, .. } => ui_runtime.frame(frame + 1),
            // Tick's frame is already the next present index (one past the
            // last FrameRendered), so no +1 here.
            AlloyEvent::Tick { frame, .. } => ui_runtime.frame(frame),
            // The back intent dispatches to JS like any window event, backed
            // by a liveness watchdog: the emit just queued runs synchronously
            // on the JS executor, so a probe queued behind it proves the
            // engine processed the dispatch (a handler prevented it, or
            // exit() already ran). No probe by the deadline means the engine
            // is wedged - and a blocked JS thread also blocks EngineCmd
            // handling here, so returning to the launcher is impossible; quit
            // the process so the user is never trapped. Skipped when the
            // engine changed meanwhile: that request belonged to an app that
            // is already gone.
            AlloyEvent::Back => {
              ui_runtime.event(&AlloyEvent::Back);
              let alive = Arc::new(std::sync::atomic::AtomicBool::new(false));
              if let Some(exec) = current_exec_probe.borrow().as_ref() {
                let probe = alive.clone();
                exec.exec(move |_| probe.store(true, Ordering::Relaxed));
              }
              let generation = engine_generation_events.load(Ordering::Relaxed);
              let generations = engine_generation_events.clone();
              tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                if !alive.load(Ordering::Relaxed) && generations.load(Ordering::Relaxed) == generation {
                  log::warn!("[srt] app did not respond to back; exiting");
                  std::process::exit(1);
                }
              });
            }
            // Surface liveness (rebind + repaint on expose, resize, return to
            // visibility) is alloy's: its loop handles it before these events
            // arrive here (see alloy's liveness.rs and the
            // SetFrameRequestLatch registration above).
            //
            // Exposed carries no app-level meaning; not forwarded.
            AlloyEvent::Exposed => {}
            AlloyEvent::Visibility { visible } => {
              // Rare lifecycle transitions; logged so device traces show
              // whether and when the platform reported them (the resume
              // repaint pipeline depends on it).
              log::info!("[srt] visibility: {}", if visible { "visible" } else { "hidden" });
              // Forward to the engine as the sticky `visibility` event.
              // Same-state repeats are normal here (app + window paths, plus
              // the Android background watch); core's env signal dedupes.
              ui_runtime.event(&AlloyEvent::Visibility { visible });
            }
            event => ui_runtime.event(&event),
          }
        }
      }
    });

    #[cfg_attr(not(feature = "go"), allow(unused_variables))]
    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<EngineCmd>();
    // True while the current engine runs the launcher itself (set at each
    // engine build); exit() at the launcher root quits instead of Stop-ing.
    #[cfg(feature = "go")]
    let launcher_active = Arc::new(std::sync::atomic::AtomicBool::new(false));
    // The dev-server client: connection supervisor, recents, proxy state and the
    // srt.dev surface. None in playback mode (and entirely absent without the
    // `go` feature). This is the runtime's only seam to the dev client.
    #[cfg(feature = "go")]
    let dev_session = go::DevSession::start(
      &handle,
      cmd_tx.clone(),
      playback_fps,
      &local,
      current_exec.clone(),
      platform.stats_handles(),
      capture_enabled,
      dev_connected.clone(),
      clock_control.clone(),
      input_inject_tx,
      resampler.clone(),
      user_input_muted.clone(),
      outbound_rx,
      go::QueryHandles {
        stats: stats_snapshot.clone(),
        history: frame_history.clone(),
        exec: query_exec.clone(),
        outbound_tx: outbound_tx.clone(),
      },
      dev_server,
    );
    let exit_policy = ExitPolicy {
      playback: playback_fps.is_some(),
      #[cfg(feature = "go")]
      launcher_active: launcher_active.clone(),
      #[cfg(feature = "go")]
      engine_tx: cmd_tx.clone(),
      #[cfg(feature = "go")]
      dev: dev_session.as_ref().map(|d| d.exit_handle()),
      alloy_cmd_tx: alloy_cmd_tx.clone(),
    };

    // flux::Timeline is the frame timeline the rAF/render timestamps march
    // on - frame-stepped, pausable by the dev clock control - for native
    // consumers (video sync). The virtual timers march on it only in
    // playback; in run mode they take the paced clock's wall-anchored timer
    // reading (see the install below). Injected into each engine; persists
    // across reloads for continuous time. performance.now() is deliberately
    // NOT on it: that is real elapsed time, for measuring work; Date.now()
    // is calendar time.
    let timeline = match playback_fps {
      // Playback mode: derive time from the present counter (frame/fps) so
      // the frame timeline is deterministic and recordings reproducible.
      Some(rfps) if rfps > 0 => {
        let playback_frame = playback_frame.clone();
        flux::Timeline::new(move || playback_frame.load(Ordering::Relaxed) as f64 * 1000.0 / rfps as f64)
      }
      // Run mode: the paced frame clock (see paced_clock; the frame verb ticks
      // it, correcting toward wall time at normal speed).
      _ => {
        let paced = paced_clock.clone().expect("run mode has a paced clock");
        flux::Timeline::new(move || paced.now_ms())
      }
    };

    if storage::get().is_none() {
      log::warn!("No fetch cache dir; fetch caching disabled");
    }

    loop {
      // Re-anchor before anything in this spin touches the sandbox: the data
      // dir may have been deleted since the last spin, and a reload naming
      // the SAME app would otherwise keep the stranded cwd forever.
      if let Some(app_id) = &current_app_id {
        ensure_anchored(app_id);
      }
      // Fetch disk cache: per app, so cached assets are browsable and
      // clearable per app (and die with it on remove). Resolved per engine:
      // the anchored app changes across reloads.
      let fetch_cache_dir = storage::get()
        .map(|store| store.cache_dir(current_app_id.as_deref().unwrap_or("default")));
      let render_tree = RenderTree::new();
      let platform = platform.clone();
      let atx = atx.clone();
      // A reloaded app must not inherit (or leak) the previous app's open
      // capture devices or playing sounds; their JS handles died with the old
      // engine, so nothing else will ever stop them.
      atx.close_all_cameras();
      atx.close_all_microphones();
      atx.close_all_audio();
      let input_state = input_state.clone();

      let draw_platform = platform.clone();
      let draw_atx = atx.clone();
      #[cfg(feature = "speech")]
      let speech_atx = atx.clone();
      let builder = FluxEngine::builder()
        .stack_size(JS_STACK_SIZE)
        .user_agent(format!("SolidRT/{VERSION}"))
        // Isolate modules are manifest assets under isolates/ (see
        // okf/done/isolates-and-ports.md), resolved through the assets mount.
        .isolate_resolver(flux::resolve_isolate_from_assets);
      let builder = match &fetch_cache_dir {
        Some(dir) => builder.cache_dir(dir.clone()),
        None => builder,
      };
      // The go client's logger also forwards lines to a connected dev server;
      // other builds log locally only.
      #[cfg(feature = "go")]
      let builder = builder.logger(go::dev_logger(outbound_tx.clone(), dev_connected.clone()));
      #[cfg(not(feature = "go"))]
      let builder = builder.logger(|level, msg| match level {
        flux::LogLevel::Debug => log::debug!("{msg}"),
        flux::LogLevel::Log => log::info!("{msg}"),
        flux::LogLevel::Warn => log::warn!("{msg}"),
        flux::LogLevel::Error => log::error!("{msg}"),
      });
      // flux owns the gui plugin set, its registration order and the frame
      // protocol the draw bridge (`srt:render`) draws through; lattice only
      // supplies the host instances they bind.
      let builder = flux::gui::install(
        builder,
        flux::gui::GuiHost {
          platform: platform.clone(),
          alloy: atx.clone(),
          render_tree,
          alloy_cmd_tx: alloy_cmd_tx.clone(),
        },
      );
      let draw_stats = stats_snapshot.clone();
      let draw_history = frame_history.clone();
      let draw_connected = dev_connected.clone();
      let draw_muted = user_input_muted.clone();
      let builder = builder
        .plugin(move |ctx| {
          plugins::draw::store_state(
            &ctx,
            draw_platform,
            draw_atx,
            input_state,
            draw_stats,
            draw_history,
            draw_connected,
            draw_muted,
          )
        })
        .module_override("srt:render", plugins::draw::SrtRenderModule)
        .module_override("srt:events", plugins::events::SrtEventsModule)
        .module_override("srt:dev", plugins::dev::SrtDevModule)
        .module_override("srt:apps", plugins::apps::SrtAppsModule)
        .module_override("srt:app", plugins::app::SrtAppModule)
        .userdata(timeline.clone())
        .userdata(flux::ProcessArgs(current_args.clone()));
      // Timers join a frame-stepped timeline (see flux virtual time): the
      // frame verb advances them once per frame signal, so a dev-clock pause
      // freezes setTimeout/setInterval too, and playback replays them
      // deterministically. In run mode that is the paced clock's
      // wall-anchored timer reading, NOT the smoothed animation reading rAF
      // gets - deadlines must not lag the wall clock under slow frames (see
      // paced_clock). Seeded with the current reading so a reload does not
      // replay the timeline from zero.
      let builder = {
        let seed = match &paced_clock {
          Some(pc) => pc.timer_now_ms(),
          None => timeline.now_ms(),
        };
        builder.plugin(move |ctx| flux::install_virtual_time(&ctx, seed))
      };
      // Run mode: anchor schedule-time deadlines to a fresh timer-timeline
      // reading, so a timer registered mid-frame measures its delay from
      // registration rather than from the previous advance (which is up to
      // one frame stale and would fire it that much early). Playback keeps
      // last-advance anchoring: deterministic replay must not read a live
      // clock.
      let builder = match &paced_clock {
        Some(pc) => {
          let pc = pc.clone();
          builder.plugin(move |ctx| {
            let pc = pc.clone();
            flux::set_virtual_now_source(&ctx, move || pc.timer_live_ms(wall_start.elapsed().as_secs_f64() * 1000.0))
          })
        }
        None => builder,
      };
      // The running app's own surface (exit()), in every build: the
      // production runtime exits too, it just always quits.
      let builder = {
        let policy = exit_policy.clone();
        builder.plugin(move |ctx| {
          plugins::app::install(
            &ctx,
            plugins::app::AppControl::new(plugins::app::AppControlInner { exit: Box::new(move || policy.exit()) }),
          )
        })
      };
      #[cfg(feature = "speech")]
      let builder = builder.plugin(move |ctx| plugins::speech::init(ctx, speech_atx));
      // The launcher's app-management surface over the version store; the
      // launch closure feeds the same reload path a dev push uses.
      #[cfg(feature = "go")]
      let builder = {
        let apps_tx = cmd_tx.clone();
        builder.plugin(move |ctx| go::control::install_apps_control(ctx, apps_tx))
      };
      // Install the dev-server control surface and (when enabled) the proxy.
      #[cfg(feature = "go")]
      let builder = match &dev_session {
        Some(dev) => dev.augment_builder(builder),
        None => builder,
      };
      let engine = builder.build();
      *current_exec.borrow_mut() = Some(engine.exec_handle());
      engine_generation.fetch_add(1, Ordering::Relaxed);
      #[cfg(feature = "go")]
      launcher_active.store(
        matches!(&current_app, AppSource::Text(src) if src.as_str() == LAUNCHER_SOURCE),
        Ordering::Relaxed,
      );
      *query_exec.lock().expect("query exec lock poisoned") = Some(engine.exec_handle());
      alloy_cmd_tx.send(alloy::AlloyCommand::EmitInitEvents).ok();
      // The window icon and title follow the app like the sandbox and fonts
      // do: the installed manifest's icon and displayName, or the client's
      // own mark and name for the launcher (its default_app_id has no store
      // entry). An app's explicit `title` window prop still wins: it applies
      // later, during render. Old manifests without displayName title as
      // their id.
      #[cfg(feature = "go")]
      {
        let app_id = current_app_id.as_deref().unwrap_or(&default_app_id);
        go::icon::apply_app_icon(app_id, &alloy_cmd_tx);
        let title = if app_id == default_app_id {
          "SolidRT".to_string()
        } else {
          go::store::app_display_name(app_id).unwrap_or_else(|| app_id.to_string())
        };
        alloy_cmd_tx.send(alloy::AlloyCommand::SetTitle(title)).ok();
      }
      // Replay the current connection state into this engine so a reload (e.g.
      // a server stop returning to the launcher) keeps the right indicator.
      #[cfg(feature = "go")]
      if let Some(dev) = &dev_session {
        dev.replay_state(&engine.exec_handle());
      }

      log::info!("[srt] flux engine start");
      let mut next_app: Option<AppSource> = None;
      let mut next_app_id: Option<String> = None;
      local
        .run_until(async {
          tokio::select! {
            _ = async {
              match &current_app {
                AppSource::Text(src) => engine.eval_source(src).await,
                AppSource::Bytecode(bytes) => engine.eval(bytes.clone()).await,
              }
            } => {}
            Some(cmd) = cmd_rx.recv() => {
              match cmd {
                EngineCmd::Reload { code, app_id, args } => {
                  next_app = Some(AppSource::Text(code));
                  next_app_id = app_id;
                  current_args = args;
                }
                #[cfg(feature = "go")]
                EngineCmd::Stop => {
                  next_app = Some(AppSource::Text(LAUNCHER_SOURCE.to_string()));
                  current_args = Vec::new();
                  // Back to the launcher: release the stopped app's sandbox
                  // by re-anchoring to the startup default, so the launcher
                  // can remove the app without the cwd (or the loop-top
                  // guard) holding its data dir alive. Its fonts go with it -
                  // the launcher runs on the base set alone.
                  current_app_id = Some(default_app_id.clone());
                  platform.reset_fonts(base_fonts.clone());
                }
              }
            }
          }
        })
        .await;
      if let Some(app) = next_app {
        if let Some(app_id) = &next_app_id {
          anchor_app(app_id, &mut current_app_id);
          #[cfg(feature = "go")]
          mount_assets(app_id);
          #[cfg(feature = "go")]
          apply_app_fonts(app_id, &platform, &base_fonts);
        }
        current_app = app;
        showing_bsod = false;
      } else if !showing_bsod {
        // Engine exited on its own (a module/startup error means render() never
        // ran, so nothing kept it alive). Show the BSOD instead of a frozen
        // frame; it stays live until a fixed app reloads.
        current_app = AppSource::Text(BSOD_SOURCE.to_string());
        showing_bsod = true;
      } else {
        // The BSOD itself exited; wait for a command rather than respinning.
        match local.run_until(cmd_rx.recv()).await {
          Some(EngineCmd::Reload { code, app_id, args }) => {
            if let Some(app_id) = &app_id {
              anchor_app(app_id, &mut current_app_id);
              #[cfg(feature = "go")]
              mount_assets(app_id);
              #[cfg(feature = "go")]
              apply_app_fonts(app_id, &platform, &base_fonts);
            }
            current_app = AppSource::Text(code);
            current_args = args;
            showing_bsod = false;
          }
          #[cfg(feature = "go")]
          Some(EngineCmd::Stop) => {
            current_app = AppSource::Text(LAUNCHER_SOURCE.to_string());
            current_args = Vec::new();
            showing_bsod = false;
            // Same sandbox and font release as the in-loop Stop arm above.
            current_app_id = Some(default_app_id.clone());
            platform.reset_fonts(base_fonts.clone());
          }
          None => break,
        }
      }
    }
  });
}

/// A panic on any runtime thread ends the process. Rust's default only
/// unwinds the panicking thread: a JS-thread panic left the window up, the
/// dev connection answering, and the process alive - a wedge that looks like
/// a hang and holds the binary open. Log through the platform logger first
/// (stderr is lost on Android; logcat is not), then let the default hook print
/// its report, then exit with the conventional panic status.
fn install_panic_hook() {
  let default_hook = std::panic::take_hook();
  std::panic::set_hook(Box::new(move |info| {
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("<unnamed>");
    let location = info.location().map(|l| format!(" at {}:{}", l.file(), l.line())).unwrap_or_default();
    log::error!("[srt] Panic in thread {name}{location}: {}", info.payload_as_str().unwrap_or("<non-string payload>"));
    default_hook(info);
    std::process::exit(101);
  }));
}

/// Err only comes out of playback mode (an incomplete capture); the binary
/// turns it into the process exit code.
pub fn start(
  rt: &tokio::runtime::Runtime,
  app_source: Option<AppSource>,
  mode: alloy::Mode,
  size: (u32, u32),
  stats: bool,
  dev_server: Option<String>,
  fonts: Vec<FontPayload>,
  storage: storage::StorageSpec,
  args: Vec<String>,
) -> Result<(), String> {
  alloy::install_logger();
  install_panic_hook();
  log::info!("[srt] SolidRT version {VERSION}");

  let handle = rt.handle().clone();
  let playback_fps = match &mode {
    alloy::Mode::Playback(playback) => Some(playback.fps),
    _ => None,
  };
  let app = alloy::setup("SolidRT", ISize::new(size.0 as i64, size.1 as i64), mode);

  let opts = RunOptions { app: app_source, playback_fps, stats, dev_server, fonts, storage, args };
  let resampler = app.resampler();
  let user_input_muted = app.user_input_mute();
  app.run(move |atx, alloy_cmd_tx, event_rx| {
    ui_thread(handle, atx, alloy_cmd_tx, event_rx, resampler, user_input_muted, opts);
  })
}
