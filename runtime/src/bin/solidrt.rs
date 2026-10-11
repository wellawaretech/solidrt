// The solidrt binary: the stock program, lattice's entry with no modules of
// its own. A custom runtime is this line with its program (see lattice's
// entry.rs and docs/runtime, "A custom runtime").
fn main() {
  lattice::main(solidrt::program());
}
