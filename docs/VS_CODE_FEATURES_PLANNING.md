# VS Code 扩展 AI 增强能力规划

> 基于 FEATURE_ANALYSIS.md 分析，为当前 ObjC-LSP 的 VS Code 扩展补充非 LSP 的 AI 增强能力

---

## 概述

LSP 协议本身存在限制，缺失以下高价值信息：
- Call Hierarchy（调用链）
- Type Hierarchy（类型层级）
- 数据流分析
- 控制流分析

这些能力可以通过 **VS Code 扩展层面** 补充，不依赖 LSP 协议。

---

## 一、优先级 P0（高频刚需） ✅ 已实现

### 1.1 智能代码片段 (Smart Snippets) ✅

**现状**: ~~VS Code 基础 snippet 支持~~ → 已实现 26 个 ObjC 专用代码片段（commit `8f01e88`）

**可补充**:

| 缩写 | 展开内容 |
|------|----------|
| `prop` | `@property (nonatomic, strong) <Class *> <name>;` |
| `propn` | `@property (nonatomic, copy) <Class *> <name>;` |
| `propw` | `@property (nonatomic, weak) <Class *> <name>;` |
| `singleton` | `dispatch_once` 单例模板 |
| `weakSelf` | `__weak typeof(self) weakSelf = self;` |
| `strongSelf` | `__strong typeof(self) strongSelf = self;` |
| `block` | typedef block 模板 |
| `protocol` | 完整协议模板 |
| `implem` | `@implementation ... @end` 模板 |
| `ifweak` | `if (!weakSelf) return;` 防御代码 |

**实现方式**: `CompletionItem` with `InsertTextFormat.Snippet`

**价值**:
- ObjC 模板代码多，手写效率低
- AI 可以直接使用这些 snippet 生成代码

---

### 1.2 Quick Fix 命令 (Commands) ✅

**已实现 6 个命令**（commit `8f01e88`）:

| 命令 ID | 功能 | 状态 |
|---------|------|------|
| `objc-lsp.addProperty` | 选中 ivar 一键生成 @property | ✅ |
| `objc-lsp.addNullability` | 添加 `nonnull`/`nullable` 注解 | ✅ |
| `objc-lsp.wrapAutoreleasepool` | 选中代码包裹 `@autoreleasepool {}` | ✅ |
| `objc-lsp.wrapDispatchAsync` | 包裹 `dispatch_async(dispatch_get_main_queue(), ^{})` | ✅ |
| `objc-lsp.addSynthesize` | 自动生成 `@synthesize` | ✅ |
| `objc-lsp.fixRetainCycle` | 添加 `__weak` 解决循环引用 | ✅ |
| `objc-lsp.extractMethod` | 选中代码提取为方法 | ❌ 未实现 |
**实现方式**: `commands.registerCommand` + `TextEditor.edit`

**价值**:
- 高频重构操作
- AI 可以调用这些命令执行修复

---

## 二、优先级 P1（AI 强需求） ✅ 已实现

### 2.1 Code Lens（代码透镜） ✅

**已实现**（commit `eddf5bf`）:

| 类型 | 说明 | 状态 |
|------|------|------|
| **调用计数** | 显示方法被调用的次数 | ✅ |
| **协议来源** | 标记方法来自哪个协议（如 `UITableViewDataSource`） | ✅ |
| **覆盖状态** | 标记 protocol 方法是否已实现 | ✅ |
| **废弃警告** | 标记使用了已废弃的 API | ✅ |
**实现方式**: `CodeLensProvider`

**价值**:
- 调用计数是 AI 最缺失的信息（FEATURE_ANALYSIS.md 明确指出）
- 协议来源帮助 AI 理解方法语义

---

### 2.2 装饰器 (Decorators) ✅

**已实现**（commit `eddf5bf`）:

| 类型 | 说明 | 状态 |
|------|------|------|
| **Retain Cycle 警告** | 在可能产生循环引用的代码处显示警告图标 | ✅ |
| **Thread Safety** | 标记非线程安全的代码（如非原子 property） | ✅ |
| **Magic Number** | 标记硬编码的数字 | ✅ |
| **Unused Code** | 标记从未使用的方法/属性 | ✅ |
| **Strong Delegate** | 标记 delegate 用 strong 而非 weak | ✅ |

**实现方式**: `TextEditorDecorationType`

**价值**:
- ObjC 特有痛点
- AI 可据此提供修复建议

---

## 三、优先级 P2（可视化增强） ✅ 已实现

### 3.1 Tree View（树视图） ✅

**已实现**（commit `1d066ac`）:

| 视图 | 说明 | 状态 |
|------|------|------|
| **Symbols Outline Pro** | 按 #pragma mark、protocol、category 分组的符号树 | ✅ |
| **Class Browser** | 项目中所有类的树状视图 | ✅ |
| **Protocol Implementations** | 列出所有协议及其实现者 | ❌ 未实现 |
| **Categories/Extensions** | 按类别分组的方法视图 | ❌ 未实现 |

