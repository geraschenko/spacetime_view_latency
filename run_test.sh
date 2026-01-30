#!/bin/bash
set -euxo pipefail

# Build the WASM module
cargo build -p module --target wasm32-unknown-unknown --release

# Generate Rust bindings from the WASM
spacetime generate --lang rust --out-dir cli/src/generated --bin-path target/wasm32-unknown-unknown/release/module.wasm

# Publish to SpacetimeDB (creates or updates database)
spacetime publish view-latency --bin-path target/wasm32-unknown-unknown/release/module.wasm --clear-database --yes

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Run the latency test:"
echo "  cargo run -p cli -- test --batch-size 100 --batches 10"
echo ""
echo "Run a single insert test:"
echo "  cargo run -p cli -- single"
echo ""
echo "Watch for messages:"
echo "  cargo run -p cli -- watch"
echo ""
