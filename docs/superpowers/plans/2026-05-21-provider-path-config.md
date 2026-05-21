# 自定义 Provider 路径 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让用户在应用内为 Claude Code / Codex / Gemini / OpenCode 四个 provider 指定自定义数据源路径，保存后立即生效。

**Architecture:** 新增 `src-tauri/src/config.rs` 负责 JSON 配置读写；四个 provider 的 `new()` 改为接受 `Option<PathBuf>`（`None` 走内置默认）；`ProviderRegistry` 用 `Arc<RwLock<...>>` 管理以支持热重载；前端加 `SettingsDialog`。

**Tech Stack:** Rust (Tauri 2 / serde / rusqlite) · TypeScript (React 19 / Zustand 5 / Tailwind 4) · Tauri 平台 API (`path::config_dir`, `dialog::open`)

**关联设计文档:** `docs/superpowers/specs/2026-05-21-provider-path-config-design.md`

---

## 文件改动总览

**新增:**
- `src-tauri/src/config.rs` — 配置读写模块
- `src/components/SettingsDialog.tsx` — 设置对话框

**修改:**
- `src-tauri/src/lib.rs` — 注册 `config` 模块、改 registry 为 `Arc<RwLock>`、注册新 IPC
- `src-tauri/src/commands.rs` — 所有 `State<ProviderRegistry>` 改为 `State<Arc<RwLock<ProviderRegistry>>>`，新增两条命令
- `src-tauri/src/providers/mod.rs` — `ProviderRegistry::new` 接受 `&ProviderPaths`，新增 `reload`
- `src-tauri/src/providers/claude.rs` · `codex.rs` · `gemini.rs` · `opencode.rs` — `new(Option<PathBuf>)`、`default_path()`
- `src/types.ts` — 新增 `ProviderConfig`、`ProviderPaths`
- `src/stores/sessionStore.ts` — 新增 `loadProviderConfig` / `saveProviderConfig` + 状态字段
- `src/components/Layout.tsx` — 标题栏加齿轮按钮 + 挂载 Dialog
- `src/i18n/locales/en.ts` · `zh.ts` · `src/i18n/index.ts` (Locale 接口) — 新增文案 key
- `README.md` — 「自定义路径」章节

---

### Task 1: Provider `new(Option<PathBuf>)` 重构 + `default_path()`

**Files:**
- Modify: `src-tauri/src/providers/claude.rs:33-45`
- Modify: `src-tauri/src/providers/codex.rs` (类似位置)
- Modify: `src-tauri/src/providers/gemini.rs` (类似位置)
- Modify: `src-tauri/src/providers/opencode.rs:22-34`

- [ ] **Step 1: 修改 `claude.rs::new` 签名 + 提取 `default_path`**

把现有 `new` 改为：

```rust
impl ClaudeCodeProvider {
    /// 返回默认的 ~/.claude 路径
    pub fn default_path() -> AppResult<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Provider("cannot locate home directory".into()))?;
        Ok(home.join(".claude"))
    }

    /// 创建 provider；`path_override` 为 None 时走默认路径
    pub fn new(path_override: Option<PathBuf>) -> AppResult<Self> {
        let base_dir = match path_override {
            Some(p) => p,
            None => Self::default_path()?,
        };
        if !base_dir.exists() {
            return Err(AppError::Provider(format!(
                "directory not found: {}",
                base_dir.display()
            )));
        }
        Ok(Self { base_dir })
    }
}
```

- [ ] **Step 2: 同样的改造应用到 `codex.rs`**

`codex.rs` 现有 `new`（位置类似 claude）改为：

```rust
impl CodexProvider {
    pub fn default_path() -> AppResult<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Provider("cannot locate home directory".into()))?;
        Ok(home.join(".codex"))
    }

    pub fn new(path_override: Option<PathBuf>) -> AppResult<Self> {
        let base_dir = match path_override {
            Some(p) => p,
            None => Self::default_path()?,
        };
        if !base_dir.exists() {
            return Err(AppError::Provider(format!(
                "directory not found: {}",
                base_dir.display()
            )));
        }
        Ok(Self { base_dir })
    }
}
```

- [ ] **Step 3: 同样改造 `gemini.rs`**

```rust
impl GeminiProvider {
    pub fn default_path() -> AppResult<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Provider("cannot locate home directory".into()))?;
        Ok(home.join(".gemini"))
    }

    pub fn new(path_override: Option<PathBuf>) -> AppResult<Self> {
        let base_dir = match path_override {
            Some(p) => p,
            None => Self::default_path()?,
        };
        if !base_dir.exists() {
            return Err(AppError::Provider(format!(
                "directory not found: {}",
                base_dir.display()
            )));
        }
        Ok(Self { base_dir })
    }
}
```

- [ ] **Step 4: 同样改造 `opencode.rs`（注意是文件而非目录）**

```rust
impl OpenCodeProvider {
    pub fn default_path() -> AppResult<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::Provider("cannot locate home directory".into()))?;
        Ok(home.join(".local/share/opencode/opencode.db"))
    }

    pub fn new(path_override: Option<PathBuf>) -> AppResult<Self> {
        let db_path = match path_override {
            Some(p) => p,
            None => Self::default_path()?,
        };
        if !db_path.exists() {
            return Err(AppError::Provider(format!(
                "database not found: {}",
                db_path.display()
            )));
        }
        Ok(Self { db_path })
    }
}
```

