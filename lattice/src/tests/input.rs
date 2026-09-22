use crate::go::{parse_input_events, Injected};
use alloy::{AlloyEvent, GamepadCommand, PointerType};
use serde_json::json;

fn parse(v: serde_json::Value) -> Result<Vec<(u64, Injected)>, String> {
  parse_input_events(Some(&v))
}

fn event(step: &(u64, Injected)) -> (u64, &AlloyEvent) {
  match &step.1 {
    Injected::Event(e) => (step.0, e),
    Injected::Gamepad(_) => panic!("expected an event step"),
  }
}

fn pad(step: &(u64, Injected)) -> (u64, &GamepadCommand) {
  match &step.1 {
    Injected::Gamepad(c) => (step.0, c),
    Injected::Event(_) => panic!("expected a gamepad step"),
  }
}

#[test]
fn pointer_tap_expands_to_down_then_up() {
  let seq = parse(json!([{ "type": "pointer", "action": "tap", "x": 40.0, "y": 60.5, "holdMs": 120 }]))
    .expect("valid tap parses");
  assert_eq!(seq.len(), 2);
  let (d0, AlloyEvent::PointerDown { pointer_type, button, x, y, .. }) = event(&seq[0]) else {
    panic!("first event must be a PointerDown");
  };
  assert_eq!(d0, 0);
  assert_eq!(*pointer_type, PointerType::Mouse);
  assert_eq!(*button, 0);
  assert_eq!((*x, *y), (40.0, 60.5));
  let (d1, AlloyEvent::PointerUp { x, y, .. }) = event(&seq[1]) else {
    panic!("second event must be a PointerUp");
  };
  assert_eq!(d1, 120);
  assert_eq!((*x, *y), (40.0, 60.5));
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
  assert_eq!(seq[0].0, 0);
  assert_eq!(seq[1].0, 16);
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
  assert!(bad(json!([{ "type": "key", "action": "down", "key": "" }])).contains("non-empty key"));
  assert!(bad(json!([{ "type": "pointer", "action": "tap", "x": 1 }])).contains("y must be"));
  assert!(bad(json!([{ "type": "pointer", "action": "down", "x": 1, "y": 1, "holdMs": 10 }])).contains("holdMs"));
  assert!(bad(json!([{ "type": "pointer", "action": "tap", "x": 1, "y": 1, "pointerType": "pen" }]))
    .contains("mouse or touch"));
  assert!(bad(json!([{ "type": "key", "action": "tap", "key": "w", "holdMs": 9000 }])).contains("0..=5000"));
  assert!(bad(json!([])).contains("not be empty"));
  assert!(parse_input_events(None).is_err());
}

#[test]
fn total_duration_is_capped() {
  // 7 x 5000 ms of delays crosses the 30 s sequence cap.
  let events: Vec<_> = (0..7).map(|_| json!({ "type": "key", "action": "tap", "key": "w", "holdMs": 5000 })).collect();
  assert!(parse(serde_json::Value::Array(events)).err().expect("must reject").contains("Sequence too long"));
}
