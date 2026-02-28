# Objective-C LSP — 进展状态

> 最后更新：2026-02-28（Phase 5 完成：代码格式化、代码折叠、调用层级、类型层级）

---

## 总体进度

| Phase | 状态 | 完成度 |
|-------|------|--------|
| Phase 1 — 核心骨架（MVP） | ✅ 完成 | 8/8 功能 |
| Phase 2 — ObjC 专属功能 | ✅ 完成 | 7/7 功能 |
| Phase 3 — 高级功能 | ✅ 完成 | 7/7 功能 |
| Phase 4 — VS Code 扩展 | ✅ 完成 | 8/8 功能 |
| Phase 5 — 编辑器增强功能 | ✅ 完成 | 4/4 功能 |

---

## Phase 1 — 核心骨架

| # | 功能 | LSP 方法 | 状态 | 实现位置 |
|---|------|----------|------|----------|
| 1 | `.h` 文件语言检测 | 内部逻辑 | ✅ | `objc-syntax/src/parser.rs`，内容启发式检测 `@interface`/`@implementation` |
| 2 | 文档符号 | `textDocument/documentSymbol` | ✅ | `objc-syntax/src/symbols.rs` |
| 3 | 语法诊断 | `textDocument/publishDiagnostics` | ✅ | `objc-semantic/src/diagnostics.rs` |
| 4 | 悬停信息 | `textDocument/hover` | ✅ | `objc-semantic/src/hover.rs` |
| 5 | 跳转定义 | `textDocument/definition` | ✅ | `objc-semantic/src/goto_def.rs` |
| 6 | 跳转声明 | `textDocument/declaration` | ✅ | `objc-semantic/src/goto_def.rs`（含 `.h` ↔ `.m` 跳转） |
| 7 | 语义 token | `textDocument/semanticTokens` | ✅ | `objc-syntax/src/tokens.rs` |
| 8 | 项目加载 | 启动初始化 | ✅ | `objc-project/`，支持 `compile_commands.json` + `.xcodeproj` |

**提交**：`2290305` — feat: complete Phase 1

---

## Phase 2 — ObjC 专属功能

| # | 功能 | LSP 方法 | 状态 | 实现位置 |
|---|------|----------|------|----------|
| 9 | 多部分 selector 补全 | `textDocument/completion` | ✅ | `objc-semantic/src/completion.rs`（修复 clangd #656） |
| 10 | `@property` 协调重命名 | `textDocument/rename` | ✅ | `objc-semantic/src/rename.rs`（修复 llvm #81775） |
| 11 | Protocol 方法桩生成 | `textDocument/codeAction` | ✅ | `objc-semantic/src/protocol_stubs.rs` |
| 12 | 查找所有引用 | `textDocument/references` | ✅ | `objc-semantic/src/references.rs` |
| 13 | Protocol 实现查找 | `textDocument/implementation` | ✅ | `objc-semantic/src/implementation.rs` |
| 14 | Inlay hints（参数标签） | `textDocument/inlayHint` | ✅ | `objc-syntax/src/inlay_hints.rs` |
| 15 | Category 聚合 | `textDocument/documentSymbol` | ✅ | `objc-syntax/src/symbols.rs`（`aggregate_categories()`） |

**提交**：`d943254` — feat: Phase 2

### Phase 2 技术备忘

两处 tree-sitter-objc 与文档/预期不符的行为，已在实现中修正：

- **Category 节点类型**：`@interface Foo (Cat)` 在 tree-sitter-objc 中产生的是 `class_interface` 节点（而非 `category_interface`），区分方式是检测是否存在 `(` 直接子节点。
- **Message expression 结构**：`message_expression` 的子节点是扁平的 `identifier : expr` 序列，**没有** `keyword_argument` 包裹节点。

---

## Phase 3 — 高级功能

| # | 功能 | 状态 |
|---|------|------|
| 16 | `clang --analyze` 集成 | ✅ 完成 |
| 17 | Nullability 检查 | ✅ 完成 |
| 18 | 代码操作（生成 interface/implementation pair 等） | ✅ 完成 |
| 19 | Apple SDK 文档（解析 SDK 头文件 `/*!` 注释） | ✅ 完成 |
| 20 | 全局符号搜索 | ✅ 完成 |
| 21 | GNUstep 支持 | ✅ 完成 |
| 22 | 完整跨文件 selector rename | ✅ 完成 |

---

## 测试状态

