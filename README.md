# Objective-C LSP

> The first Language Server Protocol implementation designed specifically for Objective-C.

Existing tools (clangd, ccls, sourcekit-lsp) treat Objective-C as an afterthought — a by-product of C/C++/Swift support. **objc-lsp** is built from the ground up to understand Objective-C semantics: selectors, categories, `@interface`/`@implementation` duality, blocks, and `@property` coordination.

## Why Not clangd?

I found current solutions are not really handy when working with Objective-C.

| Problem | clangd Issue | objc-lsp |
|---------|-------------|----------|
| `.h` files misidentified as C, not ObjC | [#621](https://github.com/clangd/clangd/issues/621) (open since 2020) | ✅ Content heuristic detects `@interface`/`@implementation` |
| Incomplete selector completion (`[tableView cellForRowAtIndexPath:]`) | [#656](https://github.com/clangd/clangd/issues/656) (open since 2020) | ✅ Multi-part selector completion with full fill-in |
| `@property` rename doesn't coordinate getter/setter/ivar | [llvm#81775](https://github.com/llvm/llvm-project/issues/81775) (open since 2024) | ✅ Coordinated rename across getter, setter, backing ivar, and dot syntax |
| sourcekit-lsp just spawns clangd for ObjC | — | ✅ Native ObjC implementation, no delegation |
| Requires `compile_commands.json` | — | ✅ Parses `.xcodeproj` directly, works out of the box |

Furthermore, VS Code extensions provide additional user-friendly features that go beyond what LSP can offer—features that clangd alone cannot achieve.

## Features

### LSP Protocol (34 features across 5 phases)

**Core (Phase 1)** — `.h` language detection, document symbols, diagnostics, hover, goto definition/declaration, semantic tokens, project loading

**ObjC-Specific (Phase 2)** — Multi-part selector completion, `@property` coordinated rename, protocol stub generation, find references, protocol implementation finder, inlay hints, category aggregation

**Advanced (Phase 3)** — `clang --analyze` integration, nullability checks, code actions, Apple SDK documentation in hover, workspace symbol search, GNUstep support, cross-file selector rename

**Editor Enhancement (Phase 5)** — Code formatting (clang-format), code folding, call hierarchy, type hierarchy

### VS Code Extension

Beyond standard LSP features, the VS Code extension provides:

- **26 ObjC Snippets** — `prop`, `singleton`, `weakSelf`, `block`, `protocol`, and more
- **6 Quick Fix Commands** — Generate `@property`, wrap in `@autoreleasepool`, fix retain cycles, etc.
- **4 Code Lens Types** — Call counts, protocol conformance, override status, deprecation warnings
- **5 Text Decorators** — Retain cycle warnings, thread safety, magic numbers, unused code, strong delegate detection
- **Symbols Outline Pro** — Tree view grouped by `#pragma mark`, protocol, and category
- **Class Browser** — Project-wide class hierarchy tree
- **Call Graph Webview** — Interactive method call relationship visualization
- **Hover Extensions** — API availability parsing, deprecation warnings, related methods with clickable navigation, quick fix links

### Zed Extension

The Zed extension brings objc-lsp to the [Zed editor](https://zed.dev) with:

- **Syntax Highlighting** — Tree-sitter based highlighting for Objective-C (`.m`, `.h`) and Objective-C++ (`.mm`)
- **LSP Integration** — Auto-downloads the objc-lsp binary from GitHub Releases at runtime
- **Code Folding** — Collapse `@interface`/`@implementation` blocks, methods, and comment regions
- **Text Objects** — Structural selection for classes, methods, and comments
- **Bracket Matching** — Automatic bracket pair detection
- **Outline View** — Navigate class interfaces, implementations, protocols, methods, and properties
- **Grammar** — Uses community-maintained [`tree-sitter-grammars/tree-sitter-objc`](https://github.com/tree-sitter-grammars/tree-sitter-objc)

## Architecture

objc-lsp uses a **dual-layer parsing architecture**: tree-sitter for fast, fault-tolerant operations (~1ms/file) and libclang for precise semantic analysis.

```
┌─────────────────────────────────────────────┐
│       Editor (VS Code / Zed / Neovim / ...)   │
│              LSP JSON-RPC over stdio         │
└──────────────────┬──────────────────────────┘
                   │
┌──────────────────▼──────────────────────────┐
│            objc-lsp (Rust binary)             │
│                                              │
│  ┌──────────────┐  ┌──────────────────────┐  │
│  │  Fast Path    │  │  Semantic Path        │  │
│  │ (tree-sitter) │  │ (libclang/clang-sys)  │  │
│  │               │  │                      │  │
│  │ • symbols     │  │ • completions        │  │
│  │ • tokens      │  │ • hover + docs       │  │
│  │ • folding     │  │ • diagnostics        │  │
│  │ • inlay hints │  │ • goto definition    │  │
│  └──────────────┘  │ • references          │  │
│                     │ • rename             │  │
│                     │ • call/type hierarchy │  │
│                     └──────────┬───────────┘  │
│                                │              │
│  ┌─────────────────────────────▼───────────┐  │
│  │  ObjC Intelligence Layer                 │  │
│  │  • Selector database & completion        │  │
│  │  • @property → getter/setter/ivar coord  │  │
│  │  • Protocol conformance & stub generator │  │
│  │  • Category aggregation                  │  │
│  │  • .h language detector                  │  │
│  └─────────────────────────────┬───────────┘  │
│                                │              │
│  ┌─────────────────────────────▼───────────┐  │
│  │  Project Layer                           │  │
│  │  • .xcodeproj / pbxproj parser           │  │
│  │  • compile_commands.json fallback        │  │
│  │  • Apple SDK / iOS Simulator detection   │  │
│  │  • CocoaPods header discovery            │  │
│  │  • GNUstep include path detection        │  │
│  └─────────────────────────────┬───────────┘  │
│                                │              │
│  ┌─────────────────────────────▼───────────┐  │
│  │  Index Store (SQLite)                    │  │
│  │  • Symbol table                          │  │
│  │  • Cross-reference graph                 │  │
│  │  • Selector → implementations mapping    │  │
│  └─────────────────────────────────────────┘  │
└──────────────────────────────────────────────┘
```

### Crate Structure

| Crate | Role |
|-------|------|
| `objc-lsp` | Binary entry point, LSP protocol layer (`lsp-server` + `tokio`) |
| `objc-syntax` | Tree-sitter fast path: parser, symbols, tokens, inlay hints, folding |
| `objc-semantic` | Libclang semantic path: hover, completion, diagnostics, goto-def, references, rename, formatting, call/type hierarchy |
| `objc-intelligence` | ObjC-specific logic: selector engine, property coordination, protocol analysis, code actions, nullability |
| `objc-project` | Build system integration: `.xcodeproj` parser, `compile_commands.json`, SDK detection |
| `objc-store` | SQLite index: symbol table, cross-references, workspace search |

## Getting Started

### Prerequisites

- **Rust** (stable, 1.75+) — for building the LSP server
- **libclang** — provided by Xcode (macOS) or LLVM (Linux)
- **Node.js** (18+) — for building the VS Code extension
- **VS Code** (1.85+) and/or **Zed** (1.x+) — editor

On macOS with Xcode installed, libclang is available automatically. On Linux, install LLVM:

```bash
# Ubuntu/Debian
sudo apt install libclang-dev

# Fedora
sudo dnf install clang-devel
```

### Build & Install

```bash
# 1. Clone the repository
git clone https://github.com/liuyanghejerry/Objective-C-LSP.git
cd Objective-C-LSP

# 2. Build the LSP server (MUST use --release)
cargo build --release --workspace

# 3. Build and install the VS Code extension
cd editors/vscode
npm install
node esbuild.mjs
npx vsce package --no-dependencies
code --install-extension objc-lsp-0.1.0.vsix --force
```

For the Zed extension, install as a dev extension:

```
Open Zed → Extensions → Install Dev Extension → select editors/zed
```

### Configuration

After installation, set the server path in VS Code settings:

```json
{
  "objc-lsp.serverPath": "/path/to/objc-lsp/target/release/objc-lsp"
}
```

If left empty, the extension auto-discovers the binary from `$PATH` or the bundled location.

#### Available Settings

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `objc-lsp.serverPath` | `string` | `""` | Absolute path to the `objc-lsp` binary |
| `objc-lsp.logLevel` | `string` | `"info"` | Server log verbosity (`error`, `warn`, `info`, `debug`) |
| `objc-lsp.extraCompilerFlags` | `string[]` | `[]` | Extra flags passed to clang (e.g. `["-DDEBUG"]`) |
| `objc-lsp.enableNullabilityChecks` | `boolean` | `true` | Report missing nullability annotations |
| `objc-lsp.enableStaticAnalyzer` | `boolean` | `false` | Run `clang --analyze` on save |
| `objc-lsp.enableCodeLens` | `boolean` | `true` | Show Code Lens annotations above methods |
| `objc-lsp.enableDecorators` | `boolean` | `true` | Show inline decorators for retain cycles, etc. |
| `objc-lsp.enableHoverExtensions` | `boolean` | `true` | Show extended hover info |

## Development

### Project Structure

```
objective-c-lsp/
├── Cargo.toml                    # Workspace definition
├── crates/
│   ├── objc-lsp/                 # LSP server binary
│   ├── objc-syntax/              # Tree-sitter fast path
│   ├── objc-semantic/            # Libclang semantic analysis
│   ├── objc-intelligence/        # ObjC-specific logic
│   ├── objc-project/             # Build system integration
│   └── objc-store/               # SQLite index
├── editors/
│   ├── vscode/                   # VS Code extension (TypeScript)
│   │   ├── src/                  # Extension source
│   │   ├── syntaxes/             # TextMate grammar
│   │   └── snippets/             # ObjC code snippets
│   └── zed/                      # Zed extension (Rust → WASM)
│       ├── src/lib.rs            # Extension entry point
│       ├── extension.toml        # Extension manifest + grammar config
│       └── languages/            # Tree-sitter query files (.scm)
│           ├── objective-c/      # Highlights, folds, outline, etc.
│           └── objective-cpp/    # Symlinks to objective-c/
└── tests/
    └── fixtures/                 # Test ObjC projects
```

### Running Tests

```bash
# Run all 133 tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p objc-syntax
cargo test -p objc-intelligence
cargo test -p objc-semantic
```

### Rebuild After Changes

**Important**: The VS Code extension uses the **release** binary. Always build with `--release`:

```bash
# Rust-only changes: rebuild server + reload VS Code window
cargo build --release --workspace

# TypeScript changes: rebuild extension + reinstall
cd editors/vscode
node esbuild.mjs
npx vsce package --no-dependencies
code --install-extension objc-lsp-0.1.0.vsix --force
```

For the Zed extension:

```
# Zed extension: install as dev extension
Open Zed → Extensions → Install Dev Extension → select editors/zed
```

### Key Dependencies

| Dependency | Version | Purpose |
|------------|---------|---------|
| `lsp-server` | 0.7 | LSP framework (by rust-analyzer team) |
| `lsp-types` | 0.97 | LSP type definitions |
| `tree-sitter` | 0.24 | Incremental, fault-tolerant parsing |
| `tree-sitter-objc` | 3.0.2 | Objective-C grammar |
| `clang-sys` | 1.8 | libclang FFI bindings |
| `rusqlite` | 0.31 | SQLite index storage |
| `tokio` | 1 | Async runtime |
| `zed_extension_api` | 0.5.0 | Zed extension WASM API |

## Platform Support

| Platform | Status | Notes |
|----------|--------|-------|
| **macOS** (Apple Silicon) | ✅ Primary | Full Xcode SDK integration, iOS Simulator SDK detection |
| **macOS** (Intel) | ✅ Supported | Full feature parity |
| **Linux** | ✅ Supported | GNUstep include path auto-detection |

## License

This project is licensed under the [MIT License](LICENSE).
