// The paint policy lattice sets on the window's platform state
// (apply_paint_policy): alloy holds no budget until told, and lattice
// chooses its own.
use crate::{apply_paint_policy, SNAPSHOT_RASTER_BUDGET, TEXT_ATLAS_MAX_SIDE};
use alloy::rendertree::PlatformContext;

#[test]
fn the_platform_takes_lattices_snapshot_raster_budget() {
  let platform = PlatformContext::new(Vec::new());
  assert_eq!(platform.snapshot_raster_budget(), None, "alloy chooses no budget of its own");
  apply_paint_policy(&platform);
  assert_eq!(platform.snapshot_raster_budget(), Some(SNAPSHOT_RASTER_BUDGET));
}

#[test]
fn the_platform_takes_lattices_text_atlas_side_cap() {
  let platform = PlatformContext::new(Vec::new());
  apply_paint_policy(&platform);
  assert_eq!(platform.text_atlas().side_cap(), TEXT_ATLAS_MAX_SIDE);
}
