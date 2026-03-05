# Objective-C LSP

> The first Language Server Protocol implementation designed specifically for Objective-C.

**objc-lsp** is built from the ground up to understand Objective-C semantics: selectors, categories, `@interface`/`@implementation` duality, blocks, and `@property` coordination.


## Features

### LSP Protocol (34 features across 5 phases)

**Core (Phase 1)** — `.h` language detection, document symbols, diagnostics, hover, goto definition/declaration, semantic tokens, project loading

**ObjC-Specific (Phase 2)** — Multi-part selector completion, `@property` coordinated rename, protocol stub generation, find references, protocol implementation finder, inlay hints, category aggregation

**Advanced (Phase 3)** — `clang --analyze` integration, nullability checks, code actions, Apple SDK documentation in hover, workspace symbol search, GNUstep support, cross-file selector rename

**Editor Enhancement (Phase 5)** — Code formatting (clang-format), code folding, call hierarchy, type hierarchy

### [VS Code Extension](https://marketplace.visualstudio.com/items?itemName=liuyanghejerry.objc-lsp)

Beyond standard LSP features, the VS Code extension provides:

- **26 ObjC Snippets** — `prop`, `singleton`, `weakSelf`, `block`, `protocol`, and more
- **6 Quick Fix Commands** — Generate `@property`, wrap in `@autoreleasepool`, fix retain cycles, etc.
- **4 Code Lens Types** — Call counts, protocol conformance, override status, deprecation warnings
- **5 Text Decorators** — Retain cycle warnings, thread safety, magic numbers, unused code, strong delegate detection
- **Symbols Outline Pro** — Tree view grouped by `#pragma mark`, protocol, and category
- **Class Browser** — Project-wide class hierarchy tree
- **Call Graph Webview** — Interactive method call relationship visualization
- **Hover Extensions** — API availability parsing, deprecation warnings, related methods with clickable navigation, quick fix links

> **Note**: For VS Code forks such as [Cursor](https://cursor.sh), install from [Open VSX Registry](https://open-vsx.org/extension/liuyanghejerry/objc-lsp) instead.

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

On macOS with Xcode installed, libclang is available automatically. On Linux, install LLVM:

```bash
# Ubuntu/Debian
sudo apt install libclang-dev

# Fedora
sudo dnf install clang-devel
```

### Build the LSP Server

```bash
# 1. Clone the repository
git clone https://github.com/liuyanghejerry/Objective-C-LSP.git
cd Objective-C-LSP

# 2. Build the LSP server binary (MUST use --release)
cargo build --release --workspace
```

The compiled binary is located at `target/release/objc-lsp`. You can copy it to any directory on your `$PATH` (e.g. `~/.local/bin/`):

```bash
cp target/release/objc-lsp ~/.local/bin/objc-lsp
```

## Usage

objc-lsp communicates over **stdio** using the standard LSP JSON-RPC protocol, so it works with any LSP-compatible editor.

### Neovim

#### Built-in LSP (`vim.lsp`, Neovim 0.10+)

No plugins required. Add the following to your Neovim configuration (`init.lua`):

```lua
vim.api.nvim_create_autocmd('FileType', {
  pattern = { 'objc', 'objcpp' },
  callback = function()
    vim.lsp.start({
      name = 'objc-lsp',
      cmd = { vim.fn.expand('~/.local/bin/objc-lsp') },
      root_dir = vim.fs.dirname(
        vim.fs.find(
          { '*.xcodeproj', 'compile_commands.json', '.git' },
          { upward = true }
        )[1]
      ),
    })
  end,
})
```

Replace `~/.local/bin/objc-lsp` with the actual path to the binary if it differs.

#### nvim-lspconfig

If you use [nvim-lspconfig](https://github.com/neovim/nvim-lspconfig), register objc-lsp as a custom server:

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.objc_lsp then
  configs.objc_lsp = {
    default_config = {
      cmd = { vim.fn.expand('~/.local/bin/objc-lsp') },
      filetypes = { 'objc', 'objcpp' },
      root_dir = lspconfig.util.root_pattern(
        '*.xcodeproj', 'compile_commands.json', '.git'
      ),
      single_file_support = true,
    },
  }
end

lspconfig.objc_lsp.setup({})
```

### Emacs

Replace `"/path/to/objc-lsp"` in the examples below with the full path to the binary (e.g. `~/.local/bin/objc-lsp`), or ensure the binary is on your system `$PATH` and use `"objc-lsp"` directly.

#### eglot (built-in, Emacs 29+)

```emacs-lisp
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               '((objc-mode objc++-mode) . ("/path/to/objc-lsp"))))
```

#### lsp-mode

```emacs-lisp
(with-eval-after-load 'lsp-mode
  (lsp-register-client
   (make-lsp-client
    :new-connection (lsp-stdio-connection "/path/to/objc-lsp")
    :major-modes '(objc-mode objc++-mode)
    :server-id 'objc-lsp)))
```

### VS Code

Install the extension from the [VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=liuyanghejerry.objc-lsp) (or [Open VSX](https://open-vsx.org/extension/liuyanghejerry/objc-lsp) for VS Code forks such as [Cursor](https://cursor.sh)).

To build and install from source:

```bash
cd editors/vscode
npm install
node esbuild.mjs
npx vsce package --no-dependencies
code --install-extension objc-lsp-0.1.0.vsix --force
```

After installation, optionally set the server binary path in VS Code settings (`settings.json`):

```json
{
  "objc-lsp.serverPath": "/path/to/objc-lsp/target/release/objc-lsp"
}
```

If left empty, the extension auto-discovers the binary from `$PATH` or the bundled location.

#### Available VS Code Settings

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

### Zed

Install the extension from the Zed extension registry, or install as a dev extension from source:

```
Open Zed → Extensions → Install Dev Extension → select editors/zed
```

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

For the Zed extension, Zed compiles the WASM automatically. Before installing the dev extension, ensure the grammar submodule is initialized:

```bash
# Ensure Rust is installed via rustup (not Homebrew)
rustup target add wasm32-wasip2

# Initialize the grammar submodule
git submodule update --init editors/zed/grammars/objc
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

## Acknowledgments

This project is made possible by the following open-source projects:

- [tree-sitter](https://github.com/tree-sitter/tree-sitter) and [tree-sitter-objc](https://github.com/tree-sitter-grammars/tree-sitter-objc) — Fast, incremental parsing for Objective-C
- [clang / libclang](https://clang.llvm.org/) — Semantic analysis backbone
- [lsp-server](https://github.com/rust-lang/rust-analyzer/tree/master/lib/lsp-server) and [lsp-types](https://github.com/gluon-lang/lsp-types) — LSP framework by the rust-analyzer team
- [rusqlite](https://github.com/rusqlite/rusqlite) — SQLite bindings for the index store
- [tokio](https://tokio.rs/) — Async runtime
- [Zed](https://zed.dev/) — Editor and extension API
- [vscode-languageclient](https://github.com/microsoft/vscode-languageserver-node) — VS Code LSP client library by Microsoft

Development of this project was assisted by [OpenCode](https://github.com/nicepkg/opencode) and [Oh My OpenCode](https://github.com/nicepkg/oh-my-opencode).

## License

This project is licensed under the [MIT License](LICENSE).
