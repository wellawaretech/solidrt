SOURCES := Cargo.toml Cargo.lock $(shell find src -type f -name '*.rs')

.PHONY: build release test clean

build: bin/flux-dev

release: bin/flux

bin/flux-dev: $(SOURCES)
	cargo build
	@mkdir -p bin
	cp target/debug/flux bin/flux-dev

bin/flux: $(SOURCES)
	cargo build --release
	@mkdir -p bin
	cp target/release/flux bin/flux

test:
	cargo test

clean:
	cargo clean
	rm -rf bin
