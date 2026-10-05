// The input plan (input_plan.rs): the `/input` event shape expanded into
// the events that are sent and what passes between them.

use crate::input_plan::{plan, Injected, Step, Wait};
use alloy::{AlloyEvent, GamepadCommand, PointerType};
use serde_json::json;

// The frame interval the plans here are made for.
const FRAME_MS: f64 = 1000.0 / 60.0;

fn parse(v: serde_json::Value) -> Result<Vec<Step>, String> {
  plan(Some(&v), FRAME_MS)
}

/// A step's event with the milliseconds that pass before it.
fn event(step: &Step) -> (u64, &AlloyEvent) {
  match &step.inject {
    Injected::Event(e) => (step.wait.ms, e),
    Injected::Gamepad(_) => panic!("expected an event step"),
  }
}

fn pad(step: &Step) -> (u64, &GamepadCommand) {
  match &step.inject {
    Injected::Gamepad(c) => (step.wait.ms, c),
    Injected::Event(_) => panic!("expected a gamepad step"),
  }
}

// A mouse is where it presses before it presses: the tap starts with a
// move to the point, and the down follows a frame later.
#[test]
fn a_mouse_tap_is_a_move_a_down_a_frame_later_and_an_up() {
  let seq = parse(json!([{ "type": "pointer", "action": "tap", "x": 40.0, "y": 60.5, "holdMs": 120, "delayMs": 30 }]))
    .expect("valid tap parses");
  assert_eq!(seq.len(), 3);
  let (_, AlloyEvent::PointerMove { x, y, .. }) = event(&seq[0]) else {
    panic!("first event must be a PointerMove");
  };
  assert_eq!((*x, *y), (40.0, 60.5));
  assert_eq!(seq[0].wait, Wait { ms: 30, frames: 0 });
  let (_, AlloyEvent::PointerDown { pointer_type, button, x, y, .. }) = event(&seq[1]) else {
    panic!("second event must be a PointerDown");
  };
  assert_eq!(seq[1].wait, Wait { ms: 0, frames: 1 });
  assert_eq!(*pointer_type, PointerType::Mouse);
  assert_eq!(*button, 0);
  assert_eq!((*x, *y), (40.0, 60.5));
  let (_, AlloyEvent::PointerUp { x, y, .. }) = event(&seq[2]) else {
    panic!("third event must be a PointerUp");
  };
  assert_eq!(seq[2].wait, Wait { ms: 120, frames: 1 });
  assert_eq!((*x, *y), (40.0, 60.5));
}

// A finger is nowhere before it touches: no move. The up still comes a
// frame after the down, whatever the hold.
#[test]
fn a_touch_tap_is_a_down_and_an_up_a_frame_later() {
  let seq = parse(json!([{ "type": "pointer", "action": "tap", "x": 1, "y": 2, "pointerType": "touch" }]))
    .expect("valid tap parses");
  assert_eq!(seq.len(), 2);
  assert!(matches!(event(&seq[0]).1, AlloyEvent::PointerDown { pointer_type: PointerType::Touch, .. }));
  assert_eq!(seq[0].wait, Wait { ms: 0, frames: 0 });
  assert!(matches!(event(&seq[1]).1, AlloyEvent::PointerUp { .. }));
  assert_eq!(seq[1].wait, Wait { ms: 0, frames: 1 });
}

// One move per frame along the line, the last at the end point, then the
// up there.
#[test]
fn a_drag_is_a_down_a_move_per_frame_and_an_up() {
  let seq = parse(json!([{
    "type": "pointer", "action": "drag", "x": 10, "y": 20, "to": { "x": 70, "y": 20 },
    "durationMs": 50, "pointerType": "touch"
  }]))
  .expect("valid drag parses");
  // 50 ms at 60 fps is 3 frames.
  assert_eq!(seq.len(), 5);
  assert!(matches!(event(&seq[0]).1, AlloyEvent::PointerDown { .. }));
  let xs: Vec<f32> = seq[1..4]
    .iter()
    .map(|step| match event(step).1 {
      AlloyEvent::PointerMove { x, .. } => *x,
      _ => panic!("expected a PointerMove"),
    })
    .collect();
  assert_eq!(xs, vec![30.0, 50.0, 70.0]);
  assert!(seq[1..].iter().all(|step| step.wait == Wait { ms: 0, frames: 1 }));
  let (_, AlloyEvent::PointerUp { x, y, .. }) = event(&seq[4]) else {
    panic!("last event must be a PointerUp");
  };
  assert_eq!((*x, *y), (70.0, 20.0));
  // A mouse drag starts with the move to its start point.
  let mouse =
    parse(json!([{ "type": "pointer", "action": "drag", "x": 0, "y": 0, "to": { "x": 9, "y": 0 }, "durationMs": 16 }]))
      .expect("valid drag parses");
  assert!(matches!(event(&mouse[0]).1, AlloyEvent::PointerMove { .. }));
  assert!(matches!(event(&mouse[1]).1, AlloyEvent::PointerDown { .. }));
  assert_eq!(mouse.len(), 4);
  let bad = parse(json!([{ "type": "pointer", "action": "drag", "x": 0, "y": 0 }])).err().expect("must reject");
  assert!(bad.contains("to: { x, y }"), "got {bad}");
}

