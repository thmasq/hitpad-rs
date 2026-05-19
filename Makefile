.PHONY: rp2040 rp2350 all run-rp2040 run-rp2350

all: rp2040 rp2350

check:
	cargo check --release --target thumbv6m-none-eabi --no-default-features --features mcu-rp2040

rp2040:
	cargo build --release --target thumbv6m-none-eabi --no-default-features --features mcu-rp2040

rp2350:
	cargo build --release --target thumbv8m.main-none-eabihf --no-default-features --features mcu-rp2350

run-rp2040:
	cargo run --release --target thumbv6m-none-eabi --no-default-features --features mcu-rp2040

run-rp2350:
	cargo run --release --target thumbv8m.main-none-eabihf --no-default-features --features mcu-rp2350
