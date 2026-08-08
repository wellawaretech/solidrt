// The JSX property decoders, driven through apply_jsx the way the FFI does.
// Pins the fail-soft contract: a bad property VALUE returns Err (which the
// caller throws as a catchable JS error), never a panic, and the message
// names the property, the offending value, and where useful the accepted set.

use std::sync::mpsc::channel;

use crate::plugins::gui::properties::apply_jsx;
use crate::plugins::gui::value::PropValue;
use alloy::rendertree::{Damage, Element};

fn apply(kind: &str, name: &str, value: PropValue) -> Result<Damage, String> {
  let mut el = Element::from_kind(kind);
  let (tx, _rx) = channel();
  apply_jsx(&mut el, name, &value, &tx)
}

fn num(n: f64) -> PropValue {
  PropValue::Number(n)
}

fn text(s: &str) -> PropValue {
  PropValue::Text(s.to_string())
}

fn map(entries: &[(&str, PropValue)]) -> PropValue {
  PropValue::Map(entries.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
}

#[test]
fn valid_values_apply() {
  assert_eq!(apply("view", "width", num(100.0)), Ok(Damage::Layout));
  assert_eq!(apply("view", "flexDirection", text("row")), Ok(Damage::Layout));
  assert!(apply("view", "rotate", num(0.5)).is_ok());
  assert!(apply("rect", "radius", num(4.0)).is_ok());
  assert!(apply("text", "fontStyle", text("normal")).is_ok());
  assert!(apply("view", "pointerEvents", PropValue::Null).is_ok());
}

#[test]
fn unknown_name_and_detached_only_keep_their_prefixes() {
  // These two prefixes are matched by core's renderer (setTreeProperty) to
  // warn-and-continue instead of rethrowing; changing them breaks that seam.
  let err = apply("view", "colr", num(0.0)).unwrap_err();
  assert!(err.starts_with("Unknown property"), "{err}");
  let err = apply("rect", "x", num(1.0)).unwrap_err();
  assert!(err.starts_with("Detached-only"), "{err}");
}

#[test]
fn unknown_enum_value_names_value_and_options() {
  let err = apply("view", "flexDirection", text("colum")).unwrap_err();
  assert!(err.contains("\"colum\""), "{err}");
  assert!(err.contains("row-reverse"), "{err}");
  let err = apply("view", "display", text("inline")).unwrap_err();
  assert!(err.contains("\"inline\""), "{err}");
  let err = apply("rect", "drawStyle", text("outline")).unwrap_err();
  assert!(err.contains("stroke-and-fill"), "{err}");
  let err = apply("rect", "blendMode", text("fancy")).unwrap_err();
  assert!(err.contains("\"fancy\""), "{err}");
  let err = apply("text", "fontStyle", text("italik")).unwrap_err();
  assert!(err.contains("italic"), "{err}");
  let err = apply("view", "position", text("fixed")).unwrap_err();
  assert!(err.contains("absolute"), "{err}");
}

#[test]
fn wrong_type_names_the_received_value() {
  let err = apply("view", "rotate", text("fast")).unwrap_err();
  assert_eq!(err, "rotate must be a number, got \"fast\"");
  let err = apply("view", "width", PropValue::Bool(true)).unwrap_err();
  assert!(err.contains("true"), "{err}");
  let err = apply("view", "flexDirection", num(1.0)).unwrap_err();
  assert!(err.contains("must be a string"), "{err}");
  let err = apply("view", "repaintBoundary", num(3.0)).unwrap_err();
  assert!(err.contains("snapshot"), "{err}");
}

#[test]
fn pct_values() {
  let pct = map(&[("__unit", text("pct")), ("v", num(50.0))]);
  assert_eq!(apply("view", "width", pct), Ok(Damage::Layout));
  let bad = map(&[("__unit", text("pct")), ("v", text("half"))]);
  let err = apply("view", "width", bad).unwrap_err();
  assert!(err.contains("pct()"), "{err}");
}

#[test]
fn radius_shapes() {
  let good = PropValue::List(vec![num(1.0), num(2.0), num(3.0), num(4.0)]);
  assert!(apply("rect", "radius", good).is_ok());
  let short = PropValue::List(vec![num(1.0), num(2.0)]);
  let err = apply("rect", "radius", short).unwrap_err();
  assert!(err.contains("4 elements"), "{err}");
  let mixed = PropValue::List(vec![num(1.0), num(2.0), num(3.0), text("x")]);
  let err = apply("rect", "radius", mixed).unwrap_err();
  assert!(err.contains("radius[3]"), "{err}");
}

#[test]
fn params_reject_non_numeric_entries() {
  // Null clears; a wrong-typed entry errors naming the key (previously it was
  // silently skipped).
  assert!(apply("texture", "params", PropValue::Null).is_ok());
  let ok = map(&[("uTime", num(1.0)), ("uVec", PropValue::List(vec![num(1.0), num(2.0)]))]);
  assert!(apply("texture", "params", ok).is_ok());
  let bad = map(&[("uTime", text("now"))]);
  let err = apply("texture", "params", bad).unwrap_err();
  assert!(err.contains("'uTime'"), "{err}");
  let bad_list = map(&[("uVec", PropValue::List(vec![num(1.0), text("x")]))]);
  let err = apply("texture", "params", bad_list).unwrap_err();
  assert!(err.contains("'uVec'"), "{err}");
}

#[test]
fn colors_and_gradients() {
  // Colors arrive pre-packed from JS; a raw string reaching the decoder is an
  // error, not black.
  let err = apply("rect", "color", text("red")).unwrap_err();
  assert!(err.contains("\"red\""), "{err}");
  assert!(apply("rect", "color", num(0xFF0000FF as u32 as f64)).is_ok());

  let no_stops =
    map(&[("__gradient", text("linear")), ("x0", num(0.0)), ("y0", num(0.0)), ("x1", num(1.0)), ("y1", num(1.0))]);
  let err = apply("rect", "color", no_stops).unwrap_err();
  assert!(err.contains("stops"), "{err}");

  let bad_kind = map(&[("__gradient", text("conic"))]);
  let err = apply("rect", "color", bad_kind).unwrap_err();
  assert!(err.contains("\"conic\""), "{err}");
}

#[test]
fn layout_string_forms() {
  assert!(apply("view", "width", text("50%")).is_ok());
  assert!(apply("view", "width", text("auto")).is_ok());
  let err = apply("view", "width", text("50px")).unwrap_err();
  assert!(err.contains("\"50px\""), "{err}");
  assert!(apply("view", "flex", text("1 0 auto")).is_ok());
  let err = apply("view", "flex", text("a b")).unwrap_err();
  assert!(err.contains("\"a\""), "{err}");
  assert!(apply("view", "gridTemplateColumns", text("1fr 2fr auto")).is_ok());
  let err = apply("view", "gridTemplateColumns", text("1fr wat")).unwrap_err();
  assert!(err.contains("\"wat\""), "{err}");
  assert!(apply("view", "aspectRatio", text("16 / 9")).is_ok());
  let err = apply("view", "aspectRatio", text("16 : 9")).unwrap_err();
  assert!(err.contains("aspectRatio"), "{err}");
}

#[test]
fn shader_object_fields_are_strict() {
  let missing_program = map(&[("params", map(&[]))]);
  let err = apply("view", "shader", missing_program).unwrap_err();
  assert!(err.contains("shader.program"), "{err}");

  let bad_outset = map(&[("program", num(1.0)), ("outset", num(-2.0))]);
  let err = apply("view", "shader", bad_outset).unwrap_err();
  assert!(err.contains("shader.outset"), "{err}");

  let bad_previous = map(&[("program", num(1.0)), ("previous", num(1.0))]);
  let err = apply("view", "shader", bad_previous).unwrap_err();
  assert!(err.contains("shader.previous"), "{err}");

  assert!(apply("view", "shader", PropValue::Null).is_ok());
  assert!(apply("view", "shader", map(&[("program", num(1.0))])).is_ok());
}

#[test]
fn overflow_reads_back_including_with_viewbox() {
  // The layout blind spot from an external report: overflow was set but the
  // props read-back omitted it, making "my bug" and "a clip bug"
  // indistinguishable. Lock the round trip, on the exact prop combination
  // the report used.
  use crate::plugins::gui::properties::{read_jsx, ReadValue};
  let mut el = Element::from_kind("view");
  let (tx, _rx) = channel();
  apply_jsx(&mut el, "overflow", &text("hidden"), &tx).expect("overflow applies");
  apply_jsx(&mut el, "viewBox", &PropValue::List(vec![num(100.0), num(40.0)]), &tx).expect("viewBox applies");
  let props = read_jsx(&el);
  assert!(
    props.iter().any(|(n, v)| *n == "overflow" && matches!(v, ReadValue::Str(s) if s == "hidden")),
    "props must include overflow"
  );
}

#[test]
fn diverging_overflow_axes_read_back_per_axis() {
  use crate::plugins::gui::properties::{read_jsx, ReadValue};
  let mut el = Element::from_kind("view");
  let (tx, _rx) = channel();
  apply_jsx(&mut el, "overflowY", &text("scroll"), &tx).expect("overflowY applies");
  let props = read_jsx(&el);
  assert!(props.iter().any(|(n, v)| *n == "overflowY" && matches!(v, ReadValue::Str(s) if s == "scroll")));
  assert!(!props.iter().any(|(n, _)| *n == "overflow" || *n == "overflowX"));
}
