use tauri::State;

use crate::error::AppResult;
use crate::models::{Session, SessionSummary, ToolKind};
use crate::providers::ProviderRegistry;

/// 列出所有工具的 session
#[tauri::command]
pub fn list_all_sessions(registry: State<ProviderRegistry>) -> AppResult<Vec<SessionSummary>> {
    registry.list_all_sessions()
}

/// 列出指定工具的 session
#[tauri::command]
pub fn list_sessions(
    tool: String,
    registry: State<ProviderRegistry>,
) -> AppResult<Vec<SessionSummary>> {
    let tool_kind = ToolKind::from_str_loose(&tool)
        .ok_or_else(|| crate::error::AppError::Provider(format!("未知工具类型: {}", tool)))?;
    registry.list_sessions_by_tool(tool_kind)
}

/// 获取完整 session（含所有消息）
#[tauri::command]
pub fn get_session(
    tool: String,
    session_id: String,
    registry: State<ProviderRegistry>,
) -> AppResult<Session> {
    let tool_kind = ToolKind::from_str_loose(&tool)
        .ok_or_else(|| crate::error::AppError::Provider(format!("未知工具类型: {}", tool)))?;
    registry.get_session(tool_kind, &session_id)
}

/// 搜索 session
#[tauri::command]
pub fn search_sessions(
    query: String,
    tool: Option<String>,
    registry: State<ProviderRegistry>,
) -> AppResult<Vec<SessionSummary>> {
    let tool_kind = tool.and_then(|t| ToolKind::from_str_loose(&t));
    registry.search_sessions(&query, tool_kind)
}
