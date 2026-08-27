// The JSX property decoders, driven through apply_jsx the way the FFI does.
// Pins the fail-soft contract: a bad property VALUE returns Err (which the
// caller throws as a catchable JS error), never a panic, and the message
// names the property, the offending value, and where useful the accepted set.

use std::sync::mpsc::channel;

use crate::alloy_plugins::properties::apply_jsx;
use crate::alloy_plugins::value::PropValue;
use alloy::rendertree::{AnimProp, AnimValue, Damage, Element, ElementKind, TransitionEntry, TransitionSpec};

fn apply(kind: &str, name: &str, value: PropValue) -> Result<Damage, String> {
  let mut el = Element::from_kind(kind).expect("known kind");
  apply_el(&mut el, name, value)
}

// The variant keeping the element, for multi-write sequences (src then
// params). The gpu_params stub accepts every write, like a valid target.
fn apply_el(el: &mut Element, name: &str, value: PropValue) -> Result<Damage, String> {
  let (tx, _rx) = channel();
  apply_jsx(el, name, &value, &tx, &|_, _| Ok(()))
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
  // The design size is the children's layout space, not just a paint fit.
  assert_eq!(apply("view", "designSize", PropValue::List(vec![num(100.0), num(40.0)])), Ok(Damage::Layout));
  assert!(apply("view", "rotate", num(0.5)).is_ok());
  assert!(apply("rect", "radius", num(4.0)).is_ok());
  assert!(apply("text", "fontStyle", text("normal")).is_ok());
  assert!(apply("view", "pointerEvents", PropValue::Null).is_ok());
}

#[test]
fn design_size_rejects_a_degenerate_design_space() {
  // A zero, negative or non-finite extent has no fit scale; throw in dev
  // (okf/backlog/dev-prod-validation-policy.md).
  for bad in [[0.0, 40.0], [100.0, -1.0], [f64::NAN, 40.0], [f64::INFINITY, 40.0]] {
    let value = PropValue::List(vec![num(bad[0]), num(bad[1])]);
    assert!(apply("view", "designSize", value).is_err(), "{bad:?} must be rejected");
  }
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
  // silently skipped). Value errors surface before the src/target routing,
  // so they read the same with or without src - src is set here because a
  // valid params write needs a target to go to.
  let mut el = Element::from_kind("texture").expect("known kind");
  apply_el(&mut el, "src", num(7.0)).expect("src applies");
  assert!(apply_el(&mut el, "params", PropValue::Null).is_ok());
  let ok = map(&[("uTime", num(1.0)), ("uVec", PropValue::List(vec![num(1.0), num(2.0)]))]);
  assert!(apply_el(&mut el, "params", ok).is_ok());
  let bad = map(&[("uTime", text("now"))]);
  let err = apply_el(&mut el, "params", bad).unwrap_err();
  assert!(err.contains("'uTime'"), "{err}");
  let bad_list = map(&[("uVec", PropValue::List(vec![num(1.0), text("x")]))]);
  let err = apply_el(&mut el, "params", bad_list).unwrap_err();
  assert!(err.contains("'uVec'"), "{err}");
}

#[test]
fn colors_and_gradients() {
  // Colors arrive as raw CSS strings (parsed runtime-side) or as the packed
  // 0xRRGGBBAA number the parseColor binding returns; both decode.
  assert!(apply("rect", "color", text("red")).is_ok());
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
  use crate::alloy_plugins::properties::{read_jsx, ReadValue};
  let mut el = Element::from_kind("view").expect("known kind");
  apply_el(&mut el, "overflow", text("hidden")).expect("overflow applies");
  apply_el(&mut el, "designSize", PropValue::List(vec![num(100.0), num(40.0)])).expect("designSize applies");
  let props = read_jsx(&el);
  assert!(
    props.iter().any(|(n, v)| *n == "overflow" && matches!(v, ReadValue::Str(s) if s == "hidden")),
    "props must include overflow"
  );
}

