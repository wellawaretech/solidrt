SOURCES := Cargo.toml Cargo.lock $(shell find src -type f -name '*.rs')

.PHONY: build release test clean

build: bin/qjsrt-dev

release: bin/qjsrt

bin/qjsrt-dev: $(SOURCES)
	cargo build
	@mkdir -p bin
	cp target/debug/qjsrt bin/qjsrt-dev

bin/qjsrt: $(SOURCES)
	cargo build --release
	@mkdir -p bin
	cp target/release/qjsrt bin/qjsrt

test:
	cargo test

clean:
	cargo clean
	rm -rf bin
