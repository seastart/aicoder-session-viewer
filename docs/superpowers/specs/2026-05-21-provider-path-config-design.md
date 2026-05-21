# 自定义 Provider 路径 — 设计文档

- 日期：2026-05-21
- 关联 issue：[seastart/aicoder-session-viewer#1](https://github.com/seastart/aicoder-session-viewer/issues/1)
- 目标：让用户可在应用内为四个 provider（Claude Code / Codex / Gemini / OpenCode）指定自定义数据源路径，覆盖内置默认值。

## 背景

当前四个 provider 的数据源路径在 `src-tauri/src/providers/*.rs::new()` 中硬编码（如 `~/.claude`、`~/.local/share/opencode/opencode.db`）。Issue #1 提出场景：有用户在 OpenCode 之上构建衍生工具，数据库不在默认位置，希望能让 viewer 读取自定义路径。

## 范围

### 目标

1. 每个 provider 支持单一可选路径覆盖。
2. 提供应用内设置 UI 编辑路径，保存后立即生效（无需重启）。
3. 配置持久化到平台标准配置目录。

### 非目标

- 不支持同一 provider 多路径（一个 provider 一份数据源）。
- 不支持显式「禁用 provider」开关（路径设成不存在自然跳过即可）。
- 不做配置版本号 / 迁移机制（首版无历史包袱）。

## 架构

### 配置文件

- 路径：`{tauri config_dir}/aicoder-session-viewer/config.json`
  - macOS：`~/Library/Application Support/aicoder-session-viewer/config.json`
  - 通过 Tauri `path::config_dir()` 跨平台获取。
- Schema（所有字段可选；`null` 或缺失 = 走 provider 内置默认）：
  ```json
  {
    "providerPaths": {
      "claudeCode": null,
      "codex": null,
      "gemini": null,
      "opencode": "/Users/me/.local/share/my-tool/opencode.db"
    }
  }
  ```

### 新增模块：`src-tauri/src/config.rs`

职责：纯文件读写 + 序列化，不负责默认值。

```rust
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub provider_paths: ProviderPaths,
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPaths {
    pub claude_code: Option<PathBuf>,
    pub codex: Option<PathBuf>,
    pub gemini: Option<PathBuf>,
    pub opencode: Option<PathBuf>,
}

pub fn config_file_path(app: &AppHandle) -> AppResult<PathBuf>;
pub fn load(app: &AppHandle) -> ProviderConfig;  // 读不到/损坏 → Default
pub fn save(app: &AppHandle, cfg: &ProviderConfig) -> AppResult<()>;
```

### Provider 改造

每个 `Provider::new()` 签名改为：

```rust
pub fn new(path_override: Option<PathBuf>) -> AppResult<Self>
```

内部逻辑：
- `path_override.is_some()` → 使用传入路径
- `path_override.is_none()` → 沿用当前的 `dirs::home_dir()` + 默认子路径

每个 provider 额外暴露一个公开静态方法 `default_path() -> AppResult<PathBuf>`，供 UI 显示「默认值预览」（如「默认: ~/.claude」）。

### Registry 改造

- `tauri::Builder::manage()` 由 `ProviderRegistry` 改为 `Arc<RwLock<ProviderRegistry>>`。
- 新增 `pub fn reload(&mut self, paths: &ProviderPaths) -> Vec<String>`：
  - 重新初始化四个 provider，沿用现有「失败静默跳过」语义。
  - 返回失败原因列表（用作 `warnings` 给前端展示）。
- 所有 IPC 命令通过读锁访问 registry；只有 `update_provider_config` 持写锁。

## IPC API

新增两条命令，注册在 `lib.rs`：

```rust
#[tauri::command]
fn get_provider_config(app: AppHandle) -> Result<ProviderConfigResponse>;

#[tauri::command]
fn update_provider_config(
    app: AppHandle,
    registry: State<Arc<RwLock<ProviderRegistry>>>,
    config: ProviderConfig,
) -> Result<UpdateProviderConfigResponse>;
```

返回类型：

```rust
struct ProviderConfigResponse {
    config: ProviderConfig,
    defaults: ProviderPaths,  // 各 provider 的默认路径（始终 Some）
}

struct UpdateProviderConfigResponse {
    warnings: Vec<String>,  // reload 期间失败的 provider 错误信息
}
```

`update_provider_config` 行为：
1. `config::save(&app, &config)`（失败直接返回 Err，不动内存状态）
2. `registry.write().reload(&config.provider_paths)` → 收集 warnings
3. 返回 warnings

## 前端

### 组件：`src/components/SettingsDialog.tsx`

使用现有 shadcn `Dialog` 组件，与 `ResumeDialog` 同一套风格。

布局：

```
┌─ 设置 - 数据源路径 ──────────────────────┐
│                                          │
│  Claude Code                             │
│  [输入框........................] [浏览] │
│  默认: ~/.claude              [重置]     │
│                                          │
│  Codex                                   │
│  [输入框........................] [浏览] │
│  默认: ~/.codex               [重置]     │
│                                          │
│  Gemini                                  │
│  [输入框........................] [浏览] │
│  默认: ~/.gemini              [重置]     │
│                                          │
│  OpenCode                                │
│  [输入框........................] [浏览] │
│  默认: ~/.local/share/opencode/...       │
│                                 [重置]   │
│                                          │
│              [取消]  [保存]              │
└──────────────────────────────────────────┘
```

交互细节：
- 「浏览」：调 `@tauri-apps/plugin-dialog` 的 `open()`。claude/codex/gemini 选目录（`directory: true`），opencode 选文件（`filters: [{ name: "SQLite", extensions: ["db", "sqlite"] }]`）。
- 「重置」：清空对应输入框（提交时即 `null`）。
- 路径存在性提示（可选增强）：输入框失焦后调一次 fs check，不存在则灰色文字提示「该路径目前不存在」；不阻塞保存。
- 「保存」：调 `update_provider_config`，成功后：
  - 若 `warnings` 非空 → Toast 显示每条警告。
  - 触发 `useSessionStore.getState().loadSessions()` 刷新列表。
  - 关闭 Dialog。

### 入口

在标题栏（`App.tsx` 或对应 header 组件）已有的图标按钮区域加一个齿轮 `<Settings />`（lucide-react），点击打开 `SettingsDialog`。

### Zustand store 扩展

`stores/sessionStore.ts` 增加：

```ts
providerConfig: ProviderConfig | null;
providerDefaults: ProviderPaths | null;

loadProviderConfig: () => Promise<void>;
saveProviderConfig: (config: ProviderConfig) => Promise<string[]>;  // 返回 warnings
```

### 前端类型

`src/types.ts` 同步增加：

```ts
export interface ProviderPaths {
  claudeCode: string | null;
  codex: string | null;
  gemini: string | null;
  opencode: string | null;
}

export interface ProviderConfig {
  providerPaths: ProviderPaths;
}
```

## 错误处理

| 场景 | 行为 |
|------|------|
| 配置文件不存在 | `config::load()` 返回 `Default`，应用照常启动 |
| 配置文件 JSON 损坏 / 读不动 | 打日志，`config::load()` 返回 `Default`，不阻塞启动 |
| 配置文件写不动（权限/磁盘） | `update_provider_config` 返回 `Err`，前端 Toast 报错，内存 registry 不变 |
| Provider 初始化失败（路径不存在等） | 沿用现有静默跳过；错误信息进 `warnings` 返回给前端 |
| 并发读写 | `Arc<RwLock<ProviderRegistry>>` 保证 reload 期间读请求阻塞，不会读到半成品 |

## 测试

### Rust 单元测试

- `config.rs`
  - `load` → `save` → `load` 往返一致
  - 损坏 JSON 文件返回 `Default`
  - 缺字段的 JSON 能正常反序列化（向前兼容）
- `providers/*.rs`（四个 provider 各一）
  - `new(Some(custom_path))` 走传入值
  - `new(None)` 走默认路径（用 `default_path()` 比对）

### 手动验收清单

写进 README 一节「自定义路径」：

1. 全新安装，无 `config.json` → 行为与 v0.1.10 完全一致。
2. 设置 OpenCode 为自定义路径 → 列表显示该路径下的 session，默认 opencode.db 不再显示。
3. 设置 OpenCode 为乱填值 → Toast 弹「OpenCode: database not found: /xxx」，另外三个 provider 仍工作。
4. 在 Settings 内改路径 → 不重启 app，session 列表立即刷新。
5. 关掉 app 再开 → 配置持久化生效。

## 文件改动总览

新增：
- `src-tauri/src/config.rs`
- `src/components/SettingsDialog.tsx`
- `docs/superpowers/specs/2026-05-21-provider-path-config-design.md`（本文）

修改：
- `src-tauri/src/lib.rs` —— 注册新模块、新 IPC、registry 改为 `Arc<RwLock<…>>`
- `src-tauri/src/commands.rs` —— 现有命令读锁取 registry；新增两条命令
- `src-tauri/src/providers/mod.rs` —— `ProviderRegistry::reload`
- `src-tauri/src/providers/{claude,codex,gemini,opencode}.rs` —— `new(Option<PathBuf>)` + `default_path()`
- `src/types.ts` —— `ProviderConfig`、`ProviderPaths`
- `src/stores/sessionStore.ts` —— `loadProviderConfig` / `saveProviderConfig`
- `src/App.tsx`（或 header 组件） —— 齿轮入口
- `README.md` —— 自定义路径章节

## 翻译/i18n

设置面板新增以下文案 key（中/英）：

```
settings.title           设置 / Settings
settings.providerPaths   数据源路径 / Data Source Paths
settings.browse          浏览 / Browse
settings.reset           重置 / Reset
settings.save            保存 / Save
settings.cancel          取消 / Cancel
settings.default         默认 / Default
settings.pathNotFound    该路径目前不存在 / Path does not exist
toast.configSaved        配置已保存 / Configuration saved
toast.providerWarning    {provider} 加载失败: {message}
```