#[test]
fn key_tap_carries_code_and_modifiers() {
  let seq = parse(json!([{ "type": "key", "action": "tap", "key": "w", "holdMs": 500, "shift": true }]))
    .expect("valid key tap parses");
  assert_eq!(seq.len(), 2);
  let (_, AlloyEvent::Key { down, key, code, modifiers, repeat }) = event(&seq[0]) else {
    panic!("first event must be a Key");
  };
  assert!(*down);
  assert_eq!(key, "w");
  assert_eq!(*code, "KeyW");
  assert!(modifiers.shift && !modifiers.ctrl);
  assert!(!*repeat);
  let (d1, AlloyEvent::Key { down, .. }) = event(&seq[1]) else {
    panic!("second event must be a Key");
  };
  assert_eq!(d1, 500);
  assert!(!*down);
}

#[test]
fn delays_and_touch_pass_through() {
  let seq = parse(json!([
    { "type": "pointer", "action": "down", "x": 0, "y": 0, "pointerType": "touch" },
    { "type": "pointer", "action": "move", "x": 10, "y": 0, "pointerType": "touch", "delayMs": 16 },
    { "type": "pointer", "action": "up", "x": 10, "y": 0, "pointerType": "touch", "delayMs": 16 },
  ]))
  .expect("drag parses");
  assert_eq!(seq.len(), 3);
  assert_eq!(seq[0].wait.ms, 0);
  assert_eq!(seq[1].wait.ms, 16);
  let (_, AlloyEvent::PointerMove { pointer_type, .. }) = event(&seq[1]) else {
    panic!("second event must be a PointerMove");
  };
  assert_eq!(*pointer_type, PointerType::Touch);
}

#[test]
fn wheel_and_text_events() {
  let seq = parse(json!([
    { "type": "wheel", "x": 100, "y": 200, "deltaX": 0, "deltaY": -300 },
    { "type": "text", "text": "hello" },
  ]))
  .expect("wheel and text parse");
  let (_, AlloyEvent::Wheel { delta_y, .. }) = event(&seq[0]) else {
    panic!("first event must be a Wheel");
  };
  assert_eq!(*delta_y, -300.0);
  let (_, AlloyEvent::TextInput { text }) = event(&seq[1]) else {
    panic!("second event must be a TextInput");
  };
  assert_eq!(text, "hello");
}

#[test]
fn gamepad_session_parses_to_commands() {
  let seq = parse(json!([
    { "type": "gamepad", "action": "connect" },
    { "type": "gamepad", "action": "connect", "slot": 3, "name": "Pad B", "delayMs": 10 },
    { "type": "gamepad", "action": "set", "slot": 0, "buttons": ["south", "dpadUp", "south"], "axes": { "leftX": -0.5, "rightTrigger": 1 } },
    { "type": "gamepad", "action": "disconnect", "slot": 3 },
  ]))
  .expect("a pad session parses");
  assert_eq!(seq.len(), 4);
  assert_eq!(pad(&seq[0]), (0, &GamepadCommand::Connect { slot: None, name: "Synthetic gamepad".into() }));
  assert_eq!(pad(&seq[1]), (10, &GamepadCommand::Connect { slot: Some(3), name: "Pad B".into() }));
  // Names come back as the static vocabulary, deduplicated.
  let (_, GamepadCommand::Set { slot, buttons, axes }) = pad(&seq[2]) else {
    panic!("third step must be a Set");
  };
  assert_eq!(*slot, 0);
  assert_eq!(buttons, &["south", "dpadUp"]);
  assert_eq!(axes.len(), 2);
  assert!(axes.contains(&("leftX", -0.5)));
  assert!(axes.contains(&("rightTrigger", 1.0)));
  assert_eq!(pad(&seq[3]), (0, &GamepadCommand::Disconnect { slot: 3 }));
}

