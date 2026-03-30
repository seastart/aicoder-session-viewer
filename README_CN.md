# AICoder Session Viewer

一个统一的桌面应用，用于浏览多种 AI 编码助手的对话历史 —— **Claude Code**、**Codex**、**Gemini CLI** 和 **OpenCode**，集中在一个界面查看。

[English](./README.md)

![截图](./public/screenshot.png)

## 为什么做这个

AI 编码助手各自使用不同的格式（JSONL / JSON / SQLite）将会话数据分散存储在不同目录中，没有统一的方式来浏览、搜索或对比这些对话。AICoder Session Viewer 在 Rust 侧将所有数据源归一化为统一模型，前端只需处理一套类型。

## 安装

从 [Releases](https://github.com/seastart/aicoder-session-viewer/releases) 页面下载最新版本。

### macOS

应用未使用 Apple 开发者证书签名，macOS Gatekeeper 可能会提示"应用已损坏"。执行以下命令解除限制：

```bash
xattr -cr /Applications/AICoder\ Session\ Viewer.app
```

### Windows / Linux

从 Release 页面下载 `.exe` / `.msi`（Windows）或 `.deb` / `.AppImage`（Linux）直接安装即可。

## 功能特性

- **多工具支持** — Claude Code、Codex、Gemini CLI、OpenCode
- **统一数据模型** — 所有格式在 Rust 侧归一化后再传给前端
- **丰富的内容渲染** — Markdown、Shiki 语法高亮代码块、可折叠的工具调用和思考过程
- **搜索与过滤** — 按工具类型筛选，按标题或项目路径搜索（300ms 防抖）
- **项目分组** — 按项目路径将 session 分组为可折叠的文件夹树，支持列表/分组视图切换
- **恢复会话** — 直接在终端中恢复历史 session（macOS 自动检测 iTerm2、Terminal.app、Warp、Kitty、Alacritty、Ghostty）
- **导出** — 支持将 session 导出为 JSONL 或 Markdown 格式
- **暗色主题** — 专为阅读对话设计的暗色 UI，每种工具有独立配色
- **快速轻量** — 原生 Tauri 应用，资源占用低；SQLite 只读访问

## 数据源

| 工具 | macOS / Linux | Windows | 格式 |
|------|--------------|---------|------|
| Claude Code | `~/.claude/projects/{path}/{uuid}.jsonl` | `%USERPROFILE%\.claude\projects\...` | JSONL（索引在 `~/.claude/sessions/*.json`）|
| Codex | `~/.codex/sessions/{Y}/{M}/{D}/rollout-*.jsonl` | `%USERPROFILE%\.codex\sessions\...` | JSONL |
| Gemini CLI | `~/.gemini/tmp/{project}/chats/session-*.json` | `%USERPROFILE%\.gemini\tmp\...` | JSON |
| OpenCode | `~/.local/share/opencode/opencode.db` | `%USERPROFILE%\.local\share\opencode\opencode.db` | SQLite |

> 四款工具在所有平台上均以用户主目录（`~` / `%USERPROFILE%`）为根目录，目录结构跨平台一致。

## 技术栈

| 层级 | 技术 |
|------|------|
| 桌面框架 | [Tauri v2](https://tauri.app/) |
| 后端 | Rust（serde、chrono、rusqlite、walkdir、thiserror）|
| 前端 | React 19 + TypeScript |
| 样式 | Tailwind CSS v4 |
| 状态管理 | Zustand |
| 代码高亮 | Shiki |
| Markdown 渲染 | react-markdown + remark-gfm |

## 环境要求

- [Node.js](https://nodejs.org/) >= 20
- [pnpm](https://pnpm.io/) >= 10
- [Rust](https://www.rust-lang.org/tools/install) >= 1.85
- Tauri v2 系统依赖 — 参见 [Tauri 前置条件](https://v2.tauri.app/start/prerequisites/)

## 快速开始

```bash
# 克隆仓库
git clone https://github.com/user/aicoder-session-viewer.git
cd aicoder-session-viewer

# 安装前端依赖
pnpm install

# 启动开发模式（编译 Rust + 启动 Vite 开发服务器）
pnpm tauri dev

# 临时用英文界面启动
VITE_LOCALE=en pnpm tauri dev

# 构建生产版本
pnpm tauri build
```

## 项目结构

```
aicoder-session-viewer/
├── src-tauri/                  # Rust 后端
│   └── src/
│       ├── main.rs             # 入口
│       ├── lib.rs              # Tauri 应用初始化与插件注册
│       ├── error.rs            # 统一错误类型（thiserror）
│       ├── models.rs           # 共享数据模型（SessionSummary、Message、ContentBlock 等）
│       ├── commands.rs         # Tauri IPC 命令
│       ├── export.rs           # Session 导出（JSONL / Markdown）
│       └── providers/          # 数据源实现
│           ├── mod.rs          # SessionProvider trait + ProviderRegistry
│           ├── claude.rs       # Claude Code（JSONL）
│           ├── codex.rs        # Codex（JSONL）
│           ├── gemini.rs       # Gemini CLI（JSON）
│           └── opencode.rs     # OpenCode（SQLite）
├── src/                        # React 前端
│   ├── App.tsx                 # 根组件
│   ├── App.css                 # Tailwind 导入 + 主题变量
│   ├── types.ts                # TypeScript 类型（对应 models.rs）
│   ├── stores/
│   │   └── sessionStore.ts     # Zustand 状态管理
│   ├── hooks/
│   │   └── useDebounce.ts      # 防抖 hook
│   ├── utils/
│   │   └── buildProjectTree.ts # Session 按项目路径构建树
│   └── components/
│       ├── Layout.tsx           # 双栏布局 + 视图切换
│       ├── Sidebar/
│       │   ├── SearchBar.tsx    # 带防抖的搜索输入
│       │   ├── ToolFilter.tsx   # 工具类型过滤标签
│       │   ├── SessionList.tsx  # 扁平 Session 列表
│       │   └── ProjectTree.tsx  # 项目分组文件夹树
│       └── Chat/
│           ├── ChatView.tsx     # Session 详情 + 恢复/导出按钮
│           ├── MessageBubble.tsx # 消息渲染（支持所有内容块类型）
│           ├── CodeBlock.tsx    # Shiki 语法高亮
│           └── ToolCallBlock.tsx # 可折叠的工具调用/结果块
├── index.html
├── package.json
├── vite.config.ts
└── tsconfig.json
```

## 架构设计

```
┌─────────────────────────────────────────────────────────┐
│  前端（React + TypeScript）                              │
│  ┌──────────────┐  ┌────────────────────────────────┐   │
│  │   侧边栏      │  │   对话视图                      │   │
│  │  - 搜索       │  │   - 消息气泡                    │   │
│  │  - 过滤       │  │   - Markdown / 代码 / 工具调用  │   │
│  │  - 列表       │  │   - 思考过程                    │   │
│  └──────┬───────┘  └───────────────┬────────────────┘   │
│         │  Zustand                  │                    │
│         └──────────┬────────────────┘                    │
│                    │ invoke()                            │
├────────────────────┼────────────────────────────────────┤
│  Tauri IPC         │                                    │
├────────────────────┼────────────────────────────────────┤
│  后端（Rust）       ▼                                    │
│  ┌─────────────────────────┐                            │
│  │   ProviderRegistry       │                            │
│  │  ┌───────┐ ┌───────┐   │                            │
│  │  │Claude │ │ Codex │   │   → SessionSummary         │
│  │  └───────┘ └───────┘   │   → Session                │
│  │  ┌───────┐ ┌────────┐  │   → Message                │
│  │  │Gemini │ │OpenCode│  │   → ContentBlock           │
│  │  └───────┘ └────────┘  │                            │
│  └─────────────────────────┘                            │
└─────────────────────────────────────────────────────────┘
```

四种数据源均实现 `SessionProvider` trait，归一化为统一模型后传给前端。如果某个 Provider 不可用（如目录不存在），会被静默跳过，不影响其他工具的正常显示。

## 许可证

MIT
