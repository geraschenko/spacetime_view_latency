#!/bin/bash
set -euxo pipefail

# Build the WASM module
cargo build -p module --target wasm32-unknown-unknown --release

# Generate Rust bindings from the WASM
spacetime generate --lang rust --out-dir roundtrip_latency_test/src/generated --bin-path target/wasm32-unknown-unknown/release/module.wasm

# Build the test binary
cargo build -p roundtrip_latency_test --release

set +x

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Test with view subscription (shows linear latency growth):"
echo "spacetime publish view-latency --bin-path target/wasm32-unknown-unknown/release/module.wasm --clear-database --yes && \ "
echo "  ./target/release/roundtrip_latency_test --subscribe-to view"
echo ""
echo "Test with table subscription (constant latency):"
echo "spacetime publish view-latency --bin-path target/wasm32-unknown-unknown/release/module.wasm --clear-database --yes && \ "
echo "  ./target/release/roundtrip_latency_test --subscribe-to table"