**实现方式**: `TreeView` + `TreeDataProvider`

**价值**:
- 弥补 LSP 缺失的 Type Hierarchy
- #pragma mark 导航是 ObjC 项目常见需求

---

### 3.2 Webview 面板 ✅ 部分实现

**部分实现**（commit `1d066ac`）:

| 面板 | 说明 | 状态 |
|------|------|------|
| **Call Graph** | 方法调用关系图（Incoming/Outgoing） | ✅ |
| **Type Hierarchy** | 类/协议继承关系可视化图 | ❌ 未实现 |
| **Dependency Graph** | 文件间 import 依赖关系图 | ❌ 未实现 |
**实现方式**: `WebviewPanel` + D3.js / Graphviz

**价值**:
- 可视化弥补 LSP 协议缺失
- AI 可以解析图结构获取调用链信息

---

## 四、优先级 P3（集成增强）

### 4.1 Hover 扩展（扩展端实现） ✅

**已实现**（扩展端 HoverProvider，补充 LSP hover）:

| 类型 | 说明 | 状态 |
|------|------|------|
| **快速修复按钮** | 在 hover 中提供 Fix-it 命令链接 | ✅ |
| **内联文档** | LSP 已提供 HeaderDoc 摘要（无需重复） | ✅ LSP 已覆盖 |
| **相关方法** | 显示同 @implementation 中的其他方法（可点击跳转） | ✅ |
| **API 版本** | 解析 API_AVAILABLE/NS_AVAILABLE 等宏显示平台版本 | ✅ |
| **弃用警告** | 解析 NS_DEPRECATED/deprecated_msg 显示弃用信息 | ✅ |

**实现方式**: `HoverProvider`（VS Code 扩展端，不走 LSP）

---

### 4.2 Test Explorer

**可补充**:

| 功能 | 说明 |
|------|------|
| **测试发现** | 发现 SenTestingCase / XCTest |
| **测试树** | 测试用例树状视图 |
| **运行测试** | 运行单个/全部/类测试 |
| **覆盖标记** | 显示测试覆盖的方法 |

**实现方式**: `TestController` + `TestItem`

---

### 4.3 调试集成

**可补充**:

| 功能 | 说明 |
|------|------|
| **Launch 模板** | 自动生成 `launch.json` |
| **方案选择器** | xcodeproj 方案列表 |
| **设备选择器** | 设备/模拟器选择器 |

---

## 五、功能矩阵

| 功能 | 优先级 | 复杂度 | AI 价值 | 状态 |
|------|--------|--------|---------|------|
| 智能 Snippets | P0 | 低 | 高 | ✅ 已实现 (26 个) |
| Quick Fix Commands | P0 | 低 | 高 | ✅ 已实现 (6/7) |
| Code Lens - 调用计数 | P1 | 中 | 极高 | ✅ 已实现 |
| Decorator - Retain Cycle | P1 | 中 | 高 | ✅ 已实现 |
| Tree View - Class Browser | P2 | 中 | 中 | ✅ 已实现 |
| Webview - Call Graph | P2 | 高 | 极高 | ✅ 已实现 |
| Hover 扩展 | P3 | 低 | 中 | ✅ 已实现 |
| Test Explorer | P3 | 高 | 中 | ❌ 未实现 |
---

## 六、技术实现要点

### 6.1 调用链信息获取

LSP 不支持 Call Hierarchy，但扩展可以：
1. **复用 LSP references**: 遍历所有引用位置
2. **扩展 LSP 私有协议**: `objc-lsp/callGraph` 返回完整调用图
3. **静态分析**: 使用 tree-sitter 解析调用模式

### 6.2 Retain Cycle 检测

1. **模式匹配**: `self` 在 block 内被强引用
2. **属性分析**: delegate 属性是否为 strong
3. **装饰器展示**: 在问题代码处显示警告图标

### 6.3 Webview 通信

```
Extension                    Webview
   |                            |
   |-- sendMessage(data) ----->|
   |<---- postMessage(response)-|
   |                            |
```

---

## 七、总结

- **P0 功能** ✅ 全部实现 — 26 个代码片段 + 6 个 Quick Fix 命令
- **P1 功能** ✅ 全部实现 — Code Lens（4 类）+ Decorators（5 类）
- **P2 功能** ✅ 大部分实现 — Symbols Outline Pro、Class Browser、Call Graph Webview
- **P3 功能** 🔶 部分实现 — Hover 扩展 ✅，Test Explorer ❌，调试集成 ❌

### 额外修复
- ✅ Document Symbol 回退机制 — tree-sitter 解析失败时使用 regex 提取符号（commit `594a252`）
- ✅ 字符串语义高亮修复 — 跳过 ERROR 节点防止字符串内容被错误标记为变量
- ✅ Hover 扩展 — 扩展端 HoverProvider：相关方法、API 可用性、弃用警告、快速修复链接

建议后续按需实现 P3 功能。
