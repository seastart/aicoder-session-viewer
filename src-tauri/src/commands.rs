use chrono::TimeZone;
use std::process::Command as StdCommand;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, State};

use crate::config::{self, ProviderConfig, ProviderPaths};
use crate::error::{AppError, AppResult};
use crate::models::{Message, Session, SessionSummary, ToolKind};
use crate::providers::{claude, codex, gemini, opencode, ProviderRegistry};

/// 列出所有工具的 session
#[tauri::command]
pub fn list_all_sessions(
    registry: State<Arc<RwLock<ProviderRegistry>>>,
) -> AppResult<Vec<SessionSummary>> {
    // RwLock::read 仅在锁被毒化（持锁线程 panic 过）时才会失败；
    // 这种情况下整个进程已不可靠，让它直接 panic 即可。
    registry.read().unwrap().list_all_sessions()
}

/// 列出指定工具的 session
#[tauri::command]
pub fn list_sessions(
    tool: String,
    registry: State<Arc<RwLock<ProviderRegistry>>>,
) -> AppResult<Vec<SessionSummary>> {
    let tool_kind = ToolKind::from_str_loose(&tool)
        .ok_or_else(|| crate::error::AppError::Provider(format!("未知工具类型: {}", tool)))?;
    registry.read().unwrap().list_sessions_by_tool(tool_kind)
}

/// 获取完整 session（含所有消息）
#[tauri::command]
pub fn get_session(
    tool: String,
    session_id: String,
    registry: State<Arc<RwLock<ProviderRegistry>>>,
) -> AppResult<Session> {
    let tool_kind = ToolKind::from_str_loose(&tool)
        .ok_or_else(|| crate::error::AppError::Provider(format!("未知工具类型: {}", tool)))?;
    registry.read().unwrap().get_session(tool_kind, &session_id)
}

/// 获取 Claude Code subagent 的对话消息（懒加载）
#[tauri::command]
pub fn get_subagent_messages(
    session_id: String,
    agent_id: String,
    registry: State<Arc<RwLock<ProviderRegistry>>>,
) -> AppResult<Vec<Message>> {
    registry
        .read()
        .unwrap()
        .get_subagent_messages(&session_id, &agent_id)
}

/// 搜索 session
#[tauri::command]
pub fn search_sessions(
    query: String,
    tool: Option<String>,
    registry: State<Arc<RwLock<ProviderRegistry>>>,
) -> AppResult<Vec<SessionSummary>> {
    let tool_kind = tool.and_then(|t| ToolKind::from_str_loose(&t));
    registry.read().unwrap().search_sessions(&query, tool_kind)
}

/// 导出 session 为 JSONL 格式
#[tauri::command]
pub fn export_session_jsonl(
    tool: String,
    session_id: String,
    save_path: String,
    registry: State<Arc<RwLock<ProviderRegistry>>>,
) -> AppResult<()> {
    let tool_kind = ToolKind::from_str_loose(&tool)
        .ok_or_else(|| AppError::Provider(format!("未知工具类型: {}", tool)))?;
    let session = registry.read().unwrap().get_session(tool_kind, &session_id)?;
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
    registry: State<Arc<RwLock<ProviderRegistry>>>,
) -> AppResult<()> {
    let tool_kind = ToolKind::from_str_loose(&tool)
        .ok_or_else(|| AppError::Provider(format!("未知工具类型: {}", tool)))?;
    let session = registry.read().unwrap().get_session(tool_kind, &session_id)?;
    let content = crate::export::to_markdown(&session);
    std::fs::write(&save_path, content)?;
    Ok(())
}

/// 恢复历史 session（在系统终端中启动对应 AI 工具并 resume）
///
/// `bypass_permissions = true` 时启动 YOLO 模式：跳过工具的权限确认提示。
/// OpenCode 暂不支持 bypass 开关，会被静默降级为普通启动。
#[tauri::command]
pub fn resume_session(
    tool: String,
    session_id: String,
    project_path: Option<String>,
    bypass_permissions: bool,
) -> AppResult<()> {
    let tool_kind = ToolKind::from_str_loose(&tool)
        .ok_or_else(|| AppError::Provider(format!("未知工具类型: {}", tool)))?;

    let command = build_resume_command(
        tool_kind,
        &session_id,
        None,
        ResumeLaunchMode::Interactive { bypass_permissions },
    )?;

    let cwd = project_path.unwrap_or_else(|| ".".to_string());
    launch_in_terminal(&cwd, &command)
}

