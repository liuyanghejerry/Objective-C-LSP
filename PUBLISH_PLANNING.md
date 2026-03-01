# Publishing Planning — Platform-Specific Closed-Source Distribution

## Overview

objc-lsp 早期采用**闭源发布**策略：TypeScript 扩展代码保持开源（用户可审查扩展与 LSP 的通信方式），Rust LSP 二进制仅以编译形式分发。

发布机制基于 VS Code 的 **Platform-Specific Extensions** 功能 —— 为每个目标平台生成独立的 `.vsix`，Marketplace 根据用户平台自动提供匹配版本。这是 rust-analyzer、clangd 等主流 LSP 扩展的标准做法。

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

## Files Changed

| File | Change | Description |
|---|---|---|
| `editors/vscode/package.json` | Modified | `license` → `"SEE LICENSE IN LICENSE"` |
| `editors/vscode/.vscodeignore` | Rewritten | 排除源码，保留 `dist/`、`bin/` |
| `editors/vscode/.gitignore` | Modified | 添加 `bin/` |
| `editors/vscode/LICENSE` | **New** | 闭源许可证文本 |
| `.github/workflows/release.yml` | **New** | 完整 CI/CD 流水线 |
| `editors/vscode/src/install.ts` | **No change** | 已有的 `bin/objc-lsp` 查找逻辑完美适配 |
