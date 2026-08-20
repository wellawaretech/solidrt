---
title: "flux:audio mix control: playback rate, master gain, ramps, voice cap, PCM validation"
description: A Playback's rate is fixed forever (no pitch sweeps, no doppler), there is no master gain or bus so every app hand-rolls mute, gain/pan writes cannot ramp, a runaway play() loop wedges the JS thread with no error, and non-finite PCM samples load silently; SDL3_mixer already exposes most of the missing controls.
created: 2026-08-20
---

# flux:audio mix control: playback rate, master gain, ramps, voice cap, PCM validation

The `load -> Clip -> overlapping Playback` decomposition is right and stays.
These are the controls missing around it, ordered by cost to an app.

## 1. Playback rate

`Playback` exposes `setGain`/`setPan` only; there is no `rate` in
`PlayOptions` and no `setRate`. A clip plays at exactly the rate it was
loaded at, forever. Anything that needs a continuous pitch sweep - an engine
note driven by revs, a doppler pass, pitch-varied footsteps or UI one-shots -
is inexpressible. The workaround is the pre-pitch-shifting console technique:
bake a bank of pitch-spaced seamless loops and equal-power crossfade the two
neighbours, which costs megabytes of resident PCM, quantises pitch to the
bank spacing, and beats audibly during every crossfade.

The runtime already resamples accurately at arbitrary rates (`loadPcm`
honours any declared sample rate, integer or not); the resampler is just
fixed at load time instead of steerable at play time.

Done looks like: `rate?: number` in `PlayOptions` (1.0 = as loaded) and
`setRate(rate)` on `Playback`, applying live. A clamped range without
formant correction is fine.

Involves: pure plumbing. sdl3's `Track::set_frequency_ratio` wraps
`MIX_SetTrackFrequencyRatio` (SDL's resampler supports roughly 0.01-100;
clamp at our layer and document). Alloy grows `set_audio_rate` plus a spawn
parameter; the plugin marshals; flux-types and the core `Sound` wrapper
mirror it.

## 2. Master gain, then buses

`stop()` is the only global control and it is all-or-nothing. No master
gain, no groups, no ducking one category against another. Every app with a
mute toggle or music/SFX sliders keeps its own master scalar and multiplies
it into every gain write it makes, per voice per frame - and the naive
version snaps gains and clicks. Cross-module, `flux:video` plays its own
audio track, untouched by anything in `flux:audio`, so "mute the app" is not
expressible at all today.

Done looks like: `setMasterGain(gain)` first (SDL: `Mixer::set_gain`); later
named buses - `play({ bus })`, per-bus gain and stop.

Constraints, learned along the way:

- The device outlives app runs, so the between-runs close must reset master
  gain to 1.0 or a reloaded app inherits the previous one's mute.
- SDL_mixer tags are a grouping mechanism, NOT a gain layer:
  `MIX_SetTagGain` SETS each tagged track's own gain (the slot
  `MIX_SetTrackGain` writes, per its doc), so a tag-based `setBusGain` would
  clobber per-voice gains and be escaped by the next `setGain`. Per-bus
  stop/pause/fade map cleanly onto tags; per-bus GAIN over SDL_mixer means
  engine-side composition (per-voice gain shadows, bus membership, bus
  gains, `voice x bus` rewritten on every change and ramp step of either).
- The planned mixer replacement (okf/backlog/video-playback.md staging:
  symphonia decode plus own mixing on the PCM sink) would make bus gains a
  few lines and would bring video audio under the same master. Keep the JS
  surface engine-neutral and do not over-invest in SDL_mixer-specific bus
  plumbing.
- Scope decision 2026-08-20: nothing video-related is touched for now.
  Cross-module mute (video under the master) is explicitly out of this
  item's scope and lands with the mixer replacement, not before.

## 3. Gain/pan/rate ramping

`setGain`/`setPan` are immediate parameter writes. A click-free fade means
calling `setGain` every frame with app-side easing, so fade smoothness is
tied to the app's frame rate and a frame hitch is an audible step. (Whether
a bare step audibly clicks is unverified - confirming needs a machine with a
loopback/monitor capture source.)

Done looks like: optional `rampMs` on `setGain`/`setPan`/`setRate`, ramped on
the mixer thread. Likely the biggest audio-quality win per engine line.

Design decided 2026-08-20. The complete `MIX_*` API (checked against the
generated sdl3-mixer-sys 3.2.4 bindings, not just the sdl3 crate wrapper)
has no ramped parameter write: track/tag/mixer gain, stereo and frequency
ratio are all immediate, and fades exist only as fade-in play options and
fade-out on stop. The cooked-callback route (`MIX_SetTrackCookedCallback`,
unwrapped in the crate) was rejected: sample-accurate for gain but unable to
ramp rate (no resampling in a callback), real-time constraints on our code,
and SDL_mixer-specific machinery the own-mixer replacement discards.

Chosen: a control-rate ramp driver. A lazily-spawned `srt-audio-ramp` thread
steps every active ramp each 10 ms (100 Hz control rate - standard parameter
automation practice, immune to app frame hitches) by calling the SDL setters,
which SDL documents as safe from any thread; it condvar-parks while the ramp
table is empty, so an app that never ramps never wakes it. The table holds
raw track pointers (usize); every code path that drops a `Track` purges its
entries under the table lock first, so the thread never touches a freed
pointer. Raw-pointer setter wrappers live in sdl_utils.rs. Linear in
parameter space; a new set replaces an active ramp from its current value;
an immediate set cancels it. The click-critical edges stay SDL-native and
sample-perfect: `fadeInMs` play option, `stop({ fadeOutMs })` fade-out (the
track keeps playing while fading, so `ended()` flips only when done).

## 4. Voice cap

