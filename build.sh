#!/bin/sh
# Build the wasm package into demo/pkg (needs: rustup target add wasm32-unknown-unknown; cargo install wasm-bindgen-cli --version 0.2.129)
set -e
cargo build --release --target wasm32-unknown-unknown
TARGET=$(cargo metadata --format-version 1 --no-deps | python3 -c "import sys,json;print(json.load(sys.stdin)['target_directory'])")
wasm-bindgen --target web --out-dir demo/pkg "$TARGET/wasm32-unknown-unknown/release/page_agent_rs.wasm"
echo "built demo/pkg — serve with: python3 -m http.server -d demo 8799"
