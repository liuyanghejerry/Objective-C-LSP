# Objective-C LSP — 项目规划

> 第一个专为 Objective-C 设计的 Language Server Protocol 实现

---

## 一、项目定位与价值主张

现有工具（clangd、ccls、sourcekit-lsp）对 Objective-C 的支持都是**顺带的**——它们把 ObjC 当作 C/C++/Swift 的副产品处理。本项目是**第一个专为 Objective-C 设计的 LSP**，核心差异化在于：

- **原生理解 ObjC 语义**：selector、category、`@interface`/`@implementation` 二元性、block、property
- **无需 compile_commands.json**：直接解析 `.xcodeproj`，开箱即用
- **两层解析架构**：tree-sitter（毫秒级，容错）+ libclang（精确语义）
- **跨平台**：macOS、Linux（GNUstep），不锁定 Xcode

### 现有工具的根本局限

| 工具 | ObjC 地位 | 核心问题 |
|------|-----------|---------|
| **clangd** | 二等公民 | `.h` 语言检测错误（#621，open since 2020）；selector 补全残缺（#656，open since 2020）；`@property` rename 缺失（#81775，open since 2024） |
| **ccls** | 同 clangd | 所有语义路由均经过 libclang，与 clangd 共享同等局限，无 ObjC 专属扩展 |
| **sourcekit-lsp** | ObjC 转包给 clangd | 内部对 ObjC/ObjC++ 文件直接 spawn clangd 子进程，自身无任何 ObjC 实现 |

---

## 二、技术选型

### 实现语言：Rust

| 层 | 选择 | 理由 |
|----|------|------|
| LSP 框架 | `lsp-server`（rust-analyzer 团队出品） | 8M+ 下载，battle-tested，低层可控 |
| 异步运行时 | `tokio` | 标准 async Rust |
| 快速解析 | `tree-sitter` + `tree-sitter-grammars/tree-sitter-objc` | 增量、容错，~1ms/文件 |
| 语义分析 | `libclang`（via `clang-sys` FFI） | Xcode 本身在用，覆盖完整 ObjC AST |
| 索引存储 | `rusqlite`（SQLite） | 持久化跨文件引用缓存 |
| 项目解析 | 自研 `.xcodeproj` 解析器（pbxproj 格式） | 消除对 `compile_commands.json` 的依赖 |

### 为什么是 Rust 而非 TypeScript / Go？

- `clang-sys` 提供成熟的 libclang FFI，无需 Node native addon 的复杂性
- 零开销异步，LSP 响应延迟敏感
- 内存安全，长跑的后台 daemon 不会内存泄漏

---

## 三、系统架构

