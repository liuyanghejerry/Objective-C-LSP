#!/bin/bash
# Build script for objc-lsp - builds both Apple Silicon and Intel binaries

set -e

echo "=== Building objc-lsp for macOS (Apple Silicon + Intel) ==="

# Add Intel target if not already installed
echo "Adding x86_64-apple-darwin target..."
rustup target add x86_64-apple-darwin 2>/dev/null || true

# Build both architectures
echo "Building for Apple Silicon (aarch64)..."
cargo build --release --workspace --target aarch64-apple-darwin

echo "Building for Intel (x86_64)..."
cargo build --release --workspace --target x86_64-apple-darwin

# Create universal binary
echo "Creating universal binary..."
lipo -create \
  target/aarch64-apple-darwin/release/objc-lsp \
  target/x86_64-apple-darwin/release/objc-lsp \
  -output target/release/objc-lsp

# Copy binaries to vscode extension
echo "Copying binaries to vscode extension..."
cd editors/vscode
npm run build

echo "=== Build complete ==="
echo "Binaries are ready in editors/vscode/bin/"
ls -la bin/