/// 在 reset 时间到达后恢复历史 session，并附带一条 `continue` prompt
///
/// 第一性原理上，这类需求本质不是“模拟键盘输入”，而是“在某个时间点恢复会话，
/// 并给模型一条后续用户消息”。只要目标 CLI 支持“resume + prompt”，就应该直接
/// 走这个原生命令，而不是向现有 TTY 注入字符。
#[tauri::command]
pub fn resume_session_with_auto_continue(
    tool: String,
    session_id: String,
    project_path: Option<String>,
    continue_at_ms: u64,
) -> AppResult<()> {
    let tool_kind = ToolKind::from_str_loose(&tool)
        .ok_or_else(|| AppError::Provider(format!("未知工具类型: {}", tool)))?;

    let continue_prompt = "continue";
    let command = build_resume_command(
        tool_kind,
        &session_id,
        Some(continue_prompt),
        ResumeLaunchMode::ScheduledAutoContinue,
    )?;
    let wrapped_command = wrap_command_with_delayed_launch(&command, continue_at_ms)?;
    let cwd = project_path.unwrap_or_else(|| ".".to_string());
    launch_in_terminal(&cwd, &wrapped_command)
}

/// 在指定项目目录中打开新 session
///
/// `bypass_permissions = true` 时启动 YOLO 模式。OpenCode 会被静默降级。
#[tauri::command]
pub fn open_new_session(
    tool: String,
    project_path: String,
    bypass_permissions: bool,
) -> AppResult<()> {
    let tool_kind = ToolKind::from_str_loose(&tool)
        .ok_or_else(|| AppError::Provider(format!("未知工具类型: {}", tool)))?;

    let command = build_new_session_command(tool_kind, bypass_permissions);
    launch_in_terminal(&project_path, &command)
}

// ── 跨平台终端启动 ──────────────────────────────────────────

/// 在系统终端模拟器中执行命令（交互式 TUI 程序需要真正的终端窗口）
///
/// `cwd` 为空或非有效目录时，由各平台直接执行原始命令
fn launch_in_terminal(cwd: &str, command: &str) -> AppResult<()> {
    // 先收敛成“可用工作目录”这个平台无关概念，再交给各平台决定如何切目录。
    // 这样可以避免把 POSIX shell 的转义规则错误地带到 Windows 分支里。
    let valid_cwd = if !cwd.is_empty()
        && std::path::Path::new(cwd).is_absolute()
        && std::path::Path::new(cwd).is_dir()
    {
        Some(cwd)
    } else {
        None
    };

    #[cfg(target_os = "macos")]
    {
        launch_terminal_macos(command, valid_cwd)
    }
    #[cfg(target_os = "linux")]
    {
        launch_terminal_linux(command, valid_cwd)
    }
    #[cfg(target_os = "windows")]
    {
        launch_terminal_windows(command, valid_cwd)
    }
}

/// 统一构建不同 AI 工具的 resume 命令，避免“普通恢复”和“自动 continue 恢复”
/// 分别维护两套分支。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResumeLaunchMode {
    /// 普通交互式恢复；`bypass_permissions = true` 表示以 YOLO 模式启动
    Interactive { bypass_permissions: bool },
    /// 定时自动 continue 路径，隐含必须 bypass（无人值守）
    ScheduledAutoContinue,
}

impl ResumeLaunchMode {
    fn needs_unattended_permissions(self) -> bool {
        matches!(
            self,
            Self::ScheduledAutoContinue | Self::Interactive { bypass_permissions: true }
        )
    }
}