#[test]
fn diverging_overflow_axes_read_back_per_axis() {
  use crate::alloy_plugins::properties::{read_jsx, ReadValue};
  let mut el = Element::from_kind("view").expect("known kind");
  apply_el(&mut el, "overflowY", text("scroll")).expect("overflowY applies");
  let props = read_jsx(&el);
  assert!(props.iter().any(|(n, v)| *n == "overflowY" && matches!(v, ReadValue::Str(s) if s == "scroll")));
  assert!(!props.iter().any(|(n, _)| *n == "overflow" || *n == "overflowX"));
}

#[test]
fn texture_params_route_to_the_gpu_channel() {
  // Params are target state: the write goes straight to the GPU channel
  // (production: Context::set_target_params) and produces NO tree damage,
  // so prop-driven shader animation keeps the present-only reuse path.
  use std::cell::RefCell;
  let mut el = Element::from_kind("texture").expect("known kind");
  apply_el(&mut el, "src", num(7.0)).expect("src applies");
  let (tx, _rx) = channel();
  let seen: RefCell<Option<(u64, usize, String)>> = RefCell::new(None);
  let damage = apply_jsx(&mut el, "params", &map(&[("uTime", num(1.5))]), &tx, &|id, params| {
    *seen.borrow_mut() = Some((id, params.len(), params[0].0.clone()));
    Ok(())
  })
  .expect("params applies");
  assert_eq!(damage, Damage::None);
  assert_eq!(*seen.borrow(), Some((7, 1, "uTime".to_string())));
}

#[test]
fn texture_params_before_src_name_the_fix() {
  let err = apply("texture", "params", map(&[("uTime", num(1.0))])).unwrap_err();
  assert!(err.contains("set src before params"), "{err}");
}

#[test]
fn texture_params_gpu_error_propagates() {
  // An unknown target/uniform errors at the write (the imperative call's
  // contract), surfacing as a catchable JS throw like every value error.
  let mut el = Element::from_kind("texture").expect("known kind");
  apply_el(&mut el, "src", num(7.0)).expect("src applies");
  let (tx, _rx) = channel();
  let err = apply_jsx(&mut el, "params", &map(&[("uX", num(1.0))]), &tx, &|_, _| Err("target 7 not found".to_string()))
    .unwrap_err();
  assert!(err.contains("not found"), "{err}");
}

// The `transition` declaration: kind inference (a curve makes a tween,
// otherwise it is a spring; bare duration = critically damped spring) and
// the decode errors.

fn entry_of(el: &Element) -> TransitionEntry {
  el.transitions.as_ref().expect("config set").props[0].1
}

fn spec_of(el: &Element) -> TransitionSpec {
  entry_of(el).spec
}

#[test]
fn transition_bare_duration_is_a_spring() {
  let mut el = Element::from_kind("d-rect").expect("known kind");
  let cfg = map(&[("x", map(&[("duration", num(300.0))]))]);
  assert_eq!(apply_el(&mut el, "transition", cfg), Ok(Damage::None));
  assert!(matches!(spec_of(&el), TransitionSpec::Spring { .. }));
}

#[test]
fn transition_bounce_is_a_spring() {
  let mut el = Element::from_kind("d-rect").expect("known kind");
  let cfg = map(&[("x", map(&[("duration", num(300.0)), ("bounce", num(0.3))]))]);
  assert_eq!(apply_el(&mut el, "transition", cfg), Ok(Damage::None));
  assert!(matches!(spec_of(&el), TransitionSpec::Spring { .. }));
}

#[test]
fn transition_curve_is_a_tween() {
  let mut el = Element::from_kind("d-rect").expect("known kind");
  let cfg = map(&[("x", map(&[("duration", num(300.0)), ("curve", text("ease-out"))]))]);
  assert_eq!(apply_el(&mut el, "transition", cfg), Ok(Damage::None));
  assert!(matches!(spec_of(&el), TransitionSpec::Tween { .. }));
}

#[test]
fn transition_curve_and_bounce_conflict() {
  let mut el = Element::from_kind("d-rect").expect("known kind");
  let cfg = map(&[("x", map(&[("duration", num(300.0)), ("curve", text("ease")), ("bounce", num(0.2))]))]);
  let err = apply_el(&mut el, "transition", cfg).unwrap_err();
  assert!(err.contains("mutually exclusive"), "{err}");
}

