rustup target add x86_64-unknown-linux-musl
cargo +nightly build --release --target x86_64-unknown-linux-musl

# unfortunately doesn't work because fucking openssl-sys is involved somehow
