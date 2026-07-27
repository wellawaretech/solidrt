//! Validates composite-time transforms on repaint boundaries: a boundary
//! View's own matrix is hoisted out of the cached recording/texture and
//! applied when the cache is composited, so a cache-hit frame must be
//! pixel-identical to the cache-miss frame that filled it, and a
//! transform-only write (Damage::Transform) must move the boundary WITHOUT
//! dropping its cache. Content writes must still invalidate.
//!
//! Scene (400x300 logical, scale 1.0, flex column):
//!   view A (0,0)-(100,100), Recording boundary, rotate 90deg CW about center:
//!     green rect filling it + red 20x20 marker at the top-left (wrapped in a
//!     non-boundary d-view D for the baked-opacity case), so the rotation is
//!     observable (marker ends up at the top-right).
//!   view B (0,100)-(100,200), Snapshot boundary, x=30:
//!     blue rect filling it, so the hoisted translate is observable.
//!   view C (0,200)-(100,300), Recording boundary, overflow hidden:
//!     yellow rect over magenta rect (200 of content in a 100 box), so a
//!     scroll write (Damage::Scroll) is observable without re-recording.
//!
//! Run: cargo run -p alloy --example boundary_transform

use alloy::impellers::{Color, DisplayListBuilder, ISize};
use alloy::rendertree::composite::{paint_phase, PaintStats};
use alloy::rendertree::{BoundaryMode, Damage, ElementKind, PlatformContext, Rectangle, RenderTree, View, Window};
use taffy::prelude::{length, Size, Style};
use taffy::style::Overflow;

const W: u32 = 400;
const H: u32 = 300;

const ROOT: u64 = 1;
const VIEW_A: u64 = 2;
const GREEN: u64 = 3;
const MARKER: u64 = 4;
const VIEW_B: u64 = 5;
const BLUE: u64 = 6;
const VIEW_C: u64 = 7;
const YELLOW: u64 = 8;
const MAGENTA: u64 = 9;
const VIEW_D: u64 = 10;

// A property write as the runtime performs it: mutate through element_write,
// then hand the setter's reported damage to apply_damage.
fn write_view(tree: &mut RenderTree, id: u64, f: impl FnOnce(&mut View) -> Damage) {
  let damage = match &mut tree.element_write(id).kind {
    ElementKind::View(v) => f(v),
    _ => panic!("node {id} is not a view"),
  };
  tree.apply_damage(id, damage);
}

fn write_color(tree: &mut RenderTree, id: u64, color: Color) {
  let damage = tree.element_write(id).kind.paint_mut().expect("paintable kind").set_color(color);
  tree.apply_damage(id, damage);
}

fn sized(style: &mut Style, w: f32, h: f32) {
  style.size = Size { width: length(w), height: length(h) };
}

