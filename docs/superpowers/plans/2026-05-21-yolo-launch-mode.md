# YOLO 启动模式 + 默认项目视图 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在手动恢复会话和新建会话两个入口上，支持以 "YOLO 模式"（绕过权限确认）启动 AI CLI；同时将默认侧边栏视图从 `flat` 改为 `grouped`。

**Architecture:**
- 后端：复用现有 `build_resume_command` 中的 bypass 逻辑，把 `ResumeLaunchMode` 拓展成承载 `bypass_permissions` 标志；为 new session 路径新增对称的 `build_new_session_command`；Tauri 命令签名各加一个 `bypass_permissions: bool` 参数。
- 前端：新增 `useAltKeyPressed` hook 监听 Alt 全局按下状态，`YoloHint` 组件做"按住即显"的徽标；在 `ChatView` 的 Resume 按钮和 `ProjectTree` 的 New Session 下拉项上接入 Alt+Click 与右键菜单两条触发路径；OpenCode 做友好降级。
- 项目无自动化测试基础设施（无 `tests/` 目录、无 `vitest`/`cargo test` 配置），验证流程是 `cargo check` + `npx tsc --noEmit` + 在 `pnpm tauri dev` 里手动验证终端命令是否带上对应 bypass 参数。

**Tech Stack:** Rust (Tauri 命令、`std::process::Command`)、React + TypeScript、Zustand、Tailwind、`lucide-react`。

**Spec:** `docs/superpowers/specs/2026-05-21-yolo-launch-mode-design.md`

---

## Task 1: 后端 — 重构 `ResumeLaunchMode` 以承载 `bypass_permissions`

**Files:**
- Modify: `src-tauri/src/commands.rs:185-260`

把 `ResumeLaunchMode` 从"是否定时启动"语义切换成"启动语义 + 是否需要 bypass 权限"，给手动 resume 留出可选 bypass 通道。

- [ ] **Step 1: 修改 `ResumeLaunchMode` enum 定义**

将 `src-tauri/src/commands.rs:185-195` 当前代码：

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResumeLaunchMode {
    Interactive,
    ScheduledAutoContinue,
}

impl ResumeLaunchMode {
    fn needs_unattended_permissions(self) -> bool {
        matches!(self, Self::ScheduledAutoContinue)
    }
}
```

改成：

```rust
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
```

- [ ] **Step 2: 更新 `resume_session` 调用点**

将 `src-tauri/src/commands.rs:102-103` 当前代码：

```rust
    let command =
        build_resume_command(tool_kind, &session_id, None, ResumeLaunchMode::Interactive)?;
```

改成（注意：`bypass_permissions` 参数还没加，此处暂时硬编码为 `false`，下一个 Task 才会加参数）：

```rust
    let command = build_resume_command(
        tool_kind,
        &session_id,
        None,
        ResumeLaunchMode::Interactive { bypass_permissions: false },
    )?;
```

- [ ] **Step 3: 运行 `cargo check` 验证编译**

```bash
cd src-tauri && cargo check
```

Expected: 编译通过，没有 warning 关于未使用的变体。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "refactor(backend): ResumeLaunchMode 拓展 bypass_permissions 字段"
```

---

## Task 2: 后端 — `resume_session` 命令新增 `bypass_permissions` 参数

**Files:**
- Modify: `src-tauri/src/commands.rs:92-107`

让前端可以显式指定是否以 YOLO 模式恢复。

- [ ] **Step 1: 修改 `resume_session` 命令签名与函数体**

将 `src-tauri/src/commands.rs:92-107` 替换为：

```rust
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
```

- [ ] **Step 2: 运行 `cargo check`**

```bash
cd src-tauri && cargo check
```

