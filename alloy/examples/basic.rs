use impellers::{Color, DisplayList, DisplayListBuilder, ISize, Paint, Point, Rect, Size, TextureSampling};
use std::time::Duration;
use alloy::Context;

fn make_pixels(size: ISize, color: u32) -> Vec<u8> {
    let bytes = color.to_be_bytes();
    let mut pixels = vec![0u8; (size.width * size.height * 4) as usize];
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.copy_from_slice(&bytes);
    }
    pixels
}

fn draw(mut builder: DisplayListBuilder, ctx: &Context, t: f32) -> DisplayList {
    let size = ISize::new(256, 256);
    let src_rect = Rect::new(Point::new(0.0, 0.0), size.cast());

    const BLUE_TEX: u64 = 1;
    let tex = ctx.get_or_create_texture(BLUE_TEX, size, || make_pixels(size, 0x334D80FF));
    let dst_rect = Rect::new(Point::new(10.0, 10.0), size.cast());
    builder.draw_texture_rect(&tex, &src_rect, &dst_rect, TextureSampling::Linear, None);

    let rect = Rect::new(Point::new(200.0, 100.0), Size::new(200.0, 200.0));
    let mut paint = Paint::default();
    paint.set_color(Color::new_srgba(1.0, 0.0, 0.0, 1.0));
    builder.draw_rect(&rect, &paint);

    const GREEN_TEX: u64 = 2;
    let alpha = ((t.sin() * 0.5 + 0.5) * 255.0) as u8;
    let tex = ctx.get_or_update_texture(GREEN_TEX, size, || make_pixels(size, 0x4D8033_00 | alpha as u32));
    let dst_rect = Rect::new(Point::new(280.0, 10.0), size.cast());
    builder.draw_texture_rect(&tex, &src_rect, &dst_rect, TextureSampling::Linear, None);

    builder.build().expect("Failed to build display list")
}

fn main() {
    alloy::setup("Alloy demo", ISize::new(1200, 800)).run(
        |ctx| {
            let mut t = 0.0f32;
            loop {
                let builder = DisplayListBuilder::new(None);
                let dl = draw(builder, ctx, t);
                if ctx.submit(dl).is_err() {
                    break;
                }

                t += 0.05;
                std::thread::sleep(Duration::from_millis(16));
            }
        },
        |display, dl| {
            display.draw_display_list(dl).expect("Failed to draw display list");
            display.present();
        },
    );
}