#[test]
fn transition_errors_name_the_problem() {
  let mut el = Element::from_kind("d-rect").expect("known kind");
  let missing = apply_el(&mut el, "transition", map(&[("x", map(&[]))])).unwrap_err();
  assert!(missing.contains("duration (ms) is required"), "{missing}");
  let unknown_key =
    apply_el(&mut el, "transition", map(&[("x", map(&[("duration", num(1.0)), ("speed", num(5.0))]))])).unwrap_err();
  assert!(unknown_key.contains("unknown key 'speed'"), "{unknown_key}");
  let bad_prop = apply_el(&mut el, "transition", map(&[("blendMode", map(&[("duration", num(1.0))]))])).unwrap_err();
  assert!(bad_prop.contains("not an animatable property"), "{bad_prop}");
  let bad_curve =
    apply_el(&mut el, "transition", map(&[("x", map(&[("duration", num(1.0)), ("curve", text("zoom"))]))]))
      .unwrap_err();
  assert!(bad_curve.contains("unknown curve"), "{bad_curve}");
  let bad_bounce =
    apply_el(&mut el, "transition", map(&[("x", map(&[("duration", num(1.0)), ("bounce", num(2.0))]))])).unwrap_err();
  assert!(bad_bounce.contains("bounce must be in (-1, 1]"), "{bad_bounce}");
}

#[test]
fn transition_shorthand_strings() {
  // A bare string is the `all` catch-all; a per-property string is that
  // entry. No curve = spring (bounce 0); a curve makes a tween; a second
  // time value is the delay.
  let mut el = Element::from_kind("d-rect").expect("known kind");
  apply_el(&mut el, "transition", text("300ms")).expect("bare duration string");
  let all = el.transitions.as_ref().expect("config set").all.expect("all set");
  assert!(matches!(all.spec, TransitionSpec::Spring { .. }));
  assert_eq!(all.delay_ms, 0.0);

  apply_el(&mut el, "transition", text("300ms ease-out 100ms")).expect("curve and delay");
  let all = el.transitions.as_ref().expect("config set").all.expect("all set");
  assert!(matches!(all.spec, TransitionSpec::Tween { .. }));
  assert_eq!(all.delay_ms, 100.0);

  apply_el(&mut el, "transition", map(&[("x", text("200ms ease-in-out"))])).expect("per-property string");
  assert!(matches!(spec_of(&el), TransitionSpec::Tween { .. }));

  let bad = apply_el(&mut el, "transition", text("fast")).unwrap_err();
  assert!(bad.contains("cannot read \"fast\""), "{bad}");
  let no_duration = apply_el(&mut el, "transition", text("ease-out")).unwrap_err();
  assert!(no_duration.contains("a duration like \"300ms\" is required"), "{no_duration}");
  let three_times = apply_el(&mut el, "transition", text("1ms 2ms 3ms")).unwrap_err();
  assert!(three_times.contains("too many time values"), "{three_times}");
}

#[test]
fn transition_delay_decodes_and_validates() {
  let mut el = Element::from_kind("d-rect").expect("known kind");
  let cfg = map(&[("x", map(&[("duration", num(300.0)), ("delay", num(150.0))]))]);
  apply_el(&mut el, "transition", cfg).expect("delay applies");
  assert_eq!(entry_of(&el).delay_ms, 150.0);
  let bad =
    apply_el(&mut el, "transition", map(&[("x", map(&[("duration", num(1.0)), ("delay", num(-5.0))]))])).unwrap_err();
  assert!(bad.contains("delay must be a non-negative number"), "{bad}");
}

