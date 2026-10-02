#!/usr/bin/env sh
set -eu
cargo run --locked --example make_assets -- web/assets/demo
cargo build --release --target wasm32-unknown-unknown --features web --locked -p grazer
bindgen_binary=${GRAZER_WASM_BINDGEN:-wasm-bindgen}
"$bindgen_binary" --target web --out-dir web/pkg target/wasm32-unknown-unknown/release/grazer.wasm
