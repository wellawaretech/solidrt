use crate::go::parse_input_events;
use alloy::{AlloyEvent, PointerType};
use serde_json::json;

fn parse(v: serde_json::Value) -> Result<Vec<(u64, AlloyEvent)>, String> {
  parse_input_events(Some(&v))
}

#[test]
fn pointer_tap_expands_to_down_then_up() {
  let seq = parse(json!([{ "type": "pointer", "action": "tap", "x": 40.0, "y": 60.5, "holdMs": 120 }]))
    .expect("valid tap parses");
  assert_eq!(seq.len(), 2);
  let (d0, AlloyEvent::PointerDown { pointer_type, button, x, y, .. }) = &seq[0] else {
    panic!("first event must be a PointerDown");
  };
  assert_eq!(*d0, 0);
  assert_eq!(*pointer_type, PointerType::Mouse);
  assert_eq!(*button, 0);
  assert_eq!((*x, *y), (40.0, 60.5));
  let (d1, AlloyEvent::PointerUp { x, y, .. }) = &seq[1] else {
    panic!("second event must be a PointerUp");
  };
  assert_eq!(*d1, 120);
  assert_eq!((*x, *y), (40.0, 60.5));
}

#[test]
fn key_tap_carries_code_and_modifiers() {
  let seq = parse(json!([{ "type": "key", "action": "tap", "key": "w", "holdMs": 500, "shift": true }]))
    .expect("valid key tap parses");
  assert_eq!(seq.len(), 2);
  let (_, AlloyEvent::Key { down, key, code, modifiers, repeat }) = &seq[0] else {
    panic!("first event must be a Key");
  };
  assert!(*down);
  assert_eq!(key, "w");
  assert_eq!(*code, "KeyW");
  assert!(modifiers.shift && !modifiers.ctrl);
  assert!(!*repeat);
  let (d1, AlloyEvent::Key { down, .. }) = &seq[1] else {
    panic!("second event must be a Key");
  };
  assert_eq!(*d1, 500);
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
  let (_, AlloyEvent::PointerMove { pointer_type, .. }) = &seq[1] else {
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
  let (_, AlloyEvent::Wheel { delta_y, .. }) = &seq[0] else {
    panic!("first event must be a Wheel");
  };
  assert_eq!(*delta_y, -300.0);
  let (_, AlloyEvent::TextInput { text }) = &seq[1] else {
    panic!("second event must be a TextInput");
  };
  assert_eq!(text, "hello");
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