#[test]
fn transition_from_decodes_per_property_only() {
  let mut el = Element::from_kind("d-rect").expect("known kind");
  let cfg = map(&[("x", map(&[("duration", num(300.0)), ("from", num(-40.0))]))]);
  apply_el(&mut el, "transition", cfg).expect("scalar from applies");
  assert!(matches!(entry_of(&el).from, Some(AnimValue::Scalar(v)) if v == -40.0));

  // The color property takes a CSS string (or a packed number).
  let cfg = map(&[("color", map(&[("duration", num(300.0)), ("from", text("tomato"))]))]);
  apply_el(&mut el, "transition", cfg).expect("color from applies");
  assert!(matches!(entry_of(&el).from, Some(AnimValue::Color(_))));

  let under_all =
    apply_el(&mut el, "transition", map(&[("all", map(&[("duration", num(1.0)), ("from", num(0.0))]))])).unwrap_err();
  assert!(under_all.contains("from is per-property"), "{under_all}");
  let bad =
    apply_el(&mut el, "transition", map(&[("x", map(&[("duration", num(1.0)), ("from", text("left"))]))])).unwrap_err();
  assert!(bad.contains("from must be a number"), "{bad}");
}

#[test]
fn transition_exit_decodes_per_property_only() {
  let mut el = Element::from_kind("d-rect").expect("known kind");
  let cfg = map(&[("y", map(&[("duration", num(500.0)), ("exit", num(640.0))]))]);
  apply_el(&mut el, "transition", cfg).expect("scalar exit applies");
  assert!(matches!(entry_of(&el).exit, Some(AnimValue::Scalar(v)) if v == 640.0));

  let cfg = map(&[("color", map(&[("duration", num(300.0)), ("exit", text("transparent"))]))]);
  apply_el(&mut el, "transition", cfg).expect("color exit applies");
  assert!(matches!(entry_of(&el).exit, Some(AnimValue::Color(_))));

  let under_all =
    apply_el(&mut el, "transition", map(&[("all", map(&[("duration", num(1.0)), ("exit", num(0.0))]))])).unwrap_err();
  assert!(under_all.contains("exit is per-property"), "{under_all}");
}

#[test]
fn transition_stagger_decodes_and_validates() {
  let mut el = Element::from_kind("view").expect("known kind");
  // A pure group declaration: stagger with no property entries.
  apply_el(&mut el, "transition", map(&[("stagger", num(60.0))])).expect("stagger applies");
  assert_eq!(el.transitions.as_ref().expect("config set").stagger_ms, Some(60.0));
  // And alongside entries.
  let cfg = map(&[("stagger", num(80.0)), ("opacity", map(&[("duration", num(200.0))]))]);
  apply_el(&mut el, "transition", cfg).expect("stagger + entries");
  let config = el.transitions.as_ref().expect("config set");
  assert_eq!(config.stagger_ms, Some(80.0));
  assert_eq!(config.props.len(), 1);

  let bad = apply_el(&mut el, "transition", map(&[("stagger", num(0.0))])).unwrap_err();
  assert!(bad.contains("stagger: must be a positive number"), "{bad}");
  let not_num = apply_el(&mut el, "transition", map(&[("stagger", text("fast"))])).unwrap_err();
  assert!(not_num.contains("stagger: must be a number"), "{not_num}");
}

#[test]
fn transition_null_clears() {
  let mut el = Element::from_kind("d-rect").expect("known kind");
  let cfg = map(&[("x", map(&[("duration", num(300.0))]))]);
  apply_el(&mut el, "transition", cfg).expect("config applies");
  assert_eq!(apply_el(&mut el, "transition", PropValue::Null), Ok(Damage::None));
  assert!(el.transitions.is_none());
}

#[test]
fn color_accepts_css_strings_and_packed_numbers() {
  assert_eq!(apply("rect", "color", text("tomato")), Ok(Damage::Paint));
  assert_eq!(apply("rect", "color", num(0xff0000ff_u32 as f64)), Ok(Damage::Paint));
  let err = apply("rect", "color", text("no-such-color")).unwrap_err();
  assert!(err.contains("Invalid color"), "{err}");
}

