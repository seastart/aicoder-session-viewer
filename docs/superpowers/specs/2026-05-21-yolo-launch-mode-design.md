# YOLO 启动模式 + 默认项目视图 — 设计稿

日期：2026-05-21

## 背景

后端已经实现了三家 AI CLI 的"高权限"启动参数（Claude `--permission-mode bypassPermissions`、
Codex `--dangerously-bypass-approvals-and-sandbox`、Gemini `--approval-mode yolo`），但目前
只在 `resume_session_with_auto_continue`（定时自动 continue）一条路径上被使用。

用户希望在**手动恢复会话**和**新建会话**两个入口上，也能可选地以高权限模式启动 CLI，
用来跑那些"我知道自己在干嘛、别再问我了"的会话。

同时，把 Sidebar 的默认视图从"时间视图（flat）"换成"项目视图（grouped）"。

## 命名

UI 文案统一叫 **"YOLO 模式"**（社区通用口语化标签，三家 CLI 实际选项名不同，但这个词大家都懂）。
代码层面使用中性字段：`bypassPermissions: boolean`。

## 交互设计

### 触发方式（两个入口共用）

1. **`Option`/`Alt` + 点击** — 熟手快速通道
2. **右键** → 弹出单项菜单 "以 YOLO 模式启动" — 显式入口、保发现性

不加二次确认弹窗：触发动作本身已经足够显式（修饰键或右键），再加 confirm 会让熟手嫌烦。

### 视觉反馈

- 按钮 hover tooltip 增加一行小字：`⌥ 切换到 YOLO 模式`
- 按住 `Option`/`Alt` 时，按钮文案末尾追加一个 `⚡YOLO` 徽标，松开恢复 —— 让用户在按下前
  就能"看到"自己将触发什么
- 这一视觉效果通过全局 keydown/keyup 监听 `Alt` 键的按下状态实现，存到一个轻量
  `useAltKeyPressed()` hook 里供 ChatView / ProjectTree 共用

### 入口位置

**A. ChatView 头部 — Resume 按钮**
- 改造 `handleResume`：接受 `bypassPermissions: boolean` 参数
- 单击 → `bypassPermissions = e.altKey`
- 右键 → 阻止默认菜单，弹自定义单项菜单 `"以 YOLO 模式恢复会话"`，点击后 `bypassPermissions = true`

**B. ProjectTree 文件夹 — New Session 工具下拉菜单**
- 改造 `handleNewSession(tool)`：接受 `bypassPermissions: boolean` 参数
- 下拉菜单中每个工具项支持 `Alt + Click` 触发 YOLO 启动
- 同样支持右键工具项 → "以 YOLO 模式新建"

### OpenCode 的降级

OpenCode CLI 目前没有 bypass 开关。当用户对 OpenCode 触发 YOLO 时：

- 前端不真正发出 YOLO 请求，回退到普通启动
- Tooltip / 菜单项灰态提示 `"OpenCode 暂不支持 YOLO 模式"`
- 后端 `build_resume_command` 在 `bypassPermissions = true` 且 `tool = OpenCode` 时静默忽略
  bypass 标记（防御性兜底）
- 代码里在 OpenCode 分支留 `// TODO(opencode-yolo): 待 OpenCode CLI 支持 bypass-approvals 开关后接入`
  作为锚点

## 后端改造

### 复用现有 `needs_unattended_permissions` 逻辑

`build_resume_command` 当前用 `ResumeLaunchMode` 区分"是否需要 bypass"。这里把语义从
"是否定时启动" 解耦成"是否要求 bypass 权限"：

```rust
// 新结构
struct ResumeLaunchOptions {
    bypass_permissions: bool,
    auto_continue: bool, // 仅 ScheduledAutoContinue 用得到
}
```

或者更轻一步：保留 `ResumeLaunchMode` 但新增 `Interactive { bypass: bool }` 变体：