#[test]
fn gamepad_set_with_hold_returns_to_neutral() {
  let seq = parse(json!([{ "type": "gamepad", "action": "set", "slot": 1, "buttons": ["start"], "holdMs": 250 }]))
    .expect("a held set parses");
  assert_eq!(seq.len(), 2);
  assert_eq!(pad(&seq[0]), (0, &GamepadCommand::Set { slot: 1, buttons: vec!["start"], axes: vec![] }));
  assert_eq!(pad(&seq[1]), (250, &GamepadCommand::Set { slot: 1, buttons: vec![], axes: vec![] }));
}

#[test]
fn gamepad_events_are_validated() {
  let bad = |v: serde_json::Value| parse(v).err().expect("must reject");
  assert!(bad(json!([{ "type": "gamepad", "action": "press", "slot": 0 }])).contains("connect, set or disconnect"));
  assert!(bad(json!([{ "type": "gamepad", "action": "set" }])).contains("slot must be"));
  assert!(bad(json!([{ "type": "gamepad", "action": "connect", "slot": 99 }])).contains("slot must be"));
  assert!(bad(json!([{ "type": "gamepad", "action": "set", "slot": 0, "buttons": ["a"] }]))
    .contains("unknown gamepad button"));
  assert!(
    bad(json!([{ "type": "gamepad", "action": "set", "slot": 0, "buttons": "south" }])).contains("buttons must be")
  );
  assert!(bad(json!([{ "type": "gamepad", "action": "set", "slot": 0, "axes": { "leftZ": 0 } }]))
    .contains("unknown gamepad axis"));
  assert!(bad(json!([{ "type": "gamepad", "action": "set", "slot": 0, "axes": { "leftX": 2 } }])).contains("in -1..1"));
  assert!(bad(json!([{ "type": "gamepad", "action": "connect", "holdMs": 10 }])).contains("holdMs"));
}

#[test]
fn invalid_events_reject_the_whole_sequence() {
  // Whole-batch validation: the valid first event must not soften the error.
  let bad = |v: serde_json::Value| parse(v).err().expect("must reject");
  assert!(bad(json!([{ "type": "key", "action": "tap", "key": "w" }, { "type": "warp" }])).contains("events[1]"));
  assert!(bad(json!([{ "type": "key", "action": "press", "key": "w" }])).contains("down, up or tap"));
  assert!(
    bad(json!([{ "type": "pointer", "action": "press", "x": 1, "y": 1 }])).contains("down, up, cancel, move, tap or drag")
  );
  assert!(bad(json!([{ "type": "key", "action": "down", "key": "" }])).contains("non-empty key"));
  assert!(bad(json!([{ "type": "pointer", "action": "tap", "x": 1 }])).contains("y must be"));
  assert!(bad(json!([{ "type": "pointer", "action": "down", "x": 1, "y": 1, "holdMs": 10 }])).contains("holdMs"));
  assert!(bad(json!([{ "type": "pointer", "action": "tap", "x": 1, "y": 1, "pointerType": "pen" }]))
    .contains("mouse or touch"));
  assert!(bad(json!([{ "type": "key", "action": "tap", "key": "w", "holdMs": 9000 }])).contains("0..=5000"));
  assert!(bad(json!([])).contains("not be empty"));
  assert!(plan(None, FRAME_MS).is_err());
}

#[test]
fn total_duration_is_capped() {
  // 7 x 5000 ms of delays crosses the 30 s sequence cap.
  let events: Vec<_> = (0..7).map(|_| json!({ "type": "key", "action": "tap", "key": "w", "holdMs": 5000 })).collect();
  assert!(parse(serde_json::Value::Array(events)).err().expect("must reject").contains("Sequence too long"));
}

// A cancel is the one way to end a synthetic press without a lift: the
// system taking the pointer away, as a platform touch cancel does.
#[test]
fn a_cancel_ends_a_synthetic_press_without_a_lift() {
  let seq = parse(json!([
    { "type": "pointer", "action": "down", "x": 10.0, "y": 20.0, "pointerType": "touch" },
    { "type": "pointer", "action": "cancel", "x": 30.0, "y": 40.0, "pointerType": "touch", "delayMs": 50 }
  ]))
  .expect("valid cancel parses");
  assert_eq!(seq.len(), 2);
  assert!(matches!(event(&seq[0]).1, AlloyEvent::PointerDown { .. }));
  let (ms, AlloyEvent::PointerCancel { pointer_type, x, y, .. }) = event(&seq[1]) else {
    panic!("second event must be a PointerCancel");
  };
  assert_eq!(ms, 50);
  assert_eq!(*pointer_type, PointerType::Touch);
  assert_eq!((*x, *y), (30.0, 40.0));
}
