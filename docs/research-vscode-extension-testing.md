# VS Code 扩展自动化测试可行性调研

**分支**: `research/vscode-extension-testing`  
**日期**: 2026-03-03

---

## 背景

目前 `objc-lsp` 的 Rust LSP 服务器已有基础单元测试（133 个，`cargo test --workspace`），但 VS Code 扩展侧（`editors/vscode/`）完全依赖手动测试。本报告调研为扩展引入自动化测试的可行性、选型及落地路径。

---

## 现状分析

### 代码结构

```
editors/vscode/src/
├── extension.ts      # activate/deactivate 入口
├── server.ts         # LSP 客户端生命周期（startClient/stopClient）
├── config.ts         # 配置读取
├── commands.ts       # 快捷命令（6 个）
├── codelens.ts       # CodeLens provider（方法引用计数、pragma mark）
├── decorators.ts     # 文本装饰（retain cycle、strong delegate、magic number）
├── hover.ts          # Hover 扩展
├── treeviews.ts      # Symbols Outline / Class Browser
├── callgraph.ts      # Call Graph webview
└── install.ts        # 二进制安装/发现
```

### 现有测试空缺

| 模块 | 逻辑复杂度 | 当前测试 |
|---|---|---|
| `decorators.ts` — retain cycle 检测、strong delegate 检测、magic number | **高**（正则 + brace 匹配算法） | ❌ 无 |
| `codelens.ts` — protocol map 解析、方法匹配 | **中** | ❌ 无 |
| `config.ts` — 配置读写 | 低 | ❌ 无 |
| `server.ts` — LSP 客户端启动/停止 | **高**（外部进程依赖） | ❌ 无 |
| `commands.ts` — 6 个快捷命令 | 中 | ❌ 无 |
| `install.ts` — 二进制发现逻辑 | 中 | ❌ 无 |

`package.json` 中无任何 `test` script，无测试依赖。

---

## 测试方案比较

### 方案 A：`@vscode/test-electron` + Mocha（官方推荐路径）

**原理**：在 Extension Development Host（EDH，即一个真实的 VS Code 进程）中运行测试。测试代码通过 `@vscode/test-electron` 拉起指定版本的 VS Code，加载扩展，执行 Mocha 测试套件。

```
test/
├── runTest.ts          ← 调用 runTests({ extensionDevelopmentPath, extensionTestsPath })
├── suite/
│   ├── index.ts        ← Mocha runner
│   ├── decorators.test.ts
│   └── codelens.test.ts
```

| 维度 | 评价 |
|---|---|
| 官方支持 | ✅ 官方维护，文档完善 |
| VS Code API 可用性 | ✅ 完整 `vscode.*` API |
| 执行速度 | ❌ 需下载/启动完整 VS Code 实例，首次慢 |
| CI（GitHub Actions Linux） | ⚠️ 需要 `xvfb-run`（虚拟显示服务器） |
| CI（macOS） | ✅ 无需额外配置 |
| 生产验证 | ✅ rust-analyzer、Python 等主流扩展均使用此路径 |

**npm 包**: `@vscode/test-electron`, `mocha`, `@types/mocha`

---

### 方案 B：`@vscode/test-cli`（2024 引入的 CLI 驱动路径）

**原理**：在方案 A 基础上加了一个 CLI 层（`vscode-test` 命令），通过 `.vscode-test.mjs` 配置文件描述测试套件和 VS Code 版本。底层仍是 test-electron。

```js
// .vscode-test.mjs
export default {
  files: 'out/test/**/*.test.js',
  version: 'stable',
};
```

| 维度 | 评价 |
|---|---|
| 配置能力 | ✅ 更清晰，支持多套件 |
| 与方案 A 兼容 | ✅ 可共存，不互斥 |
| 成熟度 | ⚠️ 2024 年引入，较新 |
| 适合场景 | 多测试套件（单元 + 集成）并行管理 |

**推荐**：方案 A 和 B 可以组合使用——用 test-cli 作为 CLI 入口，底层仍是 test-electron 运行时。

---

### 方案 C：Jest（非官方路径）

**原理**：通过 `jest-runner-vscode` 等社区包将 Jest 嵌入 EDH，或绕过 EDH 做纯 Node.js 单元测试。

| 维度 | 评价 |
|---|---|
| VS Code API 可用性 | ❌ 绕过 EDH 则无法使用 `vscode.*` |
| 官方支持 | ❌ 无官方支持 |
| 社区成熟度 | ⚠️ 工具链碎片化 |
| 适合场景 | 纯逻辑函数（无 vscode API 依赖） |

**结论**：不推荐作为主路径。`decorators.ts` 中的纯逻辑函数可以用 Jest/Node.js 单测，但需要与方案 A 并行维护两套测试基础设施，得不偿失。

---

## LSP 客户端测试的特殊考量

`server.ts` 的 `startClient` 需要：
1. 一个真实的 `objc-lsp` 二进制文件
2. 一个有 `.m`/`.xcodeproj` 的工作区

这给自动化测试带来了依赖链。业界有两种应对策略：

### 策略 1：Mock LSP Server

在测试中启动一个最小的 JSON-RPC/stdio 服务器，返回预设响应。

```typescript
// test/support/mockServer.ts
import { createConnection, ProposedFeatures } from 'vscode-languageserver/node';

// 响应 initialize → 返回固定 capabilities
// 响应 textDocument/hover → 返回固定 hover 内容
```

