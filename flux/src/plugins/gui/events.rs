use alloy::sdl3::keyboard::{Keycode, Scancode};
use alloy::sdl3::video::Orientation;
use alloy::sdl_utils::{PowerState, SystemTheme};
use alloy::{AlloyEvent, Modifiers};
use rquickjs::{Array, Null, Object};

use crate::{emit_event, emit_sticky, ExecHandle};

/// Marshal the engine-agnostic window / keyboard / device events into the JS
/// event bus, including the sticky window facts (resize, refresh rate, theme,
/// input devices, orientation) whose latest value the bus caches for replay to
/// late subscribers. The translation is pure flux marshalling. Returns true if
/// `event` was one of them (and was queued for emit on `exec`); false for
/// events the runner still owns (pointer dispatch, frame pacing). Any
/// non-marshalling bookkeeping the runner needs (e.g. the modifier state read
/// by pointer dispatch, the pacing clock's refresh rate) stays on its side.
pub fn forward(exec: &ExecHandle, event: &AlloyEvent) -> bool {
  match event {
    AlloyEvent::WindowFocus => emit_named(exec, "windowFocus"),
    AlloyEvent::WindowBlur => emit_named(exec, "windowBlur"),
    AlloyEvent::Resize { size, safe_area, display_scale } => {
      let (size, safe_area, display_scale) = (*size, *safe_area, *display_scale);
      exec.exec(move |ctx| {
        // All four are insets: distance from the corresponding window edge, like
        // CSS env(safe-area-inset-*). safe_area is a rect in absolute coords, so
        // the far edges become (window extent - far edge).
        let sa = Object::new(ctx.clone()).expect("create safeArea");
        sa.set("top", safe_area.origin.y).expect("set top");
        sa.set("left", safe_area.origin.x).expect("set left");
        sa.set("right", size.width as f32 - (safe_area.origin.x + safe_area.size.width)).expect("set right");
        sa.set("bottom", size.height as f32 - (safe_area.origin.y + safe_area.size.height)).expect("set bottom");
        let obj = Object::new(ctx.clone()).expect("create object");
        obj.set("width", size.width).expect("set width");
        obj.set("height", size.height).expect("set height");
        obj.set("safeArea", sa).expect("set safeArea");
        obj.set("displayScale", display_scale).expect("set displayScale");
        emit_sticky(&ctx, "resize", obj);
      });
    }
    AlloyEvent::DisplayRefreshRate { hz } => {
      let hz = *hz;
      exec.exec(move |ctx| {
        let obj = Object::new(ctx.clone()).expect("create object");
        obj.set("hz", hz).expect("set hz");
        emit_sticky(&ctx, "displayRefreshRate", obj);
      });
    }
    AlloyEvent::SystemTheme { theme } => {
      let name = match theme {
        SystemTheme::Dark => "dark",
        SystemTheme::Light => "light",
        SystemTheme::Unknown => "unknown",
      };
      exec.exec(move |ctx| {
        let obj = Object::new(ctx.clone()).expect("create object");
        obj.set("theme", name).expect("set theme");
        emit_sticky(&ctx, "systemTheme", obj);
      });
    }
    AlloyEvent::InputDevices { keyboard, mouse, touch } => {
      let (keyboard, mouse, touch) = (*keyboard, *mouse, *touch);
      exec.exec(move |ctx| {
        let obj = Object::new(ctx.clone()).expect("create object");
        obj.set("keyboard", keyboard).expect("set keyboard");
        obj.set("mouse", mouse).expect("set mouse");
        obj.set("touch", touch).expect("set touch");
        emit_sticky(&ctx, "inputDevices", obj);
      });
    }
    AlloyEvent::DisplayOrientation { orientation } => {
      let name = match orientation {
        Orientation::Portrait => "portrait",
        Orientation::PortraitFlipped => "portraitFlipped",
        Orientation::Landscape => "landscape",
        Orientation::LandscapeFlipped => "landscapeFlipped",
        Orientation::Unknown => "unknown",
      };
      exec.exec(move |ctx| {
        let obj = Object::new(ctx.clone()).expect("create object");
        obj.set("orientation", name).expect("set orientation");
        emit_sticky(&ctx, "displayOrientation", obj);
      });
    }
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
    AlloyEvent::Gamepads { pads } => {
      let pads = pads.clone();
      exec.exec(move |ctx| {
        // Sticky like the other device facts: a subscriber arriving after the
        // last change still sees the current pad state. `pads` is slot-stable:
        // a pad keeps its index while connected, disconnects become null.
        let arr = Array::new(ctx.clone()).expect("create pads array");
        for (i, pad) in pads.iter().enumerate() {
          match pad {
            Some(p) => {
              let buttons = Array::new(ctx.clone()).expect("create buttons");
              for (j, b) in p.buttons.iter().enumerate() {
                buttons.set(j, *b).expect("set button");
              }
              let axes = Object::new(ctx.clone()).expect("create axes");
              for (name, value) in &p.axes {
                axes.set(*name, *value).expect("set axis");
              }
              let obj = Object::new(ctx.clone()).expect("create pad");
              obj.set("id", p.id).expect("set id");
              obj.set("name", p.name.clone()).expect("set name");
              obj.set("buttons", buttons).expect("set buttons");
              obj.set("axes", axes).expect("set axes");
              obj.set("mapped", p.mapped).expect("set mapped");
              arr.set(i, obj).expect("set pad");
            }
            None => arr.set(i, Null).expect("set pad null"),
          }
        }
        let obj = Object::new(ctx.clone()).expect("create object");
        obj.set("pads", arr).expect("set pads");
        emit_sticky(&ctx, "gamepads", obj);
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
