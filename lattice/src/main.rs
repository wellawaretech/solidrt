// The solidrt binary: the stock runtime, lattice's entry with no modules of
// its own. A custom runtime is this line with its modules (see entry.rs).
fn main() {
  lattice::main(lattice::Modules::new());
}
