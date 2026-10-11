// The player is the same program as the runtime (solidrt.rs); the two
// targets differ only in the required `go` feature. Cargo warns when two
// build targets share one source file, so this shim gives the player a
// file of its own.
include!("solidrt.rs");
