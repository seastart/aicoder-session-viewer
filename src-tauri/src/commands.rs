use std::process::Command as StdCommand;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::models::{Message, Session, SessionSummary, ToolKind};
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

/// 获取 Claude Code subagent 的对话消息（懒加载）
#[tauri::command]
pub fn get_subagent_messages(
    session_id: String,
    agent_id: String,
    registry: State<ProviderRegistry>,
) -> AppResult<Vec<Message>> {
    registry.get_subagent_messages(&session_id, &agent_id)
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

/// 导出 session 为 JSONL 格式
#[tauri::command]
pub fn export_session_jsonl(
    tool: String,
    session_id: String,
    save_path: String,
    registry: State<ProviderRegistry>,
) -> AppResult<()> {
    let tool_kind = ToolKind::from_str_loose(&tool)
        .ok_or_else(|| AppError::Provider(format!("未知工具类型: {}", tool)))?;
    let session = registry.get_session(tool_kind, &session_id)?;
    let content = crate::export::to_jsonl(&session);
    std::fs::write(&save_path, content)?;
    Ok(())
}

/// 导出 session 为 Markdown 格式
#[tauri::command]
pub fn export_session_markdown(
    tool: String,
    session_id: String,
    save_path: String,
    registry: State<ProviderRegistry>,
) -> AppResult<()> {
    let tool_kind = ToolKind::from_str_loose(&tool)
        .ok_or_else(|| AppError::Provider(format!("未知工具类型: {}", tool)))?;
    let session = registry.get_session(tool_kind, &session_id)?;
    let content = crate::export::to_markdown(&session);
    std::fs::write(&save_path, content)?;
    Ok(())
}

/// 恢复历史 session（在系统终端中启动对应 AI 工具并 resume）
#[tauri::command]
pub fn resume_session(
    tool: String,
    session_id: String,
    project_path: Option<String>,
) -> AppResult<()> {
    let tool_kind = ToolKind::from_str_loose(&tool)
        .ok_or_else(|| AppError::Provider(format!("未知工具类型: {}", tool)))?;

    // 构建各工具的 resume 命令字符串
    let command = match tool_kind {
        ToolKind::ClaudeCode => format!("claude --resume {}", shell_escape(&session_id)),
        // Codex 的内部 ID 格式为 rollout-{timestamp}-{uuid}，CLI 只需要 UUID 部分
        ToolKind::Codex => {
            let codex_id = extract_codex_uuid(&session_id);
            format!("codex resume {}", shell_escape(&codex_id))
        }
        ToolKind::Gemini => format!("gemini --resume {}", shell_escape(&session_id)),
        ToolKind::OpenCode => format!("opencode --session {}", shell_escape(&session_id)),
    };

    let cwd = project_path.unwrap_or_else(|| ".".to_string());
    launch_in_terminal(&cwd, &command)
}

/// 在指定项目目录中打开新 session
#[tauri::command]
pub fn open_new_session(tool: String, project_path: String) -> AppResult<()> {
    let tool_kind = ToolKind::from_str_loose(&tool)
        .ok_or_else(|| AppError::Provider(format!("未知工具类型: {}", tool)))?;

    let command = match tool_kind {
        ToolKind::ClaudeCode => "claude".to_string(),
        ToolKind::Codex => "codex".to_string(),
        ToolKind::Gemini => "gemini".to_string(),
        ToolKind::OpenCode => "opencode".to_string(),
    };

    launch_in_terminal(&project_path, &command)
}

// ── 跨平台终端启动 ──────────────────────────────────────────

/// 在系统终端模拟器中执行命令（交互式 TUI 程序需要真正的终端窗口）
///
/// `cwd` 为空或非有效目录时，直接执行命令不做 cd
fn launch_in_terminal(cwd: &str, command: &str) -> AppResult<()> {
    // 判断 cwd 是否是真实存在的绝对路径目录
    let valid_cwd = if !cwd.is_empty() && std::path::Path::new(cwd).is_absolute() && std::path::Path::new(cwd).is_dir() {
        Some(cwd)
    } else {
        None
    };
    // 组合最终命令：有有效目录则先 cd，否则直接执行
    let full_command = match valid_cwd {
        Some(dir) => format!("cd '{}' && {}", escape_single_quotes(dir), command),
        None => command.to_string(),
    };
    #[cfg(target_os = "macos")]
    {
        launch_terminal_macos(&full_command)
    }
    #[cfg(target_os = "linux")]
    {
        launch_terminal_linux(&full_command)
    }
    #[cfg(target_os = "windows")]
    {
        launch_terminal_windows(&full_command, valid_cwd)
    }
}

/// macOS: 自动检测用户终端，通过 AppleScript 打开并执行命令
///
/// 检测优先级：
/// 1. 当前正在运行的终端应用（用户大概率就用那个）
/// 2. 已安装的常见终端（检查 /Applications）
/// 3. 兜底 Terminal.app（macOS 自带）
#[cfg(target_os = "macos")]
fn launch_terminal_macos(full_command: &str) -> AppResult<()> {
    let terminal = detect_macos_terminal();

    let script = build_applescript(&terminal, full_command);

    StdCommand::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .map_err(|e| AppError::Provider(format!("启动 {} 失败: {}", terminal, e)))?;

    Ok(())
}

/// macOS 支持的终端类型
#[cfg(target_os = "macos")]
#[derive(Debug)]
enum MacTerminal {
    ITerm2,
    WarpTerminal,
    Kitty,
    Alacritty,
    Ghostty,
    Terminal, // macOS 自带
}

#[cfg(target_os = "macos")]
impl std::fmt::Display for MacTerminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ITerm2 => write!(f, "iTerm2"),
            Self::WarpTerminal => write!(f, "Warp"),
            Self::Kitty => write!(f, "Kitty"),
            Self::Alacritty => write!(f, "Alacritty"),
            Self::Ghostty => write!(f, "Ghostty"),
            Self::Terminal => write!(f, "Terminal"),
        }
    }
}

