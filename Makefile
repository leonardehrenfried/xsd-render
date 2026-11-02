install:
	cargo build --release
	sudo cp target/release/xsd-render /usr/local/bin