#[test]
fn null_resets_props_to_defaults() {
  // The reactive clearing pattern (scale={style()?.scale} flipping back to
  // undefined): null is a defined value meaning "back to the default", on
  // numeric, transform, paint and layout props alike - not a validation
  // error. See okf/done/null-resets-numeric-props.md.
  use alloy::rendertree::ElementKind;
  use taffy::prelude::*;

  // Transform props reset to unset, restoring the translation-only path.
  let mut el = Element::from_kind("view").expect("known kind");
  apply_el(&mut el, "scale", num(2.0)).expect("scale applies");
  apply_el(&mut el, "rotate", num(1.0)).expect("rotate applies");
  assert_eq!(apply_el(&mut el, "scale", PropValue::Null), Ok(Damage::Compose));
  apply_el(&mut el, "rotate", PropValue::Null).expect("null resets rotate");
  let ElementKind::View(v) = &el.kind else { panic!("view kind") };
  assert_eq!((v.scale_x, v.scale_y, v.rotate), (None, None, None));

  // Detached geometry resets to unset - w back to "fill the inherited box",
  // which no concrete number could express.
  let mut el = Element::from_kind("d-rect").expect("known kind");
  apply_el(&mut el, "w", num(40.0)).expect("w applies");
  apply_el(&mut el, "w", PropValue::Null).expect("null resets w");
  apply_el(&mut el, "radius", num(4.0)).expect("radius applies");
  apply_el(&mut el, "radius", PropValue::Null).expect("null resets radius");
  let ElementKind::Rectangle(r) = &el.kind else { panic!("rect kind") };
  assert_eq!((r.w, r.radius), (None, None));

  // Paint metrics reset to the PaintState defaults.
  let mut el = Element::from_kind("rect").expect("known kind");
  apply_el(&mut el, "strokeWidth", num(8.0)).expect("strokeWidth applies");
  apply_el(&mut el, "strokeWidth", PropValue::Null).expect("null resets strokeWidth");
  let ElementKind::Rectangle(r) = &el.kind else { panic!("rect kind") };
  assert_eq!(r.paint.stroke_width, 0.0);

  // A span's numeric override clears back to inheriting the paragraph value.
  let mut el = Element::from_kind("span").expect("known kind");
  apply_el(&mut el, "fontSize", num(12.0)).expect("fontSize applies");
  apply_el(&mut el, "fontSize", PropValue::Null).expect("null resets fontSize");
  let ElementKind::Span(s) = &el.kind else { panic!("span kind") };
  assert_eq!(s.overrides.font_size, None);

  // Layout props reset to the KIND's initial style, not taffy's: a view's
  // flexDirection goes back to column.
  let mut el = Element::from_kind("view").expect("known kind");
  apply_el(&mut el, "width", num(100.0)).expect("width applies");
  apply_el(&mut el, "flexDirection", text("row")).expect("flexDirection applies");
  assert_eq!(apply_el(&mut el, "width", PropValue::Null), Ok(Damage::Layout));
  apply_el(&mut el, "flexDirection", PropValue::Null).expect("null resets flexDirection");
  let style = el.style().expect("layout element");
  assert_eq!(style.size.width, Dimension::auto());
  assert_eq!(style.flex_direction, FlexDirection::Column);

  // Enum props reset to their kind defaults.
  let mut el = Element::from_kind("rect").expect("known kind");
  apply_el(&mut el, "drawStyle", text("stroke")).expect("drawStyle applies");
  apply_el(&mut el, "drawStyle", PropValue::Null).expect("null resets drawStyle");
  apply_el(&mut el, "color", text("red")).expect("color applies");
  apply_el(&mut el, "color", PropValue::Null).expect("null resets color");
  let ElementKind::Rectangle(r) = &el.kind else { panic!("rect kind") };
  assert_eq!(r.paint.draw_style, alloy::impellers::DrawStyle::Fill);
  assert_eq!(r.paint.color.red, 0.5);

  // A span's color null drops the paint OVERRIDE (inherit the paragraph's
  // color), rather than pinning the default paint.
  let mut el = Element::from_kind("span").expect("known kind");
  apply_el(&mut el, "color", text("red")).expect("color applies");
  apply_el(&mut el, "color", PropValue::Null).expect("null clears override");
  let ElementKind::Span(s) = &el.kind else { panic!("span kind") };
  assert!(s.overrides.paint.is_none());

  // A bad non-null value still errors; an unknown prop stays unknown on null.
  let err = apply("view", "scale", text("big")).unwrap_err();
  assert!(err.contains("scale"), "{err}");
  let err = apply("rect", "drawStyle", text("outline")).unwrap_err();
  assert!(err.contains("stroke-and-fill"), "{err}");
  let err = apply("view", "colr", PropValue::Null).unwrap_err();
  assert!(err.starts_with("Unknown property"), "{err}");
}

