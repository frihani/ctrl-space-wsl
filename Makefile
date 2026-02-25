.PHONY: build release clean install

build:
	cargo build

release:
	cargo build --release

clean:
	cargo clean

install: release
	cp target/release/ctrl-space-wsl ~/.local/bin/
