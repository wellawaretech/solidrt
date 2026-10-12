//! JS bindings for speech recognition: a thin marshaling layer over
//! crate::speech. `speech.start()` opens a microphone session, takes a tap
//! on it for the Recognizer's worker, and drains the worker's events from a
//! task on the engine's executor, so they land whether or not a frame runs:
//! the returned promise settles once the worker reports Ready (models
//! loaded) or Error, and the session's callbacks get results as `{ text,
//! final }` plus speech start/end notifications.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use flux::rquickjs::promise::Promise;
use flux::rquickjs::{Ctx, Exception, FromJs, Function, JsLifetime, Object, Persistent, TypedArray, Value};

use crate::speech::{Recognizer, RecognizerConfig, RecognizerEvent, TapSource, SAMPLE_RATE};
use alloy::microphone::MicrophoneStatus;

/// How often the wait on the microphone's permission answer polls the
/// session, ms: a frame period, cheap (one atomic read), and the answer
/// lands about when a frame would have found it.
const MICROPHONE_POLL_MS: u64 = 16;

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> flux::rquickjs::Error {
  ctx.throw(flux::rquickjs::String::from_str(ctx.clone(), msg).expect("create error string").into())
}

struct Session {
  /// alloy microphone session the recognizer's worker reads; closing it
  /// ends the worker and with it the event task.
  mic: u64,
  /// Close after the first final result (continuous=false without a wake
  /// word; with one, the worker re-arms instead and the session stays open).
  close_on_final: bool,
  /// Receives `{ transcript, isFinal }` results.
  callback: Option<Persistent<Function<'static>>>,
  on_speech_start: Option<Persistent<Function<'static>>>,
  on_speech_end: Option<Persistent<Function<'static>>>,
  on_wake: Option<Persistent<Function<'static>>>,
}

struct PendingStart {
  session: u64,
  resolve: Persistent<Function<'static>>,
  reject: Persistent<Function<'static>>,
}

struct Inner {
  atx: Arc<alloy::Context>,
  sessions: RefCell<HashMap<u64, Session>>,
  next_id: RefCell<u64>,
  pending: RefCell<Vec<PendingStart>>,
}

impl Drop for Inner {
  // Engine teardown: the app's handles died with it, so nothing else will
  // ever close its sessions' microphones (and so end their workers).
  fn drop(&mut self) {
    for (_, session) in self.sessions.get_mut().drain() {
      self.atx.close_microphone(session.mic);
    }
  }
}

