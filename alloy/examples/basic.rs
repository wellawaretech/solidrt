use impellers::{Color, DisplayList, DisplayListBuilder, ISize, Paint, Point, Rect, Size, TextureSampling};
use wgpu_test::GpuContext;

fn make_pixels(size: ISize, color: u32) -> Vec<u8> {
    let bytes = color.to_be_bytes();
    let mut pixels = vec![0u8; (size.width * size.height * 4) as usize];
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.copy_from_slice(&bytes);
    }
    pixels
}

fn draw(mut builder: DisplayListBuilder, ctx: &GpuContext) -> DisplayList {
    let size = ISize::new(256, 256);
    let src_rect = Rect::new(Point::new(0.0, 0.0), size.cast());

    const BLUE_TEX: u64 = 1;
    let entry = ctx.get_or_create_texture(BLUE_TEX, size, || make_pixels(size, 0x334D80FF));
    let dst_rect = Rect::new(Point::new(10.0, 10.0), size.cast());
    builder.draw_texture_rect(&entry.impeller, &src_rect, &dst_rect, TextureSampling::Linear, None);

    let rect = Rect::new(Point::new(100.0, 100.0), Size::new(200.0, 200.0));
    let mut paint = Paint::default();
    paint.set_color(Color::new_srgba(1.0, 0.0, 0.0, 1.0));
    builder.draw_rect(&rect, &paint);

    const GREEN_TEX: u64 = 2;
    let entry = ctx.get_or_create_texture(GREEN_TEX, size, || make_pixels(size, 0x4D8033FF));
    let dst_rect = Rect::new(Point::new(280.0, 10.0), size.cast());
    builder.draw_texture_rect(&entry.impeller, &src_rect, &dst_rect, TextureSampling::Linear, None);

    builder.build().expect("Failed to build display list")
}

fn main() {
    wgpu_test::run(
        "wgpu test",
        ISize::new(1200, 800),
        |builder, ctx| draw(builder, ctx),
        |surface, dl| {
            surface.draw_display_list(dl).expect("Failed to draw display list");
            surface.present();
        },
    );
}