/// 检测 macOS 上用户正在使用的终端
#[cfg(target_os = "macos")]
fn detect_macos_terminal() -> MacTerminal {
    // 候选列表：进程名 → 终端类型
    let candidates = [
        ("iTerm2", MacTerminal::ITerm2),
        ("Warp", MacTerminal::WarpTerminal),
        ("kitty", MacTerminal::Kitty),
        ("Alacritty", MacTerminal::Alacritty),
        ("Ghostty", MacTerminal::Ghostty),
    ];

    // 优先检测正在运行的终端进程
    if let Ok(output) = StdCommand::new("ps")
        .args(["-eo", "comm"])
        .output()
    {
        let ps_output = String::from_utf8_lossy(&output.stdout);
        for (process_name, terminal) in &candidates {
            if ps_output.lines().any(|line| line.contains(process_name)) {
                return match terminal {
                    MacTerminal::ITerm2 => MacTerminal::ITerm2,
                    MacTerminal::WarpTerminal => MacTerminal::WarpTerminal,
                    MacTerminal::Kitty => MacTerminal::Kitty,
                    MacTerminal::Alacritty => MacTerminal::Alacritty,
                    MacTerminal::Ghostty => MacTerminal::Ghostty,
                    MacTerminal::Terminal => MacTerminal::Terminal,
                };
            }
        }
    }

    // 其次检查已安装的应用（/Applications 目录）
    let app_checks = [
        ("/Applications/iTerm.app", MacTerminal::ITerm2),
        ("/Applications/Warp.app", MacTerminal::WarpTerminal),
        ("/Applications/kitty.app", MacTerminal::Kitty),
        ("/Applications/Alacritty.app", MacTerminal::Alacritty),
        ("/Applications/Ghostty.app", MacTerminal::Ghostty),
    ];

    for (path, terminal) in &app_checks {
        if std::path::Path::new(path).exists() {
            return match terminal {
                MacTerminal::ITerm2 => MacTerminal::ITerm2,
                MacTerminal::WarpTerminal => MacTerminal::WarpTerminal,
                MacTerminal::Kitty => MacTerminal::Kitty,
                MacTerminal::Alacritty => MacTerminal::Alacritty,
                MacTerminal::Ghostty => MacTerminal::Ghostty,
                MacTerminal::Terminal => MacTerminal::Terminal,
            };
        }
    }

    // 兜底：macOS 自带 Terminal.app
    MacTerminal::Terminal
}