fn build_resume_command(
    tool_kind: ToolKind,
    session_id: &str,
    prompt: Option<&str>,
    launch_mode: ResumeLaunchMode,
) -> AppResult<String> {
    let command = match tool_kind {
        ToolKind::ClaudeCode => {
            let mut command = String::from("claude");
            if launch_mode.needs_unattended_permissions() {
                command.push_str(" --permission-mode ");
                command.push_str(&shell_escape("bypassPermissions"));
            }
            command.push_str(" --resume ");
            command.push_str(&shell_escape(session_id));
            if let Some(prompt) = prompt {
                command.push(' ');
                command.push_str(&shell_escape(prompt));
            }
            command
        }
        // Codex 的内部 ID 格式为 rollout-{timestamp}-{uuid}，CLI 只需要 UUID 部分
        ToolKind::Codex => {
            let codex_id = extract_codex_uuid(session_id);
            let mut command = String::from("codex resume");
            if launch_mode.needs_unattended_permissions() {
                // 官方 CLI 文档要求：带子命令时，全局参数写在子命令后面。
                // 这里使用长参数名而不是 --yolo，兼容性更直接，也更易读。
                command.push_str(" --dangerously-bypass-approvals-and-sandbox");
            }
            command.push(' ');
            command.push_str(&shell_escape(&codex_id));
            if let Some(prompt) = prompt {
                command.push(' ');
                command.push_str(&shell_escape(prompt));
            }
            command
        }
        ToolKind::Gemini => {
            let mut command = String::from("gemini");
            if launch_mode.needs_unattended_permissions() {
                command.push_str(" --approval-mode ");
                command.push_str(&shell_escape("yolo"));
            }
            command.push_str(" --resume ");
            command.push_str(&shell_escape(session_id));
            if let Some(prompt) = prompt {
                command.push(' ');
                command.push_str(&shell_escape(prompt));
            }
            command
        }
        // TODO(opencode-yolo): OpenCode CLI 暂无 bypass-approvals 开关，
        // 待上游加入后，需要在这里同时根据 `launch_mode.needs_unattended_permissions()` 拼接。
        ToolKind::OpenCode => {
            let mut command = format!("opencode --session {}", shell_escape(session_id));
            if let Some(prompt) = prompt {
                command.push_str(" --prompt ");
                command.push_str(&shell_escape(prompt));
            }
            command
        }
    };

    Ok(command)
}

/// 构建 "新建 session" 的 CLI 启动命令。
///
/// 与 `build_resume_command` 对称，但不带 `--resume` 与 `prompt`：仅决定是否拼接 bypass 参数。
/// OpenCode 目前没有 bypass 开关，`bypass_permissions = true` 时会被静默忽略。
fn build_new_session_command(tool_kind: ToolKind, bypass_permissions: bool) -> String {
    match tool_kind {
        ToolKind::ClaudeCode => {
            if bypass_permissions {
                format!("claude --permission-mode {}", shell_escape("bypassPermissions"))
            } else {
                "claude".to_string()
            }
        }
        ToolKind::Codex => {
            if bypass_permissions {
                // 官方 CLI 文档要求：带子命令时全局参数写在子命令后面。
                // 新建 session 没有子命令，直接放主命令后即可。
                "codex --dangerously-bypass-approvals-and-sandbox".to_string()
            } else {
                "codex".to_string()
            }
        }
        ToolKind::Gemini => {
            if bypass_permissions {
                format!("gemini --approval-mode {}", shell_escape("yolo"))
            } else {
                "gemini".to_string()
            }
        }
        // TODO(opencode-yolo): OpenCode CLI 暂无 bypass-approvals 开关，
        // 待上游加入后在此分支补上对应参数。
        ToolKind::OpenCode => "opencode".to_string(),
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn wrap_command_with_delayed_launch(command: &str, continue_at_ms: u64) -> AppResult<String> {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| AppError::Provider(format!("读取系统时间失败: {}", e)))?
        .as_millis() as u64;
    let delay_seconds = continue_at_ms.saturating_sub(now_ms).div_ceil(1_000).max(0);
    let target_epoch_seconds = continue_at_ms.div_ceil(1_000);

    // 这里用本机本地时区展示计划时间即可，因为真正执行依赖的是绝对时间戳。
    let continue_at_text = chrono::Local
        .timestamp_millis_opt(continue_at_ms as i64)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| continue_at_ms.to_string());

    let scheduled_msg = if delay_seconds > 0 {
        format!(
            "[AICoder Session Viewer] 已计划在 {} 自动恢复会话并发送 continue。",
            continue_at_text
        )
    } else {
        "[AICoder Session Viewer] reset 时间已到，立即恢复会话并发送 continue。".to_string()
    };
    let firing_msg = "[AICoder Session Viewer] reset 时间已到，正在恢复会话。";

    if delay_seconds == 0 {
        return Ok(format!(
            "printf '%s\\n' '{scheduled}' ; exec {command}",
            scheduled = escape_single_quotes(&scheduled_msg),
            command = command
        ));
    }

    // 系统睡眠时，终端里的长 sleep 可能不会按墙上时间推进。
    // 因此这里以绝对 epoch 时间为准，短周期睡眠后反复校验；机器唤醒后最多再等
    // 一个轮询周期就会触发，而不是继续补完睡眠前剩余的数小时。
    Ok(format!(
        "printf '%s\\n' '{scheduled}' ; \
         target_epoch={target_epoch}; \
         while [ \"$(date +%s)\" -lt \"$target_epoch\" ]; do \
           now_epoch=$(date +%s); \
           remaining=$((target_epoch - now_epoch)); \
           if [ \"$remaining\" -le 0 ]; then break; fi; \
           if [ \"$remaining\" -gt 60 ]; then sleep 60; else sleep \"$remaining\"; fi; \
         done; \
         printf '%s\\n' '{firing}' ; \
         exec {command}",
        scheduled = escape_single_quotes(&scheduled_msg),
        target_epoch = target_epoch_seconds,
        firing = escape_single_quotes(firing_msg),
        command = command
    ))
}

