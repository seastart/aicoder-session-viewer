pub mod antigravity;
pub mod claude;
pub mod codex;
pub mod gemini;
pub mod opencode;
pub mod search;

use crate::config::ProviderPaths;
use crate::error::AppResult;
use crate::models::{Message, Session, SessionSummary, ToolKind};

/// 统一的 Session 数据源接口
pub trait SessionProvider: Send + Sync {
    /// 返回该 Provider 对应的工具类型
    fn tool_kind(&self) -> ToolKind;

    /// 列出所有可用的 session 摘要
    fn list_sessions(&self) -> AppResult<Vec<SessionSummary>>;

    /// 获取完整 session（含所有消息）
    fn get_session(&self, session_id: &str) -> AppResult<Session>;

    /// 文本搜索，返回匹配的 session 摘要
    ///
    /// `include_content` 为 false 时只匹配标题/项目路径（便宜，适合实时输入触发）；
    /// 为 true 时额外做会话内容全文匹配（需要扫描所有会话文件，由用户显式触发）
    fn search_sessions(&self, query: &str, include_content: bool)
        -> AppResult<Vec<SessionSummary>>;
}

/// Provider 注册中心，统一管理所有数据源
pub struct ProviderRegistry {
    providers: Vec<Box<dyn SessionProvider>>,
    /// Claude Code provider 的引用，用于 subagent 查询
    claude_provider: Option<claude::ClaudeCodeProvider>,
}

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
        // 重新构建时收集 warnings，便于前端展示
        let (providers, claude_provider) =
            Self::init_providers_with_warnings(paths, &mut warnings);
        self.providers = providers;
        self.claude_provider = claude_provider;
        warnings
    }

    /// 内部：根据路径构造所有 provider（沉默跳过失败，用于应用启动）
    fn init_providers(
        paths: &ProviderPaths,
    ) -> (Vec<Box<dyn SessionProvider>>, Option<claude::ClaudeCodeProvider>) {
        // 启动阶段保持「失败静默跳过」语义，丢弃 warnings
        let mut warnings = Vec::new();
        Self::init_providers_with_warnings(paths, &mut warnings)
    }

    /// 内部：同 init_providers，但把失败原因写入 `warnings`
    /// Claude Code 需要两份实例：一份作为通用 provider，一份专用于 subagent 查询
    fn init_providers_with_warnings(
        paths: &ProviderPaths,
        warnings: &mut Vec<String>,
    ) -> (Vec<Box<dyn SessionProvider>>, Option<claude::ClaudeCodeProvider>) {
        let mut providers: Vec<Box<dyn SessionProvider>> = Vec::new();
        let mut claude_provider = None;

        match claude::ClaudeCodeProvider::new(paths.claude_code.clone()) {
            Ok(p) => {
                providers.push(Box::new(p));
                // 再创建一份独立实例供 subagent 查询使用（保持原有双实例模式）
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
        match antigravity::AntigravityProvider::new(paths.antigravity.clone()) {
            Ok(p) => providers.push(Box::new(p)),
            Err(e) => warnings.push(format!("Antigravity: {}", e)),
        }
        match opencode::OpenCodeProvider::new(paths.opencode.clone()) {
            Ok(p) => providers.push(Box::new(p)),
            Err(e) => warnings.push(format!("OpenCode: {}", e)),
        }

        (providers, claude_provider)
    }

    /// 列出所有工具的 session
    pub fn list_all_sessions(&self) -> AppResult<Vec<SessionSummary>> {
        let mut all = Vec::new();
        for provider in &self.providers {
            match provider.list_sessions() {
                Ok(sessions) => all.extend(sessions),
                Err(e) => eprintln!("[{}] 列出 session 失败: {}", provider.tool_kind_label(), e),
            }
        }
        // 按最后活跃时间倒序
        all.sort_by(|a, b| {
            let a_time = a.updated_at.or(a.started_at);
            let b_time = b.updated_at.or(b.started_at);
            b_time.cmp(&a_time)
        });
        Ok(all)
    }

    /// 列出指定工具的 session
    pub fn list_sessions_by_tool(&self, tool: ToolKind) -> AppResult<Vec<SessionSummary>> {
        for provider in &self.providers {
            if provider.tool_kind() == tool {
                return provider.list_sessions();
            }
        }
        Ok(Vec::new())
    }

    /// 获取完整 session
    pub fn get_session(&self, tool: ToolKind, session_id: &str) -> AppResult<Session> {
        for provider in &self.providers {
            if provider.tool_kind() == tool {
                return provider.get_session(session_id);
            }
        }
        Err(crate::error::AppError::SessionNotFound(
            session_id.to_string(),
        ))
    }

    /// 获取 Claude Code subagent 的对话消息
    pub fn get_subagent_messages(
        &self,
        session_id: &str,
        agent_id: &str,
    ) -> AppResult<Vec<Message>> {
        match &self.claude_provider {
            Some(provider) => provider.get_subagent_messages(session_id, agent_id),
            None => Err(crate::error::AppError::Provider(
                "Claude Code provider 不可用".into(),
            )),
        }
    }

    /// 搜索 session（include_content 含义见 SessionProvider::search_sessions）
    pub fn search_sessions(
        &self,
        query: &str,
        tool: Option<ToolKind>,
        include_content: bool,
    ) -> AppResult<Vec<SessionSummary>> {
        let mut results = Vec::new();
        for provider in &self.providers {
            if let Some(t) = tool {
                if provider.tool_kind() != t {
                    continue;
                }
            }
            match provider.search_sessions(query, include_content) {
                Ok(sessions) => results.extend(sessions),
                Err(e) => eprintln!("[{}] 搜索失败: {}", provider.tool_kind_label(), e),
            }
        }
        results.sort_by(|a, b| {
            let a_time = a.updated_at.or(a.started_at);
            let b_time = b.updated_at.or(b.started_at);
            b_time.cmp(&a_time)
        });
        Ok(results)
    }
}

/// 辅助方法：获取工具的显示名称
trait ToolKindLabel {
    fn tool_kind_label(&self) -> &'static str;
}

impl<T: SessionProvider + ?Sized> ToolKindLabel for T {
    fn tool_kind_label(&self) -> &'static str {
        match self.tool_kind() {
            ToolKind::ClaudeCode => "Claude Code",
            ToolKind::Codex => "Codex",
            ToolKind::Gemini => "Gemini",
            ToolKind::Antigravity => "Antigravity",
            ToolKind::OpenCode => "OpenCode",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderPaths;

    /// 用本机真实数据手动验证全文搜索效果与耗时（依赖本地数据，默认忽略）
    /// 运行: cargo test --release real_data_search -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_data_search_smoke() {
        let reg = ProviderRegistry::new(&ProviderPaths::default());

        // 基线：纯列表耗时（浅搜索的主要成本）
        for _ in 0..2 {
            let t = std::time::Instant::now();
            let n = reg.list_all_sessions().unwrap().len();
            println!("list_all_sessions: {} sessions, elapsed={:?}", n, t.elapsed());
        }
        // 浅搜索（标题/路径）
        let t = std::time::Instant::now();
        let n = reg.search_sessions("部署", None, false).unwrap().len();
        println!("shallow search: matched={} elapsed={:?}", n, t.elapsed());

        for query in ["deploy", "部署", "zzqxv_nomatch"] {
            let t = std::time::Instant::now();
            let results = reg.search_sessions(query, None, true).unwrap();
            let mut by_tool: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for s in &results {
                *by_tool.entry(format!("{:?}", s.tool)).or_insert(0) += 1;
            }
            println!(
                "query={:?} matched={} elapsed={:?} by_tool={:?}",
                query,
                results.len(),
                t.elapsed(),
                by_tool
            );
        }
    }

    /// 给定全部 provider 都是不存在的路径时，reload 应当为每个 provider
    /// 各产生一条 warning，且前缀与显示名一致。
    /// 这条测试守住「失败信息能上报给前端」这条核心契约。
    #[test]
    fn reload_with_bad_paths_returns_warnings() {
        let mut reg = ProviderRegistry::new(&ProviderPaths::default());
        let bad = ProviderPaths {
            claude_code: Some(std::path::PathBuf::from("/nonexistent/xyz1")),
            codex: Some(std::path::PathBuf::from("/nonexistent/xyz2")),
            gemini: Some(std::path::PathBuf::from("/nonexistent/xyz3")),
            antigravity: Some(std::path::PathBuf::from("/nonexistent/xyz4")),
            opencode: Some(std::path::PathBuf::from("/nonexistent/xyz5")),
        };
        let warnings = reg.reload(&bad);
        assert_eq!(warnings.len(), 5);
        assert!(warnings.iter().any(|w| w.starts_with("Claude Code:")));
        assert!(warnings.iter().any(|w| w.starts_with("Codex:")));
        assert!(warnings.iter().any(|w| w.starts_with("Gemini:")));
        assert!(warnings.iter().any(|w| w.starts_with("Antigravity:")));
        assert!(warnings.iter().any(|w| w.starts_with("OpenCode:")));
    }
}
