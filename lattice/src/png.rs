// PNG files for the headless hosts: the frames `srt render` writes and the
// snapshot a failed app test leaves behind. Pixels come as alloy's readbacks
// hand them out (RGBA8, rows top to bottom), so nothing is reordered here.

use std::path::Path;

/// Write `pixels` (RGBA8, `width` x `height`, rows top to bottom) as a PNG.
pub(crate) fn write(path: &Path, width: u32, height: u32, pixels: &[u8]) -> Result<(), String> {
  let expected = width as usize * height as usize * 4;
  if pixels.len() != expected {
    return Err(format!("{} bytes of pixels for a {width}x{height} image, expected {expected}", pixels.len()));
  }
  if let Some(dir) = path.parent() {
    std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
  }
  image::save_buffer(path, pixels, width, height, image::ColorType::Rgba8)
    .map_err(|e| format!("could not write {}: {e}", path.display()))
}
