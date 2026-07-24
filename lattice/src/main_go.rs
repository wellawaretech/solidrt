// The solidrt-go binary is the same program as solidrt (src/main.rs); the two
// targets differ only in the required `go` feature. Cargo warns when two build
// targets share one source file, so this shim gives the go target a file of
// its own.
include!("main.rs");
