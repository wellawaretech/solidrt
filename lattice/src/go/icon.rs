// The dev client's window icon: the running app's manifest-declared SVG
// rasterized for SDL_SetWindowIcon, or the SolidRT puzzle mark when the
// launcher is showing or the app declares none. Go-only on purpose: packed
// runners get their icons from platform packaging (okf/backlog/app-icons.md,
// stage 3), so the production runtime never pulls a rasterizer.

use resvg::{tiny_skia, usvg};

const ICON_RASTER_SIZE: u32 = 128;
// The client's own mark: the gradient puzzle (the flat icon-puzzle.svg is
// the launcher's in-UI variant; websites and window icons use the gradient).
const DEFAULT_ICON_SVG: &str = include_str!("../../assets/icon-puzzle-gradient.svg");

/// Set the window icon for the app about to run: its store-installed icon,
/// falling back to the client's own mark (also when rasterization fails).
pub fn apply_app_icon(app_id: &str, cmd_tx: &std::sync::mpsc::Sender<alloy::AlloyCommand>) {
  let pixels = super::store::app_icon(app_id)
    .and_then(|svg| rasterize(&svg, ICON_RASTER_SIZE))
    .or_else(|| rasterize(DEFAULT_ICON_SVG, ICON_RASTER_SIZE));
  match pixels {
    Some((width, height, rgba)) => {
      cmd_tx.send(alloy::AlloyCommand::SetIcon { width, height, rgba }).ok();
    }
    None => log::warn!("[srt] Could not rasterize the window icon"),
  }
}

// SVG source -> straight-alpha RGBA8 at size x size, aspect-fit centered on a
// transparent canvas. None on a malformed document.
fn rasterize(svg: &str, size: u32) -> Option<(u32, u32, Vec<u8>)> {
  let tree = match usvg::Tree::from_str(svg, &usvg::Options::default()) {
    Ok(tree) => tree,
    Err(e) => {
      log::warn!("[srt] Window icon SVG parse failed: {e}");
      return None;
    }
  };
  let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
  let tree_size = tree.size();
  let scale = (size as f32 / tree_size.width()).min(size as f32 / tree_size.height());
  let tx = (size as f32 - tree_size.width() * scale) / 2.0;
  let ty = (size as f32 - tree_size.height() * scale) / 2.0;
  let transform = tiny_skia::Transform::from_scale(scale, scale).post_translate(tx, ty);
  resvg::render(&tree, transform, &mut pixmap.as_mut());
  // Pixmap pixels are premultiplied; SDL expects straight alpha.
  let rgba: Vec<u8> = pixmap
    .pixels()
    .iter()
    .flat_map(|p| {
      let c = p.demultiply();
      [c.red(), c.green(), c.blue(), c.alpha()]
    })
    .collect();
  Some((size, size, rgba))
}
