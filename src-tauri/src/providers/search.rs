//! 会话内容全文搜索的共享匹配逻辑
//!
//! 设计原则：搜索范围与「用户在界面上看到的正文」一致——
//! 匹配消息正文、思考过程、代码块和工具调用参数；
//! 排除两类高噪音内容：
//! - 工具返回结果（常含整个文件内容/命令输出，几乎任何关键字都会命中）
//! - Claude Code 注入的 `<system-reminder>`（每个会话都含 skills 列表等模板文本）

use std::borrow::Cow;

use crate::models::{ContentBlock, Message};

/// ASCII 大小写不敏感的子串匹配
///
/// 对英文忽略大小写，对中文等多字节字符做精确匹配（UTF-8 多字节序列
/// 不含 ASCII 字母字节，逐字节比较即等价于精确匹配）。
///
/// 性能关键路径：会话原始内容预筛要扫描 GB 级数据，
/// 用 memchr（SIMD）定位首字节候选位置，再做窗口比较，
/// 避免朴素 O(n·m) 全量逐字节比较。
pub fn contains_ci(haystack: &str, needle: &str) -> bool {
    let n = needle.as_bytes();
    if n.is_empty() {
        return true;
    }
    let h = haystack.as_bytes();
    if n.len() > h.len() {
        return false;
    }

    // 首字节的大小写两种形态（非 ASCII 字母时两者相同）
    let lo = n[0].to_ascii_lowercase();
    let up = n[0].to_ascii_uppercase();
    // 候选起点的最大下标，保证窗口不越界
    let last_start = h.len() - n.len();

    let mut pos = 0;
    while pos <= last_start {
        // SIMD 加速查找下一个首字节候选
        let found = if lo == up {
            memchr::memchr(lo, &h[pos..])
        } else {
            memchr::memchr2(lo, up, &h[pos..])
        };
        let Some(off) = found else {
            return false;
        };
        let i = pos + off;
        if i > last_start {
            return false;
        }
        if h[i..i + n.len()].eq_ignore_ascii_case(n) {
            return true;
        }
        pos = i + 1;
    }
    false
}

