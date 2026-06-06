use std::collections::{HashMap, HashSet};

use crate::models::{ContentBlock, Message, Role, Session};

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
    md.push_str(&format!("**Tool:** {}\n", tool_label(session.summary.tool)));
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

// ============================ HTML 导出 ============================
//
// 设计目标：生成「自包含」单文件 HTML —— CSS/JS 全部内联、图片 base64 内嵌，
// 这样导出后的文件可以直接发给同事，用浏览器打开、投屏讲解，不依赖任何外部资源。
//
// 布局：用户消息靠右（深灰气泡），助手/系统消息靠左（白卡占满宽度，给代码/工具留空间）。
// 思考块与工具调用默认折叠（<details>），右上角提供深/浅色切换。

/// 内联样式表（与设计样张一致：中性灰配色 + 左右不对称布局）
const HTML_STYLE: &str = r#"
:root, [data-theme="light"] {
  --bg: #f6f7f8; --card: #ffffff; --border: #e5e7eb; --text: #1f2328; --muted: #6e7681;
  --user-bg: #2f333a; --user-fg: #f4f5f6;
  --block-bg: #f0f1f3; --block-bar: #c4c8cf; --block-fg: #4a4f57;
  --code-bg: #e8eaed; --code-fg: #24292f; --code-border: #d3d7dd; --toggle-bg: #ffffff;
}
[data-theme="dark"] {
  --bg: #16181d; --card: #1f2228; --border: #2c3038; --text: #e6e9ed; --muted: #8b919c;
  --user-bg: #3b5bdb; --user-fg: #f4f5f6;
  --block-bg: #23262d; --block-bar: #3a3f48; --block-fg: #b8bdc6;
  --code-bg: #12151b; --code-fg: #e6e9ed; --code-border: #2c3038; --toggle-bg: #1f2228;
}
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--text);
  font: 18px/1.75 -apple-system, "PingFang SC", "Microsoft YaHei", sans-serif; transition: background .2s, color .2s; }
.wrap { max-width: 980px; margin: 0 auto; padding: 40px 28px 100px; }
header { text-align: center; margin-bottom: 40px; }
.badge { display: inline-block; background: var(--text); color: var(--bg);
  font-size: 13px; font-weight: 600; padding: 4px 13px; border-radius: 6px; }
h1 { font-size: 28px; font-weight: 700; margin: 16px 0 8px; letter-spacing: -.02em; word-break: break-word; }
.meta { color: var(--muted); font-size: 15px; word-break: break-all; }
.theme-toggle { position: fixed; top: 18px; right: 18px; z-index: 10;
  background: var(--toggle-bg); border: 1px solid var(--border); color: var(--text);
  border-radius: 8px; padding: 8px 12px; font-size: 14px; cursor: pointer; }
.user-row { display: flex; justify-content: flex-end; margin: 26px 0; }
.user-bubble { max-width: 70%; background: var(--user-bg); color: var(--user-fg);
  padding: 12px 18px; border-radius: 14px 14px 4px 14px; }
.user-bubble .who { font-size: 12px; font-weight: 600; opacity: .65; margin-bottom: 4px; }
.ai-row { display: flex; gap: 14px; margin: 26px 0; }
.tool-output-row { margin: 26px 0 26px 50px; }
.avatar { flex: none; width: 36px; height: 36px; border-radius: 8px; background: var(--text);
  color: var(--bg); font-weight: 700; display: flex; align-items: center; justify-content: center; font-size: 14px; }
.ai-body { flex: 1; min-width: 0; background: var(--card); border: 1px solid var(--border);
  border-radius: 4px 12px 12px 12px; padding: 14px 20px; }
.ai-body .who { font-size: 13px; font-weight: 600; color: var(--text); margin-bottom: 8px; }
.who .ts { color: var(--muted); font-weight: 400; margin-left: 8px; }
/* AI 消息整体可折叠：点击 who 行收起/展开整条回复（默认展开） */
details.ai-body > summary.who { cursor: pointer; list-style: none; user-select: none; }
details.ai-body > summary.who::-webkit-details-marker { display: none; }
details.ai-body > summary.who::before { content: "\25bc"; font-size: 10px; color: var(--muted); margin-right: 7px; }
details.ai-body:not([open]) > summary.who { margin-bottom: 0; }
details.ai-body:not([open]) > summary.who::before { content: "\25b6"; }
.text { white-space: pre-wrap; word-break: break-word; margin: 0 0 14px; }
.text:last-child { margin-bottom: 0; }
pre { background: var(--code-bg); color: var(--code-fg); border: 1px solid var(--code-border); border-radius: 10px;
  padding: 16px 18px; overflow-x: auto; font: 15px/1.6 "SF Mono", Menlo, monospace; margin: 16px 0; }
