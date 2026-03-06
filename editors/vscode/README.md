# Objective-C Language Server (objc-lsp)

First-class Objective-C language support for VS Code, built for iOS and macOS developers.

## Features

### Language Support

- **Smart code completion** — Multi-part selector completion with full fill-in (e.g. `[tableView cellForRowAtIndexPath:]`)
- **Go to definition / declaration** — Jump to SDK symbols (UIView, UIBezierPath, etc.) without `compile_commands.json`; reads `.xcodeproj` directly
- **Hover documentation** — Apple SDK docs with API availability and deprecation warnings
- **Diagnostics** — Real-time compiler errors and warnings
- **Find references** — Cross-file reference search
- **Rename** — `@property` rename automatically coordinates getter, setter, backing ivar, and dot syntax in one operation
- **Formatting** — `clang-format`-based code formatting
- **Document symbols** — Class, method, and property outline for the current file
- **Workspace symbol search** — Cross-file symbol search
- **Inlay hints** — Inline type hints
- **Protocol stub generation** — Auto-generate required protocol method stubs
- **Code actions** — Quick-fix suggestions
- **Call hierarchy / Type hierarchy** — Explore call chains and inheritance trees
- **Nullability checks** — Detect missing nullability annotations (configurable)
- **Static analysis** — Integrated `clang --analyze` on save (off by default, configurable)
- **`.h` file language detection** — Correctly identifies `.h` files as Objective-C vs C, fixing a long-standing clangd limitation

### VS Code-Specific Features

#### Code Snippets

26 snippets for common Objective-C patterns:

| Prefix | Description |
|--------|-------------|
| `prop` | Strong property |
| `propn` | Copy property (NSString, etc.) |
| `propw` | Weak property (for delegates) |
| `propa` | Assign property (primitives) |
| `propr` | Readonly property |
| `singleton` | `dispatch_once` singleton pattern |
| `weakSelf` | `__weak typeof(self) weakSelf = self;` |
| `strongSelf` | Promote weakSelf to strongSelf with nil guard |
| `block` | Block typedef declaration |
| `blockprop` | Block property declaration |
| `blocki` | Inline block literal |
| `protocol` | Full `@protocol` template |
| `implem` | `@implementation...@end` template |
| `interface` | `@interface...@end` template |
| `category` | Category template |
| `extension` | Class extension (anonymous category) template |
| `ifweak` | Nil guard for weakSelf |
| `dispatch_main` | `dispatch_async` to main queue |
| `dispatch_bg` | `dispatch_async` to background queue |
| `autorelease` | `@autoreleasepool` block |
| `synthesize` | `@synthesize property = _property` |
| `nslog` | NSLog format string |
| `init` | Standard `init` method template |
| `dealloc` | `dealloc` method template |
| `pragma` | `#pragma mark` section divider |
| `trycatch` | `@try/@catch/@finally` block |

#### Code Lens

Displays annotations above methods — reference counts, protocol conformance status, and more. Toggle with `objc-lsp.enableCodeLens`.

#### Inline Decorators

Real-time inline warnings for common Objective-C pitfalls. Toggle with `objc-lsp.enableDecorators`.

- ⚠️ **Retain cycle warning** — Highlights `self` captured strongly inside a block, with a hover explanation and fix suggestion
- ⚠️ **Strong delegate warning** — Flags `delegate` / `dataSource` properties declared with `strong` (typically a retain cycle)
- 💡 **Magic number hint** — Suggests extracting hardcoded numeric literals into named constants

#### Sidebar Views

- **Symbols Outline** — Symbol tree for the current file, grouped by `#pragma mark`, protocol, and category
- **Class Browser** — Project-wide class hierarchy tree

#### Call Graph

Run `ObjC: Show Call Graph` to open an interactive webview showing method call relationships.

#### Extended Hover

Hover popups include related methods (clickable), API availability info, deprecation warnings, and quick-fix links. Toggle with `objc-lsp.enableHoverExtensions`.

### Command Palette

Access via `Cmd+Shift+P` (macOS) or `Ctrl+Shift+P` (Linux):

| Command | Description |
|---------|-------------|
| `ObjC: Restart Language Server` | Restart the language server process |
| `ObjC: Show Language Server Output` | Open the server log panel |
| `ObjC: Report an Issue` | File a bug report on GitHub |
| `ObjC: Generate @property from ivar` | Generate a `@property` declaration from an instance variable |
| `ObjC: Wrap in @autoreleasepool` | Wrap selected code in an `@autoreleasepool` block |
| `ObjC: Wrap in dispatch_async (main queue)` | Wrap selected code in a main-queue `dispatch_async` |
| `ObjC: Add @synthesize` | Insert a `@synthesize` directive |
| `ObjC: Fix Retain Cycle (add weakSelf)` | Insert `weakSelf` to break a retain cycle |
| `ObjC: Extract Method` | Extract selected code into a new method |
| `ObjC: Refresh Class Browser` | Refresh the Class Browser tree |
| `ObjC: Show Call Graph` | Open the call graph webview |

## Installation

### Build from Source

```bash
# 1. Clone the repository
git clone https://github.com/liuyanghejerry/Objective-C-LSP
cd objective-c-lsp

# 2. Build the language server (release build required)
cargo build --release --workspace

# 3. Build and install the VS Code extension
cd editors/vscode
npm install
node esbuild.mjs
npx vsce package --no-dependencies
code --install-extension objc-lsp-0.1.3.vsix --force
```

If the extension does not auto-discover the server binary, set the path in settings:

```json
{
  "objc-lsp.serverPath": "/path/to/objective-c-lsp/target/release/objc-lsp"
}
```

## Configuration

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `objc-lsp.serverPath` | string | `""` | Path to the `objc-lsp` binary. Leave empty to auto-discover. |
| `objc-lsp.logLevel` | string | `"info"` | Server log verbosity: `error` / `warn` / `info` / `debug` |
| `objc-lsp.extraCompilerFlags` | string[] | `[]` | Extra clang flags, e.g. `["-DDEBUG", "-I/usr/local/include"]` |
| `objc-lsp.enableNullabilityChecks` | boolean | `true` | Report missing nullability annotations |
| `objc-lsp.enableStaticAnalyzer` | boolean | `false` | Run `clang --analyze` on save (slower) |
| `objc-lsp.enableCodeLens` | boolean | `true` | Show Code Lens annotations above methods |
| `objc-lsp.enableDecorators` | boolean | `true` | Show inline decorators (retain cycles, magic numbers, etc.) |
| `objc-lsp.enableHoverExtensions` | boolean | `true` | Show extended hover information |

## Requirements

- **VS Code** 1.85+
- **macOS** — Xcode installed (provides libclang automatically)
- **Linux** — `libclang-dev` installed:

  ```bash
  # Ubuntu/Debian
  sudo apt install libclang-dev

  # Fedora
  sudo dnf install clang-devel
  ```

## Platform Support

| Platform | Status | Notes |
|----------|--------|-------|
| macOS (Apple Silicon) | ✅ Fully supported | Needs Xcode SDK installed. |
| macOS (Intel) | ✅ Supported | Features implemented, testing needed on real hardware |
| Linux (x64) | ✅ Supported | Requires `libclang-dev` installed |

## License

This extension is licensed under the [MIT License](./LICENSE).
