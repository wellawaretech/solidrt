fn main() {
  let mut args = std::env::args().skip(1);
  let mut record = false;
  let mut source_path: Option<String> = None;
  while let Some(arg) = args.next() {
    if arg == "--record" {
      record = true;
    } else {
      source_path = Some(arg);
      break;
    }
  }
  let source = source_path.map(|path| {
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read '{path}': {e}"))
  });
  let record_config = if record {
    Some(alloy::RecordConfig {
      fps: 60,
      frames: 60,
      output_prefix: "video".to_string(),
    })
  } else {
    None
  };
  let rt = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .expect("Failed to build Tokio runtime");
  lattice::start(&rt, source, record_config);
}
