#!/bin/bash
# Quick test script to run the keyboard middleware with debug logging

echo "Building keyboard-middleware..."
cargo build || exit 1

echo ""
echo "Running with debug logging..."
echo "Press Ctrl+C to stop"
echo ""

newgrp input <<EOF
RUST_LOG=info ./target/debug/keyboard-middleware
EOF