#[cfg(target_os = "windows")]
fn wrap_command_with_delayed_launch(_command: &str, _continue_at_ms: u64) -> AppResult<String> {
    Err(AppError::Provider(
        "自动 continue 目前仅支持 macOS / Linux 终端。".to_string(),
    ))
}

/// macOS: 自动检测用户终端，通过 AppleScript 打开并执行命令
///
/// 检测优先级：
/// 1. 当前正在运行的终端应用（用户大概率就用那个）
/// 2. 已安装的常见终端（检查 /Applications）
/// 3. 兜底 Terminal.app（macOS 自带）
#[cfg(target_os = "macos")]
fn launch_terminal_macos(command: &str, cwd: Option<&str>) -> AppResult<()> {
    let full_command = prefix_cwd_for_posix_shell(command, cwd);
    let terminal = detect_macos_terminal();

    let script = build_applescript(&terminal, &full_command);

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
    if let Ok(output) = StdCommand::new("ps").args(["-eo", "comm"]).output() {
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
fn escape_single_quotes(s: &str) -> String {
    s.replace('\'', "'\\''")
}

/// 为 POSIX shell 命令按需补上工作目录切换。
fn prefix_cwd_for_posix_shell(command: &str, cwd: Option<&str>) -> String {
    match cwd {
        Some(dir) => format!("cd '{}' && {}", escape_single_quotes(dir), command),
        None => command.to_string(),
    }
}

/// Linux: 通过 x-terminal-emulator 或常见终端打开
#[cfg(target_os = "linux")]
fn launch_terminal_linux(command: &str, cwd: Option<&str>) -> AppResult<()> {
    let shell_cmd = prefix_cwd_for_posix_shell(command, cwd);

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
fn launch_terminal_windows(command: &str, cwd: Option<&str>) -> AppResult<()> {
    let win_cmd = match cwd {
        Some(dir) => format!("cd /d \"{}\" && {}", dir, command),
        None => command.to_string(),
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
            && parts
                .iter()
                .all(|p| p.chars().all(|c| c.is_ascii_hexdigit()))
        {
            return candidate.to_string();
        }
    }
    // 无法提取时原样返回
    session_id.to_string()
}

// ── 配置读写 IPC ────────────────────────────────────────────

/// 前端获取当前配置 + 各 provider 的默认路径预览
///
/// `defaults` 用于前端在“路径输入框”里展示占位提示：
/// 用户不填时，后端会用这些默认路径。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigResponse {
    config: ProviderConfig,
    defaults: ProviderPaths,
}

#[tauri::command]
pub fn get_provider_config(app: AppHandle) -> AppResult<ProviderConfigResponse> {
    let cfg = config::load(&app);
    // default_path 失败说明该平台/环境下连默认路径都拿不到（例如 HOME 未设置），
    // 这种情况展示为 None，让前端把输入框留空并提示用户手动配置即可。
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
///
/// 返回的 warnings 包含初始化失败的 provider（例如路径不存在），
/// 前端据此提示用户，但不阻塞保存流程。
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
    // 1. 先写文件；失败直接返回，不动内存状态，避免“磁盘和内存不一致”
    config::save(&app, &config)?;
    // 2. 热重载 registry；收集 warnings
    //    write 锁仅在毒化时失败，让其 panic 即可，与其他命令保持一致
    let warnings = registry.write().unwrap().reload(&config.provider_paths);
    Ok(UpdateProviderConfigResponse { warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn delayed_launch_uses_wall_clock_polling_instead_of_one_long_sleep() {
        let continue_at_ms = (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64)
            + 5 * 60 * 1_000;

        let command =
            wrap_command_with_delayed_launch("claude --resume abc continue", continue_at_ms)
                .unwrap();

        assert!(
            command.contains("date +%s"),
            "定时恢复应反复读取墙上时钟，避免系统睡眠后长 sleep 继续等待"
        );
        assert!(
            !command.contains("sleep 300"),
            "定时恢复不应生成一次性长 sleep"
        );
    }
}
