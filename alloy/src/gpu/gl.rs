use impellers::{Context, DisplayListBuilder, Rect, Point, Size, TextureSampling, Paint};
use crate::gpu::GpuTexture;

/// Extract the GL texture name from a wgpu texture (GL backend only).
fn wgpu_texture_gl_handle(texture: &wgpu::Texture) -> u32 {
    let hal_texture = unsafe { texture.as_hal::<wgpu::hal::gles::Api>() }
        .expect("not a GL-backed wgpu texture");
    match hal_texture.inner {
        wgpu::hal::gles::TextureInner::Texture { raw, .. } => raw.0.get() as u32,
        _ => panic!("wgpu texture is not a GL texture"),
    }
}

/// Adopt a wGPU GL texture into Impeller's display list (zero-copy).
pub fn texture_to_display_list(
    gpu_texture: &GpuTexture,
    impeller_ctx: &Context,
    builder: &mut DisplayListBuilder,
    width: u32,
    height: u32,
) {
    let gl_handle = wgpu_texture_gl_handle(&gpu_texture.wgpu_texture);

    // Adopt the GL texture into Impeller (zero-copy)
    // Format code 1 = BGRA8888 (or RGBA8888 on little-endian systems)
    let impeller_texture = unsafe {
        impeller_ctx.adopt_opengl_texture(width, height, 1, gl_handle as u64)
    };

    if let Some(tex) = impeller_texture {
        let src_rect = Rect::new(Point::new(0.0, 0.0), Size::new(width as f32, height as f32));
        let dst_rect = Rect::new(Point::new(10.0, 10.0), Size::new(256.0, 256.0));
        builder.draw_texture_rect(&tex, &src_rect, &dst_rect, TextureSampling::Linear, Some(&Paint::default()));
    }
}
