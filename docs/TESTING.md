# objc-lsp 测试体系

> 最后更新：2026-03-04

---

## 概览

测试分两层：Rust 单元/集成测试覆盖语言服务器核心逻辑，VS Code GUI 测试覆盖最高频的编辑器端用户场景。

| 层级 | 框架 | 用例数 | 运行命令 |
|---|---|---|---|
| Rust 单元/集成 | `cargo test` | 64 | `cargo test --workspace` |
| VS Code GUI | `vscode-extension-tester` | 7 | `npm run test:ui`（在 `editors/vscode/` 下） |

---

## 第一层：Rust 单元/集成测试

### 运行

```bash
# 全量
cargo test --workspace

# 单个 crate
cargo test -p objc-syntax
cargo test -p objc-intelligence
cargo test -p objc-semantic
```

### 覆盖范围

| Crate | 测试内容 |
|---|---|
| `objc-syntax` | Tree-sitter 解析、文档符号提取、语义 token、折叠范围 |
| `objc-intelligence` | `@property` 生成逻辑、selector 引擎、protocol 分析、代码动作 |
| `objc-semantic` | libclang hover、补全、诊断、goto-def、引用、重命名、格式化 |
| `objc-project` | `.xcodeproj` / pbxproj 解析、SDK 路径检测、compile_commands.json |
| `objc-store` | SQLite 符号索引、跨引用图、workspace symbol 查询 |

这一层不依赖编辑器，测试速度快，是核心逻辑的主要保障。

---

## 第二层：VS Code GUI 测试

### 运行

```bash
cd editors/vscode
npm run test:ui
```

首次运行会自动下载 VS Code 1.109.5 和对应的 ChromeDriver，缓存后复用。

### 框架

[vscode-extension-tester](https://github.com/redhat-developer/vscode-extension-tester)：驱动真实 VS Code 实例，通过 ChromeDriver 操作 DOM，验证扩展在真实环境中的端到端行为。

### 测试用例

| 文件 | 用例 | 覆盖场景 |
|---|---|---|
| `addProperty.test.ts` | `NSString *_name;` → `@property (nonatomic, copy) NSString *name;` | `ObjC: Generate @property from ivar` 命令，copy 语义 |
| `addProperty.test.ts` | `NSInteger _count;` → `@property (nonatomic, assign) NSInteger count;` | `ObjC: Generate @property from ivar` 命令，assign 语义 |
| `decorators.test.ts` | retain-cycle warning decoration（line 16，`self` in block） | `after.contentText` 装饰器渲染 |
| `decorators.test.ts` | magic-number decoration（line 14，字面量 `42`） | 下划线装饰器渲染 |
| `codelens.test.ts` | UITestFixture.m 中至少一个 Code Lens | Code Lens 基础可用性 |
| `codelens.test.ts` | 方法声明上方存在 `references` Code Lens | 引用计数 Code Lens |
| `codelens.test.ts` | `@implementation` 上的 protocol-conformance Code Lens | Protocol 一致性 Code Lens |

### Fixture 文件

所有 GUI 测试共用一个 fixture：

```
editors/vscode/test/fixtures/workspace/UITestFixture.m
```

23 行，包含：
- ivar 声明（`NSString *_name;`、`NSInteger _count;`）供 addProperty 命令测试
- magic number（`42`）供 decorator 测试
- `dispatch_async` block 中的裸 `self` 供 retain-cycle decorator 测试
- protocol 实现供 Code Lens 测试

每个 `addProperty` 测试用例执行后通过 `workbench.action.revertFile` 还原 fixture，保证测试间互不干扰。

### 辅助函数（`helpers.ts`）

| 函数 | 说明 |
|---|---|
| `openAndWaitForContent(path, title)` | 打开文件并等待 Monaco 编辑器就绪 |
| `gotoLine(n)` | 通过 `Cmd+P` + `:N` 跳转到指定行（macOS 上 `Ctrl+G` 无效） |
| `runObjcCommand(label)` | 通过命令面板精确匹配并执行 ObjC 扩展命令 |
| `getLineText(n)` | 读取指定行的 DOM 文本内容 |
| `waitForCodeLenses()` | 等待 Code Lens 出现（带超时） |
| `safeCloseAllEditors()` | 测试结束后关闭所有编辑器 |
| `dismissModals()` | 关闭可能弹出的模态对话框 |

---

## 已知技术细节（踩坑记录）

在 VS Code 1.109.5 / macOS 环境下发现的问题及解法：

### Monaco DOM 中的非断行空格

Monaco 在 DOM 中渲染文本时，token 之间使用 `\u00a0`（U+00A0，非断行空格，char code 160）而非普通空格（U+0020，char code 32）。直接用 `.includes("@property ...")` 比较会失败。

**解法**：比较前先归一化：

```typescript
const cleanLine = transformedLine
  .replace(/\u00a0/g, ' ')
  .replace(/[\u200b\u200c\u200d\ufeff]/g, '')
  .trim();
```

### `after.contentText` 装饰器不在 DOM 文本节点中

VS Code 的 `setDecorations` 中 `after.contentText` 选项通过 CSS `::after` 伪元素渲染，**不是**真实的 DOM 文本节点。XPath `contains(., 'text')`、`TreeWalker`、`textContent` 均无法检测到它。

**解法**：使用 `getComputedStyle` 读取伪元素的 CSS `content` 属性：

```javascript
const spans = document.querySelectorAll('.view-lines span');
for (const span of spans) {
  const style = window.getComputedStyle(span, '::after');
  const content = style.getPropertyValue('content');
  if (content && content.replace(/['"]/g, '').includes('retain cycle')) return true;
}
```

注意：`getPropertyValue('content')` 返回值带引号（如 `" ⚠️ retain cycle"`），需去掉引号再比较。

### macOS 上 Go-to-Line 快捷键

`Ctrl+G` 在 macOS VS Code 中打开的是文件搜索（Quick Open），而非跳行。

**解法**：使用 `Cmd+P` + `:行号` 的方式跳转：

```typescript
// helpers.ts gotoLine() 实现原理
await workbench.openCommandPrompt();   // Cmd+P
await input.setText(`:${lineNumber}`);
await input.confirm();
```

### `window.monaco` 不可访问

在 vscode-extension-tester 的 WebDriver 上下文中，`window.monaco` 被 VS Code 限制，无法通过 `executeScript` 访问。不应将其作为主要验证手段。

---

## 当前覆盖空白

| 场景 | 状态 |
|---|---|
| Zed 扩展 | ❌ 无自动化测试 |
| LSP 协议层端到端（JSON-RPC over stdio） | ❌ 未覆盖 |
| hover / completion / goto-def GUI 验证 | ❌ 未覆盖 |
| 多文件跨引用场景 | ❌ 未覆盖 |
| Linux 平台 CI | ❌ 未配置 |
