// The stats query's reply, built once for both its readers: the dev
// server's `/stats` (go/connection.rs, over the websocket) and an app
// test's `app.stats()` (plugins/test.rs, on the JS thread). The figures
// come from three places - the draw loop's latched snapshot, the frame
// history's window summary, and live reads on the JS thread (node counts,
// raster counters, the residency gate, the app's registered stats) - so a
// wedged app still answers the first two.

/// Round to two decimals for the JSON payloads: raw f32s serialize with float
/// noise (0.1 -> 0.10000000149...) that only bloats the wire format.
pub(crate) fn round2(v: f32) -> f64 {
  (v as f64 * 100.0).round() / 100.0
}

/// Default window the stats summary covers when the query names none.
pub(crate) const STATS_WINDOW_DEFAULT_MS: f64 = 5000.0;

/// Everything a stats reply is built from. `snap` is the draw loop's latched
/// figures; `clock` the client's own clock at query time (`timeMs` on its
/// monotonic origin, the latest present index) so two samples can be
/// differenced; `ask` the (clamped) window the query asked for and
/// `window` the frame-history summary over it (None when no frame changed
/// the picture inside it; the reply then says so with `frames: 0`);
/// `counts` (mounted, total) from the live tree when the query could run on
/// the JS thread - the reply then carries mountedNodes and orphanNodes
/// (total - mounted: nodes unreachable from the root, i.e. leaked or
/// intentionally kept detached); `raster` the live raster counters;
/// `residency` the live residency gate figures; `app` the counters the app
/// registered through `sol:dev` `registerStat`, read now. Without an
/// engine the last four are simply absent.
pub(crate) struct StatsQuery<'a> {
  pub(crate) snap: crate::stats::StatsSnapshot,
  pub(crate) time_ms: f64,
  /// Age of the newest frame record, the frame the top-level figures
  /// describe; None while the history is empty.
  pub(crate) latest_frame_age_ms: Option<f64>,
  pub(crate) ask: crate::frame_history::Window,
  pub(crate) window: Option<&'a crate::frame_history::WindowSummary>,
  pub(crate) counts: Option<(usize, usize)>,
  pub(crate) raster: Option<alloy::RasterCounters>,
  /// Frames submitted to the raster thread so far (alloy `Context::submitted_frames`).
  pub(crate) submitted_frames: Option<u64>,
  pub(crate) residency: Option<alloy::spatial::ResidencyStats>,
  /// The mixer's counters; None until the audio device has been opened.
  pub(crate) audio: Option<alloy::audio::AudioStats>,
  pub(crate) app: Option<serde_json::Map<String, serde_json::Value>>,
}

pub(crate) fn round2_64(v: f64) -> f64 {
  (v * 100.0).round() / 100.0
}