#[derive(Clone, JsLifetime)]
struct SpeechPluginState(#[qjs(skip_trace)] Rc<Inner>);

pub fn init(ctx: Ctx<'_>, atx: Arc<alloy::Context>) {
  ctx
    .store_userdata(SpeechPluginState(Rc::new(Inner {
      atx,
      sessions: RefCell::new(HashMap::new()),
      next_id: RefCell::new(0),
      pending: RefCell::new(Vec::new()),
    })))
    .expect("store speech state");

  let start = Function::new(ctx.clone(), start_impl).expect("create speech.start");
  let set_callback = Function::new(ctx.clone(), set_callback_impl).expect("create speech.setResultCallback");
  let set_start = Function::new(ctx.clone(), set_speech_start_impl).expect("create speech.setSpeechStartCallback");
  let set_end = Function::new(ctx.clone(), set_speech_end_impl).expect("create speech.setSpeechEndCallback");
  let set_wake = Function::new(ctx.clone(), set_wake_impl).expect("create speech.setWakeCallback");
  let stop = Function::new(ctx.clone(), stop_impl).expect("create speech.stop");

  let speech = Object::new(ctx.clone()).expect("create speech object");
  speech.set("start", start).expect("set speech.start");
  speech.set("setResultCallback", set_callback).expect("set speech.setResultCallback");
  speech.set("setSpeechStartCallback", set_start).expect("set speech.setSpeechStartCallback");
  speech.set("setSpeechEndCallback", set_end).expect("set speech.setSpeechEndCallback");
  speech.set("setWakeCallback", set_wake).expect("set speech.setWakeCallback");
  speech.set("stop", stop).expect("set speech.stop");
  ctx.globals().set("speech", speech).expect("set speech global");
}

/// Copy a required Uint8Array option into an owned buffer (the bytes cross to
/// the recognizer's worker thread, so they cannot borrow the JS heap).
fn bytes_option(ctx: &Ctx<'_>, options: &Object<'_>, key: &str) -> flux::rquickjs::Result<Vec<u8>> {
  let value: Option<TypedArray<u8>> = options.get(key)?;
  let value = value.ok_or_else(|| throw_str(ctx, &format!("startRecognition: {key} must be a Uint8Array")))?;
  let raw = value.as_raw().ok_or_else(|| throw_str(ctx, &format!("startRecognition: {key} buffer is detached")))?;
  Ok(unsafe { std::slice::from_raw_parts(raw.as_ptr().cast::<u8>(), raw.len()) }.to_vec())
}

fn start_impl<'js>(ctx: Ctx<'js>, options: Object<'js>) -> flux::rquickjs::Result<Promise<'js>> {
  let model = bytes_option(&ctx, &options, "model")?;
  let vad_model = bytes_option(&ctx, &options, "vadModel")?;
  // wakeWord stays engine-agnostic in the API (string | string[] | bytes);
  // this engine detects with a trained classifier, so only model bytes work
  // and phrase strings are rejected rather than silently ignored.
  let wake_value: Option<Value> = options.get("wakeWord")?;
  let wake_model: Option<Vec<u8>> = match wake_value {
    None => None,
    Some(v) if v.is_undefined() || v.is_null() => None,
    Some(v) if v.is_string() || v.is_array() => {
      return Err(throw_str(
        &ctx,
        "startRecognition: this engine needs a trained wake word classifier (Uint8Array model bytes), not a phrase string",
      ));
    }
    Some(v) => {
      let arr = TypedArray::<u8>::from_js(&ctx, v)
        .map_err(|_| throw_str(&ctx, "startRecognition: wakeWord must be a Uint8Array of classifier model bytes"))?;
      let raw = arr.as_raw().ok_or_else(|| throw_str(&ctx, "startRecognition: wakeWord buffer is detached"))?;
      Some(unsafe { std::slice::from_raw_parts(raw.as_ptr().cast::<u8>(), raw.len()) }.to_vec())
    }
  };
  let wake_threshold: Option<f64> = options.get("wakeThreshold")?;
  let language: Option<String> = options.get("lang")?;
  let microphone: Option<u32> = options.get("microphone")?;
  let continuous: Option<bool> = options.get("continuous")?;
  let interim: Option<bool> = options.get("interimResults")?;

  let state = ctx.userdata::<SpeechPluginState>().expect("speech state");
  // Option validation above throws (caller bug); a microphone that cannot be
  // opened rejects the promise instead - the failure is environmental (no
  // capture device, device busy, audio subsystem init). A sync throw here
  // unwinds through the caller's reactive computation, which no .catch can
  // intercept, so createSpeechRecognition's error signal would never be set.
  let (promise, resolve, reject) = Promise::new(&ctx)?;
  let mic = match state.0.atx.open_microphone(microphone, SAMPLE_RATE) {
    Ok(mic) => mic,
    Err(e) => {
      let error = Exception::from_message(ctx.clone(), &format!("startRecognition: {e}"))?;
      reject.call::<_, ()>((error,))?;
      return Ok(promise);
    }
  };
  let tap = match state.0.atx.microphone_tap(mic) {
    Ok(tap) => tap,
    Err(e) => {
      state.0.atx.close_microphone(mic);
      let error = Exception::from_message(ctx.clone(), &format!("startRecognition: {e}"))?;
      reject.call::<_, ()>((error,))?;
      return Ok(promise);
    }
  };
  let single = !continuous.unwrap_or(true);
  let close_on_final = single && wake_model.is_none();
  let mut recognizer = Recognizer::start(
    RecognizerConfig {
      model,
      vad_model,
      language: language.unwrap_or_else(|| "en".to_string()),
      interim: interim.unwrap_or(false),
      wake_model,
      wake_threshold: wake_threshold.unwrap_or(0.5) as f32,
      realtime: true,
      single_utterance: single,
    },
    TapSource::new(tap),
  );

  let sid = {
    let mut next = state.0.next_id.borrow_mut();
    *next += 1;
    *next
  };
  state.0.sessions.borrow_mut().insert(
    sid,
    Session { mic, close_on_final, callback: None, on_speech_start: None, on_speech_end: None, on_wake: None },
  );

  state.0.pending.borrow_mut().push(PendingStart {
    session: sid,
    resolve: Persistent::save(&ctx, resolve),
    reject: Persistent::save(&ctx, reject),
  });

  // The worker's events reach JS from a task on the engine's executor, so a
  // result lands whether or not a frame runs. The task ends with the worker,
  // which ends when the session's microphone closes.
  let (task_ctx, task_state) = (ctx.clone(), (*state).clone());
  ctx.spawn(async move {
    while let Some(event) = recognizer.next().await {
      handle_event(&task_ctx, &task_state, sid, event);
    }
  });

  // The microphone's permission answer (Android's prompt; at once elsewhere)
  // arrives by polling: a task waits on it holding the engine, and a
  // denial or a device that failed to open ends the session, rejecting the
  // start if it is still pending (the worker's Ready may have beaten the
  // user's answer; then the session just ends).
  let hold = flux::hold_engine(&ctx, "microphone open");
  let (task_ctx, task_state) = (ctx.clone(), (*state).clone());
  ctx.spawn(async move {
    let status = loop {
      match task_state.0.atx.pump_microphone(mic) {
        Some(MicrophoneStatus::Pending) => {
          tokio::time::sleep(std::time::Duration::from_millis(MICROPHONE_POLL_MS)).await
        }
        other => break other,
      }
    };
    let failure = match status {
      Some(MicrophoneStatus::Denied) => Some("microphone permission denied".to_string()),
      Some(MicrophoneStatus::Failed(message)) => Some(message),
      Some(MicrophoneStatus::Ready { .. }) | Some(MicrophoneStatus::Pending) | None => None,
    };
    if let Some(message) = failure {
      settle_pending(&task_ctx, &task_state, sid, Err(message));
      close_session(&task_state, sid);
    }
    drop(hold);
  });
  Ok(promise)
}

/// Register (or replace) the JS callback receiving transcripts.
fn set_callback_impl<'js>(ctx: Ctx<'js>, session: u64, callback: Function<'js>) {
  let state = ctx.userdata::<SpeechPluginState>().expect("speech state");
  let mut sessions = state.0.sessions.borrow_mut();
  if let Some(s) = sessions.get_mut(&session) {
    s.callback = Some(Persistent::save(&ctx, callback));
  }
}