#[test]
fn line_points_decode_and_reject() {
  let pts = |v: &[f64]| PropValue::List(v.iter().map(|n| num(*n)).collect());
  // Content, not box geometry: allowed on the layout form too, and sizes it.
  assert_eq!(apply("d-line", "points", pts(&[0.0, 0.0, 10.0, 5.0])), Ok(Damage::Layout));
  assert_eq!(apply("line", "points", pts(&[0.0, 0.0, 10.0, 5.0])), Ok(Damage::Layout));
  assert_eq!(apply("d-line", "points", PropValue::Null), Ok(Damage::Layout));
  assert_eq!(apply("d-line", "closed", PropValue::Bool(true)), Ok(Damage::Paint));
  assert_eq!(apply("d-line", "closed", PropValue::Null), Ok(Damage::Paint));

  let odd = apply("d-line", "points", pts(&[0.0, 0.0, 10.0])).expect_err("odd count");
  assert!(odd.contains("points") && odd.contains("even"), "{odd}");
  let bad = apply("d-line", "points", PropValue::List(vec![num(0.0), text("x")])).expect_err("non-number");
  assert!(bad.contains("points[1]"), "{bad}");
  let not_list = apply("d-line", "points", num(3.0)).expect_err("not a list");
  assert!(not_list.contains("points must be an array"), "{not_list}");
  let bad_closed = apply("d-line", "closed", num(1.0)).expect_err("closed");
  assert!(bad_closed.contains("closed must be a boolean"), "{bad_closed}");
}

// A Float32Array/Float64Array is an object, not an array, to the marshaller;
// the typed-array branch turns it into the same list a number[] gives, so the
// `points` decoder sees one shape.
#[test]
fn float_typed_arrays_marshal_as_number_lists() {
  let rt = rquickjs::Runtime::new().expect("js runtime");
  let context = rquickjs::Context::full(&rt).expect("js context");
  context.with(|ctx| {
    let marshal = |src: &str| -> PropValue {
      let v: rquickjs::Value = ctx.eval(src).expect("eval");
      crate::alloy_plugins::tree::to_prop_value(&v).expect("marshal")
    };
    let nums = |v: &PropValue| -> Option<Vec<f64>> { v.as_list()?.iter().map(|x| x.as_f64()).collect() };
    assert_eq!(nums(&marshal("new Float32Array([1.5, -2, 3, 4])")), Some(vec![1.5, -2.0, 3.0, 4.0]));
    assert_eq!(nums(&marshal("new Float64Array([1.5, -2, 3, 4])")), Some(vec![1.5, -2.0, 3.0, 4.0]));
    assert_eq!(nums(&marshal("new Float32Array([1, 2, 3, 4]).subarray(1, 3)")), Some(vec![2.0, 3.0]));
    assert_eq!(nums(&marshal("new Float32Array(0)")), Some(vec![]));
    assert_eq!(apply("d-line", "points", marshal("new Float32Array([0, 0, 10, 5])")), Ok(Damage::Layout));
  });
}