pub(crate) fn stats_data(r: StatsQuery<'_>) -> serde_json::Map<String, serde_json::Value> {
  let s = r.snap;
  let mut data = serde_json::Map::new();
  let mut put = |k: &str, v: serde_json::Value| {
    data.insert(k.into(), v);
  };
  put("timeMs", round2_64(r.time_ms).into());
  put("frame", s.frame.into());
  if let Some(age) = r.latest_frame_age_ms {
    put("latestFrameAgeMs", round2_64(age).into());
  }
  put("fps", s.fps.into());
  put("cpuPct", round2(s.cpu_pct).into());
  put("memBytes", s.mem_bytes.into());
  put("jsMs", round2(s.js_ms).into());
  put("frameMs", round2(s.frame_ms).into());
  put("setPropsPerFrame", round2(s.set_count).into());
  put("layoutMs", round2(s.layout_ms).into());
  put("postLayoutMs", round2(s.post_ms).into());
  put("paintMs", round2(s.paint_ms).into());
  put("hoverMs", round2(s.hover_ms).into());
  put("reusedPerSec", s.reused.into());
  put("skippedPerSec", s.skipped.into());
  put("textures", s.textures.into());
  put("nodes", s.node_count.into());
  put("measureCalls", s.measure_calls.into());
  put("wordShapes", s.word_shapes.into());
  put("wordHits", s.word_hits.into());
  put("dirtiedNodes", s.dirtied.into());
  put("cacheGets", s.cache_gets.into());
  put("cacheHits", s.cache_hits.into());
  put("nodesPainted", s.paint.nodes_painted.into());
  put("backdropsPrepainted", s.paint.backdrops_prepainted.into());
  put("damagePx", (s.paint.damage_px.round() as i64).into());
  put("paintOps", paint_ops_json(&s.counters));
  put("window", window_json(r.window, r.time_ms, r.ask));
  if let Some((mounted, total)) = r.counts {
    put("mountedNodes", mounted.into());
    put("orphanNodes", total.saturating_sub(mounted).into());
  }
  if let Some(rc) = r.raster {
    put("rasterQueue", rc.queue.into());
    put("idleTicks", rc.idle_ticks.into());
    put("partialPresents", rc.partial_presents.into());
    put("videoLatched", rc.video_latched.into());
    put("videoSkipped", rc.video_skipped.into());
    put("videoLateLatches", rc.video_late.into());
    put("cameraUploaded", rc.camera_uploaded.into());
    put("cameraSkipped", rc.camera_skipped.into());
    put("fenceTimeouts", rc.fence_timeouts.into());
    put("retiredFrames", rc.retired_frames.into());
    if let Some(submitted) = r.submitted_frames {
      put("submittedFrames", submitted.into());
    }
    put("contextLosses", rc.context_losses.into());
    put("missedPresents", rc.missed_presents.into());
    put("cadenceHold", rc.cadence_hold.into());
    put("frameWorkMeanMs", round2(rc.work_mean_ms).into());
    put("frameWorkMaxMs", round2(rc.work_max_ms).into());
    put("gpuPasses", rc.passes.into());
    // Integer ms: sub-ms increments accumulate in the microsecond counters
    // before this division, so the cumulative rounding loss stays under 1ms.
    put("gpuPassIssueMs", (rc.pass_issue_micros / 1000).into());
    // Absent (not 0) when the context has no timer queries.
    if let Some(exec) = rc.pass_exec_micros {
      put("gpuPassExecMs", (exec / 1000).into());
    }
    if let Some(exec) = rc.frame_exec_micros {
      put("gpuFrameExecMs", (exec / 1000).into());
    }
    put("rasterCmdMs", (rc.cmd_micros / 1000).into());
  }
  // The residency gate, read live like the raster counters: the resident
  // set now, and cumulative enters and leaves (the window carries its own
  // pair).
  if let Some(g) = r.residency {
    put("regionsWanted", g.wanted.into());
    put("regionsResident", g.resident.into());
    put("regionsPending", g.pending.into());
    put("regionsStalled", g.stalled.into());
    put("regionEnters", g.enters.into());
    put("regionLeaves", g.leaves.into());
    put("residentWeight", g.weight.into());
  }
  // The mixer, read live: voices in the mix, rings outrun since the device
  // opened, and the longest callback render pass since the last query (the
  // number that says whether the mix keeps its budget on a device).
  if let Some(a) = r.audio {
    put("audioVoices", a.live_voices.into());
    put("audioUnderruns", a.underruns.into());
    put("audioRenderMaxMs", round2_64(a.render_max_ms).into());
    put("audioDeviceRate", a.device_rate.into());
    put("audioDeviceFrames", a.device_frames.into());
    // How far the mix runs ahead of the device: rendered audio waiting in
    // the device stream, in ms at the device rate.
    put("audioDeviceQueuedMs", round2_64(a.device_queued_frames as f64 * 1000.0 / a.device_rate.max(1) as f64).into());
  }
  if let Some(app) = r.app {
    put("app", app.into());
  }
  data
}

