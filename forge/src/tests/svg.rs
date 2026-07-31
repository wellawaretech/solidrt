use crate::svg::{parse, SvgDrawStyle, SvgFillRule, SvgPaint, SvgSpread, SvgStrokeCap};

#[test]
fn solid_fill_and_size() {
  let doc = parse(
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 20"><rect width="10" height="20" fill="#ff0000"/></svg>"##,
    None,
  )
  .expect("parse");
  assert_eq!((doc.width, doc.height), (10.0, 20.0));
  assert_eq!(doc.draws.len(), 1);
  let draw = &doc.draws[0];
  assert_eq!(draw.style, SvgDrawStyle::Fill);
  assert_eq!(draw.fill_rule, Some(SvgFillRule::NonZero));
  assert!(matches!(draw.paint, SvgPaint::Solid(0xff0000ff)));
  // Rect converted to absolute path data starting at the origin.
  assert!(draw.d.starts_with("M 0 0"), "unexpected d: {}", draw.d);
}

#[test]
fn fill_and_stroke_split_into_two_draws() {
  let doc = parse(
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
      <path d="M 1 1 L 9 9" fill="#00ff00" stroke="#0000ff" stroke-width="2" stroke-linecap="round"/>
    </svg>"##,
    None,
  )
  .expect("parse");
  assert_eq!(doc.draws.len(), 2);
  assert_eq!(doc.draws[0].style, SvgDrawStyle::Fill);
  assert_eq!(doc.draws[1].style, SvgDrawStyle::Stroke);
  assert!(matches!(doc.draws[1].paint, SvgPaint::Solid(0x0000ffff)));
  assert_eq!(doc.draws[1].stroke_width, Some(2.0));
  assert_eq!(doc.draws[1].stroke_cap, Some(SvgStrokeCap::Round));
  assert_eq!(doc.draws[1].fill_rule, None);
}

#[test]
fn group_scale_bakes_geometry_and_scales_stroke() {
  let doc = parse(
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20">
      <g transform="scale(2)"><path d="M 1 1 L 9 9" fill="none" stroke="#000000" stroke-width="3"/></g>
    </svg>"##,
    None,
  )
  .expect("parse");
  assert_eq!(doc.draws.len(), 1);
  let draw = &doc.draws[0];
  // Geometry is baked through the transform...
  assert!(draw.d.starts_with("M 2 2"), "unexpected d: {}", draw.d);
  // ...and the stroke width scales with it.
  assert_eq!(draw.stroke_width, Some(6.0));
}

#[test]
fn current_color_recolors_deferring_shapes() {
  let src = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
    <rect width="10" height="10" fill="currentColor"/>
    <rect y="5" width="10" height="5" fill="#00ff00"/>
  </svg>"##;
  let doc = parse(src, Some(0x336699ff)).expect("parse");
  assert!(matches!(doc.draws[0].paint, SvgPaint::Solid(0x336699ff)));
  // Explicit fills still win.
  assert!(matches!(doc.draws[1].paint, SvgPaint::Solid(0x00ff00ff)));
}

#[test]
fn opacity_folds_into_color() {
  let doc = parse(
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
      <rect width="10" height="10" fill="#ff0000" fill-opacity="0.5"/>
    </svg>"##,
    None,
  )
  .expect("parse");
  assert!(matches!(doc.draws[0].paint, SvgPaint::Solid(0xff000080)));
}

#[test]
fn linear_gradient_comes_out_absolute() {
  let doc = parse(
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
      <defs><linearGradient id="g" x1="0" y1="0" x2="10" y2="0" gradientUnits="userSpaceOnUse">
        <stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff" stop-opacity="0.5"/>
      </linearGradient></defs>
      <rect width="10" height="10" fill="url(#g)"/>
    </svg>"##,
    None,
  )
  .expect("parse");
  let SvgPaint::Linear { x0, x1, ref stops, spread, transform, .. } = doc.draws[0].paint else {
    panic!("expected linear gradient");
  };
  assert_eq!((x0, x1), (0.0, 10.0));
  assert_eq!(spread, SvgSpread::Pad);
  assert_eq!(transform, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
  assert_eq!(stops.len(), 2);
  assert_eq!(stops[0].color, 0xff0000ff);
  assert_eq!(stops[1].color, 0x0000ff80);
}

#[test]
fn invalid_document_errors() {
  assert!(parse("not svg at all", None).is_err());
}