- [ ] **Step 5: 编译并修正引用**

在 `src-tauri/` 目录下运行：

```bash
cargo check
```

预期：编译失败，因为 `providers/mod.rs::ProviderRegistry::new()` 仍在调 `XxxProvider::new()`（无参数）。报错形如 `this function takes 1 argument but 0 arguments were supplied`。

临时让编译通过：把 `mod.rs:33-58` 内的四处 `XxxProvider::new()` 改为 `XxxProvider::new(None)`。这只是过桥，下一个 Task 会重写 `ProviderRegistry::new`。

```rust
if let Ok(p) = claude::ClaudeCodeProvider::new(None) {
    providers.push(Box::new(p));
    claude_provider = claude::ClaudeCodeProvider::new(None).ok();
}
if let Ok(p) = codex::CodexProvider::new(None) {
    providers.push(Box::new(p));
}
if let Ok(p) = gemini::GeminiProvider::new(None) {
    providers.push(Box::new(p));
}
if let Ok(p) = opencode::OpenCodeProvider::new(None) {
    providers.push(Box::new(p));
}
```

再次运行 `cargo check`，预期：编译通过。

- [ ] **Step 6: 加单元测试，验证 `default_path` 与覆盖路径都生效**

在 `claude.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_path_points_to_home_claude() {
        let p = ClaudeCodeProvider::default_path().unwrap();
        assert!(p.ends_with(".claude"));
    }

    #[test]
    fn new_with_override_uses_passed_path() {
        // 用一个临时存在的目录（系统临时目录一定存在）
        let tmp = std::env::temp_dir();
        let p = ClaudeCodeProvider::new(Some(tmp.clone())).unwrap();
        assert_eq!(p.base_dir, tmp);
    }

    #[test]
    fn new_with_nonexistent_path_fails() {
        let bogus = std::path::PathBuf::from("/nonexistent/path/xyz123");
        assert!(ClaudeCodeProvider::new(Some(bogus)).is_err());
    }
}
```

> 注：`ClaudeCodeProvider::base_dir` 当前是私有字段；如果测试模块无法访问，把字段加 `pub(crate)` 或在测试中只验证「能构造成功」就好。沿用项目现有可见性约定，在 `claude.rs` 内 `base_dir: PathBuf` 改为 `pub(crate) base_dir: PathBuf`。

对 `codex.rs`、`gemini.rs`、`opencode.rs` 各加同样三个测试（替换 `.codex` / `.gemini` / `opencode.db` 后缀；opencode 的 `nonexistent` 测试已自然满足）。注意 opencode 的覆盖路径测试需要指向一个 *存在* 的文件（`std::env::temp_dir()` 是目录，不是文件，opencode 检查的是 `exists()` 所以传目录路径也能通过 —— 但更准确的是用 `tempfile` crate 或者直接用 `Cargo.toml` 这种已知存在的文件路径作为占位）。

opencode 的「覆盖路径」测试使用：

```rust
#[test]
fn new_with_override_uses_passed_path() {
    // 使用 Cargo.toml 这种项目内一定存在的文件作为占位
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let p = OpenCodeProvider::new(Some(manifest.clone())).unwrap();
    assert_eq!(p.db_path, manifest);
}
```

- [ ] **Step 7: 运行测试，确认通过**

```bash
cd src-tauri && cargo test --lib providers::
```

预期：所有 provider 测试通过。

- [ ] **Step 8: 提交**

```bash
git add src-tauri/src/providers/{claude,codex,gemini,opencode,mod}.rs
git commit -m "refactor(providers): new() 接受 Option<PathBuf>，新增 default_path()

为后续支持自定义 provider 路径做准备。None 时沿用原默认路径。"
```

---

### Task 2: 新增 `config.rs` 配置读写模块

**Files:**
- Create: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/lib.rs` (新增 `mod config;`)

- [ ] **Step 1: 创建 `config.rs` 并实现 load/save**

新建 `src-tauri/src/config.rs`：

```rust
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};

/// 应用配置（持久化到 JSON 文件）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderConfig {
    pub provider_paths: ProviderPaths,
}

/// 四个 provider 的可选路径覆盖；None = 走 provider 内置默认
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderPaths {
    pub claude_code: Option<PathBuf>,
    pub codex: Option<PathBuf>,
    pub gemini: Option<PathBuf>,
    pub opencode: Option<PathBuf>,
}

/// 返回配置文件路径：{app_config_dir}/config.json
///
/// macOS: ~/Library/Application Support/aicoder-session-viewer/config.json
pub fn config_file_path(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| AppError::Provider(format!("无法获取配置目录: {}", e)))?;
    Ok(dir.join("config.json"))
}

/// 读取配置；文件不存在、损坏、或反序列化失败都返回默认值（不阻塞应用启动）
pub fn load(app: &AppHandle) -> ProviderConfig {
    let path = match config_file_path(app) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[config] 无法定位配置文件: {}", e);
            return ProviderConfig::default();
        }
    };

    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return ProviderConfig::default(),
    };

    match serde_json::from_str::<ProviderConfig>(&content) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("[config] 配置文件 JSON 损坏，使用默认值: {}", e);
            ProviderConfig::default()
        }
    }
}