- ✅ 快速、确定性、无外部依赖
- ✅ 可在 CI 中无缝运行
- ✅ 适合测试客户端初始化流程、消息序列化、错误处理
- ❌ 不验证真实服务器行为

**参考实现**：
- [`octref/vscode-language-server-e2e-test`](https://github.com/octref/vscode-language-server-e2e-test)
- [`rockerBOO/mock-lsp-server`](https://github.com/rockerBOO/mock-lsp-server)

### 策略 2：真实 LSP 服务器集成测试

在 CI 中编译 `objc-lsp` 二进制并在测试中启动，对真实 `.m` fixture 文件执行端到端验证。

- ✅ 最高保真度，能捕捉服务器/客户端协议不匹配问题
- ❌ 依赖 Rust 编译、libclang、macOS 环境
- ❌ 适合 macOS-only CI job，Linux 支持受限

---

## CI/CD 可行性

### GitHub Actions — Linux (`ubuntu-latest`)

```yaml
- name: Install dependencies
  run: npm ci
  working-directory: editors/vscode

- name: Compile tests
  run: npx tsc -p tsconfig.test.json
  working-directory: editors/vscode

- name: Run VS Code extension tests
  run: xvfb-run -a npm test
  working-directory: editors/vscode
  env:
    DISPLAY: ':99.0'
```

**关键点**：Linux CI 必须通过 `xvfb-run` 提供虚拟显示，否则 Electron 无法启动。`ubuntu-latest` runner 默认已安装 `xvfb`，只需 `xvfb-run -a` 包装命令即可。

### GitHub Actions — macOS (`macos-latest`)

无需虚拟显示，直接运行。适合需要真实 LSP 二进制（libclang）的端到端测试。

---

## 推荐落地方案

### 分层测试策略

```
Layer 1: 纯逻辑单元测试（Node.js，无 vscode 依赖）
  ├── decorators.ts 中的 findRetainCycles()、findStrongDelegates()、findMagicNumbers()
  ├── codelens.ts 中的 buildProtocolMap()
  └── install.ts 中的二进制发现逻辑
  工具：Mocha + ts-node（在 EDH 内也可运行，统一工具链）

Layer 2: 集成测试（EDH，Mock LSP Server）
  ├── 扩展 activate/deactivate 生命周期
  ├── LSP 客户端初始化握手（使用 Mock Server）
  ├── 命令注册验证
  └── CodeLens provider provideCodeLenses() 输出
  工具：@vscode/test-electron + Mocha + Mock LSP Server

Layer 3: 端到端测试（仅 macOS CI，真实二进制）[可选]
  ├── 真实工作区打开 → LSP 启动 → hover 请求
  └── 诊断推送到 VS Code Problems 面板
  工具：@vscode/test-electron + 真实 objc-lsp binary
```

### 实施优先级

| 优先级 | 内容 | 工作量估算 |
|---|---|---|
| P0 | `decorators.ts` 纯逻辑单元测试（findRetainCycles 等）| 1–2 天 |
| P0 | `codelens.ts` `buildProtocolMap` 单元测试 | 0.5 天 |
| P1 | 搭建 `@vscode/test-electron` + Mocha 基础框架 | 1 天 |
| P1 | 扩展激活/命令注册集成测试 | 1 天 |
| P2 | Mock LSP Server 实现 + 客户端初始化测试 | 2–3 天 |
| P3 | GitHub Actions workflow（Linux + macOS） | 0.5 天 |
| P4（可选）| 真实 LSP 端到端测试（macOS only） | 3–5 天 |

---

## 结论

**可行性：高。**

最高价值的切入点是 `decorators.ts` 中的纯逻辑函数——`findRetainCycles()`、`findStrongDelegates()`、`findMagicNumbers()` 均为独立的纯函数，仅依赖 `vscode.TextDocument` 接口，极易单测，且目前没有任何测试覆盖。这是风险最低、收益最快的起点。

次优先是搭建 `@vscode/test-electron` + Mocha 框架，为后续集成测试打基础。Mock LSP Server 路径是客户端协议测试的可行解，不依赖 Rust 编译或 macOS 特定环境，Linux CI 可全量运行。

**推荐技术栈**：
- `@vscode/test-electron` + `@vscode/test-cli` + Mocha（集成测试主框架）
- `vscode-languageserver/node`（Mock LSP Server 实现）
- `xvfb-run`（Linux CI 虚拟显示）
- GitHub Actions matrix（linux + macOS）

---

## 参考资料

- [VS Code: Testing Extensions](https://code.visualstudio.com/api/working-with-extensions/testing-extension)
- [VS Code: Continuous Integration](https://code.visualstudio.com/api/working-with-extensions/continuous-integration)
- [@vscode/test-electron](https://www.npmjs.com/package/@vscode/test-electron)
- [@vscode/test-cli](https://www.npmjs.com/package/@vscode/test-cli)
- [microsoft/vscode-test](https://github.com/microsoft/vscode-test)
- [octref/vscode-language-server-e2e-test](https://github.com/octref/vscode-language-server-e2e-test)
- [rockerBOO/mock-lsp-server](https://github.com/rockerBOO/mock-lsp-server)
- [Electron: Testing on Headless CI](https://electronjs.org/docs/latest/tutorial/testing-on-headless-ci)
