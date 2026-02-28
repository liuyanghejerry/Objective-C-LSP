# Objective-C LSP — 进展状态

> 最后更新：2026-02-28（Phase 3 #16, #18, #20, #22 完成）

---

## 总体进度

| Phase | 状态 | 完成度 |
|-------|------|--------|
| Phase 1 — 核心骨架（MVP） | ✅ 完成 | 8/8 功能 |
| Phase 2 — ObjC 专属功能 | ✅ 完成 | 7/7 功能 |
| Phase 3 — 高级功能 | 🚧 进行中 | 4/7 功能 |

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
| 17 | Nullability 检查 | ⏳ 未开始 |
| 18 | 代码操作（生成 interface/implementation pair 等） | ✅ 完成 |
| 19 | Apple SDK 文档（解析 SDK 头文件 `/*!` 注释） | ⏳ 未开始 |
| 20 | 全局符号搜索 | ✅ 完成 |
| 21 | GNUstep 支持 | ⏳ 未开始 |
| 22 | 完整跨文件 selector rename | ✅ 完成 |

---

## 测试状态

| Crate | 测试数 | 状态 | 备注 |
|-------|--------|------|------|
| `objc-syntax` | 26 unit + 14 integration = **40** | ✅ 全部通过 | inlay_hints, symbols, tokens, header_detect |
| `objc-intelligence` | **36** | ✅ 全部通过 | selector, property, protocol, category, header_nav, code_actions |
| `objc-semantic` | 0 | ✅ 二进制启动正常 | 尚无测试用例 |
| `objc-lsp` | 0 | ✅ 二进制启动正常 | 尚无测试用例 |
| `objc-project` | **8** | ✅ 全部通过 | shell_words_split (compile_db) |
| `objc-store` | **10** | ✅ 全部通过 | upsert_file, find_symbols_by_name, search_symbols |

> `cargo test --workspace` 全部通过（96 tests，零 failure）。libclang 路径通过 `.cargo/config.toml` 固化，无需手动设置环境变量。
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

规划中尚未创建的文件：`header_nav.rs`、`category.rs`（逻辑已内联到 `symbols.rs`）、`protocol.rs`（逻辑已内联到 `protocol_stubs.rs`）、tree-sitter `.scm` 查询文件（目前以 Rust 代码直接遍历 AST 代替）。