/// 写入配置；自动创建父目录
pub fn save(app: &AppHandle, cfg: &ProviderConfig) -> AppResult<()> {
    let path = config_file_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Provider(format!("创建配置目录失败: {}", e)))?;
    }
    let content = serde_json::to_string_pretty(cfg)
        .map_err(|e| AppError::Provider(format!("序列化配置失败: {}", e)))?;
    std::fs::write(&path, content)
        .map_err(|e| AppError::Provider(format!("写入配置文件失败: {}", e)))?;
    Ok(())
}
```

- [ ] **Step 2: 注册模块**

修改 `src-tauri/src/lib.rs`，在 `mod commands;` 那组里加：

```rust
mod config;
```

运行 `cd src-tauri && cargo check`，预期：通过。

- [ ] **Step 3: 写测试 — 往返一致 / 损坏文件 / 缺字段兼容**

由于 `config_file_path` 依赖 `AppHandle`，单元测试里把读写逻辑提取成对裸路径操作的私有函数：

在 `config.rs` 末尾加：

```rust
// 仅用于测试：直接对路径读/写，绕过 AppHandle
#[cfg(test)]
fn load_from_path(path: &std::path::Path) -> ProviderConfig {
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return ProviderConfig::default(),
    };
    serde_json::from_str(&content).unwrap_or_default()
}

