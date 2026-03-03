#!/bin/bash
# Build script for objc-lsp - builds both Apple Silicon and Intel binaries
# Supports both VSCode (.vsix) and OpenVSX packaging

set -e

# Default values
BUILD_OVSX=false
BUILD_INTEL=false

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --ovsx)
      BUILD_OVSX=true
      shift
      ;;
    --intel)
      BUILD_INTEL=true
      shift
      ;;
    --all)
      BUILD_OVSX=true
      BUILD_INTEL=true
      shift
      ;;
    *)
      echo "Unknown option: $1"
      echo "Usage: $0 [--ovsx] [--intel] [--all]"
      exit 1
      ;;
  esac
done

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

# Install dependencies
echo "Installing dependencies..."
npm install

# Build the extension
echo "Building VSCode extension..."
npm run compile

# Package based on options
if [ "$BUILD_INTEL" = true ]; then
  echo "Packaging for Intel (x86_64)..."
  if [ "$BUILD_OVSX" = true ]; then
    npm run package:ovsx:intel
  else
    npm run package:intel
  fi
elif [ "$BUILD_OVSX" = true ]; then
  echo "Packaging for OpenVSX (universal)..."
  npm run package:ovsx
else
  echo "Packaging for VSCode (universal)..."
  npm run package
fi

echo "=== Build complete ==="
echo "Extension files are ready in editors/vscode/"
ls -la *.vsix 2>/dev/null || echo "No .vsix files found"
