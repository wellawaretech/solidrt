fn main() {
  let mut args = std::env::args().skip(1);
  let mut record = false;
  let mut fps: u32 = 60;
  let mut duration: u32 = 1;
  let mut size: (u32, u32) = (1280, 720);
  let mut source_path: Option<String> = None;
  while let Some(arg) = args.next() {
    if arg == "--record" {
      record = true;
    } else if arg == "--fps" {
      fps = args.next().expect("--fps requires a value").parse().expect("--fps value must be a positive integer");
    } else if arg == "--duration" {
      duration =
        args.next().expect("--duration requires a value").parse().expect("--duration value must be a positive integer");
    } else if arg == "--size" {
      let val = args.next().expect("--size requires a value");
      let (w, h) = val.split_once('x').expect("--size must be in WxH format, e.g. 1920x1080");
      size = (
        w.parse().expect("--size width must be a positive integer"),
        h.parse().expect("--size height must be a positive integer"),
      );
    } else {
      source_path = Some(arg);
    }
  }
  let source =
    source_path.map(|path| std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read '{path}': {e}")));
  let mode = if record {
    alloy::Mode::Record(alloy::RecordConfig { fps, frames: (duration * fps) as u64, output_prefix: "frame".to_string() })
  } else {
    alloy::Mode::Run
  };
  let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("Failed to build Tokio runtime");
  lattice::start(&rt, source, mode, size);
}
