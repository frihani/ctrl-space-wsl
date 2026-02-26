.PHONY: build release clean install lint

build:
	cargo build

release:
	cargo build --release

clean:
	cargo clean

lint:
	cargo fmt
	cargo clippy --all-targets --all-features -- -D warnings

install: release
	cp target/release/ctrl-space-wsl ~/.local/bin/