code { font-family: "SF Mono", Menlo, monospace; }
:not(pre) > code { background: var(--block-bg); padding: 2px 6px; border-radius: 5px; font-size: .92em; }
details.block { background: var(--block-bg); border-left: 3px solid var(--block-bar);
  border-radius: 0 8px 8px 0; padding: 10px 16px; margin: 16px 0; }
details.block summary { cursor: pointer; list-style: none; font-size: 13px; font-weight: 600;
  color: var(--muted); text-transform: uppercase; letter-spacing: .04em; }
details.block summary::before { content: "\25b6  "; font-size: 10px; }
details.block[open] summary::before { content: "\25bc  "; }
.block .text { margin-top: 8px; color: var(--block-fg); }
.block pre { margin: 8px 0 0; }
.block .sub { font-size: 12px; color: var(--muted); margin-top: 10px; font-weight: 600; }
.block.error { border-left-color: #d1666f; }
figure { margin: 16px 0; }
figure img { max-width: 100%; border: 1px solid var(--border); border-radius: 8px; display: block; }
.img-missing { color: var(--muted); font-size: 14px; font-style: italic; }
/* Markdown 正文 */
.md > :first-child { margin-top: 0; }
.md > :last-child { margin-bottom: 0; }
.md p { margin: 0 0 14px; word-break: break-word; }
.md h1, .md h2, .md h3, .md h4 { margin: 18px 0 10px; line-height: 1.3; font-weight: 700; }
.md h1 { font-size: 24px; } .md h2 { font-size: 20px; } .md h3 { font-size: 17px; } .md h4 { font-size: 15px; }
.md ul, .md ol { margin: 0 0 14px; padding-left: 26px; }
.md li { margin: 4px 0; }
.md blockquote { margin: 14px 0; padding: 4px 16px; border-left: 3px solid var(--block-bar); color: var(--block-fg); }
.md a { color: #3b6fe0; text-decoration: none; word-break: break-all; }
.md a:hover { text-decoration: underline; }
.md table { border-collapse: collapse; margin: 14px 0; display: block; overflow-x: auto; }
.md th, .md td { border: 1px solid var(--border); padding: 6px 10px; text-align: left; }
.md th { background: var(--block-bg); }
.md hr { border: none; border-top: 1px solid var(--border); margin: 20px 0; }
.md img { max-width: 100%; border: 1px solid var(--border); border-radius: 8px; margin: 6px 0; }
"#;

/// 切换深/浅色的内联脚本
const HTML_TOGGLE_SCRIPT: &str = "var r=document.documentElement;\
r.dataset.theme=r.dataset.theme==='dark'?'light':'dark';";

/// 将 Session 导出为自包含的 HTML 网页
pub fn to_html(session: &Session) -> String {
    let mut html = String::new();

    // 文档头 + 内联样式 + 深浅色切换按钮
    html.push_str("<!DOCTYPE html>\n<html lang=\"zh\" data-theme=\"light\">\n<head>\n");
    html.push_str("<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str(&format!(
        "<title>{}</title>\n",
        html_escape(&session.summary.title)
    ));
    html.push_str(&format!("<style>{}</style>\n", HTML_STYLE));
    html.push_str("</head>\n<body>\n");
    html.push_str(&format!(
        "<button class=\"theme-toggle\" onclick=\"{}\">切换深/浅色</button>\n",
        HTML_TOGGLE_SCRIPT
    ));
    html.push_str("<div class=\"wrap\">\n");

    // 头部元信息
    html.push_str("<header>\n");
    html.push_str(&format!(
        "<span class=\"badge\">{}</span>\n",
        html_escape(tool_label(session.summary.tool))
    ));
    html.push_str(&format!(
        "<h1>{}</h1>\n",
        html_escape(&session.summary.title)
    ));
    html.push_str("<div class=\"meta\">");
    if let Some(path) = &session.summary.project_path {
        html.push_str(&html_escape(path));
        html.push_str(" · ");
    }
    if let Some(ts) = &session.summary.started_at {
        html.push_str(&ts.format("%Y-%m-%d %H:%M").to_string());
        html.push_str(" · ");
    }
    html.push_str(&format!("{} 条消息", session.summary.message_count));
    html.push_str("</div>\n</header>\n");

    // 预扫描：建立 tool_id -> 结果 的映射，以及哪些 tool_id 存在对应的 ToolUse。
    // 这样渲染 ToolUse 时可把结果合并进同一个折叠块；落单的 ToolResult 仍单独渲染。
    let (result_map, tool_use_ids) = collect_tool_results(&session.messages);

    // 逐条消息渲染
    for msg in &session.messages {
        match msg.role {
            Role::User => render_user_message(&mut html, msg, &result_map, &tool_use_ids),
            _ => render_assistant_message(&mut html, msg, &result_map, &tool_use_ids),
        }
    }

    html.push_str("</div>\n</body>\n</html>\n");
    html
}

/// 预扫描所有内容块：收集 tool_id->(结果文本, 是否报错)，以及所有 ToolUse 的 tool_id 集合
fn collect_tool_results(
    messages: &[Message],
) -> (HashMap<String, (String, bool)>, HashSet<String>) {
    let mut result_map = HashMap::new();
    let mut tool_use_ids = HashSet::new();
    for msg in messages {
        for block in &msg.content {
            match block {
                ContentBlock::ToolUse {
                    tool_id: Some(id), ..
                } => {
                    tool_use_ids.insert(id.clone());
                }
                ContentBlock::ToolResult {
                    tool_id: Some(id),
                    content,
                    is_error,
                } => {
                    result_map.insert(id.clone(), (content.clone(), *is_error));
                }
                _ => {}
            }
        }
    }
    (result_map, tool_use_ids)
}

/// 渲染用户回合
///
/// 关键设计：渲染按「内容」而非「role」决定。工具结果在协议上被塞进 user 回合，
/// 但它属于工具输出、不是人在说话，因此不放进用户气泡：
/// - 用户气泡里只放真正的人类输入（文本/代码/图片）
/// - 只含工具结果的回合不渲染空气泡
/// - 工具结果交由 render_block_html 处理：已配对的会被合并进左侧工具调用（自动跳过），
///   落单的渲染成一个不归属任何人的中性折叠块，靠左对齐
fn render_user_message(
    html: &mut String,
    msg: &Message,
    result_map: &HashMap<String, (String, bool)>,
    tool_use_ids: &HashSet<String>,
) {
    // 真正的人类输入内容块
    let is_human_input = |b: &ContentBlock| {
        matches!(
            b,
            ContentBlock::Text { .. } | ContentBlock::Code { .. } | ContentBlock::Image { .. }
        )
    };
    let human_blocks: Vec<&ContentBlock> = msg.content.iter().filter(|b| is_human_input(b)).collect();

    // 仅当存在人类输入时才渲染右侧气泡，避免「只含工具结果」的回合产生空气泡
    if !human_blocks.is_empty() {
        html.push_str("<div class=\"user-row\">\n<div class=\"user-bubble\">\n");
        html.push_str("<div class=\"who\">User");
        if let Some(ts) = &msg.timestamp {
            html.push_str(&format!(" · {}", ts.format("%H:%M:%S")));
        }
        html.push_str("</div>\n");
        for block in human_blocks {
            render_block_html(html, block, result_map, tool_use_ids);
        }
        html.push_str("</div>\n</div>\n");
    }

    // 落单的工具结果（无对应工具调用）单独渲染成靠左的中性块；已配对的在这里跳过
    for block in &msg.content {
        if let ContentBlock::ToolResult { tool_id, .. } = block {
            let paired = tool_id
                .as_ref()
                .map(|id| tool_use_ids.contains(id))
                .unwrap_or(false);
            if paired {
                continue;
            }
            html.push_str("<div class=\"tool-output-row\">\n");
            render_block_html(html, block, result_map, tool_use_ids);
            html.push_str("</div>\n");
        }
    }
}

/// 渲染助手/系统消息（左侧白卡，占满宽度）
fn render_assistant_message(
    html: &mut String,
    msg: &Message,
    result_map: &HashMap<String, (String, bool)>,
    tool_use_ids: &HashSet<String>,
) {
    let (avatar, label) = match msg.role {
        Role::Assistant => ("AI", "Assistant"),
        Role::System => ("SYS", "System"),
        Role::Tool => ("TL", "Tool"),
        Role::User => ("U", "User"),
    };
    html.push_str("<div class=\"ai-row\">\n");
    html.push_str(&format!("<div class=\"avatar\">{}</div>\n", avatar));
    // ai-body 用 <details open> 包裹：summary 即角色/时间行，点击可折叠整条回复
    html.push_str("<details class=\"ai-body\" open>\n");
    html.push_str(&format!("<summary class=\"who\">{}", label));
    let mut sub = String::new();
    if let Some(ts) = &msg.timestamp {
        sub.push_str(&ts.format("%H:%M:%S").to_string());
    }
    if let Some(model) = &msg.model {
        if !sub.is_empty() {
            sub.push_str(" · ");
        }
        sub.push_str(model);
    }
    if !sub.is_empty() {
        html.push_str(&format!("<span class=\"ts\">{}</span>", html_escape(&sub)));
    }
    html.push_str("</summary>\n");
    for block in &msg.content {
        render_block_html(html, block, result_map, tool_use_ids);
    }
    html.push_str("</details>\n</div>\n");
}

/// 渲染单个内容块为 HTML
fn render_block_html(
    html: &mut String,
    block: &ContentBlock,
    result_map: &HashMap<String, (String, bool)>,
    tool_use_ids: &HashSet<String>,
) {
    match block {
        ContentBlock::Text { text } => {
            // 助手/用户文本按 Markdown 渲染（与 App 内一致）
            html.push_str(&format!("<div class=\"md\">{}</div>\n", markdown_to_html(text)));
        }
        ContentBlock::Code { language, code } => {
            let lang = language
                .as_deref()
                .map(|l| format!(" data-lang=\"{}\"", html_escape(l)))
                .unwrap_or_default();
            html.push_str(&format!(
                "<pre{}><code>{}</code></pre>\n",
                lang,
                html_escape(code)
            ));
        }
        ContentBlock::ToolUse {
            tool_name,
            tool_id,
            input,
            ..
        } => {
            // 工具调用默认折叠；若能配对到结果则一并放进同一折叠块
            html.push_str("<details class=\"block\">\n");
            html.push_str(&format!(
                "<summary>Tool · {}</summary>\n",
                html_escape(tool_name)
            ));
            if let Ok(pretty) = serde_json::to_string_pretty(input) {
                html.push_str(&format!("<pre><code>{}</code></pre>\n", html_escape(&pretty)));
            }
            if let Some(id) = tool_id {
                if let Some((content, is_error)) = result_map.get(id) {
                    let label = if *is_error { "结果（错误）" } else { "结果" };
                    html.push_str(&format!("<div class=\"sub\">{}</div>\n", label));
                    html.push_str(&format!("<pre><code>{}</code></pre>\n", html_escape(content)));
                }
            }
            html.push_str("</details>\n");
        }
        ContentBlock::ToolResult {
            tool_id,
            content,
            is_error,
        } => {
            // 已被对应 ToolUse 合并展示的结果，这里跳过，避免重复
            if let Some(id) = tool_id {
                if tool_use_ids.contains(id) {
                    return;
                }
            }
            let cls = if *is_error { "block error" } else { "block" };
            let label = if *is_error {
                "Tool Result · 错误"
            } else {
                "Tool Result"
            };
            html.push_str(&format!("<details class=\"{}\">\n", cls));
            html.push_str(&format!("<summary>{}</summary>\n", label));
            html.push_str(&format!("<pre><code>{}</code></pre>\n", html_escape(content)));
            html.push_str("</details>\n");
        }
        ContentBlock::Thinking { text } => {
            // 思考块默认折叠
            html.push_str("<details class=\"block\">\n");
            html.push_str("<summary>Thinking</summary>\n");
            html.push_str(&format!("<div class=\"text\">{}</div>\n", html_escape(text)));
            html.push_str("</details>\n");
        }
        ContentBlock::Image { source, media_type } => {
            match resolve_image_src(source, media_type.as_deref()) {
                Some(src) => html.push_str(&format!(
                    "<figure><img alt=\"image\" src=\"{}\"></figure>\n",
                    html_escape_attr(&src)
                )),
                None => html.push_str(
                    "<figure><div class=\"img-missing\">[图片无法加载]</div></figure>\n",
                ),
            }
        }
    }
}

/// 单张内嵌图片的大小上限（超过则不内嵌，避免导出 HTML 过大）
const MAX_INLINE_IMAGE_BYTES: u64 = 10 * 1024 * 1024;

/// 把图片来源解析成可在「自包含 HTML」中直接显示的 src：
/// - data: / http(s) → 原样
/// - 本地文件路径（含 file://、~ 开头）→ 读取文件并 base64 内嵌（超限/读失败返回 None）
/// - 疑似裸 base64 → 拼成 data URI
///
/// 返回 None 表示无法显示（调用方据此渲染占位）。
pub fn resolve_image_src(source: &str, media_type: Option<&str>) -> Option<String> {
    let s = source.trim();
    if s.starts_with("data:") || s.starts_with("http://") || s.starts_with("https://") {
        return Some(s.to_string());
    }
    // 裸 base64 优先判定：用「字符集」而非「是否含 '/'」——base64 本身就包含 '/'，
    // 用含 '/' 排除会把图片数据误判成文件路径。文件路径含 '.'、'-'、中文等非 base64 字符，可区分。
    if is_probably_base64(s) {
        let mt = media_type.unwrap_or("image/png");
        return Some(format!("data:{};base64,{}", mt, s));
    }
    // 其余视为本地文件路径（绝对路径 / file:// / ~）→ 读取内嵌
    let path = s.strip_prefix("file://").unwrap_or(s);
    read_image_as_data_uri(path)
}

/// 是否疑似裸 base64：足够长且仅由 base64 字符集（A-Za-z0-9+/=，含换行）组成
fn is_probably_base64(s: &str) -> bool {
    s.len() > 200
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(b, b'+' | b'/' | b'=' | b'\n' | b'\r')
        })
}

