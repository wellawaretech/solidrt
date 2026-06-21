use alloy::sdl3::keyboard::{Keycode, Scancode};
use alloy::sdl_utils::PowerState;
use alloy::{AlloyEvent, Modifiers};
use rquickjs::{Null, Object};

use crate::{emit_event, ExecHandle};

/// Marshal the engine-agnostic window / keyboard / device events into the JS
/// event bus. These carry no runner policy (no pacing, no sticky replay, no
/// hit-testing), so the translation is pure flux marshalling. Returns true if
/// `event` was one of them (and was queued for emit on `exec`); false for events
/// the runner still owns (pointer dispatch, frame pacing, the sticky resize /
/// refreshRate). Any non-marshalling bookkeeping the runner needs (e.g. the
/// modifier state read by pointer dispatch) stays on its side.
pub fn forward(exec: &ExecHandle, event: &AlloyEvent) -> bool {
  match event {
    AlloyEvent::WindowFocus => emit_named(exec, "windowFocus"),
    AlloyEvent::WindowBlur => emit_named(exec, "windowBlur"),
    AlloyEvent::KeyDown { keycode, scancode, modifiers } => emit_key(exec, "keydown", *keycode, *scancode, *modifiers),
    AlloyEvent::KeyUp { keycode, scancode, modifiers } => emit_key(exec, "keyup", *keycode, *scancode, *modifiers),
    AlloyEvent::TextInput { text } => {
      let text = text.clone();
      exec.exec(move |ctx| {
        let obj = Object::new(ctx.clone()).expect("create object");
        obj.set("text", text).expect("set text");
        emit_event(&ctx, "textInput", obj);
      });
    }
    AlloyEvent::KeyboardVisibility { shown, height } => {
      let (shown, height) = (*shown, *height);
      exec.exec(move |ctx| {
        let obj = Object::new(ctx.clone()).expect("create object");
        obj.set("shown", shown).expect("set shown");
        obj.set("height", height).expect("set height");
        emit_event(&ctx, "keyboardVisibility", obj);
      });
    }
    AlloyEvent::CameraDeviceChange { added } => {
      let added = *added;
      exec.exec(move |ctx| {
        let obj = Object::new(ctx.clone()).expect("create object");
        obj.set("added", added).expect("set added");
        emit_event(&ctx, "cameraDeviceChange", obj);
      });
    }
    AlloyEvent::PowerStatus { info } => {
      let state = match info.state {
        PowerState::OnBattery => "onBattery",
        PowerState::Charging => "charging",
        PowerState::Charged => "charged",
        PowerState::NoBattery => "noBattery",
        PowerState::Unknown => "unknown",
      };
      let percent = info.percent;
      exec.exec(move |ctx| {
        let obj = Object::new(ctx.clone()).expect("create object");
        obj.set("state", state).expect("set state");
        match percent {
          Some(p) => obj.set("percent", p).expect("set percent"),
          None => obj.set("percent", Null).expect("set percent null"),
        }
        emit_event(&ctx, "powerStatus", obj);
      });
    }
    _ => return false,
  }
  true
}

fn emit_named(exec: &ExecHandle, name: &'static str) {
  exec.exec(move |ctx| {
    let obj = Object::new(ctx.clone()).expect("create object");
    emit_event(&ctx, name, obj);
  });
}

fn emit_key(
  exec: &ExecHandle,
  name: &'static str,
  keycode: Option<Keycode>,
  scancode: Option<Scancode>,
  modifiers: Modifiers,
) {
  let key = keycode.map(|k| k.name()).unwrap_or_default();
  let code = scancode.map(|s| s.name().to_string()).unwrap_or_default();
  exec.exec(move |ctx| {
    let obj = Object::new(ctx.clone()).expect("create object");
    obj.set("key", key).expect("set key");
    obj.set("code", code).expect("set code");
    obj.set("shiftKey", modifiers.shift).expect("set shiftKey");
    obj.set("ctrlKey", modifiers.ctrl).expect("set ctrlKey");
    obj.set("altKey", modifiers.alt).expect("set altKey");
    obj.set("metaKey", modifiers.meta).expect("set metaKey");
    emit_event(&ctx, name, obj);
  });
}