/// 根据终端类型生成对应的 AppleScript
/// 不同终端的 AppleScript 接口各不相同
#[cfg(target_os = "macos")]
fn build_applescript(terminal: &MacTerminal, shell_cmd: &str) -> String {
    let escaped = shell_cmd.replace('\\', "\\\\").replace('"', "\\\"");

    match terminal {
        // iTerm2: 新建 tab（加载用户 shell profile 确保 PATH 完整），再写入命令
        MacTerminal::ITerm2 => format!(
            r#"tell application "iTerm"
    activate
    if (count of windows) > 0 then
        tell current window
            set newTab to (create tab with default profile)
        end tell
    else
        set newWindow to (create window with default profile)
    end if
    tell current session of current window
        write text "{cmd}"
    end tell
end tell"#,
            cmd = escaped
        ),
        // Warp: 通过 do script 执行（Warp 会自动加载用户 shell）
        MacTerminal::WarpTerminal => format!(
            r#"tell application "Warp"
    activate
end tell
delay 0.3
tell application "System Events"
    tell process "Warp"
        keystroke "t" using command down
    end tell
end tell
delay 0.3
tell application "System Events"
    tell process "Warp"
        keystroke "{cmd}"
        key code 36
    end tell
end tell"#,
            cmd = escaped
        ),
        // Kitty / Alacritty / Ghostty: 不支持 AppleScript，通过命令行启动
        MacTerminal::Kitty => format!(
            r#"do shell script "/Applications/kitty.app/Contents/MacOS/kitty /bin/sh -c '{}' &"
delay 0.5
tell application "kitty" to activate"#,
            shell_cmd.replace('\'', "'\\''")
        ),
        MacTerminal::Alacritty => format!(
            r#"do shell script "/Applications/Alacritty.app/Contents/MacOS/alacritty -e /bin/sh -c '{}' &"
delay 0.5
tell application "Alacritty" to activate"#,
            shell_cmd.replace('\'', "'\\''")
        ),
        MacTerminal::Ghostty => format!(
            r#"do shell script "/Applications/Ghostty.app/Contents/MacOS/ghostty -e /bin/sh -c '{}' &"
delay 0.5
tell application "Ghostty" to activate"#,
            shell_cmd.replace('\'', "'\\''")
        ),
        // Terminal.app: do script 会自动打开新窗口并加载用户 shell
        // Terminal.app 的 do script 本身就是在用户 shell 中执行，PATH 完整
        MacTerminal::Terminal => format!(
            r#"tell application "Terminal"
    activate
    do script "{}"
end tell"#,
            escaped
        ),
    }
}

/// 转义单引号（用于嵌入 shell 单引号字符串）
#[cfg(target_os = "macos")]
fn escape_single_quotes(s: &str) -> String {
    s.replace('\'', "'\\''")
}

/// Linux: 通过 x-terminal-emulator 或常见终端打开
#[cfg(target_os = "linux")]
fn launch_terminal_linux(full_command: &str) -> AppResult<()> {
    let shell_cmd = full_command.to_string();

    // 按优先级尝试常见终端模拟器
    let terminals = [
        ("x-terminal-emulator", vec!["-e"]),
        ("gnome-terminal", vec!["--"]),
        ("konsole", vec!["-e"]),
        ("xfce4-terminal", vec!["-e"]),
        ("xterm", vec!["-e"]),
    ];

    for (term, prefix_args) in &terminals {
        let mut cmd = StdCommand::new(term);
        for arg in prefix_args {
            cmd.arg(arg);
        }
        cmd.arg("sh").arg("-c").arg(&shell_cmd);

        if cmd.spawn().is_ok() {
            return Ok(());
        }
    }

    Err(AppError::Provider(
        "未找到可用的终端模拟器，请安装 gnome-terminal / konsole / xterm".to_string(),
    ))
}

/// Windows: 通过 cmd.exe 打开新终端窗口
#[cfg(target_os = "windows")]
fn launch_terminal_windows(full_command: &str, cwd: Option<&str>) -> AppResult<()> {
    let win_cmd = match cwd {
        Some(dir) => format!("cd /d \"{}\" && {}", dir, full_command),
        None => full_command.to_string(),
    };

    StdCommand::new("cmd")
        .args(["/c", "start", "cmd", "/k"])
        .arg(&win_cmd)
        .spawn()
        .map_err(|e| AppError::Provider(format!("启动终端失败: {}", e)))?;

    Ok(())
}

/// 对 shell 参数进行简单转义（防止注入）
fn shell_escape(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// 从 Codex 内部 session ID 中提取 UUID
///
/// 内部 ID 格式: `rollout-2026-03-27T10-09-45-019d2d0e-01e4-7480-98a1-e9c9bf633fa2`
/// Codex CLI 需要的: `019d2d0e-01e4-7480-98a1-e9c9bf633fa2` (标准 UUID, 36 字符)
fn extract_codex_uuid(session_id: &str) -> String {
    // UUID 格式: 8-4-4-4-12 = 36 字符（含连字符）
    if session_id.len() >= 36 {
        let candidate = &session_id[session_id.len() - 36..];
        // 验证是否符合 UUID 格式
        let parts: Vec<&str> = candidate.split('-').collect();
        if parts.len() == 5
            && parts[0].len() == 8
            && parts[1].len() == 4
            && parts[2].len() == 4
            && parts[3].len() == 4
            && parts[4].len() == 12
            && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_hexdigit()))
        {
            return candidate.to_string();
        }
    }
    // 无法提取时原样返回
    session_id.to_string()
}