fn set_speech_start_impl<'js>(ctx: Ctx<'js>, session: u64, callback: Function<'js>) {
  let state = ctx.userdata::<SpeechPluginState>().expect("speech state");
  let mut sessions = state.0.sessions.borrow_mut();
  if let Some(s) = sessions.get_mut(&session) {
    s.on_speech_start = Some(Persistent::save(&ctx, callback));
  }
}

fn set_speech_end_impl<'js>(ctx: Ctx<'js>, session: u64, callback: Function<'js>) {
  let state = ctx.userdata::<SpeechPluginState>().expect("speech state");
  let mut sessions = state.0.sessions.borrow_mut();
  if let Some(s) = sessions.get_mut(&session) {
    s.on_speech_end = Some(Persistent::save(&ctx, callback));
  }
}

fn set_wake_impl<'js>(ctx: Ctx<'js>, session: u64, callback: Function<'js>) {
  let state = ctx.userdata::<SpeechPluginState>().expect("speech state");
  let mut sessions = state.0.sessions.borrow_mut();
  if let Some(s) = sessions.get_mut(&session) {
    s.on_wake = Some(Persistent::save(&ctx, callback));
  }
}

fn stop_impl(ctx: Ctx<'_>, session: u64) {
  let state = ctx.userdata::<SpeechPluginState>().expect("speech state");
  close_session(&state, session);
}

/// Release the mic and drop the session; dropping the Recognizer disconnects
/// the sample channel and the worker exits.
fn close_session(state: &SpeechPluginState, session: u64) {
  if let Some(s) = state.0.sessions.borrow_mut().remove(&session) {
    state.0.atx.close_microphone(s.mic);
  }
}