/// 读取本地图片文件并编码为 data URI；文件不存在/过大/读失败返回 None
pub fn read_image_as_data_uri(path: &str) -> Option<String> {
    use base64::Engine;

    let expanded = expand_tilde(path);
    let meta = std::fs::metadata(&expanded).ok()?;
    if !meta.is_file() || meta.len() > MAX_INLINE_IMAGE_BYTES {
        return None;
    }
    let bytes = std::fs::read(&expanded).ok()?;
    let mime = guess_image_mime(&expanded);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:{};base64,{}", mime, b64))
}

/// 把开头的 `~` 展开为用户主目录
fn expand_tilde(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    std::path::PathBuf::from(path)
}

/// 按扩展名猜测图片 MIME 类型
fn guess_image_mime(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        _ => "image/png",
    }
}

/// 把 Markdown 渲染为安全的 HTML：
/// - 支持表格/删除线/任务列表
/// - 图片来源经 resolve_image_src 解析（本地图片内嵌 base64）
/// - 安全：丢弃 Markdown 中内嵌的原始 HTML，按纯文本转义，防止 session 内容注入脚本
fn markdown_to_html(md: &str) -> String {
    use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag};

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(md, opts).map(|event| match event {
        // 图片：把本地路径/裸 base64 解析成可显示 src；无法解析则置空，浏览器回退显示 alt
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let resolved = resolve_image_src(&dest_url, None).unwrap_or_default();
            Event::Start(Tag::Image {
                link_type,
                dest_url: CowStr::from(resolved),
                title,
                id,
            })
        }
        // 原始 HTML 一律降级为纯文本（push_html 会转义），避免注入
        Event::Html(h) => Event::Text(h),
        Event::InlineHtml(h) => Event::Text(h),
        other => other,
    });

    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