```
┌─────────────────────────────────────────────────────────┐
│                   Editor Client                          │
│           (Neovim / VSCode / Emacs / ...)                │
│                LSP JSON-RPC over stdio                   │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│              objc-lsp  (Rust binary)                     │
│                                                          │
│  ┌─────────────────────────────────────────────────┐    │
│  │  LSP Protocol Layer  (lsp-server + tokio)        │    │
│  │  JSON-RPC dispatch, capability negotiation       │    │
│  └──────────┬────────────────────┬─────────────────┘    │
│             │                    │                       │
│  ┌──────────▼──────┐  ┌──────────▼──────────────────┐   │
│  │  Fast Path       │  │  Semantic Path               │   │
│  │  (tree-sitter)   │  │  (libclang via clang-sys)    │   │
│  │                  │  │                              │   │
│  │  • documentSymbol│  │  • completions               │   │
│  │  • syntax tokens │  │  • hover (types/docs)        │   │
│  │  • breadcrumbs   │  │  • diagnostics               │   │
│  │  • fast indexing │  │  • go-to-definition          │   │
│  │  • folding ranges│  │  • find references           │   │
│  └──────────────────┘  │  • rename (selectors)        │   │
│                        │  • signature help            │   │
│                        └──────────┬──────────────────┘   │
│                                   │                       │
│  ┌────────────────────────────────▼──────────────────┐   │
│  │  ObjC Intelligence Layer  (核心差异化)              │   │
│  │                                                    │   │
│  │  • Selector database & multi-part completion       │   │
│  │  • @interface ↔ @implementation navigator         │   │
│  │  • Category aggregation index                      │   │
│  │  • @property → getter/setter/ivar coordinator     │   │
│  │  • Protocol conformance checker + stub generator  │   │
│  │  • .h language detector (ObjC vs C/C++)           │   │
│  └────────────────────────────────┬──────────────────┘   │
│                                   │                       │
│  ┌────────────────────────────────▼──────────────────┐   │
│  │  Project Layer                                     │   │
│  │  • .xcodeproj / pbxproj parser                    │   │
│  │  • compile_commands.json fallback                  │   │
│  │  • Apple SDK framework header discovery            │   │
│  │  • GNUstep include path detection                  │   │
│  └────────────────────────────────┬──────────────────┘   │
│                                   │                       │
│  ┌────────────────────────────────▼──────────────────┐   │
│  │  Index Store (SQLite)                              │   │
│  │  • Symbol table (classes, methods, properties)     │   │
│  │  • Cross-reference graph                           │   │
│  │  • Selector → implementations mapping             │   │
│  │  • Category → base class mapping                  │   │
│  └────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

---

## 四、目录结构

```
objective-c-lsp/
├── Cargo.toml                    # workspace
├── PLANNING.md                   # 本文件
├── README.md
├── crates/
│   ├── objc-lsp/                 # 主 binary，LSP 协议层
│   │   └── src/
│   │       ├── main.rs
│   │       ├── server.rs         # lsp-server handler
│   │       ├── capabilities.rs
│   │       └── dispatch.rs
│   ├── objc-syntax/              # tree-sitter 快速解析层
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── parser.rs
│   │       └── queries/          # tree-sitter 查询文件 (.scm)
│   │           ├── highlights.scm
│   │           ├── symbols.scm
│   │           └── injections.scm
│   ├── objc-semantic/            # libclang 语义分析层
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── index.rs          # libclang 索引管理
│   │       ├── completion.rs
│   │       ├── hover.rs
│   │       └── diagnostics.rs
│   ├── objc-intelligence/        # ObjC 专属逻辑层（核心差异化）
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── selector.rs       # selector database & completion
│   │       ├── header_nav.rs     # .h ↔ .m navigation
│   │       ├── category.rs       # category aggregation
│   │       ├── protocol.rs       # protocol conformance & stubs
│   │       └── property.rs       # @property rename coordination
│   ├── objc-project/             # 项目/构建系统层
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── xcodeproj.rs      # .xcodeproj / pbxproj parser
│   │       ├── compile_db.rs     # compile_commands.json
│   │       └── sdk.rs            # Apple SDK / GNUstep path discovery
│   └── objc-store/               # SQLite 索引存储
│       └── src/
│           ├── lib.rs
│           ├── schema.rs
│           └── queries.rs
├── editors/
│   ├── vscode/                   # VSCode 扩展（TypeScript）
│   └── neovim/                   # Neovim 配置示例（Lua）
├── tests/
│   ├── fixtures/                 # 测试用 ObjC 项目
│   └── integration/
└── docs/
    ├── architecture.md
    └── contributing.md