Expected: 编译通过。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat(backend): resume_session 新增 bypass_permissions 参数"
```

---

## Task 3: 后端 — 新增 `build_new_session_command` 并扩展 `open_new_session`

**Files:**
- Modify: `src-tauri/src/commands.rs:136-150`（`open_new_session`）
- Modify: `src-tauri/src/commands.rs:197-260`（在 `build_resume_command` 之后新增 `build_new_session_command`）

复用三家 CLI 的 bypass 拼接规则，但用一个独立的 helper 避免和 resume 路径的语义混在一起。

- [ ] **Step 1: 新增 `build_new_session_command` helper**

在 `src-tauri/src/commands.rs` 的 `build_resume_command` 函数结束（约第 260 行 `Ok(command)` 之后）之后，新增：

```rust
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
```

- [ ] **Step 2: 修改 `open_new_session` 命令使用新 helper**

将 `src-tauri/src/commands.rs:136-150` 当前代码替换为：

```rust
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
```

- [ ] **Step 3: 在 `build_resume_command` 的 OpenCode 分支也加 TODO 注释**

在 `src-tauri/src/commands.rs` 中找到 `build_resume_command` 函数的 `ToolKind::OpenCode` 分支（约第 249 行），在该 match arm 起始处加一行注释：

将：

```rust
        ToolKind::OpenCode => {
            let mut command = format!("opencode --session {}", shell_escape(session_id));
```

改成：

```rust
        // TODO(opencode-yolo): OpenCode CLI 暂无 bypass-approvals 开关，
        // 待上游加入后，需要在这里同时根据 `launch_mode.needs_unattended_permissions()` 拼接。
        ToolKind::OpenCode => {
            let mut command = format!("opencode --session {}", shell_escape(session_id));
```

- [ ] **Step 4: 运行 `cargo check`**

```bash
cd src-tauri && cargo check
```

Expected: 编译通过。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat(backend): open_new_session 支持 bypass_permissions + 新增 build_new_session_command"
```

---

## Task 4: 前端 — 新增 `useAltKeyPressed` hook

**Files:**
- Create: `src/hooks/useAltKeyPressed.ts`

全局监听 Alt 键的按下/松开状态，供 ChatView / ProjectTree 共用，避免每个组件各装一份监听。

- [ ] **Step 1: 创建 hook 文件**

写入 `src/hooks/useAltKeyPressed.ts`：

```ts
import { useEffect, useState } from "react";

/**
 * 监听全局 Alt（macOS 上是 Option）键的按下状态。
 *
 * 用于在 UI 上实时反馈"按住 Alt 将以 YOLO 模式启动"。注意需要在 window blur 时
 * 把状态重置为 false——否则用户按住 Alt 切换到其它窗口再切回来，会卡在按下态。
 */
export function useAltKeyPressed(): boolean {
  const [pressed, setPressed] = useState(false);

  useEffect(() => {
    const handleDown = (e: KeyboardEvent) => {
      if (e.key === "Alt") setPressed(true);
    };
    const handleUp = (e: KeyboardEvent) => {
      if (e.key === "Alt") setPressed(false);
    };
    const handleBlur = () => setPressed(false);

    window.addEventListener("keydown", handleDown);
    window.addEventListener("keyup", handleUp);
    window.addEventListener("blur", handleBlur);

    return () => {
      window.removeEventListener("keydown", handleDown);
      window.removeEventListener("keyup", handleUp);
      window.removeEventListener("blur", handleBlur);
    };
  }, []);

  return pressed;
}
```

- [ ] **Step 2: 运行 TypeScript 检查**

```bash
npx tsc --noEmit
```

Expected: 通过，无错误。

- [ ] **Step 3: Commit**

```bash
git add src/hooks/useAltKeyPressed.ts
git commit -m "feat(frontend): 新增 useAltKeyPressed hook 监听全局 Alt 按下"
```

---

## Task 5: 前端 — 新增 `YoloHint` 徽标组件

**Files:**
- Create: `src/components/common/YoloHint.tsx`

一个非常小的视觉徽标，在按下 Alt 时渲染在按钮文案末尾。

- [ ] **Step 1: 检查 common 目录是否存在**

```bash
ls src/components/common 2>/dev/null || mkdir -p src/components/common
```

- [ ] **Step 2: 创建组件文件**

写入 `src/components/common/YoloHint.tsx`：

```tsx
import { Zap } from "lucide-react";

/**
 * YOLO 模式徽标：仅在 Alt 按下时由调用方条件渲染。
 *
 * 配色用项目里的 accent / warning 语义色（reuse Tailwind 的 amber 系），
 * 表达"危险但有意识"的意图。
 */
export function YoloHint({ className = "" }: { className?: string }) {
  return (
    <span
      className={`inline-flex items-center gap-0.5 rounded bg-amber-500/15 px-1 py-[1px] text-[10px] font-medium text-amber-500 ${className}`}
      aria-label="YOLO mode"
    >
      <Zap size={10} />
      YOLO
    </span>
  );
}
```

- [ ] **Step 3: 运行 TypeScript 检查**

```bash
npx tsc --noEmit
```

Expected: 通过。

- [ ] **Step 4: Commit**

```bash
git add src/components/common/YoloHint.tsx
git commit -m "feat(frontend): 新增 YoloHint 徽标组件"
```

---

## Task 6: 前端 — 新增 YOLO 相关 i18n 文案

**Files:**
- Modify: `src/i18n/locales/zh.ts`
- Modify: `src/i18n/locales/en.ts`

- [ ] **Step 1: 扩展 `Locale` 接口**

在 `src/i18n/locales/zh.ts:52-58` 的"恢复会话"段落末尾、`// 项目视图` 注释之前，插入：

```ts
  // YOLO 模式
  yoloMode: string;
  yoloResumeMenuItem: string;
  yoloNewSessionMenuItem: string;
  yoloAltHint: string;
  yoloUnsupportedOpenCode: string;
```

接口完整新增段（插入位置：`autoContinueError` 之后、`// 项目视图` 之前）应当形如：

```ts
  autoContinueError: (err: string) => string;

  // YOLO 模式
  yoloMode: string;
  yoloResumeMenuItem: string;
  yoloNewSessionMenuItem: string;
  yoloAltHint: string;
  yoloUnsupportedOpenCode: string;

  // 项目视图
```

- [ ] **Step 2: 添加中文翻译值**

在 `src/i18n/locales/zh.ts` 的 `const zh: Locale = { ... }` 内，`autoContinueError` 之后、`viewFlat` 之前，插入：

```ts
  // YOLO 模式
  yoloMode: "YOLO 模式",
  yoloResumeMenuItem: "以 YOLO 模式恢复会话",
  yoloNewSessionMenuItem: "以 YOLO 模式新建",
  yoloAltHint: "按住 ⌥ 切换到 YOLO 模式",
  yoloUnsupportedOpenCode: "OpenCode 暂不支持 YOLO 模式",
```

- [ ] **Step 3: 添加英文翻译值**

在 `src/i18n/locales/en.ts` 的 `autoContinueError` 之后、`viewFlat` 之前，插入：

```ts
  // YOLO mode
  yoloMode: "YOLO mode",
  yoloResumeMenuItem: "Resume in YOLO mode",
  yoloNewSessionMenuItem: "Open new YOLO session",
  yoloAltHint: "Hold ⌥ for YOLO mode",
  yoloUnsupportedOpenCode: "OpenCode does not support YOLO mode yet",
```

- [ ] **Step 4: 运行 TypeScript 检查**

```bash
npx tsc --noEmit
```

Expected: 通过。两份 locale 文件结构对齐、Locale 接口不报缺字段。

- [ ] **Step 5: Commit**

```bash
git add src/i18n/locales/zh.ts src/i18n/locales/en.ts
git commit -m "feat(i18n): 新增 YOLO 模式相关文案"
```

---

## Task 7: 前端 — `ChatView` Resume 按钮接入 Alt + 右键

**Files:**
- Modify: `src/components/Chat/ChatView.tsx:1-29`（imports）
- Modify: `src/components/Chat/ChatView.tsx:133-144`（`handleResume`）
- Modify: `src/components/Chat/ChatView.tsx:246-256`（Resume 按钮 JSX）

- [ ] **Step 1: 追加 imports**

`lucide-react` 中的 `Zap` 已在原 imports 里，无需新增。仅追加 `useAltKeyPressed` 和 `YoloHint`：

在 `src/components/Chat/ChatView.tsx` 最末一行 `import` 之后（约第 35 行 `from "../../utils/sessionSearch"` 之后）追加：

```tsx
import { useAltKeyPressed } from "../../hooks/useAltKeyPressed";
import { YoloHint } from "../common/YoloHint";
```

- [ ] **Step 2: 在组件内拿到 alt 按下状态、判断 OpenCode**

在 `ChatView` 函数体内 `const config = TOOL_CONFIG[summary.tool];`（约第 120 行）之后，加入：

```tsx
  const altPressed = useAltKeyPressed();
  // OpenCode 暂不支持 bypass 开关，UI 上需要做友好降级
  const yoloSupported = summary.tool !== "open_code";
  const [resumeMenuOpen, setResumeMenuOpen] = useState(false);
```

注意 `useState` 已在文件第 1 行 import，无需重复 import。

在 `useEffect` 段落里追加一个关闭右键菜单的点击监听（放在 `setExportOpen` 关闭逻辑附近，约第 60-70 行的 useEffect 之后）：

```tsx
  useEffect(() => {
    if (!resumeMenuOpen) return;
    const close = () => setResumeMenuOpen(false);
    document.addEventListener("click", close);
    return () => document.removeEventListener("click", close);
  }, [resumeMenuOpen]);
```

- [ ] **Step 3: 重写 `handleResume` 支持 bypass 参数**

将 `src/components/Chat/ChatView.tsx:133-144` 当前 `handleResume` 替换为：

```tsx
  /** 恢复会话；bypass=true 时以 YOLO 模式启动 */
  const handleResume = async (opts: { bypass: boolean } = { bypass: false }) => {
    // OpenCode 不支持 bypass：即使用户按了 Alt 也只能按普通模式启动
    const effectiveBypass = opts.bypass && yoloSupported;
    try {
      await invoke("resume_session", {
        tool: summary.tool,
        sessionId: summary.id,
        projectPath: summary.project_path,
        bypassPermissions: effectiveBypass,
      });
    } catch (err) {
      console.error("Resume failed:", err);
    }
  };
```

- [ ] **Step 4: 改造 Resume 按钮的 JSX（onClick + onContextMenu + 徽标 + tooltip）**

将 `src/components/Chat/ChatView.tsx:247-256` 当前 Resume 按钮：

```tsx
            <button
              onClick={handleResume}
              className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded px-2 py-1 text-xs text-text-muted transition-colors hover:bg-surface-hover hover:text-text-primary"
              title={t.resumeSession}
            >
              <Play size={12} />
              <span className="hidden whitespace-nowrap md:inline">
                {t.resumeSession}
              </span>
            </button>
```

替换为：

```tsx
            <div className="relative shrink-0">
              <button
                onClick={(e) => handleResume({ bypass: e.altKey })}
                onContextMenu={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  setResumeMenuOpen((open) => !open);
                }}
                className="flex shrink-0 items-center gap-1 whitespace-nowrap rounded px-2 py-1 text-xs text-text-muted transition-colors hover:bg-surface-hover hover:text-text-primary"
                title={yoloSupported ? `${t.resumeSession} · ${t.yoloAltHint}` : t.resumeSession}
              >
                <Play size={12} />
                <span className="hidden whitespace-nowrap md:inline">
                  {t.resumeSession}
                </span>
                {altPressed && yoloSupported && <YoloHint />}
              </button>

              {resumeMenuOpen && (
                <div
                  className="absolute right-0 top-full mt-1 z-20 w-48 rounded-md border border-border bg-surface shadow-lg"
                  onClick={(e) => e.stopPropagation()}
                >
                  <button
                    onClick={() => {
                      setResumeMenuOpen(false);
                      if (yoloSupported) handleResume({ bypass: true });
                    }}
                    disabled={!yoloSupported}
                    className="flex w-full items-center gap-2 px-3 py-2 text-xs text-text-primary hover:bg-surface-hover transition-colors disabled:cursor-not-allowed disabled:text-text-muted disabled:hover:bg-transparent"
                    title={yoloSupported ? undefined : t.yoloUnsupportedOpenCode}
                  >
                    <Zap size={12} />
                    {t.yoloResumeMenuItem}
                  </button>
                </div>
              )}
            </div>
```

- [ ] **Step 5: 运行 TypeScript 检查**

```bash
npx tsc --noEmit
```

Expected: 通过。

- [ ] **Step 6: Commit**

```bash
git add src/components/Chat/ChatView.tsx
git commit -m "feat(frontend): Resume 按钮支持 Alt+点击 / 右键以 YOLO 模式恢复"
```

---

## Task 8: 前端 — `ProjectTree` 新建 session 菜单接入 Alt + 右键

**Files:**
- Modify: `src/components/Sidebar/ProjectTree.tsx:1-21`（imports）
- Modify: `src/components/Sidebar/ProjectTree.tsx:91-113`（`ProjectFolder` 函数体）
- Modify: `src/components/Sidebar/ProjectTree.tsx:170-186`（工具项 button JSX）

- [ ] **Step 1: 追加 imports**

`ProjectTree.tsx` 的工具项 JSX 不会直接用到 `Zap`（图标已封装在 `YoloHint` 内部），所以只追加两行 import。

在 `src/components/Sidebar/ProjectTree.tsx:21` 的 `import type { Locale as DateLocale } from "date-fns/locale";` 之后追加：

```tsx
import { useAltKeyPressed } from "../../hooks/useAltKeyPressed";
import { YoloHint } from "../common/YoloHint";
```

- [ ] **Step 2: 在 `ProjectFolder` 函数体内引入 Alt 状态并扩展 `handleNewSession`**

将 `src/components/Sidebar/ProjectTree.tsx:91-113` 当前代码：

```tsx
}) {
  const hasContent = node.sessions.length > 0 || node.children.length > 0;
  const [toolMenuOpen, setToolMenuOpen] = useState(false);

  // 点击外部关闭菜单
  useEffect(() => {
    if (!toolMenuOpen) return;
    const close = () => setToolMenuOpen(false);
    document.addEventListener("click", close);
    return () => document.removeEventListener("click", close);
  }, [toolMenuOpen]);

  const handleNewSession = async (tool: ToolKind) => {
    if (!node.path) return;
    setToolMenuOpen(false);
    try {
      await invoke("open_new_session", {
        tool,
        projectPath: node.path,
      });
    } catch (err) {
      console.error("Failed to open new session:", err);
    }
  };
```

替换为：

```tsx
}) {
  const hasContent = node.sessions.length > 0 || node.children.length > 0;
  const [toolMenuOpen, setToolMenuOpen] = useState(false);
  const altPressed = useAltKeyPressed();

  // 点击外部关闭菜单
  useEffect(() => {
    if (!toolMenuOpen) return;
    const close = () => setToolMenuOpen(false);
    document.addEventListener("click", close);
    return () => document.removeEventListener("click", close);
  }, [toolMenuOpen]);

  /**
   * 启动新 session；bypass=true 时以 YOLO 模式启动。
   * OpenCode 暂不支持 bypass，调用方需要先判断 yoloSupported。
   */
  const handleNewSession = async (
    tool: ToolKind,
    opts: { bypass: boolean } = { bypass: false },
  ) => {
    if (!node.path) return;
    setToolMenuOpen(false);
    const effectiveBypass = opts.bypass && tool !== "open_code";
    try {
      await invoke("open_new_session", {
        tool,
        projectPath: node.path,
        bypassPermissions: effectiveBypass,
      });
    } catch (err) {
      console.error("Failed to open new session:", err);
    }
  };
```

- [ ] **Step 3: 改造工具下拉项 JSX**

将 `src/components/Sidebar/ProjectTree.tsx:170-186` 当前工具项渲染：

```tsx
                {(Object.keys(TOOL_CONFIG) as ToolKind[]).map((tool) => {
                  const cfg = TOOL_CONFIG[tool];
                  return (
                    <button
                      key={tool}
                      onClick={() => handleNewSession(tool)}
                      className="flex w-full items-center gap-2 px-3 py-1.5 text-xs hover:bg-surface-hover transition-colors"
                    >
                      <span
                        className="inline-block h-2 w-2 rounded-full"
                        style={{ backgroundColor: cfg.color }}
                      />
                      <span className="text-text-primary">{cfg.label}</span>
                    </button>
                  );
                })}
```

替换为：

```tsx
                {(Object.keys(TOOL_CONFIG) as ToolKind[]).map((tool) => {
                  const cfg = TOOL_CONFIG[tool];
                  const yoloSupported = tool !== "open_code";
                  const showYoloBadge = altPressed && yoloSupported;
                  return (
                    <button
                      key={tool}
                      onClick={(e) =>
                        handleNewSession(tool, { bypass: e.altKey && yoloSupported })
                      }
                      onContextMenu={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        if (yoloSupported) {
                          handleNewSession(tool, { bypass: true });
                        }
                      }}
                      className="flex w-full items-center gap-2 px-3 py-1.5 text-xs hover:bg-surface-hover transition-colors"
                      title={
                        yoloSupported
                          ? `${cfg.label} · ${t.yoloAltHint}`
                          : t.yoloUnsupportedOpenCode
                      }
                    >
                      <span
                        className="inline-block h-2 w-2 rounded-full"
                        style={{ backgroundColor: cfg.color }}
                      />
                      <span className="text-text-primary">{cfg.label}</span>
                      {showYoloBadge && <YoloHint className="ml-auto" />}
                    </button>
                  );
                })}
```

- [ ] **Step 4: 运行 TypeScript 检查**

```bash
npx tsc --noEmit
```

Expected: 通过。

- [ ] **Step 5: Commit**

```bash
git add src/components/Sidebar/ProjectTree.tsx
git commit -m "feat(frontend): 新建 session 菜单支持 Alt+点击 / 右键以 YOLO 模式启动"
```

---

## Task 9: 前端 — 默认视图改为项目视图

**Files:**
- Modify: `src/stores/sessionStore.ts:43`

- [ ] **Step 1: 修改 store 初始 viewMode**

将 `src/stores/sessionStore.ts:43` 当前代码：

```ts
  viewMode: "flat",
```

替换为：

```ts
  viewMode: "grouped",
```

- [ ] **Step 2: 运行 TypeScript 检查**

```bash
npx tsc --noEmit
```

Expected: 通过。

- [ ] **Step 3: Commit**

```bash
git add src/stores/sessionStore.ts
git commit -m "feat(frontend): 默认视图改为项目视图"
```

---

## Task 10: 端到端手动验证

**Files:** 无修改，验证步骤。

- [ ] **Step 1: 启动开发环境**

```bash
pnpm tauri dev
```

Expected: 应用启动后，默认侧边栏视图为"项目视图"，不再是列表视图。

- [ ] **Step 2: 验证 Resume 普通模式（Claude / Codex / Gemini 任选一个有 session 的工具）**

操作：选中一个会话 → 点击"恢复会话"按钮（不按 Alt）。

Expected: 系统终端打开，命令行不包含 `--permission-mode bypassPermissions` / `--dangerously-bypass-approvals-and-sandbox` / `--approval-mode yolo` 任何一个。

- [ ] **Step 3: 验证 Resume YOLO 模式（Alt+点击）**

操作：选中一个 Claude 会话 → 按住 Option（macOS）/Alt（Win/Linux）→ 观察按钮文案末尾应出现 `⚡YOLO` 徽标 → 点击按钮。

Expected: 终端启动的命令为 `claude --permission-mode bypassPermissions --resume <id>`。

重复对 Codex 和 Gemini 验证一遍，确认带上各自的 bypass 参数。

- [ ] **Step 4: 验证 Resume YOLO 模式（右键）**

操作：在 Resume 按钮上右键 → 弹出"以 YOLO 模式恢复会话"菜单 → 点击。

Expected: 与 Step 3 同样以 bypass 模式启动。

- [ ] **Step 5: 验证 OpenCode 友好降级**

操作：选中一个 OpenCode 会话 → 按住 Alt 时观察按钮，YOLO 徽标不显示 → 右键 → 菜单项灰态、tooltip 显示"OpenCode 暂不支持 YOLO 模式" → 即便点击 Alt+按钮，启动的也是普通 `opencode --session <id>`。

Expected: 终端命令不包含任何 bypass 标志，且 UI 上 YOLO 徽标在 OpenCode session 中不出现。

- [ ] **Step 6: 验证 New Session 普通 / YOLO 模式**

操作：在项目文件夹的 "+" 按钮 → 弹出工具下拉。
1. 普通点击 Claude → 终端启动 `claude`，不带 bypass。
2. Alt+点击 Claude → 终端启动 `claude --permission-mode bypassPermissions`。
3. 右键 Claude → 同样以 bypass 启动。
4. Alt+点击 OpenCode → 终端启动 `opencode`（普通模式），无报错。

Expected: 与上文描述一致。

- [ ] **Step 7: 验证窗口失焦时 YOLO 徽标自动消失**

操作：按住 Alt → 切换到另一个 app（不松开 Alt）→ 切回本 app。

Expected: YOLO 徽标已消失（避免卡在按下态）。

- [ ] **Step 8: 全部通过后不需要 commit；如发现问题，回到对应 Task 修复并重新验证。**

---

## Self-Review 备注

写完计划后我自查过：

- **Spec 覆盖**：5 项交互（命名、触发、视觉反馈、入口位置、OpenCode 降级）+ 后端改造 + 前端改造 + 默认视图 + i18n + 测试方案，均已对应到具体 Task。
- **占位符**：所有代码块都展示了真实代码，没有 "TODO / TBD"；OpenCode 的 `TODO(opencode-yolo)` 是有意的代码锚点（spec 明确要求），不是计划占位符。
- **类型一致性**：后端参数名统一 `bypass_permissions`（Rust）；Tauri IPC 传 `bypassPermissions`（驼峰约定）；前端 hook 名 `useAltKeyPressed`、组件名 `YoloHint`、i18n 键 `yolo*` 全程一致。`ResumeLaunchMode::Interactive { bypass_permissions }` 在 Task 1、2 中签名一致。
