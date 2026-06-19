//! JS bindings for speech recognition: a thin marshaling layer over
//! crate::speech. `speech.start()` opens a microphone session and spawns a
//! Recognizer; the returned promise settles from `tick` once the worker
//! reports Ready (models loaded) or Error. Each tick also pumps mic samples
//! into the worker and forwards its events to the session's callbacks:
//! results as `{ text, final }`, plus speech start/end notifications.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use flux::rquickjs::promise::Promise;
use flux::rquickjs::{Ctx, Exception, FromJs, Function, JsLifetime, Object, Persistent, TypedArray, Value};

use crate::speech::{Recognizer, RecognizerConfig, RecognizerEvent, SAMPLE_RATE};
use flux::gui::AlloyContext;

fn throw_str(ctx: &Ctx<'_>, msg: &str) -> flux::rquickjs::Error {
  ctx.throw(flux::rquickjs::String::from_str(ctx.clone(), msg).expect("create error string").into())
}

struct Session {
  /// alloy microphone session feeding this recognizer.
  mic: u64,
  recognizer: Recognizer,
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
  atx: AlloyContext,
  sessions: RefCell<HashMap<u64, Session>>,
  next_id: RefCell<u64>,
  pending: RefCell<Vec<PendingStart>>,
}

#[derive(Clone, JsLifetime)]
struct SpeechPluginState(#[qjs(skip_trace)] Rc<Inner>);

pub fn init(ctx: Ctx<'_>, atx: AlloyContext) {
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
  Ok(unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) }.to_vec())
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
      Some(unsafe { std::slice::from_raw_parts(raw.ptr.as_ptr(), raw.len) }.to_vec())
    }
  };
  let wake_threshold: Option<f64> = options.get("wakeThreshold")?;
  let language: Option<String> = options.get("lang")?;
  let microphone: Option<u32> = options.get("microphone")?;
  let continuous: Option<bool> = options.get("continuous")?;
  let interim: Option<bool> = options.get("interimResults")?;

  let state = ctx.userdata::<SpeechPluginState>().expect("speech state");
  let mic = state
    .0
    .atx
    .open_microphone(microphone, SAMPLE_RATE)
    .map_err(|e| throw_str(&ctx, &format!("startRecognition: {e}")))?;
  let single = !continuous.unwrap_or(true);
  let close_on_final = single && wake_model.is_none();
  let recognizer = Recognizer::start(RecognizerConfig {
    model,
    vad_model,
    language: language.unwrap_or_else(|| "en".to_string()),
    interim: interim.unwrap_or(false),
    wake_model,
    wake_threshold: wake_threshold.unwrap_or(0.5) as f32,
    realtime: true,
    single_utterance: single,
  });

  let sid = {
    let mut next = state.0.next_id.borrow_mut();
    *next += 1;
    *next
  };
  state.0.sessions.borrow_mut().insert(
    sid,
    Session {
      mic,
      recognizer,
      close_on_final,
      callback: None,
      on_speech_start: None,
      on_speech_end: None,
      on_wake: None,
    },
  );

  let (promise, resolve, reject) = Promise::new(&ctx)?;
  state.0.pending.borrow_mut().push(PendingStart {
    session: sid,
    resolve: Persistent::save(&ctx, resolve),
    reject: Persistent::save(&ctx, reject),
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

/// Per-frame hook, called from the FrameRendered handler alongside raf::flush:
/// pump mic samples into each worker, then dispatch worker events.
pub fn tick(ctx: &Ctx<'_>) {
  let Some(state) = ctx.userdata::<SpeechPluginState>() else {
    return;
  };
  if state.0.sessions.borrow().is_empty() {
    return;
  }

  // (session, event) pairs collected first: dispatching may close sessions,
  // which needs the sessions map unborrowed.
  let mut events: Vec<(u64, RecognizerEvent)> = Vec::new();
  for (sid, session) in state.0.sessions.borrow().iter() {
    match state.0.atx.read_microphone(session.mic) {
      Ok(samples) => session.recognizer.feed(samples),
      Err(e) => log::warn!("[speech] mic read failed: {e}"),
    }
    for event in session.recognizer.take_events() {
      events.push((*sid, event));
    }
  }

  for (sid, event) in events {
    match event {
      RecognizerEvent::Ready => settle_pending(ctx, &state, sid, Ok(())),
      RecognizerEvent::Error(msg) => {
        settle_pending(ctx, &state, sid, Err(msg.clone()));
        close_session(&state, sid);
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
      RecognizerEvent::Interim(text) => dispatch_result(ctx, &state, sid, &text, false),
      RecognizerEvent::Final(text) => {
        dispatch_result(ctx, &state, sid, &text, true);
        let close = state.0.sessions.borrow().get(&sid).map_or(false, |s| s.close_on_final);
        if close {
          close_session(&state, sid);
        }
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