// PaintState's null reset is fill; a line's is stroke, and the adapter keeps
// it that way (a component forwarding an unset drawStyle must not turn a
// line into a polygon).
#[test]
fn line_draw_style_null_resets_to_stroke() {
  use alloy::impellers::DrawStyle;
  let style = |el: &Element| el.kind.paint().expect("line has paint").draw_style;
  let mut el = Element::from_kind("d-line").expect("known kind");
  assert_eq!(style(&el), DrawStyle::Stroke);
  assert_eq!(apply_el(&mut el, "drawStyle", text("fill")), Ok(Damage::Paint));
  assert_eq!(style(&el), DrawStyle::Fill);
  assert_eq!(apply_el(&mut el, "drawStyle", PropValue::Null), Ok(Damage::Paint));
  assert_eq!(style(&el), DrawStyle::Stroke);
  let mut rect = Element::from_kind("d-rect").expect("known kind");
  assert_eq!(apply_el(&mut rect, "drawStyle", PropValue::Null), Ok(Damage::Paint));
  assert_eq!(style(&rect), DrawStyle::Fill);
}

#[test]
fn path_dash_props_apply_and_transition() {
  let mut el = Element::from_kind("d-path").expect("known kind");
  let dash = |el: &Element| match &el.kind {
    ElementKind::Path(p) => (p.on_length, p.off_length, p.dash_offset),
    _ => unreachable!(),
  };
  assert_eq!(apply_el(&mut el, "onLength", num(12.0)), Ok(Damage::Paint));
  assert_eq!(apply_el(&mut el, "offLength", num(8.0)), Ok(Damage::Paint));
  assert_eq!(apply_el(&mut el, "dashOffset", num(3.5)), Ok(Damage::Paint));
  assert_eq!(dash(&el), (Some(12.0), Some(8.0), Some(3.5)));
  assert_eq!(apply_el(&mut el, "offLength", PropValue::Null), Ok(Damage::Paint));
  assert_eq!(dash(&el), (Some(12.0), None, Some(3.5)));
  let err = apply_el(&mut el, "onLength", text("far")).unwrap_err();
  assert!(err.contains("onLength"), "{err}");
  let cfg = map(&[("dashOffset", map(&[("duration", num(1.0))]))]);
  assert_eq!(apply_el(&mut el, "transition", cfg), Ok(Damage::None));
  assert_eq!(el.transitions.as_ref().expect("config set").props[0].0, AnimProp::DashOffset);
}

#[test]
fn path_length_applies_on_both_kinds_and_must_be_positive() {
  for kind in ["d-path", "d-line"] {
    let mut el = Element::from_kind(kind).expect("known kind");
    let declared = |el: &Element| match &el.kind {
      ElementKind::Path(p) => p.path_length,
      ElementKind::Line(l) => l.path_length,
      _ => unreachable!(),
    };
    assert_eq!(apply_el(&mut el, "pathLength", num(1.0)), Ok(Damage::Paint));
    assert_eq!(declared(&el), Some(1.0));
    let err = apply_el(&mut el, "pathLength", num(0.0)).unwrap_err();
    assert!(err.contains("pathLength") && err.contains("positive"), "{err}");
    assert_eq!(declared(&el), Some(1.0), "{kind}: a rejected write changes nothing");
    assert_eq!(apply_el(&mut el, "pathLength", PropValue::Null), Ok(Damage::Paint));
    assert_eq!(declared(&el), None);
  }
}

#[test]
fn line_dash_offset_applies_and_transitions() {
  let mut el = Element::from_kind("d-line").expect("known kind");
  let offset = |el: &Element| match &el.kind {
    ElementKind::Line(l) => l.dash_offset,
    _ => unreachable!(),
  };
  assert_eq!(apply_el(&mut el, "dashOffset", num(12.5)), Ok(Damage::Paint));
  assert_eq!(offset(&el), Some(12.5));
  assert_eq!(apply_el(&mut el, "dashOffset", PropValue::Null), Ok(Damage::Paint));
  assert_eq!(offset(&el), None);
  let err = apply_el(&mut el, "dashOffset", text("far")).unwrap_err();
  assert!(err.contains("dashOffset"), "{err}");
  let cfg = map(&[("dashOffset", map(&[("duration", num(1.0))]))]);
  assert_eq!(apply_el(&mut el, "transition", cfg), Ok(Damage::None));
  assert_eq!(el.transitions.as_ref().expect("config set").props[0].0, AnimProp::DashOffset);
}