| Crate | 测试数 | 状态 | 备注 |
|-------|--------|------|------|
| `objc-syntax` | 34 unit + 14 integration = **48** | ✅ 全部通过 | inlay_hints, symbols, tokens, header_detect, folding(Phase 5) |
| `objc-intelligence` | **43** | ✅ 全部通过 | selector, property, protocol, category, header_nav, code_actions, nullability |
| `objc-semantic` | **14** | ✅ 全部通过 | hover, formatting(4), call_hierarchy(1), type_hierarchy(1) — Phase 5 新增 6 |
| `objc-lsp` | 0 | ✅ 二进制启动正常 | 尚无测试用例 |
| `objc-project` | **13** | ✅ 全部通过 | sdk flags, synthetic pod headers, cocoapods fallback |
| `objc-store` | **12** | ✅ 全部通过 | upsert_file, find_symbols_by_name, search_symbols |
> `cargo test --workspace` 全部通过（**130 tests**，零 failure）。libclang 路径通过 `.cargo/config.toml` 固化，无需手动设置环境变量。
---

## 目录结构（实际 vs 规划）

```
crates/
├── objc-lsp/src/
│   ├── main.rs            ✅
│   ├── dispatch.rs        ✅
│   ├── server.rs          ✅  Phase 1–5 handlers 全部接入（含 workspace/symbol、code actions、formatting、folding、call/type hierarchy）
│   ├── capabilities.rs    ✅  Phase 1–5 capabilities 全部声明
├── objc-syntax/src/
│   ├── parser.rs          ✅
│   ├── symbols.rs         ✅  含 aggregate_categories()
│   ├── tokens.rs          ✅
│   ├── inlay_hints.rs     ✅  Phase 2 新增
│   ├── folding.rs          ✅  Phase 5 新增（代码折叠）
│   └── lib.rs             ✅
├── objc-semantic/src/
│   ├── index.rs           ✅
│   ├── completion.rs      ✅
│   ├── hover.rs           ✅
│   ├── diagnostics.rs     ✅
│   ├── goto_def.rs        ✅
│   ├── references.rs      ✅  Phase 2 新增
│   ├── rename.rs          ✅  Phase 2 新增
│   ├── protocol_stubs.rs  ✅  Phase 2 新增
│   ├── implementation.rs  ✅  Phase 2 新增
│   ├── formatting.rs      ✅  Phase 5 新增（代码格式化）
│   ├── call_hierarchy.rs  ✅  Phase 5 新增（调用层级）
│   ├── type_hierarchy.rs  ✅  Phase 5 新增（类型层级）
│   └── lib.rs             ✅
├── objc-intelligence/src/
│   ├── selector.rs        ✅
│   ├── property.rs        ✅
│   ├── code_actions.rs    ✅  Phase 3 新增（syntax-based code actions）
│   └── lib.rs             ✅
├── objc-project/src/      ✅  骨架已建立
└── objc-store/src/        ✅  含 SymbolInput, index_file_symbols() — Phase 3 新增
```

