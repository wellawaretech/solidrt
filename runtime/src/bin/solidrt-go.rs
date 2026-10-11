// The solidrt-go binary is the same program as solidrt (solidrt.rs); the two
// targets differ only in the required `go` feature. Cargo warns when two build
// targets share one source file, so this shim gives the go target a file of
// its own.
include!("solidrt.rs");
