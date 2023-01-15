CFLAGS=-Wall -Wextra -Werror -g -std=c17

all: aligned_alloc_demo
	cargo fmt
	cargo test
	cargo check
	# https://zhauniarovich.com/post/2021/2021-09-pedantic-clippy/#paranoid-clippy
	# -D clippy::restriction is way too "safe"/careful
	# -D clippy::pedantic too paranoid
	# -A clippy::option-if-let-else: I stylistically disagree with this
	cargo clippy --all-targets --all-features -- \
		-D warnings \
		-D clippy::nursery \
		-A clippy::option-if-let-else

	clang-format -i '-style={BasedOnStyle: Google, ColumnLimit: 100}' *.c

run_native:
	RUSTFLAGS="-C target-cpu=native" cargo run --profile=release-nativecpu
