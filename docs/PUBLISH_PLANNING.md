# Publishing Planning — Platform-Specific Distribution

## Overview

objc-lsp 早期采用**闭源发布**策略：编辑器扩展代码保持开源（用户可审查扩展与 LSP 的通信方式），Rust LSP 二进制仅以编译形式分发。

- **VS Code**：基于 VS Code 的 **Platform-Specific Extensions** 功能，为每个目标平台生成独立的 `.vsix`，Marketplace 根据用户平台自动提供匹配版本。这是 rust-analyzer、clangd 等主流 LSP 扩展的标准做法。
- **Zed**：扩展代码（Rust → WASM）以 MIT 开源发布（Zed Marketplace 要求），LSP 二进制通过扩展在运行时从 GitHub Releases 自动下载（保持闭源）。

## Target Platforms

| `vsce --target` | Rust target | Runner | 说明 |
|---|---|---|---|
| `darwin-arm64` | `aarch64-apple-darwin` | `macos-14` | macOS Apple Silicon（主力开发平台） |
| `darwin-x64` | `x86_64-apple-darwin` | `macos-13` | macOS Intel |
| `linux-x64` | `x86_64-unknown-linux-gnu` | `ubuntu-latest` | Linux x86_64（GNUstep 用户） |
| `linux-arm64` | `aarch64-unknown-linux-gnu` | `ubuntu-latest` (cross) | Linux ARM64（交叉编译） |

> Windows 不在目标范围 —— Objective-C 开发几乎不在 Windows 上进行。后续可按需添加。

## VSIX Internal Structure

每个平台的 `.vsix` 包含**该平台对应的唯一一个二进制文件**：

```
editors/vscode/            # .vsix 打包根目录
├── package.json
├── LICENSE                # 闭源许可证
├── dist/
│   └── extension.js       # esbuild 打包的 TypeScript 扩展
├── bin/
│   └── objc-lsp           # 平台特定的预编译 Rust 二进制（仅一个）
├── syntaxes/
│   └── objc.tmLanguage.json
├── snippets/
│   └── objc.json
└── language-configuration.json
```

`install.ts` 中已有的查找逻辑完美适配此结构：

```typescript
// 已有代码 — 无需改动
const bundled = path.join(context.extensionPath, "bin", "objc-lsp");
if (fs.existsSync(bundled)) {
  return bundled;
}
```

## Licensing Strategy

| 组件 | 许可证 | 分发方式 |
|---|---|---|
| TypeScript 扩展代码 (`editors/vscode/src/`) | MIT（开源） | 源码公开在 GitHub |
| Rust LSP 二进制 (`bin/objc-lsp`) | 闭源 | 仅以编译形式分发 |
| TextMate 语法、Snippets | MIT（开源） | 源码公开在 GitHub |
|
| Zed 扩展代码 (`editors/zed/src/`) | MIT（开源） | 源码公开在 GitHub，编译为 WASM |
| Zed Tree-sitter 查询文件 (`editors/zed/languages/`) | MIT（开源） | 源码公开在 GitHub |

`package.json` 的 `license` 字段改为 `"SEE LICENSE IN LICENSE"`，指向扩展目录内的 `LICENSE` 文件。

## Publishing Channels

### Phase 1: Early Access (Alpha/Beta)

- **渠道**：GitHub Releases
- **优点**：完全控制分发、无需审核、支持 pre-release 标记
- **用户安装方式**：

```bash
# 下载对应平台的 .vsix
curl -LO https://github.com/aspect-build/objc-lsp/releases/download/v0.1.0/objc-lsp-darwin-arm64-0.1.0.vsix

# 安装
code --install-extension objc-lsp-darwin-arm64-0.1.0.vsix
```

### Phase 2: Public Release

- **渠道**：VS Code Marketplace
- **优点**：用户直接搜索安装、自动更新、平台自动匹配
- **要求**：Azure DevOps publisher 账号 + Personal Access Token (PAT)
- 只需将 CI 中的 GitHub Release 步骤替换为 `vsce publish --packagePath *.vsix`

## CI/CD Pipeline Design

### Release Flow

```
git tag v0.1.0 → push tag → GitHub Actions triggers:

┌─────────────────────────────────────────────────────────┐
│  Job: build-binary (4 parallel jobs)                     │
│                                                          │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐     │
│  │ darwin-arm64  │ │ darwin-x64   │ │ linux-x64    │ ... │
│  │ macos-14      │ │ macos-13     │ │ ubuntu       │     │
│  │ cargo build   │ │ cargo build  │ │ cargo build  │     │
│  │ --release     │ │ --release    │ │ --release    │     │
│  └──────┬───────┘ └──────┬───────┘ └──────┬───────┘     │
│         │                │                │              │
│         ▼                ▼                ▼              │
│  upload-artifact   upload-artifact  upload-artifact      │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│  Job: package-vsix (4 parallel jobs, needs build-binary) │
│                                                          │
│  For each platform:                                      │
│    1. npm ci + esbuild (build extension JS)              │
│    2. download-artifact (get that platform's binary)     │
│    3. chmod +x bin/objc-lsp                              │
│    4. vsce package --target <platform> --no-dependencies │
│    5. upload .vsix artifact                              │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│  Job: publish (needs package-vsix)                       │
│                                                          │
│  1. Download all .vsix artifacts                         │
│  2. Create GitHub Release with .vsix files attached      │
│  3. (Optional) vsce publish to Marketplace               │
└─────────────────────────────────────────────────────────┘
```