/// The window block: percentiles of the JS-thread critical path over the
/// rebuilt frames in the window, the count over the refresh period, and the
/// worst frame with its phase breakdown and layout activity - the frame the
/// smoothed figures average away. Raster rates ride along when the window
/// spans two or more frames. `now_ms` is the query instant the worst frame's
/// age is measured from; `ask` the window asked for, echoed even when no
/// frame fell inside it (the summary then has no window of its own).
fn window_json(
  window: Option<&crate::frame_history::WindowSummary>,
  now_ms: f64,
  ask: crate::frame_history::Window,
) -> serde_json::Value {
  let Some(w) = window else {
    return match ask {
      crate::frame_history::Window::Ms(ms) => serde_json::json!({ "windowMs": ms, "frames": 0 }),
      crate::frame_history::Window::Frames(n) => serde_json::json!({ "windowFrames": n, "frames": 0 }),
    };
  };
  let worst = &w.worst;
  let mut data = serde_json::Map::new();
  let mut put = |k: &str, v: serde_json::Value| {
    data.insert(k.into(), v);
  };
  put("windowMs", round2_64(w.window_ms).into());
  if let Some(n) = w.window_frames {
    put("windowFrames", n.into());
  }
  put("frames", w.frames.into());
  put("p50Ms", round2(w.p50_ms).into());
  put("p95Ms", round2(w.p95_ms).into());
  put("maxMs", round2(w.max_ms).into());
  put("slowFrames", w.slow_frames.into());
  put("captureFrames", w.capture_frames.into());
  put("periodMs", round2(worst.period_ms).into());
  put("backdropsPrepainted", w.backdrops_prepainted.into());
  put("textLayers", w.text_layers.into());
  put("nodesPaintedMax", w.nodes_painted_max.into());
  put("regionEnters", w.region_enters.into());
  put("regionLeaves", w.region_leaves.into());
  put(
    "worst",
    serde_json::json!({
      "ageMs": round2_64(now_ms - worst.at_ms),
      "frame": worst.frame,
      "totalMs": round2(worst.total_ms),
      "jsMs": round2(worst.js_ms),
      "layoutMs": round2(worst.layout_ms),
      "postLayoutMs": round2(worst.post_ms),
      "paintMs": round2(worst.paint_ms),
      "hoverMs": round2(worst.hover_ms),
      "measureCalls": worst.counters.measure_calls,
      "wordShapes": worst.counters.word_shapes,
      "wordHits": worst.counters.word_hits,
      "dirtiedNodes": worst.counters.dirtied,
      "cacheGets": worst.counters.cache_gets,
      "cacheHits": worst.counters.cache_hits,
      "nodesPainted": worst.nodes_painted,
      "backdropsPrepainted": worst.backdrops_prepainted,
      "paintOps": paint_ops_json(&worst.counters),
    }),
  );
  if let Some(r) = &w.raster_rates {
    put("missedPresents", r.missed_presents.into());
    put("fenceTimeoutsPerSec", round2(r.fence_timeouts_per_sec).into());
    put("gpuPassesPerFrame", round2(r.passes_per_frame).into());
    put("gpuPassIssueMsPerFrame", round2(r.pass_issue_ms_per_frame).into());
    if let Some(exec) = r.pass_exec_ms_per_frame {
      put("gpuPassExecMsPerFrame", round2(exec).into());
    }
    if let Some(exec) = r.frame_exec_ms_per_frame {
      put("gpuFrameExecMsPerFrame", round2(exec).into());
    }
    put("rasterCmdMsPerSec", round2(r.cmd_ms_per_sec).into());
  }
  if !w.target_rates.is_empty() {
    let targets: Vec<serde_json::Value> = w
      .target_rates
      .iter()
      .map(|t| {
        let mut obj = serde_json::json!({
          "id": t.id,
          "passesPerFrame": round2(t.passes_per_frame),
          "gpuPassIssueMsPerFrame": round2(t.issue_ms_per_frame),
          "verticesPerFrame": round2(t.vertices_per_frame),
        });
        flux::gui::inspect::insert_label(&mut obj, &t.label);
        if let Some(exec) = t.exec_ms_per_frame {
          obj
            .as_object_mut()
            .expect("target json is an object")
            .insert("gpuPassExecMsPerFrame".into(), round2(exec).into());
        }
        obj
      })
      .collect();
    put("targets", targets.into());
  }
  data.into()
}

/// The display-list ops one rebuild's paint walk recorded (see
/// alloy::rendertree::counters), the per-frame counts that name a paint
/// cost on a tiled GPU without a reload per hypothesis.
fn paint_ops_json(c: &alloy::rendertree::counters::LayoutCounters) -> serde_json::Value {
  serde_json::json!({
    "draws": c.draws,
    "textLayers": c.text_layers,
    "clips": c.clips,
    "roundedClips": c.rounded_clips,
    "saveLayers": c.save_layers,
    "blends": c.blends,
    "gradients": c.gradients,
  })
}
