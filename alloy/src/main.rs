use sdl3::event::Event;
use sdl3::video::GLProfile;

fn main() {
    let sdl_context = sdl3::init().expect("Failed to initialize SDL3");

    sdl3::hint::set("SDL_OPENGL_ES_DRIVER", "1");

    let video = sdl_context.video().expect("Failed to get video subsystem");
    let gl_attr = video.gl_attr();
    gl_attr.set_context_profile(GLProfile::GLES);
    gl_attr.set_context_version(3, 0);

    let window = video
        .window("wgpu test", 1200, 800)
        .position_centered()
        .opengl()
        .resizable()
        .high_pixel_density()
        .build()
        .expect("Failed to create window");

    let gl_context = window.gl_create_context().expect("Failed to create GL context");
    window.gl_make_current(&gl_context).expect("Failed to make GL context current");
    video.gl_set_swap_interval(sdl3::video::SwapInterval::VSync).expect("Failed to set swap interval");

    window.gl_swap_window();

    let mut event_pump = sdl_context.event_pump().expect("Failed to get event pump");

    'running: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => {
                    break 'running;
                }
                _ => {}
            }
        }
    }
}