**Phase 3 提交**:
- `662a193` — feat(Phase3-#20): workspace/symbol
- `b0c8a80` — feat(Phase3-#18): code actions
- `b26e888` — feat(Phase3-#22): cross-file selector rename
- `8b78787` — feat(#16): clang static analyzer diagnostics
- `48b1ab2` — feat(#17): nullability checker
- `27036d0` — feat(#19): Apple SDK doc comment rendering in hover
- `c6e4df6` — feat(#21): GNUstep support with 3-strategy detection
规划中尚未创建的文件：`header_nav.rs`、`category.rs`（逻辑已内联到 `symbols.rs`）、`protocol.rs`（逻辑已内联到 `protocol_stubs.rs`）、tree-sitter `.scm` 查询文件（目前以 Rust 代码直接遍历 AST 代替）。

---

## Phase 4 — VS Code 扩展

| # | 功能 | 状态 |
|---|------|------|
| 23 | LSP 客户端集成（`vscode-languageclient` 启动 `objc-lsp`） | ✅ 完成 |
| 24 | 二进制自动发现与安装引导 | ✅ 完成 |
| 25 | 语言 id 注册（`.m`、`.mm`、`.h` → `objective-c`） | ✅ 完成 |
| 26 | TextMate 语法增强（block、消息发送、编译器指令） | ✅ 完成 |
| 27 | 工作区设置（serverPath / logLevel / extraCompilerFlags 等） | ✅ 完成 |
| 28 | 状态栏指示器（⚡ Indexing / ✓ Ready / ✗ Error） | ✅ 完成 |
| 29 | 命令面板（Restart / Show Output / Report Issue） | ✅ 完成 |
| 30 | initializationOptions 传递用户设置给 objc-lsp | ✅ 完成 |

**Phase 4 提交**:
- `1c308b5` — feat(Phase4): VS Code extension — all 8 features (#23–#30)
- `e147c66` — chore: untrack node_modules and dist, add .gitignore for vscode extension

---

## 崩溃修复（Post Phase 4）

| # | 修复 | 状态 |
|---|------|------|
| F1 | DYLD_LIBRARY_PATH 注入（`@@HOMEBREW_PREFIX@@` dylib 安装名问题） | ✅ 完成 (`d3fb675`) |
| F2 | 崩溃隔离：`crash_guard` 模块（`sigsetjmp/siglongjmp` 保护 `clang_parseTranslationUnit`） | ✅ 完成 (`69cf39c`) |
| F3 | iOS SDK 检测：读取 Podfile/podspec/pbxproj，自动切换 iPhoneSimulator SDK | ✅ 完成 (`69cf39c`) |
| F4 | CocoaPods 头文件路径发现：自动添加 `Pods/Headers/Public/` 子目录 `-I` flags | ✅ 完成 (`69cf39c`) |
| F5 | 合成 Pod 头文件目录：无 `Pods/` 时扫描源树创建平铺 symlink 目录，解决 `#import <PodName/Header.h>` 报红 | ✅ 完成 (`4af5afa`) |
| F6 | `.h` 文件全功能 LSP：修复 documentSelector / languageId / `-x objective-c-header` 三处缺失 | ✅ 完成 (`907ff28`) |
| F7 | UIKit 类型识别（UIViewController 等）：`-fmodules` + 项目前缀头（`.pch`）注入 | ✅ 完成 (`5dbf267`) |
| F8 | 富化 SDK 符号 hover：继承链/协议列表/方法签名/@property 属性 + 物理头注释回退 | ✅ 完成 (`33aae60`) |
| F9 | goto-definition 修复：移除 `-fmodules` 根治 `CXError_ASTReadError`；`.pch` `@import` → `#import` 转换；`UIImage`/`NSString` 现可正确跳转到 SDK .h | ✅ 完成 (`6b24af6`) |
| F10 | hover 叶节点修复：`tight_cursor_at()` 用 `clang_tokenize`+`clang_annotateTokens` 获取 token 级 cursor，消除 `@implementation` 块中 hover 显示 `ObjCImplementationDecl` 而非实际符号的问题；增加容器体守卫 | ✅ 完成 (`1905dba`) |
| F11 | hover 方法体修复：移除 `CXTranslationUnit_SkipFunctionBodies` 使 clang 完整解析方法体 AST；添加 `-ferror-limit=0` 防止第三方头文件缺失导致提前终止解析 | ✅ 完成 (`5437693`) |
| F12 | URI percent-decode 修复：`uri_to_path()` 增加 `%2B` 等百分号编码解码，解决含 `+` 的 ObjC Category 文件（如 `MyClass+CategoryName.h`）hover 和 goto-definition 完全失效的问题 | ✅ 完成 (`0745ca5`) |

### 修复详情

- **根本原因**：`@@HOMEBREW_PREFIX@@` 是 Homebrew LLVM 的 dylib 安装名，`LC_RPATH` 不足以让 dyld 找到它，必须显式设置 `DYLD_LIBRARY_PATH`。
- **iOS SIGSEGV 根因**：项目目标是 iOS，但 `default_include_flags()` 使用 macOS SDK；`CoreNFC` 等框架不存在于 macOS SDK，导致 libclang 在 `clang_parseTranslationUnit` 中 SIGSEGV。
- **修复策略**：双保险 — 先检测 iOS 项目并切换 SDK（根治），再用 `sigsetjmp/siglongjmp` guard 防止任何残余崩溃杀死进程。
- **框架式 import 根因（F5）**：`#import <SAKIdentityCardRecognizer/SPKNfcIdentifyCommand.h>` 需要 CocoaPods 平铺目录结构（`PodName/Foo.h`），但 `pod install` 未运行时 `Pods/` 不存在。无法用单个 `-I parent/` 解决，因为头文件实际路径有多层嵌套。**修复**：在 `/tmp/objc-lsp-headers/<hash>/` 下建立 symlink 镜像（`PodName/Foo.h → 实际路径`），并将该目录作为 `-I` 传入 libclang。外部 Pod 仍会报 "file not found"（正确行为，需 `pod install`）。
- **`.h` 文件 LSP 不响应根因（F6）**：三处联合缺失：① VS Code 将 `.h` 识别为 `c` 语言 id 而非 `objective-c`；② extension `documentSelector` 未覆盖 `**/*.h`；③ libclang 以纯 C 解析 `.h`，未传 `-x objective-c-header`。三处同步修复。
- **UIKit 类型未知根因（F7）**：`.h` 文件通常只写 `#import <Foundation/Foundation.h>`，而 `UIViewController` 等 UIKit 类型由 Xcode 全局注入前缀头（`GCC_PREFIX_HEADER` .pch）及 `-fmodules` 提供。**修复 A**：`find_ios_simulator_sdk()` 追加 `-fmodules -fmodule-cache-path /tmp/objc-lsp-module-cache`，与 Xcode `CLANG_ENABLE_MODULES=YES` 一致。**修复 B**：`workspace_include_flags()` 扫描工作区（最多 3 层，跳过 Pods/build/DerivedData），找含 `#import <UIKit` 的 `.pch` 并追加 `-include <path>`，复现 Xcode 前缀头全局注入效果。
- **SDK hover 富化根因（F8）**：`-fmodules` 下 `clang_Cursor_getBriefCommentText` 对来自 PCM 缓存的声明返回空字符串（注释未存入编译好的模块）；且旧 hover 只显示类型名，不展示继承链/协议/方法签名。**修复 A**（富签名）：`hover_at()` 先调用 `clang_getCursorReferenced()` 解析引用到声明，再按 cursor kind 分发到专用构建函数：`@interface` 用 `clang_visitChildren` 收集 `ObjCSuperClassRef`/`ObjCProtocolRef` 子节点；方法用 `clang_getCursorResultType` + `ParmDecl` 迭代构建完整带类型签名；`@property` 用 `clang_Cursor_getObjCPropertyAttributes` 读取属性位掩码。**修复 B**（物理头注释回退）：三级策略：① `clang_Cursor_getBriefCommentText`；② `clang_Cursor_getRawCommentText`；③ 通过 `clang_getSpellingLocation` 获取物理 `.h` 文件路径，从磁盘读取并从声明行向上扫描 `/*!`/`/**`/`///`/`//!` 注释块。
- **goto-definition 根因（F9）**：`-fmodules` 旗标导致 Xcode 的 libclang 返回 `CXError_ASTReadError`（err=4），`CXTranslationUnit` 为 null，所有跳转操作均失败。实际上 SDK 头文件（UIKit、Foundation 等）通过 `-isysroot` 即可正确解析，无需 modules。此外，当 `.pch` 前缀头包含 `@import UIKit` 时，无 `-fmodules` 情况下解析失败；通过新增 `convert_at_imports()` 辅助函数将 `@import Foo;` 转换为 `#import <Foo/Foo.h>`，并将 `.pch` 内容复制到 `/tmp/objc-lsp-headers/prefix_header_src.h`（`.h` 扩展名）供 libclang 以普通源文本而非预编译 PCH 处理。修复后 `UIImage` → `UIImage.h:77:12`、`NSString` → `NSString.h:103:12` 跳转正常。
- **hover 叶节点根因（F10）**：`clang_getCursor` 返回最内层**包含**光标的 AST 节点，在 `@implementation` 方法体内部该节点是 `ObjCImplementationDecl`，而非实际被悬停的变量/方法引用。**修复**：新增 `tight_cursor_at()` 函数，以目标列号为中心 tokenize 单行小范围，调用 `clang_annotateTokens` 将每个 token 映射到其叶级 AST cursor，找到覆盖目标列的 token 并返回对应 cursor。同时增加容器体守卫：若解析后 cursor kind 仍为 `ObjCImplementationDecl | ObjCCategoryImplDecl | TranslationUnit`，则返回 `None`（不显示悬停）。
- **hover 方法体修复根因（F11）**：`CXTranslationUnit_SkipFunctionBodies` 旧旗标指示 clang 跳过所有方法体的解析（加速索引用途），导致 `@implementation` 内部完全没有 AST 节点 —— `clang_getCursor` 对方法体内任意位置均返回 `ObjCImplementationDecl`。另一个加剧因素：缺失第三方头文件时 clang 遇到第一个 fatal error 就停止解析，使 AST 更加不完整。**修复 A**：从 `index.rs` 移除 `CXTranslationUnit_SkipFunctionBodies` 旗标，允许 clang 完整解析方法体，使局部变量、属性访问、方法调用的 cursor 均可被正确解析。**修复 B**：`find_ios_simulator_sdk()` 和 `find_macos_sdk()` 均添加 `-ferror-limit=0`，防止第一个 "file not found" fatal error 中断后续解析，在缺少第三方 Pod 头文件时也能尽量生成完整 AST。
- **URI percent-decode 根因（F12）**：VS Code 在构造 `file://` URI 时将文件名中的 `+` 编码为 `%2B`（标准 URI 规范行为）。旧 `uri_to_path()` 仅做 `strip_prefix("file://")` 而不解码，导致传给 `clang_getFile()` 的路径包含字面 `%2B`，磁盘上不存在该路径，`clang_getFile()` 返回 null —— 所有 hover 和 goto-definition 操作均静默失败。**修复**：在 `server.rs` 中新增 `percent_decode()` / `hex_val()` 辅助函数，对 `file://` 路径部分做完整 `%XX` 解码（`%2B` → `+`，`%20` → 空格等），使 ObjC category 文件（如 `MyClass+CategoryName.h/.m`）的所有 LSP 功能恢复正常。

---

## Phase 5 — 编辑器增强功能

| # | 功能 | LSP 方法 | 状态 | 实现位置 |
|---|------|----------|------|----------|
| 31 | 代码格式化 | `textDocument/formatting` | ✅ 完成 | `objc-semantic/src/formatting.rs`（调用 clang-format 外部进程，4 unit tests） |
| 32 | 代码折叠 | `textDocument/foldingRange` | ✅ 完成 | `objc-syntax/src/folding.rs`（tree-sitter AST walk，8 unit tests） |
| 33 | 调用层级 | `callHierarchy/*` | ✅ 完成 | `objc-semantic/src/call_hierarchy.rs`（libclang 驱动，1 unit test） |
| 34 | 类型层级 | `typeHierarchy/*` | ✅ 完成 | `objc-semantic/src/type_hierarchy.rs`（libclang 驱动，1 unit test） |

**Phase 5 技术备忘**：

- **`type_hierarchy_provider` capability 限制**：`lsp-types` v0.97.0 的 `ServerCapabilities` 结构体不包含 `type_hierarchy_provider` 字段。Handler 已注册并可正常工作（客户端发送请求即可触发），但无法通过 capabilities 主动广告。未来可升级 `lsp-types` 或使用 `experimental` 字段解决。
- **`clang_visitChildren` 签名**：`clang-sys` v1.8.1 期望普通函数指针（非 `Option`），已在实现中修正。
- **VS Code 格式化注册**：`vscode-languageclient` v9 对自定义 language id 的 formatting 自动注册存在已知问题（microsoft/vscode#158760、microsoft/vscode-languageserver-node#1395），因此在 `extension.ts` 中手动调用 `vscode.languages.registerDocumentFormattingEditProvider()` 将格式化请求转发至 LSP 服务器。
- **clang-format 路径发现**：`find_clang_format()` 按以下优先级查找：① Homebrew LLVM (Apple Silicon) → ② Homebrew LLVM (Intel) → ③ Xcode 工具链 (`/Applications/Xcode.app/.../XcodeDefault.xctoolchain/usr/bin/clang-format`) → ④ 系统 `/usr/bin/clang-format` → ⑤ PATH 中的 `clang-format`。

---

## 开发注意事项

### ⚠️ 必须使用 Release 模式构建

VS Code 用户设置中 `objc-lsp.serverPath` 指向 **release** 二进制：

```
"objc-lsp.serverPath": ".../target/release/objc-lsp"
```

因此每次修改 Rust 代码后，**必须使用 `--release` 构建**，否则 VS Code 仍会使用旧的 release 二进制，导致修改不生效：

```bash
# ✅ 正确：构建 release 模式
cargo build --release --workspace

# ❌ 错误：仅构建 debug 模式，VS Code 不会使用
cargo build --workspace
```

完整的重新部署流程：

```bash
# 1. 构建 release 二进制
cargo build --release --workspace

# 2. 打包 VS Code 扩展（如修改了 TypeScript 代码）
cd editors/vscode && npx vsce package --no-dependencies

# 3. 安装扩展（如修改了 TypeScript 代码）
code --install-extension objc-lsp-0.1.0.vsix --force

# 4. 在 VS Code 中重新加载窗口
# Cmd+Shift+P → "Developer: Reload Window"
```

如果只修改了 Rust 代码（未修改 TypeScript），只需执行步骤 1 和 4。