/// 移除文本中所有 `<system-reminder>...</system-reminder>` 段落
///
/// Claude Code 会在用户消息里注入系统提示（skills 列表、hook 输出等），
/// 这些模板文本会让常见关键字命中几乎所有会话，搜索前必须剔除。
/// 未闭合的标签视为延伸到文本末尾。
pub fn strip_system_reminders(text: &str) -> Cow<'_, str> {
    const OPEN: &str = "<system-reminder>";
    const CLOSE: &str = "</system-reminder>";

    if !text.contains(OPEN) {
        return Cow::Borrowed(text);
    }

    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        out.push_str(&rest[..start]);
        match rest[start..].find(CLOSE) {
            Some(end_rel) => rest = &rest[start + end_rel + CLOSE.len()..],
            // 未闭合：剩余部分全部视为 reminder 内容丢弃
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// 递归检查 JSON 中的字符串/数字「值」是否包含关键字
///
/// 只匹配值不匹配键名，避免 `command`/`file_path` 等通用键名
/// 让搜索命中所有带工具调用的会话
fn json_values_match(value: &serde_json::Value, query: &str) -> bool {
    match value {
        serde_json::Value::String(s) => contains_ci(s, query),
        serde_json::Value::Number(n) => contains_ci(&n.to_string(), query),
        serde_json::Value::Array(arr) => arr.iter().any(|v| json_values_match(v, query)),
        serde_json::Value::Object(map) => map.values().any(|v| json_values_match(v, query)),
        _ => false,
    }
}

/// 判断单个内容块是否命中关键字（搜索范围见模块注释）
pub fn block_matches_query(block: &ContentBlock, query: &str) -> bool {
    match block {
        ContentBlock::Text { text } => contains_ci(&strip_system_reminders(text), query),
        ContentBlock::Thinking { text } => contains_ci(text, query),
        ContentBlock::Code { code, .. } => contains_ci(code, query),
        ContentBlock::ToolUse { input, .. } => json_values_match(input, query),
        // 工具结果与图片不参与搜索：结果常含整文件内容，噪音过大
        ContentBlock::ToolResult { .. } | ContentBlock::Image { .. } => false,
    }
}

/// 判断单条消息是否命中关键字
pub fn message_matches_query(msg: &Message, query: &str) -> bool {
    msg.content.iter().any(|b| block_matches_query(b, query))
}

/// 判断消息列表中是否有任意一条命中关键字
pub fn messages_match_query(messages: &[Message], query: &str) -> bool {
    messages.iter().any(|m| message_matches_query(m, query))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Role;

    fn msg(blocks: Vec<ContentBlock>) -> Message {
        Message {
            id: "test".into(),
            role: Role::User,
            content: blocks,
            timestamp: None,
            model: None,
            usage: None,
        }
    }

    #[test]
    fn contains_ci_ignores_ascii_case() {
        assert!(contains_ci("Deploy the APP", "deploy"));
        assert!(contains_ci("deploy", "DEPLOY"));
        assert!(!contains_ci("deplo", "deploy"));
    }

    #[test]
    fn contains_ci_matches_chinese_exactly() {
        assert!(contains_ci("准备部署到生产环境", "部署"));
        assert!(!contains_ci("准备发布到生产环境", "部署"));
    }

    #[test]
    fn strip_removes_reminder_segments() {
        let text = "正文A<system-reminder>含 deploy 的注入</system-reminder>正文B";
        assert_eq!(strip_system_reminders(text), "正文A正文B");
    }

    #[test]
    fn strip_removes_multiple_and_unclosed() {
        let text = "a<system-reminder>x</system-reminder>b<system-reminder>未闭合";
        assert_eq!(strip_system_reminders(text), "ab");
        // 无标签时原样返回（借用，不分配）
        assert!(matches!(
            strip_system_reminders("plain"),
            Cow::Borrowed("plain")
        ));
    }

    #[test]
    fn text_block_matches_but_reminder_content_does_not() {
        let hit = msg(vec![ContentBlock::Text {
            text: "帮我写个 deploy 脚本".into(),
        }]);
        assert!(message_matches_query(&hit, "deploy"));

        // 关键字仅出现在 system-reminder 内 → 不命中
        let noise = msg(vec![ContentBlock::Text {
            text: "<system-reminder>skills: check the deploy every 5 minutes</system-reminder>".into(),
        }]);
        assert!(!message_matches_query(&noise, "deploy"));
    }

    #[test]
    fn thinking_and_code_blocks_match() {
        let m = msg(vec![
            ContentBlock::Thinking {
                text: "需要先 deploy".into(),
            },
            ContentBlock::Code {
                language: Some("bash".into()),
                code: "kubectl apply".into(),
            },
        ]);
        assert!(message_matches_query(&m, "deploy"));
        assert!(message_matches_query(&m, "kubectl"));
    }

    #[test]
    fn tool_use_matches_values_not_keys() {
        let m = msg(vec![ContentBlock::ToolUse {
            tool_name: "Bash".into(),
            tool_id: None,
            input: serde_json::json!({"command": "./deploy.sh --env prod", "timeout": 8080}),
            agent_id: None,
        }]);
        // 命中参数值（命令内容、数字）
        assert!(message_matches_query(&m, "deploy.sh"));
        assert!(message_matches_query(&m, "8080"));
        // 键名不参与匹配
        assert!(!message_matches_query(&m, "timeout"));
    }

    #[test]
    fn tool_result_is_excluded() {
        let m = msg(vec![ContentBlock::ToolResult {
            tool_id: None,
            content: "读到的文件里有 deploy 字样".into(),
            is_error: false,
        }]);
        assert!(!message_matches_query(&m, "deploy"));
    }
}
