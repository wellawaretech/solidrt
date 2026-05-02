#[derive(Debug, Clone, Copy)]
pub enum Backend {
    Gl,
    Vulkan,
    // Metal,
}

use impellers::{Context, DisplayListBuilder};
use crate::gpu::{GpuTexture, gl};

pub fn texture_to_display_list(
    gpu_texture: &GpuTexture,
    impeller_ctx: &Context,
    builder: &mut DisplayListBuilder,
    width: u32,
    height: u32,
) {
    match gpu_texture.backend {
        Backend::Gl => gl::texture_to_display_list(gpu_texture, impeller_ctx, builder, width, height),
        Backend::Vulkan => {
            // TODO: Implement Vulkan GPU copy path
            panic!("Vulkan backend not yet implemented");
        }
    }
}
