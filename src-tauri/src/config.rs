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
        // 文件不存在是首次运行的正常路径，保持静默
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ProviderConfig::default(),
        // 其他读取错误（权限、IO 等）记录路径方便排查
        Err(e) => {
            eprintln!("[config] 读取配置文件失败 ({}): {}", path.display(), e);
            return ProviderConfig::default();
        }
    };

    match serde_json::from_str::<ProviderConfig>(&content) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!(
                "[config] 配置文件 JSON 损坏 ({})，使用默认值: {}",
                path.display(),
                e
            );
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
