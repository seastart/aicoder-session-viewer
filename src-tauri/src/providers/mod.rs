pub mod claude;
pub mod codex;
pub mod gemini;
pub mod opencode;

use crate::error::AppResult;
use crate::models::{Session, SessionSummary, ToolKind};

/// 统一的 Session 数据源接口
pub trait SessionProvider: Send + Sync {
    /// 返回该 Provider 对应的工具类型
    fn tool_kind(&self) -> ToolKind;

    /// 列出所有可用的 session 摘要
    fn list_sessions(&self) -> AppResult<Vec<SessionSummary>>;

    /// 获取完整 session（含所有消息）
    fn get_session(&self, session_id: &str) -> AppResult<Session>;

    /// 简单文本搜索，返回匹配的 session 摘要
    fn search_sessions(&self, query: &str) -> AppResult<Vec<SessionSummary>>;
}

/// Provider 注册中心，统一管理所有数据源
pub struct ProviderRegistry {
    providers: Vec<Box<dyn SessionProvider>>,
}

impl ProviderRegistry {
    /// 初始化所有 Provider（自动跳过不可用的）
    pub fn new() -> Self {
        let mut providers: Vec<Box<dyn SessionProvider>> = Vec::new();

        // 逐个尝试初始化，失败不影响其他
        if let Ok(p) = claude::ClaudeCodeProvider::new() {
            providers.push(Box::new(p));
        }
        if let Ok(p) = codex::CodexProvider::new() {
            providers.push(Box::new(p));
        }
        if let Ok(p) = gemini::GeminiProvider::new() {
            providers.push(Box::new(p));
        }
        if let Ok(p) = opencode::OpenCodeProvider::new() {
            providers.push(Box::new(p));
        }

        Self { providers }
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
        // 按时间倒序排列
        all.sort_by(|a, b| b.started_at.cmp(&a.started_at));
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
        Err(crate::error::AppError::SessionNotFound(session_id.to_string()))
    }

    /// 搜索 session
    pub fn search_sessions(
        &self,
        query: &str,
        tool: Option<ToolKind>,
    ) -> AppResult<Vec<SessionSummary>> {
        let mut results = Vec::new();
        for provider in &self.providers {
            if let Some(t) = tool {
                if provider.tool_kind() != t {
                    continue;
                }
            }
            match provider.search_sessions(query) {
                Ok(sessions) => results.extend(sessions),
                Err(e) => eprintln!("[{}] 搜索失败: {}", provider.tool_kind_label(), e),
            }
        }
        results.sort_by(|a, b| b.started_at.cmp(&a.started_at));
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
            ToolKind::OpenCode => "OpenCode",
        }
    }
}
