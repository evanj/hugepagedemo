CFLAGS=-Wall -Wextra -Werror -g -std=c17

all: aligned_alloc_demo
	cargo fmt
	cargo test --all-targets
	cargo check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo verify-project
	cargo audit
	clang-format -i '-style={BasedOnStyle: Google, ColumnLimit: 100}' *.c

run_native:
	RUSTFLAGS="-C target-cpu=native" cargo run --profile=release-nativecpu
