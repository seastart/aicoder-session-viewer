use crate::models::{ContentBlock, Role, Session};

/// 将 Session 导出为 JSONL 格式
/// 第一行为 session 元数据，后续每行一条消息
pub fn to_jsonl(session: &Session) -> String {
    let mut lines = Vec::new();

    // 元数据行
    let meta = serde_json::json!({
        "type": "session_meta",
        "id": session.summary.id,
        "tool": session.summary.tool,
        "title": session.summary.title,
        "project_path": session.summary.project_path,
        "started_at": session.summary.started_at,
        "message_count": session.summary.message_count,
    });
    if let Ok(json) = serde_json::to_string(&meta) {
        lines.push(json);
    }

    // 每条消息一行
    for msg in &session.messages {
        if let Ok(json) = serde_json::to_string(msg) {
            lines.push(json);
        }
    }

    lines.join("\n")
}

/// 将 Session 导出为可读的 Markdown 文档
pub fn to_markdown(session: &Session) -> String {
    let mut md = String::new();

    // 标题与元数据
    md.push_str(&format!("# {}\n\n", session.summary.title));
    md.push_str(&format!(
        "**Tool:** {}\n",
        tool_label(session.summary.tool)
    ));
    if let Some(path) = &session.summary.project_path {
        md.push_str(&format!("**Project:** `{}`\n", path));
    }
    if let Some(ts) = &session.summary.started_at {
        md.push_str(&format!("**Date:** {}\n", ts.format("%Y-%m-%d %H:%M:%S")));
    }
    md.push_str(&format!(
        "**Messages:** {}\n",
        session.summary.message_count
    ));
    md.push_str("\n---\n\n");

    // 逐条消息渲染
    for msg in &session.messages {
        let role = match msg.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::System => "System",
            Role::Tool => "Tool",
        };

        // 消息头：角色 + 时间戳
        md.push_str(&format!("## {}", role));
        if let Some(ts) = &msg.timestamp {
            md.push_str(&format!("  *({})*", ts.format("%H:%M:%S")));
        }
        if let Some(model) = &msg.model {
            md.push_str(&format!("  `{}`", model));
        }
        md.push_str("\n\n");

        // 渲染每个内容块
        for block in &msg.content {
            render_block(&mut md, block);
        }

        md.push_str("---\n\n");
    }

    md
}

/// 渲染单个内容块为 Markdown
fn render_block(md: &mut String, block: &ContentBlock) {
    match block {
        ContentBlock::Text { text } => {
            md.push_str(text);
            md.push_str("\n\n");
        }
        ContentBlock::Code { language, code } => {
            let lang = language.as_deref().unwrap_or("");
            md.push_str(&format!("```{}\n{}\n```\n\n", lang, code));
        }
        ContentBlock::ToolUse {
            tool_name, input, ..
        } => {
            md.push_str(&format!("**Tool Call:** `{}`\n\n", tool_name));
            if let Ok(pretty) = serde_json::to_string_pretty(input) {
                md.push_str(&format!("```json\n{}\n```\n\n", pretty));
            }
        }
        ContentBlock::ToolResult {
            content, is_error, ..
        } => {
            let label = if *is_error {
                "Error Result"
            } else {
                "Tool Result"
            };
            md.push_str(&format!("**{}:**\n\n```\n{}\n```\n\n", label, content));
        }
        ContentBlock::Thinking { text } => {
            md.push_str(&format!(
                "<details>\n<summary>Thinking</summary>\n\n{}\n\n</details>\n\n",
                text
            ));
        }
        ContentBlock::Image { source, .. } => {
            // base64 图片无法直接嵌入 Markdown，标记为占位
            if source.starts_with("data:") || source.len() > 200 {
                md.push_str("*[Image: base64 data]*\n\n");
            } else {
                md.push_str(&format!("![Image]({})\n\n", source));
            }
        }
    }
}

/// 工具类型的显示名称
fn tool_label(tool: crate::models::ToolKind) -> &'static str {
    match tool {
        crate::models::ToolKind::ClaudeCode => "Claude Code",
        crate::models::ToolKind::Codex => "Codex",
        crate::models::ToolKind::Gemini => "Gemini",
        crate::models::ToolKind::OpenCode => "OpenCode",
    }
}