fn build_scene() -> RenderTree {
  let mut tree = RenderTree::new();
  tree.create_node(ROOT, Window::default().with_layout());
  tree.root = Some(ROOT);

  tree.create_node(VIEW_A, View::default().with_layout());
  tree.create_node(GREEN, Rectangle::default().with_layout());
  tree.create_node(VIEW_D, View::default().no_layout());
  tree.create_node(MARKER, Rectangle::default().no_layout());
  tree.create_node(VIEW_B, View::default().with_layout());
  tree.create_node(BLUE, Rectangle::default().with_layout());
  tree.create_node(VIEW_C, View::default().with_layout());
  tree.create_node(YELLOW, Rectangle::default().with_layout());
  tree.create_node(MAGENTA, Rectangle::default().with_layout());

  tree.insert_node(ROOT, VIEW_A, None);
  tree.insert_node(VIEW_A, GREEN, None);
  tree.insert_node(VIEW_A, VIEW_D, None);
  tree.insert_node(VIEW_D, MARKER, None);
  tree.insert_node(ROOT, VIEW_B, None);
  tree.insert_node(VIEW_B, BLUE, None);
  tree.insert_node(ROOT, VIEW_C, None);
  tree.insert_node(VIEW_C, YELLOW, None);
  tree.insert_node(VIEW_C, MAGENTA, None);

  sized(tree.element_write(VIEW_A).style_mut().expect("layout"), 100.0, 100.0);
  sized(tree.element_write(GREEN).style_mut().expect("layout"), 100.0, 100.0);
  sized(tree.element_write(VIEW_B).style_mut().expect("layout"), 100.0, 100.0);
  sized(tree.element_write(BLUE).style_mut().expect("layout"), 100.0, 100.0);
  {
    let style = tree.element_write(VIEW_C).style_mut().expect("layout");
    sized(style, 100.0, 100.0);
    style.overflow.x = Overflow::Hidden;
    style.overflow.y = Overflow::Hidden;
  }
  // 200 of stacked content in C's 100 box; flex_shrink 0 keeps the children
  // at full size so they overflow (and scroll) instead of shrinking to fit.
  for id in [YELLOW, MAGENTA] {
    let style = tree.element_write(id).style_mut().expect("layout");
    sized(style, 100.0, 100.0);
    style.flex_shrink = 0.0;
  }

  write_color(&mut tree, GREEN, Color::new_srgba(0.0, 1.0, 0.0, 1.0));
  write_color(&mut tree, BLUE, Color::new_srgba(0.0, 0.0, 1.0, 1.0));
  write_color(&mut tree, YELLOW, Color::new_srgba(1.0, 1.0, 0.0, 1.0));
  write_color(&mut tree, MAGENTA, Color::new_srgba(1.0, 0.0, 1.0, 1.0));
  {
    let marker = tree.element_write(MARKER);
    match &mut marker.kind {
      ElementKind::Rectangle(r) => {
        r.set_w(20.0);
        r.set_h(20.0);
      }
      _ => panic!("marker is not a rect"),
    }
  }
  write_color(&mut tree, MARKER, Color::new_srgba(1.0, 0.0, 0.0, 1.0));

  write_view(&mut tree, VIEW_A, |v| v.set_rotate(std::f32::consts::FRAC_PI_2));
  write_view(&mut tree, VIEW_B, |v| v.set_x(30.0));

  tree.element_write(VIEW_A).repaint_boundary = BoundaryMode::Recording;
  tree.element_write(VIEW_B).repaint_boundary = BoundaryMode::Snapshot;
  tree.element_write(VIEW_C).repaint_boundary = BoundaryMode::Recording;
  tree
}

fn frame(tree: &mut RenderTree, platform: &PlatformContext, ctx: &alloy::Context) -> (Vec<u8>, PaintStats) {
  let mut builder = DisplayListBuilder::new(None);
  let stats = paint_phase(&mut builder, tree, platform, ctx);
  let dl = builder.build().expect("frame display list");
  let texture = ctx.render_display_list_to_texture(&dl, W, H, true).expect("frame rasterization");
  let pixels = ctx.read_texture(&texture, W, H).expect("texture readback");
  (pixels, stats)
}

fn expect_color(pixels: &[u8], x: u32, y: u32, expected: (u8, u8, u8, u8), what: &str) {
  let i = ((y * W + x) * 4) as usize;
  let (r, g, b, a) = (pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]);
  let close = |a: u8, e: u8| (a as i32 - e as i32).abs() <= 2;
  assert!(
    close(r, expected.0) && close(g, expected.1) && close(b, expected.2) && close(a, expected.3),
    "{what} at ({x},{y}): got ({r},{g},{b},{a}), expected {expected:?}"
  );
  println!("ok: {what} at ({x},{y}) = ({r},{g},{b},{a})");
}

const GREEN_PX: (u8, u8, u8, u8) = (0, 255, 0, 255);
const RED_PX: (u8, u8, u8, u8) = (255, 0, 0, 255);
const BLUE_PX: (u8, u8, u8, u8) = (0, 0, 255, 255);
const WHITE_PX: (u8, u8, u8, u8) = (255, 255, 255, 255);
const YELLOW_PX: (u8, u8, u8, u8) = (255, 255, 0, 255);
const MAGENTA_PX: (u8, u8, u8, u8) = (255, 0, 255, 255);
const EMPTY_PX: (u8, u8, u8, u8) = (0, 0, 0, 0);