```

---

## 五、功能路线图

### Phase 1 — 核心骨架（MVP）

优先修复 clangd 已知影响最广的 ObjC 缺陷，建立可用基线：

| # | 功能 | LSP 方法 | 说明 |
|---|------|----------|------|
| 1 | **`.h` 文件语言检测** | 内部逻辑 | 内容启发式：检测 `@interface`/`@implementation` 自动判定为 ObjC（修复 clangd #621） |
| 2 | **文档符号** | `textDocument/documentSymbol` | tree-sitter 驱动，毫秒级，正确展示 `@interface`、方法、`@property` |
| 3 | **语法诊断** | `textDocument/publishDiagnostics` | libclang 驱动，clang errors/warnings |
| 4 | **悬停信息** | `textDocument/hover` | 类型信息 + 方法签名 |
| 5 | **跳转定义** | `textDocument/definition` | 含 `.h` ↔ `.m` 跳转逻辑 |
| 6 | **跳转声明** | `textDocument/declaration` | 跳到 `@interface` 而非 `@implementation` |
| 7 | **语义 token** | `textDocument/semanticTokens` | ObjC 专属：message send、selector、keyword |
| 8 | **项目加载** | 启动初始化 | `compile_commands.json` + `.xcodeproj` 解析 |

### Phase 2 — ObjC 专属功能（核心差异化）

| # | 功能 | 解决的问题 |
|---|------|-----------|
| 9 | **多部分 selector 补全** | `[tableView:_ cellForRowAtIndexPath:_]` 完整填充，clangd #656 open since 2020 |
| 10 | **`@property` 协调重命名** | getter + setter + ivar + 点语法统一改名，clangd #81775 open since 2024 |
| 11 | **Protocol 方法桩生成** | `@interface Foo <Bar>` → 自动生成未实现方法的桩代码 |
| 12 | **查找所有引用** | `textDocument/references` 含 message send 全扫描 |
| 13 | **Protocol 实现查找** | `textDocument/implementation` 找到所有 conform 某 protocol 的类 |
| 14 | **Inlay hints（参数标签）** | `textDocument/inlayHint` 为 message send 参数显示标签 |
| 15 | **Category 聚合** | 一个类的所有 category 方法在 documentSymbol 中汇总展示 |

### Phase 3 — 高级功能

| # | 功能 | 说明 |
|---|------|------|
| 16 | **`clang --analyze` 集成** | Clang 静态分析结果通过 diagnostics 暴露 |
| 17 | **Nullability 检查** | `NS_ASSUME_NONNULL` 区域分析，缺失标注提示 |
| 18 | **代码操作** | 生成 `@interface`/`@implementation` pair、添加 `NS_ASSUME_NONNULL_BEGIN/END` |
| 19 | **Apple SDK 文档** | 悬停时显示 Apple 文档注释（解析 SDK 头文件 `/*!` 注释） |
| 20 | **全局符号搜索** | `workspace/symbol` 全项目类/方法搜索 |
| 21 | **GNUstep 支持** | Linux 下 GNUstep include 路径自动发现 |
| 22 | **完整 rename** | `textDocument/rename` 完整 selector rename，含跨文件 |

---

## 六、关键依赖

```toml
# Cargo.toml (核心依赖)
[dependencies]
lsp-server    = "0.7"                           # rust-analyzer 的 LSP 框架
lsp-types     = "0.97"                          # LSP 类型定义
tokio         = { version = "1", features = ["full"] }
tree-sitter   = "0.22"
tree-sitter-objc = "3.0"                        # tree-sitter-grammars/tree-sitter-objc
clang-sys     = "1.8"                           # libclang FFI
rusqlite      = { version = "0.31", features = ["bundled"] }
serde         = { version = "1", features = ["derive"] }
serde_json    = "1"
tracing       = "0.1"
tracing-subscriber = "0.3"
```

---

## 七、已知 ObjC LSP 缺陷清单（来源追踪）

以下是驱动本项目的具体 issue，均有 GitHub 原始链接：

### 代码补全
- **clangd #656**（open since 2020）：不自动插入 message send 的 `[` 括号
- **sourcekit-lsp #2398**（open）：不生成 Protocol 缺失方法的桩

### 重命名/重构
- **llvm #81775**（open since Feb 2024）：`@property` rename 不协调 getter/setter/ivar
- **llvm #76466**（fixed Feb 2024）：多部分 selector rename 曾完全失效
- **llvm #78872**（fixed 2024）：基础 selector rename 曾完全缺失

### 导航/跨文件引用
- **clangd #621**（open since 2020）：`.h` 文件被错误识别为 C 而非 ObjC
- **llvm #127109**（fixed Feb 2025）：Protocol 实现查找刚刚才加入
- **clangd #2457**（open）：相邻 token 光标位置导致 Find References 出错
- **llvm #82061**（fixed 2024）：ObjC selector 在符号索引中的表示方式根本上就是错的

### 诊断/静态分析
- **llvm #181209**（open）：`misc-include-cleaner` 对 ObjC++ 显式禁用
- **llvm #65105**（open）：`cppcoreguidelines` 在 ObjC++ 文件上崩溃

### 代码操作
- **sourcekit-lsp #2399**（open）：无"显示/复制 ObjC selector"操作
- **sourcekit-lsp #2398**（open）：无"添加 Protocol 缺失实现"操作

### 格式化
- **llvm #84133**（closed, workaround only）：大型单头文件 ObjC 格式化启发式导致 OOM

---

## 八、AI Coding 协作策略

本项目充分利用 AI Coding，按收益高低分配：

| 模块 | AI 收益 | 原因 |
|------|---------|------|
| LSP 协议 handler 样板 | ⭐⭐⭐ 高 | 纯结构化、协议驱动，模式重复 |
| tree-sitter `.scm` 查询文件 | ⭐⭐⭐ 高 | 声明式，有大量参考范例 |
| SQLite schema & CRUD | ⭐⭐⭐ 高 | 标准模式 |
| pbxproj 解析器 | ⭐⭐ 中 | 格式有文档，但边缘情况多 |
| libclang FFI 绑定封装 | ⭐⭐ 中 | 有 `clang-sys` 文档可参考 |
| Selector 补全引擎 | ⭐ 低 | ObjC 专属逻辑，需要人工设计算法 |

**推荐工作流**：人工编写每个功能的集成测试 fixture（真实 ObjC 代码片段）→ AI 生成实现 → 对照测试验证。

---

## 九、参考资料

- [LSP 3.17 规范](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)
- [lsp-server crate](https://crates.io/crates/lsp-server)
- [tree-sitter-objc grammar](https://github.com/tree-sitter-grammars/tree-sitter-objc)
- [clang-sys FFI bindings](https://github.com/KyleMayes/clang-sys)
- [libclang 文档](https://clang.llvm.org/docs/LibClang.html)
- [clangd ObjC issues](https://github.com/clangd/clangd/issues?q=objc)
- [sourcekit-lsp](https://github.com/swiftlang/sourcekit-lsp)
- [tower-lsp-server](https://github.com/tower-lsp-community/tower-lsp-server)
