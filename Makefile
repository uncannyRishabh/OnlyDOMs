.PHONY: all init install build test run clean

all: build

init:
	@./setup.sh --install

install:
	@./setup.sh --install

build:
	cd mcp-server && cargo build --release

test:
	cd mcp-server && cargo test

run:
	cd mcp-server && cargo run --release -- --port 9222

clean:
	cd mcp-server && cargo clean