Starting a voice sweeps the whole live-track table (`sweep_finished`), so
`play()` is O(live voices) - fine at real voice counts, but there is no cap:
a runaway `play()` loop, a completely ordinary app bug, wedges the JS thread
permanently once live voices reach the thousands (reproduced at 4096 looping
voices; 1024 still played). No error, nothing in the log, SIGTERM ignored -
an unkillable window.

Done looks like: a live-voice cap that makes `play()` throw with a clear
message, turning the wedge into a stack trace. 256 is far above any real
mix (each track is a full SDL_AudioStream; real mixes run dozens).

## 5. loadPcm validation

`loadPcm` accepts Float32Array samples containing NaN/Infinity and feeds
them straight to the mixer. Non-finite samples are the signature of a
synthesis bug (a divide-by-zero in a filter) - exactly when a loud error is
wanted, and the validation policy is throw
(okf/backlog/dev-prod-validation-policy.md). Relatedly, `gain` has a floor
check (finite, >= 0) but no ceiling: `gain: 1e6` passes and one bad multiply
can blow out the whole mix.

Done looks like: reject non-finite f32 samples at `loadPcm`. Open question:
whether gain deserves a ceiling or just documented headroom expectations.
(Zero-length buffers and out-of-range sample values are considered fine:
an empty clip is a voice that ends instantly, and overdriven samples are
headroom.)

## 6. play() after unload leaks a raw id

`clip.play()` after `unload()` throws `play: unknown sound 279`. The
module's stated contract is that raw ids never leave the runtime, and the id
is useless to app code anyway. The `.d.ts` also documents `unload()` as
"Playbacks already running keep going" without saying `play()` afterwards
throws - the rest of the post-lifetime contract (no-op sets after end,
`ended()` true, safe double-unload) is documented and honoured exactly.

Done looks like: `play: clip has been unloaded`, and the contract stated in
the `.d.ts`.

## 7. Discoverability and smaller follow-ups

- Detection: `Flux.capabilities.includes("audio")` already exists and is
  typed, but the `flux:audio` `.d.ts` header does not mention it, so apps
  invent `try { await import("flux:audio") }` probing. Document the
  supported path (check the capability, then import).
- Playback position and clip duration: `ended()` is the entire query
  surface; syncing visuals to audio or chaining one-shots means polling.
  SDL exposes `Track::playback_position`/`remaining` and `Audio::duration` -
  plumbing, when a consumer shows up.
- `loop` is fixed at `play()` time; "finish this loop then stop" is not
  expressible. `Track::set_loops(0)` is exactly that - a `setLoop` would be
  plumbing.
- Output rate: an app synthesizing PCM has to guess the device rate (44100).
  `Mixer::format()` has the real spec; expose the output sample rate so
  synthesis can avoid resampling.

Deliberate non-goals, for the record: no completion callback (`ended()`
polling is the contract; the module has no audio-thread pump - the C-level
`MIX_SetTrackStoppedCallback` exists but fires on the audio thread, see
okf/done/flux-audio-voice-control.md), and the ~3 dB step between omitted
`pan` and `pan: 0` is documented equal-power behavior.

## Stages

1. DONE 2026-08-20 (uncommitted). Everything SDL already provides plus the
   guards: `rate` + `setRate`, `setMasterGain` (reset on reload), the voice
   cap, non-finite PCM rejection, the unloaded-clip error, detection +
   contract docs. flux-types and core `Sound` mirrored.
2. DONE 2026-08-20 (uncommitted). Ramps via the control-rate driver above:
   `{ rampMs }` on `setGain`/`setPan`/`setRate`/`setMasterGain` (options bag,
   per the optionals-in-a-bag rule), `{ fadeOutMs }` on `playback.stop` and
   the module `stop`, `fadeInMs` in `PlayOptions`. SDL semantics verified by
   alloy/examples/audio_rate_probe.rs (fade-out keeps playing until done at
   the expected time; fade-in leaves duration unchanged; cross-thread setter
   stepping works). Verified end-to-end through the JS layer on a release
   client with probes/audio-mix-probe.tsx (12/12: rate timing at play and
   live, fade-in/fade-out timing, ramped setters, master ramp, all
   validation errors, unload contract, 256-voice cap, global fade stop;
   stable across an app reload) and by driving examples/audio over the
   control API (taps ping at tap-height pitch, counter decays, no errors).
3. Buses, audio-only (video explicitly out of scope for now):
   a. DONE 2026-08-20 (uncommitted). Thin grouping: `play({ bus })` tags the
      voice; `stop({ bus, fadeOutMs? })` stops or fades one bus
      (MIX_StopTag; bus-stopped tracks are not destroyed, the sweep reclaims
      them, so no ramp purge is needed). Solves "stop() cannot clean up one
      subsystem". Verified end-to-end (probes/audio-mix-probe.tsx 15/15:
      scoped stop leaves other buses playing, bus fade plays through,
      empty-name rejected).
   b. Bus GAINS: contract decided and DECLARED 2026-08-20 (uncommitted) -
      `setBusGain(bus, gain, { rampMs? })`, audible level = voice x bus x
      master with each layer independent, applies to live + future voices,
      resets on reload. The surface is mixer-implementation-neutral, so it
      survives the replacement unchanged. NOT implemented: the export throws
      "not implemented yet" (validation policy: throw in dev; a silent no-op
      would pretend to work), and the .d.ts documents the interim pattern
      (fold an app-side bus gain into each voice's ramped setGain). The
      implementation lands with the own mixer, where it is three multipliers
      in the mix loop; building it over SDL_mixer instead would take ~150
      disposable lines of engine-side composition (voice-gain shadows, bus
      membership, product writes on every change and ramp step).
4. Position/duration/setLoop/output-rate as consumers appear.