#[cfg(test)]
fn save_to_path(path: &std::path::Path, cfg: &ProviderConfig) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(cfg).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_file(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("aicoder-session-viewer-test-{}-{}", std::process::id(), name));
        p
    }

    #[test]
    fn round_trip() {
        let path = tmp_file("roundtrip.json");
        let _ = std::fs::remove_file(&path);

        let mut cfg = ProviderConfig::default();
        cfg.provider_paths.opencode = Some(PathBuf::from("/tmp/custom.db"));
        save_to_path(&path, &cfg).unwrap();

        let loaded = load_from_path(&path);
        assert_eq!(loaded.provider_paths.opencode, Some(PathBuf::from("/tmp/custom.db")));
        assert_eq!(loaded.provider_paths.claude_code, None);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupt_json_returns_default() {
        let path = tmp_file("corrupt.json");
        std::fs::write(&path, "{ not valid json").unwrap();

        let loaded = load_from_path(&path);
        assert!(loaded.provider_paths.opencode.is_none());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_fields_are_compatible() {
        let path = tmp_file("missing.json");
        std::fs::write(&path, r#"{"providerPaths":{"opencode":"/tmp/x.db"}}"#).unwrap();

        let loaded = load_from_path(&path);
        assert_eq!(loaded.provider_paths.opencode, Some(PathBuf::from("/tmp/x.db")));
        assert!(loaded.provider_paths.claude_code.is_none());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_returns_default() {
        let path = tmp_file("does-not-exist.json");
        let _ = std::fs::remove_file(&path);

        let loaded = load_from_path(&path);
        assert!(loaded.provider_paths.opencode.is_none());
    }
}
```

- [ ] **Step 4: 运行测试**

```bash
cd src-tauri && cargo test --lib config::
```

预期：全部通过。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/config.rs src-tauri/src/lib.rs
git commit -m "feat(config): 新增 ProviderConfig + load/save 读写模块"
```

---

### Task 3: `ProviderRegistry` 接受路径并支持 `reload`

**Files:**
- Modify: `src-tauri/src/providers/mod.rs`

- [ ] **Step 1: 修改 `ProviderRegistry::new` 签名 + 抽出共用初始化**

把 `providers/mod.rs:31-58` 改写为：

```rust
use crate::config::ProviderPaths;

impl ProviderRegistry {
    /// 用给定路径初始化所有 provider（路径为 None 时走内置默认）
    pub fn new(paths: &ProviderPaths) -> Self {
        let (providers, claude_provider) = Self::init_providers(paths);
        Self {
            providers,
            claude_provider,
        }
    }

    /// 用新路径重新初始化所有 provider；返回失败原因列表（用于前端 toast）
    pub fn reload(&mut self, paths: &ProviderPaths) -> Vec<String> {
        let mut warnings = Vec::new();
        // 重新构建时收集 warnings
        let (providers, claude_provider) = Self::init_providers_with_warnings(paths, &mut warnings);
        self.providers = providers;
        self.claude_provider = claude_provider;
        warnings
    }

    /// 内部：根据路径构造所有 provider（沉默跳过失败）
    fn init_providers(
        paths: &ProviderPaths,
    ) -> (Vec<Box<dyn SessionProvider>>, Option<claude::ClaudeCodeProvider>) {
        let mut warnings = Vec::new();
        Self::init_providers_with_warnings(paths, &mut warnings)
    }

    /// 内部：同上但把失败原因写入 `warnings`
    fn init_providers_with_warnings(
        paths: &ProviderPaths,
        warnings: &mut Vec<String>,
    ) -> (Vec<Box<dyn SessionProvider>>, Option<claude::ClaudeCodeProvider>) {
        let mut providers: Vec<Box<dyn SessionProvider>> = Vec::new();
        let mut claude_provider = None;

        match claude::ClaudeCodeProvider::new(paths.claude_code.clone()) {
            Ok(p) => {
                providers.push(Box::new(p));
                claude_provider = claude::ClaudeCodeProvider::new(paths.claude_code.clone()).ok();
            }
            Err(e) => warnings.push(format!("Claude Code: {}", e)),
        }
        match codex::CodexProvider::new(paths.codex.clone()) {
            Ok(p) => providers.push(Box::new(p)),
            Err(e) => warnings.push(format!("Codex: {}", e)),
        }
        match gemini::GeminiProvider::new(paths.gemini.clone()) {
            Ok(p) => providers.push(Box::new(p)),
            Err(e) => warnings.push(format!("Gemini: {}", e)),
        }
        match opencode::OpenCodeProvider::new(paths.opencode.clone()) {
            Ok(p) => providers.push(Box::new(p)),
            Err(e) => warnings.push(format!("OpenCode: {}", e)),
        }

        (providers, claude_provider)
    }
}
```

> 注：保留「失败静默跳过」的语义 —— 应用启动时 `init_providers` 不暴露 warnings（保持现状行为）；只有 `reload` 把 warnings 返回给前端用于显示。

- [ ] **Step 2: 编译**

```bash
cd src-tauri && cargo check
```

预期：报 `lib.rs` 仍在调 `ProviderRegistry::new()` 无参 —— 暂时不修，下一个 Task 处理。当前任务只为让 `mod.rs` 内部一致即可。可临时把 `lib.rs:11` 改为 `let registry = ProviderRegistry::new(&crate::config::ProviderPaths::default());`，让编译通过。

```rust
let registry = ProviderRegistry::new(&crate::config::ProviderPaths::default());
```

再 `cargo check`，预期：通过。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/providers/mod.rs src-tauri/src/lib.rs
git commit -m "refactor(providers): Registry 接受 ProviderPaths，新增 reload()"
```

---

### Task 4: `lib.rs` 装配 + 现有命令改用 `Arc<RwLock<...>>`

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: `lib.rs` 启动时读配置 + 包 `Arc<RwLock>`**

把 `src-tauri/src/lib.rs` 改为：

```rust
mod commands;
mod config;
mod error;
mod export;
mod models;
mod providers;

use std::sync::{Arc, RwLock};
use tauri::Manager;

use providers::ProviderRegistry;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 启动时读配置 + 初始化 registry（先读，再 manage）
            let cfg = config::load(app.handle());
            let registry = ProviderRegistry::new(&cfg.provider_paths);
            app.manage(Arc::new(RwLock::new(registry)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_all_sessions,
            commands::list_sessions,
            commands::get_session,
            commands::get_subagent_messages,
            commands::search_sessions,
            commands::export_session_jsonl,
            commands::export_session_markdown,
            commands::resume_session,
            commands::resume_session_with_auto_continue,
            commands::open_new_session,
            commands::get_provider_config,
            commands::update_provider_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

> 注：`get_provider_config` / `update_provider_config` 暂时不存在，这一步先列上，Task 5 会补实现。如果想让中间状态可编译，先注释掉这两行。

为让本 Task 编译通过，注释掉新加的两行：

```rust
// commands::get_provider_config,
// commands::update_provider_config,
```

- [ ] **Step 2: 改 `commands.rs` 所有 `State<ProviderRegistry>` 为 `State<Arc<RwLock<ProviderRegistry>>>`**

`commands.rs` 顶部加：

```rust
use std::sync::{Arc, RwLock};
```

然后把现有 10 条命令的 registry 参数全部从 `registry: State<ProviderRegistry>` 改为 `registry: State<Arc<RwLock<ProviderRegistry>>>`，调用处加 `.read().unwrap()`。例如：

```rust
#[tauri::command]
pub fn list_all_sessions(
    registry: State<Arc<RwLock<ProviderRegistry>>>,
) -> AppResult<Vec<SessionSummary>> {
    registry.read().unwrap().list_all_sessions()
}
```

对以下命令全部应用同样改造（共 10 条）：

- `list_all_sessions`
- `list_sessions`
- `get_session`
- `get_subagent_messages`
- `search_sessions`
- `export_session_jsonl`
- `export_session_markdown`
- `resume_session`
- `resume_session_with_auto_continue`
- `open_new_session`

每条命令体内的 `registry.xxx(...)` 替换为 `registry.read().unwrap().xxx(...)`。

> 注：用 `.read().unwrap()` 简洁直接；理论上 `RwLock::read` 仅在锁被毒化时才 panic，毒化意味着写线程 panic 过 —— 整个进程已不可靠，panic 是合理选择。

- [ ] **Step 3: 编译并运行 cargo check**

```bash
cd src-tauri && cargo check
```

预期：通过。

- [ ] **Step 4: 运行所有现有测试**

```bash
cd src-tauri && cargo test
```

预期：通过（包括 Task 1、Task 2 新增的）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/lib.rs src-tauri/src/commands.rs
git commit -m "refactor(commands): registry 改为 Arc<RwLock>，启动时读配置初始化"
```

---

### Task 5: 新增 IPC `get_provider_config` 和 `update_provider_config`

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 在 `commands.rs` 末尾加两条命令**

```rust
use crate::config::{self, ProviderConfig, ProviderPaths};
use crate::providers::{claude, codex, gemini, opencode};
use tauri::AppHandle;

/// 前端获取当前配置 + 各 provider 的默认路径预览
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigResponse {
    config: ProviderConfig,
    defaults: ProviderPaths,
}

#[tauri::command]
pub fn get_provider_config(app: AppHandle) -> AppResult<ProviderConfigResponse> {
    let cfg = config::load(&app);
    let defaults = ProviderPaths {
        claude_code: claude::ClaudeCodeProvider::default_path().ok(),
        codex: codex::CodexProvider::default_path().ok(),
        gemini: gemini::GeminiProvider::default_path().ok(),
        opencode: opencode::OpenCodeProvider::default_path().ok(),
    };
    Ok(ProviderConfigResponse {
        config: cfg,
        defaults,
    })
}

/// 前端保存配置并触发 registry 热重载
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProviderConfigResponse {
    warnings: Vec<String>,
}

#[tauri::command]
pub fn update_provider_config(
    app: AppHandle,
    registry: State<Arc<RwLock<ProviderRegistry>>>,
    config: ProviderConfig,
) -> AppResult<UpdateProviderConfigResponse> {
    // 1. 先写文件；失败直接返回，不动内存状态
    config::save(&app, &config)?;
    // 2. 热重载 registry；收集 warnings
    let warnings = registry.write().unwrap().reload(&config.provider_paths);
    Ok(UpdateProviderConfigResponse { warnings })
}
```

> 注：这里需要 `commands.rs` 能 `use crate::providers::{claude, codex, gemini, opencode}`。当前 `providers/mod.rs` 内 `pub mod claude;` 等已是 `pub`，可以直接用。

- [ ] **Step 2: 取消 `lib.rs` 的两行注释**

把 Task 4 临时注释的两行启用：

```rust
commands::get_provider_config,
commands::update_provider_config,
```

- [ ] **Step 3: 编译**

```bash
cd src-tauri && cargo check
```

预期：通过。

- [ ] **Step 4: 写一个 smoke 测试** —— 直接构造 `ProviderPaths` 调 `reload`

在 `providers/mod.rs` 末尾加：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderPaths;

    #[test]
    fn reload_with_bad_paths_returns_warnings() {
        let mut reg = ProviderRegistry::new(&ProviderPaths::default());
        let bad = ProviderPaths {
            claude_code: Some(std::path::PathBuf::from("/nonexistent/xyz1")),
            codex: Some(std::path::PathBuf::from("/nonexistent/xyz2")),
            gemini: Some(std::path::PathBuf::from("/nonexistent/xyz3")),
            opencode: Some(std::path::PathBuf::from("/nonexistent/xyz4")),
        };
        let warnings = reg.reload(&bad);
        assert_eq!(warnings.len(), 4);
        assert!(warnings.iter().any(|w| w.starts_with("Claude Code:")));
        assert!(warnings.iter().any(|w| w.starts_with("Codex:")));
        assert!(warnings.iter().any(|w| w.starts_with("Gemini:")));
        assert!(warnings.iter().any(|w| w.starts_with("OpenCode:")));
    }
}
```

- [ ] **Step 5: 运行测试**

```bash
cd src-tauri && cargo test
```

预期：通过。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/src/providers/mod.rs
git commit -m "feat(ipc): get_provider_config / update_provider_config

支持读取/保存配置并热重载 registry，失败的 provider 通过 warnings 上报。"
```

---

### Task 6: 前端类型定义

**Files:**
- Modify: `src/types.ts`

- [ ] **Step 1: 在 `types.ts` 末尾追加**

```ts
/** 各 provider 的可选路径覆盖；null = 走 provider 内置默认 */
export interface ProviderPaths {
  claudeCode: string | null;
  codex: string | null;
  gemini: string | null;
  opencode: string | null;
}

/** 应用配置（持久化到 JSON） */
export interface ProviderConfig {
  providerPaths: ProviderPaths;
}

/** get_provider_config 返回值 */
export interface ProviderConfigResponse {
  config: ProviderConfig;
  defaults: ProviderPaths;  // 每个字段都应为非 null（来自 default_path()）
}

/** update_provider_config 返回值 */
export interface UpdateProviderConfigResponse {
  warnings: string[];
}
```

- [ ] **Step 2: TypeScript 类型检查**

```bash
npx tsc --noEmit
```

预期：通过。

- [ ] **Step 3: 提交**

```bash
git add src/types.ts
git commit -m "feat(types): ProviderConfig / ProviderPaths 类型定义"
```

---

### Task 7: Zustand store 扩展

**Files:**
- Modify: `src/stores/sessionStore.ts`

- [ ] **Step 1: 加状态字段 + actions**

在 `SessionState` 接口末尾追加：

```ts
  // Provider 路径配置
  providerConfig: ProviderConfig | null;
  providerDefaults: ProviderPaths | null;

  loadProviderConfig: () => Promise<void>;
  // 返回 warnings 数组，让调用方决定如何展示
  saveProviderConfig: (config: ProviderConfig) => Promise<string[]>;
```

顶部 import 改为：

```ts
import type {
  ProviderConfig,
  ProviderConfigResponse,
  ProviderPaths,
  Session,
  SessionSummary,
  ToolKind,
  UpdateProviderConfigResponse,
} from "../types";
```

`create` 初始 state 加：

```ts
  providerConfig: null,
  providerDefaults: null,
```

在 `create` 内、`togglePathExpanded` 之后追加两个 action：

```ts
  loadProviderConfig: async () => {
    try {
      const resp: ProviderConfigResponse = await invoke("get_provider_config");
      set({
        providerConfig: resp.config,
        providerDefaults: resp.defaults,
      });
    } catch (e) {
      console.error("[store] loadProviderConfig 失败:", e);
    }
  },

  saveProviderConfig: async (config) => {
    const resp: UpdateProviderConfigResponse = await invoke(
      "update_provider_config",
      { config }
    );
    // 保存成功后立即刷新本地副本 + session 列表
    set({ providerConfig: config });
    await get().fetchSessions();
    return resp.warnings;
  },
```

- [ ] **Step 2: TS 类型检查**

```bash
npx tsc --noEmit
```

预期：通过。

- [ ] **Step 3: 提交**

```bash
git add src/stores/sessionStore.ts
git commit -m "feat(store): loadProviderConfig / saveProviderConfig"
```

---

### Task 8: i18n 文案

**Files:**
- Modify: `src/i18n/locales/zh.ts`（`Locale` 接口定义 + 中文实现）
- Modify: `src/i18n/locales/en.ts`（英文实现）

> 注：`Locale` 接口当前定义在 `src/i18n/locales/zh.ts` 顶部（第 2 行 `export interface Locale`）。如果实际位置已迁移到 `src/i18n/index.ts`，按实际位置修改即可。

- [ ] **Step 1: 在 `Locale` 接口末尾追加新 key**

定位到 `src/i18n/locales/zh.ts` 的 `Locale` 接口，在末尾追加：

```ts
  // 设置
  settingsTitle: string;
  settingsProviderPaths: string;
  settingsBrowse: string;
  settingsReset: string;
  settingsSave: string;
  settingsCancel: string;
  settingsDefault: string;
  settingsProviderLabel: (provider: string) => string;  // 例如 "Claude Code"
  settingsPathPlaceholder: string;
  settingsSaved: string;
  settingsProviderWarning: (provider: string, message: string) => string;
```

- [ ] **Step 2: 在 `zh.ts` 末尾追加**

```ts
  settingsTitle: "设置",
  settingsProviderPaths: "数据源路径",
  settingsBrowse: "浏览…",
  settingsReset: "重置",
  settingsSave: "保存",
  settingsCancel: "取消",
  settingsDefault: "默认",
  settingsProviderLabel: (provider) => provider,
  settingsPathPlaceholder: "留空使用默认路径",
  settingsSaved: "配置已保存",
  settingsProviderWarning: (provider, message) => `${provider} 加载失败: ${message}`,
```

- [ ] **Step 3: 在 `en.ts` 末尾追加**

```ts
  settingsTitle: "Settings",
  settingsProviderPaths: "Data Source Paths",
  settingsBrowse: "Browse…",
  settingsReset: "Reset",
  settingsSave: "Save",
  settingsCancel: "Cancel",
  settingsDefault: "Default",
  settingsProviderLabel: (provider) => provider,
  settingsPathPlaceholder: "Leave empty to use default",
  settingsSaved: "Configuration saved",
  settingsProviderWarning: (provider, message) => `${provider} failed to load: ${message}`,
```

- [ ] **Step 4: TS 类型检查**

```bash
npx tsc --noEmit
```

预期：通过。

- [ ] **Step 5: 提交**

```bash
git add src/i18n/locales/en.ts src/i18n/locales/zh.ts
git commit -m "feat(i18n): 设置对话框文案 (zh/en)"
```

---

### Task 9: `SettingsDialog.tsx` 组件

**Files:**
- Create: `src/components/SettingsDialog.tsx`

- [ ] **Step 1: 创建组件**

```tsx
import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { X } from "lucide-react";
import { useLocale } from "../i18n";
import { useSessionStore } from "../stores/sessionStore";
import type { ProviderConfig, ProviderPaths } from "../types";

interface Props {
  open: boolean;
  onClose: () => void;
}

type ProviderKey = keyof ProviderPaths;

/** 4 个 provider 的元信息：用于决定标签和文件选择器模式 */
const PROVIDERS: { key: ProviderKey; label: string; isFile: boolean }[] = [
  { key: "claudeCode", label: "Claude Code", isFile: false },
  { key: "codex", label: "Codex", isFile: false },
  { key: "gemini", label: "Gemini", isFile: false },
  { key: "opencode", label: "OpenCode", isFile: true },
];

export function SettingsDialog({ open, onClose }: Props) {
  const { t } = useLocale();
  const providerConfig = useSessionStore((s) => s.providerConfig);
  const providerDefaults = useSessionStore((s) => s.providerDefaults);
  const loadProviderConfig = useSessionStore((s) => s.loadProviderConfig);
  const saveProviderConfig = useSessionStore((s) => s.saveProviderConfig);

  // 本地编辑态：空字符串表示「未覆盖」（最终写入 null）
  const [draft, setDraft] = useState<Record<ProviderKey, string>>({
    claudeCode: "",
    codex: "",
    gemini: "",
    opencode: "",
  });
  const [saving, setSaving] = useState(false);

  // 打开时拉一次配置 + 用现值初始化草稿
  useEffect(() => {
    if (!open) return;
    (async () => {
      await loadProviderConfig();
      const cfg = useSessionStore.getState().providerConfig;
      if (cfg) {
        setDraft({
          claudeCode: cfg.providerPaths.claudeCode ?? "",
          codex: cfg.providerPaths.codex ?? "",
          gemini: cfg.providerPaths.gemini ?? "",
          opencode: cfg.providerPaths.opencode ?? "",
        });
      }
    })();
  }, [open, loadProviderConfig]);

  if (!open) return null;

  const setField = (key: ProviderKey, value: string) => {
    setDraft((d) => ({ ...d, [key]: value }));
  };

  const browse = async (key: ProviderKey, isFile: boolean) => {
    const selected = await openDialog(
      isFile
        ? {
            multiple: false,
            directory: false,
            filters: [{ name: "SQLite", extensions: ["db", "sqlite"] }],
          }
        : { multiple: false, directory: true }
    );
    if (typeof selected === "string") {
      setField(key, selected);
    }
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      const config: ProviderConfig = {
        providerPaths: {
          claudeCode: draft.claudeCode.trim() || null,
          codex: draft.codex.trim() || null,
          gemini: draft.gemini.trim() || null,
          opencode: draft.opencode.trim() || null,
        },
      };
      const warnings = await saveProviderConfig(config);
      if (warnings.length > 0) {
        // 简单 alert；项目当前没有 toast 系统（与 ChatView.autoContinueError 一致风格）
        window.alert(warnings.join("\n"));
      }
      onClose();
    } catch (e) {
      window.alert(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      onClick={onClose}
    >
      <div
        className="w-[640px] max-w-[90vw] rounded-lg border border-border bg-surface p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-base font-semibold text-text-primary">
            {t.settingsTitle}
          </h2>
          <button
            onClick={onClose}
            className="rounded p-1 text-text-muted hover:bg-surface-hover hover:text-text-primary"
          >
            <X size={16} />
          </button>
        </div>

        <h3 className="mb-3 text-sm font-medium text-text-secondary">
          {t.settingsProviderPaths}
        </h3>

        <div className="space-y-4">
          {PROVIDERS.map(({ key, label, isFile }) => (
            <div key={key} className="space-y-1">
              <label className="block text-xs font-medium text-text-primary">
                {label}
              </label>
              <div className="flex gap-2">
                <input
                  type="text"
                  value={draft[key]}
                  onChange={(e) => setField(key, e.target.value)}
                  placeholder={t.settingsPathPlaceholder}
                  className="flex-1 rounded border border-border bg-background px-2 py-1 text-xs"
                />
                <button
                  type="button"
                  onClick={() => browse(key, isFile)}
                  className="rounded border border-border bg-background px-2 py-1 text-xs hover:bg-surface-hover"
                >
                  {t.settingsBrowse}
                </button>
                <button
                  type="button"
                  onClick={() => setField(key, "")}
                  className="rounded border border-border bg-background px-2 py-1 text-xs hover:bg-surface-hover"
                >
                  {t.settingsReset}
                </button>
              </div>
              {providerDefaults && providerDefaults[key] && (
                <p className="text-[10px] text-text-muted">
                  {t.settingsDefault}: {providerDefaults[key]}
                </p>
              )}
            </div>
          ))}
        </div>

        <div className="mt-5 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded border border-border bg-background px-3 py-1 text-xs hover:bg-surface-hover"
          >
            {t.settingsCancel}
          </button>
          <button
            onClick={handleSave}
            disabled={saving}
            className="rounded bg-primary px-3 py-1 text-xs text-primary-foreground hover:opacity-90 disabled:opacity-50"
          >
            {t.settingsSave}
          </button>
        </div>
      </div>
    </div>
  );
}
```

> 注：颜色 token（`bg-surface`、`text-text-primary` 等）沿用项目现有 Tailwind 主题；如类名不存在，按照 `src/components/Sidebar/*.tsx` 与 `Layout.tsx` 中实际使用的类替换。

- [ ] **Step 2: TS 类型检查**

```bash
npx tsc --noEmit
```

预期：通过。

- [ ] **Step 3: 提交**

```bash
git add src/components/SettingsDialog.tsx
git commit -m "feat(frontend): SettingsDialog 组件"
```

---

### Task 10: 入口齿轮按钮 + 挂载 Dialog

**Files:**
- Modify: `src/components/Layout.tsx`

- [ ] **Step 1: 在 `Layout` 加齿轮按钮和 Dialog**

```tsx
import { SearchBar } from "./Sidebar/SearchBar";
import { ToolFilter } from "./Sidebar/ToolFilter";
import { SessionList } from "./Sidebar/SessionList";
import { ProjectTree } from "./Sidebar/ProjectTree";
import { ChatView } from "./Chat/ChatView";
import { SettingsDialog } from "./SettingsDialog";
import { useLocale } from "../i18n";
import { useSessionStore } from "../stores/sessionStore";
import { List, FolderTree, Settings } from "lucide-react";
import { clsx } from "clsx";
import { useState } from "react";

export function Layout() {
  const { t } = useLocale();
  const { viewMode, setViewMode } = useSessionStore();
  const [settingsOpen, setSettingsOpen] = useState(false);

  return (
    <div className="flex h-screen">
      {/* 侧边栏 */}
      <aside className="flex w-80 shrink-0 flex-col border-r border-border bg-sidebar">
        {/* 标题 + 视图切换 + 设置 */}
        <div className="shrink-0 border-b border-border px-4 py-3 flex items-center justify-between">
          <h1 className="text-sm font-semibold text-text-primary">
            {t.appTitle}
          </h1>

          <div className="flex items-center gap-2">
            {/* 视图模式切换 */}
            <div className="flex items-center gap-0.5 rounded-md bg-surface p-0.5">
              <button
                onClick={() => setViewMode("flat")}
                className={clsx(
                  "rounded p-1 transition-colors",
                  viewMode === "flat"
                    ? "bg-surface-hover text-text-primary"
                    : "text-text-muted hover:text-text-primary"
                )}
                title={t.viewFlat}
              >
                <List size={14} />
              </button>
              <button
                onClick={() => setViewMode("grouped")}
                className={clsx(
                  "rounded p-1 transition-colors",
                  viewMode === "grouped"
                    ? "bg-surface-hover text-text-primary"
                    : "text-text-muted hover:text-text-primary"
                )}
                title={t.viewGrouped}
              >
                <FolderTree size={14} />
              </button>
            </div>

            {/* 设置 */}
            <button
              onClick={() => setSettingsOpen(true)}
              className="rounded p-1 text-text-muted hover:bg-surface-hover hover:text-text-primary"
              title={t.settingsTitle}
            >
              <Settings size={14} />
            </button>
          </div>
        </div>

        {/* 搜索 */}
        <SearchBar />

        {/* 工具过滤 */}
        <ToolFilter />

        {/* Session 列表 / 项目树 */}
        {viewMode === "flat" ? <SessionList /> : <ProjectTree />}
      </aside>

      {/* 主内容区 */}
      <main className="flex-1 min-w-0">
        <ChatView />
      </main>

      {/* 设置对话框 */}
      <SettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />
    </div>
  );
}
```

- [ ] **Step 2: TS 类型检查**

```bash
npx tsc --noEmit
```

预期：通过。

- [ ] **Step 3: 启动应用手动验证**

```bash
pnpm tauri dev
```

逐项验证：

1. 应用启动正常，列表与改造前一致（无 `config.json` 时的默认行为）。
2. 标题栏出现齿轮按钮，点击打开 SettingsDialog，显示四个 provider 字段和默认路径。
3. 点击「浏览」能弹出原生选择器；选目录（claude/codex/gemini）或文件（opencode）后自动填回输入框。
4. 把 OpenCode 字段改为一个不存在的路径，点保存 → 弹 `OpenCode: database not found: /xxx`，其它三个 provider 仍正常显示。
5. 把 OpenCode 字段清空（「重置」按钮），保存 → 列表恢复为默认 opencode.db 的数据（如果默认路径存在）。
6. 关闭并重启 app → 之前保存的设置仍生效。
7. 检查 macOS 下 `~/Library/Application Support/aicoder-session-viewer/config.json` 存在且内容符合预期。

- [ ] **Step 4: 提交**

```bash
git add src/components/Layout.tsx
git commit -m "feat(frontend): 标题栏新增设置入口，挂载 SettingsDialog"
```

---

### Task 11: README 更新

**Files:**
- Modify: `README.md`

- [ ] **Step 1: 在 README 找到合适位置插入「自定义数据源路径」一节**

读取 `README.md`，在「Features」或「使用」相关章节后追加：

```markdown
## 自定义数据源路径

四个 provider 默认从如下位置读取：

| Provider     | 默认路径                                    |
| ------------ | ------------------------------------------- |
| Claude Code  | `~/.claude`                                 |
| Codex        | `~/.codex`                                  |
| Gemini       | `~/.gemini`                                 |
| OpenCode     | `~/.local/share/opencode/opencode.db`       |

若你的工具基于上述任一改造、数据存放在非默认位置（例如基于 OpenCode 衍生的工具），可在应用标题栏点击齿轮按钮打开「设置」，为对应 provider 指定自定义路径。保存后立即生效，无需重启。

配置存放在：

- macOS: `~/Library/Application Support/aicoder-session-viewer/config.json`
- Linux: `~/.config/aicoder-session-viewer/config.json`
- Windows: `%APPDATA%\aicoder-session-viewer\config.json`
```

- [ ] **Step 2: 提交**

```bash
git add README.md
git commit -m "docs(readme): 增加自定义数据源路径章节"
```

---

## 验收

实施完成后整体过一遍：

1. `cd src-tauri && cargo test` → 所有测试通过
2. `npx tsc --noEmit` → 通过
3. `pnpm tauri dev` → Task 10 的手动验收清单全部通过

完成。
