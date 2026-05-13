fn main() {
  let source = std::env::args().nth(1).map(|path| {
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read '{path}': {e}"))
  });
  let rt = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .expect("Failed to build Tokio runtime");
  lattice::start(&rt, source);
}