```rust
enum ResumeLaunchMode {
    Interactive { bypass_permissions: bool },
    ScheduledAutoContinue, // 隐含 bypass_permissions = true
}
```

**推荐第二种**：改动小、语义清晰、ScheduledAutoContinue 的"必须 bypass"约束被类型系统固化。

### 命令签名变更

```rust
#[tauri::command]
pub fn resume_session(
    tool: String,
    session_id: String,
    project_path: Option<String>,
    bypass_permissions: bool, // 新增
) -> AppResult<()>

#[tauri::command]
pub fn open_new_session(
    tool: String,
    project_path: String,
    bypass_permissions: bool, // 新增
) -> AppResult<()>
```

`open_new_session` 当前直接拼工具名启动，没复用 `build_resume_command`。需要新增一个对称
的 `build_new_session_command(tool, bypass_permissions)` 来生成命令字符串，避免新建路径
和 resume 路径的 bypass 参数拼接逻辑漂移。

`resume_session_with_auto_continue` 保持现状（定时自动 continue 隐含必带 bypass）。

## 前端改造

### 新增 hook：`useAltKeyPressed()`

```ts
// src/hooks/useAltKeyPressed.ts
// 全局监听 Alt 键按下/松开状态，用于在按钮上实时显示 YOLO 徽标。
// 注意：window blur 时要重置为 false，避免切走再切回 UI 卡在按下态。
```

### 新增组件：`YoloHint`（小徽标）

按钮右侧条件渲染的 `⚡YOLO` 小标签，用 accent / warning 色。

### ChatView 改造点

- `handleResume(opts: { bypass: boolean })`
- 按钮：`onClick={e => handleResume({ bypass: e.altKey })}`
- 按钮：`onContextMenu` → 自定义单项菜单
- OpenCode session 的按钮，右键菜单项灰态、tooltip 给出说明

### ProjectTree 改造点

- `handleNewSession(tool, opts: { bypass: boolean })`
- 下拉菜单工具项同样支持 Alt + Click 和右键
- OpenCode 项灰态

### 默认视图

`src/stores/sessionStore.ts:43` 的 `viewMode` 初值从 `"flat"` 改成 `"grouped"`。
不做持久化（如果用户希望"记住上次选择"，是另一个改动，本期不做）。

## i18n

中英文新增：

- `yoloMode`: "YOLO 模式" / "YOLO mode"
- `yoloResumeMenuItem`: "以 YOLO 模式恢复会话" / "Resume in YOLO mode"
- `yoloNewSessionMenuItem`: "以 YOLO 模式新建" / "Open new YOLO session"
- `yoloAltHint`: "⌥ 切换到 YOLO 模式" / "Hold ⌥ for YOLO mode"
- `yoloUnsupportedOpenCode`: "OpenCode 暂不支持 YOLO 模式" / "OpenCode does not support YOLO mode yet"

## 测试方案

手动验证清单（无自动化测试，参考现有 resume 路径）：

- [ ] Claude / Codex / Gemini：常规点击 Resume → 普通模式启动，终端命令不含 bypass 参数
- [ ] Claude / Codex / Gemini：Alt + 点击 Resume → 终端命令包含对应 bypass 参数
- [ ] Claude / Codex / Gemini：右键 Resume → 菜单弹出，点击后以 YOLO 启动
- [ ] OpenCode：Alt + 点击 / 右键 → 不触发 YOLO，菜单项灰态、tooltip 提示
- [ ] New Session 下拉菜单：Alt + Click 工具项 → YOLO 新建
- [ ] 按住 Alt 时按钮显示 `⚡YOLO` 徽标，松开消失，窗口失焦时也消失
- [ ] 默认视图启动为"项目视图"

## 不做的事（YAGNI）

- 不做 YOLO 模式的全局开关 / 偏好设置 —— 每次都显式触发
- 不做用户视图选择的持久化 —— 单纯换默认值
- 不做 YOLO 模式的二次确认弹窗
- 不做 OpenCode 的 YOLO 实现（等上游）
