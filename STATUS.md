# Objective-C LSP — 进展状态

> 最后更新：2026-02-28（合成 Pod 头文件目录，解决无 `pod install` 时框架式 import 报错；commit 待提交）

---

## 总体进度

| Phase | 状态 | 完成度 |
|-------|------|--------|
| Phase 1 — 核心骨架（MVP） | ✅ 完成 | 8/8 功能 |
| Phase 2 — ObjC 专属功能 | ✅ 完成 | 7/7 功能 |
| Phase 3 — 高级功能 | ✅ 完成 | 7/7 功能 |
| Phase 4 — VS Code 扩展 | ✅ 完成 | 8/8 功能 |

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
| `objc-syntax` | 26 unit + 14 integration = **40** | ✅ 全部通过 | inlay_hints, symbols, tokens, header_detect |
| `objc-intelligence` | **43** | ✅ 全部通过 | selector, property, protocol, category, header_nav, code_actions, nullability |
| `objc-semantic` | **5** | ✅ 全部通过 | hover (Apple SDK doc comment tests) |
| `objc-lsp` | 0 | ✅ 二进制启动正常 | 尚无测试用例 |
| `objc-project` | **13** | ✅ 全部通过 | sdk flags, synthetic pod headers, cocoapods fallback |
| `objc-store` | **12** | ✅ 全部通过 | upsert_file, find_symbols_by_name, search_symbols |

> `cargo test --workspace` 全部通过（**113 tests**，零 failure）。libclang 路径通过 `.cargo/config.toml` 固化，无需手动设置环境变量。
---

## 目录结构（实际 vs 规划）

```
crates/
├── objc-lsp/src/
│   ├── main.rs            ✅
│   ├── dispatch.rs        ✅
│   ├── server.rs          ✅  Phase 1, 2 & 3 handlers 全部接入（含 workspace/symbol、code actions）
│   ├── capabilities.rs    ✅  Phase 1, 2 & 3 capabilities 全部声明
├── objc-syntax/src/
│   ├── parser.rs          ✅
│   ├── symbols.rs         ✅  含 aggregate_categories()
│   ├── tokens.rs          ✅
│   ├── inlay_hints.rs     ✅  Phase 2 新增
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
| F5 | 合成 Pod 头文件目录：无 `Pods/` 时扫描源树创建平铺 symlink 目录，解决 `#import <PodName/Header.h>` 报红 | ✅ 完成 (本次提交) |

### 修复详情

- **根本原因**：`@@HOMEBREW_PREFIX@@` 是 Homebrew LLVM 的 dylib 安装名，`LC_RPATH` 不足以让 dyld 找到它，必须显式设置 `DYLD_LIBRARY_PATH`。
- **iOS SIGSEGV 根因**：项目目标是 iOS，但 `default_include_flags()` 使用 macOS SDK；`CoreNFC` 等框架不存在于 macOS SDK，导致 libclang 在 `clang_parseTranslationUnit` 中 SIGSEGV。
- **修复策略**：双保险 — 先检测 iOS 项目并切换 SDK（根治），再用 `sigsetjmp/siglongjmp` guard 防止任何残余崩溃杀死进程。
- **框架式 import 根因（F5）**：`#import <SAKIdentityCardRecognizer/SPKNfcIdentifyCommand.h>` 需要 CocoaPods 平铺目录结构（`PodName/Foo.h`），但 `pod install` 未运行时 `Pods/` 不存在。无法用单个 `-I parent/` 解决，因为头文件实际路径有多层嵌套。**修复**：在 `/tmp/objc-lsp-headers/<hash>/` 下建立 symlink 镜像（`PodName/Foo.h → 实际路径`），并将该目录作为 `-I` 传入 libclang。外部 Pod 仍会报 "file not found"（正确行为，需 `pod install`）。
