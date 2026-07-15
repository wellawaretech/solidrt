use crate::impellers::{Color, DrawStyle};
use crate::rendertree::kinds::svg::Built;
use crate::rendertree::{Gradient, Svg};

fn has_color(built: &Built, r: f32, g: f32, b: f32) -> bool {
  built.cmds.iter().any(|cmd| {
    let c = &cmd.paint.color;
    (c.red - r).abs() < 0.02 && (c.green - g).abs() < 0.02 && (c.blue - b).abs() < 0.02
  })
}

#[test]
fn parses_multicolor_shapes() {
  let mut svg = Svg::default();
  svg.set_src(
    r##"<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg">
         <circle cx="50" cy="50" r="40" fill="#ff0000"/>
         <rect x="10" y="10" width="30" height="30" fill="#00ff00"/>
       </svg>"##
      .to_string(),
  );
  svg.ensure_built();
  let built = svg.built.borrow();
  let built = built.as_ref().expect("svg should parse");

  assert_eq!(built.intrinsic, (100.0, 100.0));
  // Two filled shapes -> two draws, with their own colors preserved.
  assert_eq!(built.cmds.len(), 2);
  assert!(has_color(built, 1.0, 0.0, 0.0), "red shape missing");
  assert!(has_color(built, 0.0, 1.0, 0.0), "green shape missing");
}

#[test]
fn injects_current_color() {
  let mut svg = Svg::default();
  svg.set_color(Color::new_srgba(0.0, 0.0, 1.0, 1.0)); // blue
  svg.set_src(
    r##"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
         <path d="M5 12h14"/>
       </svg>"##
      .to_string(),
  );
  svg.ensure_built();
  let built = svg.built.borrow();
  let built = built.as_ref().expect("svg should parse");

  // One stroked path, colored via currentColor -> the injected blue.
  assert_eq!(built.cmds.len(), 1);
  assert_eq!(built.cmds[0].paint.draw_style, DrawStyle::Stroke);
  assert!(has_color(built, 0.0, 0.0, 1.0), "currentColor not applied");
}

#[test]
fn builds_linear_gradient() {
  let mut svg = Svg::default();
  svg.set_src(
    r##"<svg viewBox="0 0 100 100">
         <defs><linearGradient id="g">
           <stop offset="0" stop-color="#000000"/>
           <stop offset="1" stop-color="#ffffff"/>
         </linearGradient></defs>
         <rect x="0" y="0" width="100" height="100" fill="url(#g)"/>
       </svg>"##
      .to_string(),
  );
  svg.ensure_built();
  let built = svg.built.borrow();
  let built = built.as_ref().expect("svg should parse");

  assert_eq!(built.cmds.len(), 1);
  // A real linear gradient with both stops, plus the averaged mid-gray fallback.
  match &built.cmds[0].paint.gradient {
    Some(Gradient::Linear { stops, .. }) => {
      assert_eq!(stops.len(), 2);
      assert!((stops[0].color.red - 0.0).abs() < 0.02, "first stop should be black");
      assert!((stops[1].color.red - 1.0).abs() < 0.02, "second stop should be white");
    }
    other => panic!("expected a linear gradient, got {other:?}"),
  }
  assert!(has_color(built, 0.5, 0.5, 0.5), "fallback should be averaged mid-gray");
}
