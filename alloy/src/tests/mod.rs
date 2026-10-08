mod audio;
mod box_dash;
mod cadence;
mod color;
mod composite;
mod cull;
mod damage;
mod effects;
mod fonts;
mod frame;
mod frame_timestamps;
mod glyphs;
mod gpu_graph;
mod gpu_lease;
mod gpu_order;
mod gpu_validate;
mod grid;
mod hit;
mod inspect;
mod keymap;
mod layout;
mod layout_slides;
mod line;
mod link;
mod liveness;
mod paint;
mod path;
mod present;
mod release;
mod resample;
mod router;
mod spatial;
mod spatial_collide;
mod spatial_normals;
mod spatial_players;
mod spatial_transitions;
mod text_baseline;
mod text_gradient;
mod text_layout;
mod texture;
mod transitions;
mod tree;
mod view;
mod yuv;

/// The shipped Noto Sans registered as "sans": the platform a test that
/// shapes text builds, since the glyph engine resolves registered faces
/// only (a platform with no fonts shapes nothing).
pub(super) fn text_platform() -> crate::rendertree::PlatformContext {
  crate::rendertree::PlatformContext::new(vec![crate::rendertree::FontPayload {
    alias: Some("sans".to_string()),
    bytes: std::borrow::Cow::Borrowed(include_bytes!("../../assets/fonts/NotoSans.ttf")),
  }])
}
