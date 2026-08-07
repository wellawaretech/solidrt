use crate::keymap::w3c_code_for_key;

#[test]
fn letters_map_case_insensitively() {
  assert_eq!(w3c_code_for_key("w"), "KeyW");
  assert_eq!(w3c_code_for_key("W"), "KeyW");
  assert_eq!(w3c_code_for_key("a"), "KeyA");
  assert_eq!(w3c_code_for_key("z"), "KeyZ");
}

#[test]
fn digits_space_and_punctuation() {
  assert_eq!(w3c_code_for_key("0"), "Digit0");
  assert_eq!(w3c_code_for_key("9"), "Digit9");
  assert_eq!(w3c_code_for_key(" "), "Space");
  assert_eq!(w3c_code_for_key(","), "Comma");
  assert_eq!(w3c_code_for_key("/"), "Slash");
  assert_eq!(w3c_code_for_key("'"), "Quote");
}

#[test]
fn named_keys_pass_through() {
  assert_eq!(w3c_code_for_key("Enter"), "Enter");
  assert_eq!(w3c_code_for_key("ArrowLeft"), "ArrowLeft");
  assert_eq!(w3c_code_for_key("F12"), "F12");
  assert_eq!(w3c_code_for_key("PageDown"), "PageDown");
}

#[test]
fn modifiers_synthesize_the_left_position() {
  assert_eq!(w3c_code_for_key("Shift"), "ShiftLeft");
  assert_eq!(w3c_code_for_key("Control"), "ControlLeft");
  assert_eq!(w3c_code_for_key("Meta"), "MetaLeft");
}

#[test]
fn positionless_keys_are_unidentified() {
  // Shifted punctuation has no unshifted position of its own.
  assert_eq!(w3c_code_for_key("!"), "Unidentified");
  assert_eq!(w3c_code_for_key("\u{00fc}"), "Unidentified");
  assert_eq!(w3c_code_for_key("NoSuchKey"), "Unidentified");
  assert_eq!(w3c_code_for_key(""), "Unidentified");
}