fn main() {
  let app = alloy::setup("boundary transform", ISize::new(600, 400), alloy::Mode::Run);
  app.run(|ctx, _cmd_tx, _event_rx| {
    let platform = PlatformContext::new(Vec::new());
    platform.set_window_size(W as f32, H as f32);
    platform.set_display_scale(1.0);
    let mut tree = build_scene();

    // Frame 1: cache miss, all three boundaries filled.
    let (f1, s1) = frame(&mut tree, &platform, &ctx);
    assert_eq!((s1.boundaries_recorded, s1.snapshots_rasterized), (2, 1), "frame 1 fills all caches");
    // 90deg CW about (50,50) sends the top-left marker to the top-right.
    expect_color(&f1, 90, 10, RED_PX, "rotated marker (top-right)");
    expect_color(&f1, 10, 10, GREEN_PX, "marker's old spot is green");
    expect_color(&f1, 50, 50, GREEN_PX, "view A center");
    // x=30 shifts view B's blue box to (30,100)-(130,200).
    expect_color(&f1, 80, 150, BLUE_PX, "translated snapshot content");
    expect_color(&f1, 10, 150, EMPTY_PX, "left of translated snapshot is empty");
    // C unscrolled: the yellow first child fills the box, magenta is below.
    expect_color(&f1, 50, 250, YELLOW_PX, "unscrolled C shows yellow");

    // Frame 2: cache hit, must be pixel-identical to the miss frame.
    let (f2, s2) = frame(&mut tree, &platform, &ctx);
    assert_eq!((s2.boundaries_reused, s2.snapshots_reused), (2, 1), "frame 2 reuses all caches");
    assert_eq!((s2.boundaries_recorded, s2.snapshots_rasterized), (0, 0), "frame 2 records nothing");
    assert_eq!(f1, f2, "cache-hit frame differs from cache-miss frame");
    println!("ok: cache-hit frame is pixel-identical to the cache-miss frame");

    // Frame 3: un-rotate view A. Damage::Transform keeps A's recording; the
    // cached content is drawn with the new matrix, no re-record.
    write_view(&mut tree, VIEW_A, |v| v.set_rotate(0.0));
    let (f3, s3) = frame(&mut tree, &platform, &ctx);
    assert_eq!((s3.boundaries_reused, s3.snapshots_reused), (2, 1), "frame 3 reuses all caches");
    assert_eq!(s3.boundaries_recorded, 0, "transform write must not re-record");
    expect_color(&f3, 10, 10, RED_PX, "marker back at the top-left");
    expect_color(&f3, 90, 10, GREEN_PX, "top-right green again");

    // Frame 4: move view B back to x=0. Damage::Transform keeps B's snapshot;
    // the same texture is composited at the new position, no re-rasterize.
    write_view(&mut tree, VIEW_B, |v| v.set_x(0.0));
    let (f4, s4) = frame(&mut tree, &platform, &ctx);
    assert_eq!((s4.snapshots_reused, s4.snapshots_rasterized), (1, 0), "frame 4 reuses B's texture");
    expect_color(&f4, 10, 150, BLUE_PX, "snapshot content back at x=0");
    expect_color(&f4, 110, 150, EMPTY_PX, "right of moved snapshot is empty");

    // Frame 5: a content write (marker color) must still invalidate A's
    // recording; siblings B and C stay cached.
    write_color(&mut tree, MARKER, Color::new_srgba(1.0, 1.0, 1.0, 1.0));
    let (f5, s5) = frame(&mut tree, &platform, &ctx);
    assert_eq!(
      (s5.boundaries_recorded, s5.boundaries_reused, s5.snapshots_reused),
      (1, 1, 1),
      "frame 5 re-records A, reuses C and B"
    );
    expect_color(&f5, 10, 10, WHITE_PX, "marker repainted white");

    // Frame 6: scroll view C by a full page. Damage::Scroll on a Recording
    // boundary keeps its cache (clip and scroll are applied at composite
    // time): magenta scrolls into view with zero re-records, and the content
    // sliding up must not leak above C's clip box (view B's rows stay blue).
    write_view(&mut tree, VIEW_C, |v| v.set_scroll_y(100.0));
    let (f6, s6) = frame(&mut tree, &platform, &ctx);
    assert_eq!(s6.boundaries_recorded, 0, "scroll write must not re-record");
    assert_eq!((s6.boundaries_reused, s6.snapshots_reused), (2, 1), "frame 6 reuses all caches");
    expect_color(&f6, 50, 250, MAGENTA_PX, "scrolled C shows magenta");
    expect_color(&f6, 50, 150, BLUE_PX, "scrolled-up yellow is clipped, B intact");

    // Frame 7: scroll view B. A Snapshot texture has no scrolled-out pixels,
    // so Damage::Scroll must re-rasterize it (correctness over reuse): blue
    // shifts up 20 inside B's box, the vacated bottom strip is empty.
    write_view(&mut tree, VIEW_B, |v| v.set_scroll_y(20.0));
    let (f7, s7) = frame(&mut tree, &platform, &ctx);
    assert_eq!((s7.snapshots_rasterized, s7.snapshots_reused), (1, 0), "scroll write must re-rasterize a snapshot");
    assert_eq!(s7.boundaries_reused, 2, "frame 7 reuses A and C");
    expect_color(&f7, 10, 150, BLUE_PX, "scrolled snapshot content");
    expect_color(&f7, 10, 190, EMPTY_PX, "vacated strip below scrolled content is empty");

    // Frame 8: opacity 0.5 on non-boundary view D (inside A). Baked as a
    // save_layer into A's recording, so A re-records; the white marker fades
    // to half over the green underneath (group composited, then blended).
    write_view(&mut tree, VIEW_D, |v| v.set_opacity(0.5));
    let (f8, s8) = frame(&mut tree, &platform, &ctx);
    assert_eq!((s8.boundaries_recorded, s8.boundaries_reused), (1, 1), "baked opacity re-records A, reuses C");
    expect_color(&f8, 10, 10, (128, 255, 128, 255), "half-faded marker over green");

    // Frame 9: opacity 0.5 on Recording boundary A. Hoisted like the matrix:
    // the cached recording replays through draw_display_list's opacity arg,
    // zero re-records. The marker pixel proves nested group opacity: the
    // inner 0.5 layer composites first, then A fades as a whole.
    write_view(&mut tree, VIEW_A, |v| v.set_opacity(0.5));
    let (f9, s9) = frame(&mut tree, &platform, &ctx);
    assert_eq!((s9.boundaries_recorded, s9.boundaries_reused), (0, 2), "opacity write must not re-record");
    expect_color(&f9, 50, 50, (0, 128, 0, 128), "view A faded to half");
    expect_color(&f9, 10, 10, (64, 128, 64, 128), "nested group opacity on the marker");

    // Frame 10: opacity 0.5 on Snapshot boundary B. The texture is reused;
    // the fade rides on the composited quad's paint.
    write_view(&mut tree, VIEW_B, |v| v.set_opacity(0.5));
    let (f10, s10) = frame(&mut tree, &platform, &ctx);
    assert_eq!((s10.snapshots_reused, s10.snapshots_rasterized), (1, 0), "opacity write reuses B's texture");
    expect_color(&f10, 10, 150, (0, 0, 128, 128), "snapshot faded to half");

    image::save_buffer("/tmp/boundary_transform.png", &f6, W, H, image::ColorType::Rgba8).expect("save png");
    println!("saved /tmp/boundary_transform.png");
    println!("all boundary transform checks passed");
  });
}