fn reject_with(ctx: &Ctx<'_>, reject: Persistent<Function<'static>>, msg: &str) {
  let (Ok(func), Ok(error)) = (reject.restore(ctx), Exception::from_message(ctx.clone(), msg)) else {
    return;
  };
  if let Err(e) = func.call::<_, ()>((error,)) {
    log::warn!("[speech] reject call failed: {e}");
  }
}

/// Dispatch one worker event: settle the start promise, call the session's
/// callbacks, close the session where the event says so.
fn handle_event(ctx: &Ctx<'_>, state: &SpeechPluginState, sid: u64, event: RecognizerEvent) {
  match event {
    RecognizerEvent::Ready => settle_pending(ctx, state, sid, Ok(())),
    RecognizerEvent::Error(msg) => {
      settle_pending(ctx, state, sid, Err(msg.clone()));
      close_session(state, sid);
    }
    RecognizerEvent::SpeechStart => {
      let callback = state.0.sessions.borrow().get(&sid).and_then(|s| s.on_speech_start.clone());
      notify(ctx, callback, "speech start");
    }
    RecognizerEvent::SpeechEnd => {
      let callback = state.0.sessions.borrow().get(&sid).and_then(|s| s.on_speech_end.clone());
      notify(ctx, callback, "speech end");
    }
    RecognizerEvent::Wake => {
      let callback = state.0.sessions.borrow().get(&sid).and_then(|s| s.on_wake.clone());
      notify(ctx, callback, "wake");
    }
    RecognizerEvent::Interim(text) => dispatch_result(ctx, state, sid, &text, false),
    RecognizerEvent::Final(text) => {
      dispatch_result(ctx, state, sid, &text, true);
      let close = state.0.sessions.borrow().get(&sid).map_or(false, |s| s.close_on_final);
      if close {
        close_session(state, sid);
      }
    }
  }
}

/// Call a session's `{ transcript, isFinal }` result callback, if registered.
fn dispatch_result(ctx: &Ctx<'_>, state: &SpeechPluginState, sid: u64, text: &str, is_final: bool) {
  let Some(callback) = state.0.sessions.borrow().get(&sid).and_then(|s| s.callback.clone()) else {
    return;
  };
  let call = || -> flux::rquickjs::Result<()> {
    let obj = Object::new(ctx.clone())?;
    obj.set("transcript", text)?;
    obj.set("isFinal", is_final)?;
    callback.restore(ctx)?.call::<_, ()>((obj,))
  };
  if let Err(e) = call() {
    log::warn!("[speech] result callback failed: {e}");
  }
}

/// Call an argument-less notification callback, if registered.
fn notify(ctx: &Ctx<'_>, callback: Option<Persistent<Function<'static>>>, what: &str) {
  let Some(callback) = callback else {
    return;
  };
  let call = || -> flux::rquickjs::Result<()> { callback.restore(ctx)?.call::<_, ()>(()) };
  if let Err(e) = call() {
    log::warn!("[speech] {what} callback failed: {e}");
  }
}

/// Settle the start() promise of `session`, if still pending: resolve with
/// the session handle, or reject with the error message.
fn settle_pending(ctx: &Ctx<'_>, state: &SpeechPluginState, session: u64, outcome: Result<(), String>) {
  let mut pending = state.0.pending.borrow_mut();
  let Some(pos) = pending.iter().position(|p| p.session == session) else {
    // An Error event after Ready lands here; the session just closes.
    if let Err(msg) = outcome {
      log::warn!("[speech] session {session} failed: {msg}");
    }
    return;
  };
  let entry = pending.swap_remove(pos);
  drop(pending);
  match outcome {
    Ok(()) => {
      let settle = || -> flux::rquickjs::Result<()> {
        let obj = Object::new(ctx.clone())?;
        obj.set("handle", session)?;
        entry.resolve.restore(ctx)?.call::<_, ()>((obj,))
      };
      if let Err(e) = settle() {
        log::warn!("[speech] resolve call failed: {e}");
      }
    }
    Err(msg) => reject_with(ctx, entry.reject, &format!("startRecognition: {msg}")),
  }
}