### Versioning

- 版本号在 `editors/vscode/package.json` 的 `version` 字段管理
- Git tag 格式：`v{version}`（如 `v0.1.0`、`v0.2.0-alpha.1`）
- Pre-release tag（含 `-alpha` 或 `-beta`）在 GitHub Release 中标记为 prerelease

### Special Considerations

#### libclang 动态链接

- **macOS**：libclang 由 Xcode 提供，`DYLD_LIBRARY_PATH` 在 `server.ts` 中已设置
- **Linux**：需要 `libclang-dev` 安装在构建环境中；发布的二进制通过 `clang-sys` 在运行时查找 libclang.so

#### Linux ARM64 交叉编译

- 使用 `ubuntu-latest` runner + `gcc-aarch64-linux-gnu` 交叉编译工具链
- 设置 `CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc`

## Zed Extension Distribution

### 架构差异

与 VS Code 不同，Zed 扩展**不内嵌** LSP 二进制：

- Zed 扩展是 Rust 编译为 **WebAssembly** (WASM) 的模块
- 扩展在首次启动时从 GitHub Releases **下载**对应平台的 `objc-lsp` 二进制
- 这意味着同一个 WASM 扩展可服务所有平台，无需平台特定的构建产物

### 许可证要求

Zed Extension Marketplace 要求扩展代码必须**开源**。我们的策略：

| 组件 | 许可证 | 说明 |
|---|---|---|
| 扩展代码 (`editors/zed/src/lib.rs`) | MIT | 实现 `Extension` trait，处理 LSP 下载/启动 |
| Tree-sitter 查询文件 (`languages/**/*.scm`) | MIT | 语法高亮、折叠、大纲等 |
| `extension.toml` + `Cargo.toml` | MIT | 扩展配置 |
| LSP 二进制 (`objc-lsp`) | 闭源 | 仅通过 GitHub Releases 分发编译产物 |

### 发布流程

```
git tag v0.1.0 → push tag → GitHub Actions triggers:

┌─────────────────────────────────────────────────────────┐
│  Job: build-zed (needs build-binary)                     │
│                                                          │
│  1. LSP 二进制已由 build-binary job 上传至 GitHub Release │
│  2. Zed 扩展无需打包二进制 — 运行时自动下载              │
│  3. 用户通过 Zed Extension Marketplace 安装扩展          │
│  4. 首次激活时，扩展从 Release 下载对应平台的 objc-lsp   │
└─────────────────────────────────────────────────────────┘
```

### 用户安装方式

**Phase 1（开发阶段）**：
```
# 安装为 Dev Extension
Open Zed → Extensions → Install Dev Extension → 选择 editors/zed 目录
```

**Phase 2（公开发布）**：
```
# 通过 Zed Extension Marketplace
Open Zed → Extensions → 搜索 "objc-lsp" → Install
```

### 发布到 Zed Marketplace 的前提

1. 扩展源码托管在公开 GitHub 仓库
2. 仓库包含开源许可证（MIT）
3. 提交 PR 至 [zed-industries/extensions](https://github.com/zed-industries/extensions) 注册扩展
4. Zed 团队审核后，扩展自动出现在 Marketplace

## Files Changed

### VS Code

| File | Change | Description |
|---|---|---|
| `editors/vscode/package.json` | Modified | `license` → `"SEE LICENSE IN LICENSE"` |
| `editors/vscode/.vscodeignore` | Rewritten | 排除源码，保留 `dist/`、`bin/` |
| `editors/vscode/.gitignore` | Modified | 添加 `bin/` |
| `editors/vscode/LICENSE` | **New** | 闭源许可证文本 |
| `.github/workflows/release.yml` | **New** | 完整 CI/CD 流水线（含 VS Code + Zed） |
| `editors/vscode/src/install.ts` | **No change** | 已有的 `bin/objc-lsp` 查找逻辑完美适配 |

### Zed

| File | Change | Description |
|---|---|---|
| `editors/zed/extension.toml` | **New** | 扩展清单，声明语言、grammar、LSP |
| `editors/zed/Cargo.toml` | **New** | WASM cdylib，依赖 `zed_extension_api = "0.5.0"` |
| `editors/zed/src/lib.rs` | **New** | Extension trait 实现，GitHub Releases 下载 LSP |
| `editors/zed/LICENSE` | **New** | MIT 许可证 |
| `editors/zed/languages/objective-c/*.scm` | **New** | 9 个 Tree-sitter 查询文件 |
| `editors/zed/languages/objective-cpp/*.scm` | **New** | Symlinks → `objective-c/` |
| `.cargo/config.toml` | Modified | rustflags 限定为 native target |
| `Cargo.toml` | Modified | workspace exclude `editors/zed` |