/// HTML 文本转义（用于元素内容），防止 session 内容破坏页面结构
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// HTML 属性值转义（额外处理引号）
fn html_escape_attr(s: &str) -> String {
    html_escape(s).replace('"', "&quot;").replace('\'', "&#39;")
}

#[cfg(test)]
mod html_smoke_test {
    use super::*;
    use crate::models::{Message, Role, SessionSummary, ToolKind};

    #[test]
    fn writes_sample_html() {
        let messages = vec![
            Message {
                id: "m1".into(),
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "帮我加 HTML 导出，<script>alert(1)</script> 要被转义".into(),
                }],
                timestamp: None,
                model: None,
                usage: None,
            },
            Message {
                id: "m2".into(),
                role: Role::Assistant,
                content: vec![
                    ContentBlock::Thinking { text: "先看现有结构".into() },
                    ContentBlock::Text {
                        text: "## 小结\n\n**好的**，新增 to_html。".into(),
                    },
                    ContentBlock::ToolUse {
                        tool_name: "Bash".into(),
                        tool_id: Some("t1".into()),
                        input: serde_json::json!({"command": "cat export.rs"}),
                        agent_id: None,
                    },
                    ContentBlock::Code {
                        language: Some("rust".into()),
                        code: "fn main() { println!(\"hi <>&\"); }".into(),
                    },
                ],
                timestamp: None,
                model: Some("claude-opus-4-8".into()),
                usage: None,
            },
            Message {
                id: "m3".into(),
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_id: Some("t1".into()),
                    content: "已读取 export.rs".into(),
                    is_error: false,
                }],
                timestamp: None,
                model: None,
                usage: None,
            },
        ];
        let session = Session {
            summary: SessionSummary {
                id: "demo".into(),
                tool: ToolKind::ClaudeCode,
                title: "给 SessionViewer 增加 HTML 导出".into(),
                project_path: Some("~/workspace/aicoder-session-viewer".into()),
                started_at: None,
                updated_at: None,
                message_count: 3,
                total_tokens: None,
            },
            messages,
        };
        let html = to_html(&session);
        // 关键断言：转义生效、折叠块存在、结果被合并进 ToolUse（不重复独立渲染）
        assert!(html.contains("<details class=\"ai-body\" open>"));
        // m3 只含工具结果（已与 t1 配对），不应产生空的用户气泡 —— 全文只有 m1 一个气泡
        assert_eq!(html.matches("<div class=\"user-bubble\">").count(), 1);
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("<details class=\"block\">"));
        assert!(html.contains("结果")); // 合并的工具结果
        // t1 的结果应被 ToolUse 合并，不再有独立的 "Tool Result" 折叠块
        assert!(!html.contains("<summary>Tool Result</summary>"));
        // Markdown 渲染生效：标题与加粗都转成了 HTML 标签
        assert!(html.contains("<h2>小结</h2>"));
        assert!(html.contains("<strong>好的</strong>"));
    }

    #[test]
    fn inlines_local_image_as_base64() {
        // 写一张最小 PNG 到临时文件，验证本地路径图片被读出来并内嵌成 data URI
        let png: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
        let mut path = std::env::temp_dir();
        path.push("export_test_image.png");
        std::fs::write(&path, png).unwrap();
        let path_str = path.to_string_lossy().to_string();

        // 直接传绝对路径
        let resolved = resolve_image_src(&path_str, None).unwrap();
        assert!(resolved.starts_with("data:image/png;base64,"));

        // Markdown 里的本地图片引用也应被内嵌
        let md_html = markdown_to_html(&format!("![海报]({})", path_str));
        assert!(md_html.contains("src=\"data:image/png;base64,"));

        // data: 与 http 来源原样返回
        assert_eq!(
            resolve_image_src("data:image/png;base64,AAAA", None).as_deref(),
            Some("data:image/png;base64,AAAA")
        );
        // 不存在的本地路径 → None（调用方渲染占位）
        assert!(resolve_image_src("/no/such/file/xyz.png", None).is_none());

        // 裸 base64（含 '/'，模拟 Codex 用户图片）应识别为图片而非路径
        let fake_b64 = format!("iVBOR{}/{}+ABC==", "A".repeat(120), "Z".repeat(120));
        let resolved_b64 = resolve_image_src(&fake_b64, Some("image/png")).unwrap();
        assert_eq!(resolved_b64, format!("data:image/png;base64,{}", fake_b64));

        std::fs::remove_file(&path).ok();
    }
}
