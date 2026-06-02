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
- **丰富的内容渲染** — Markdown、Shiki 语法高亮代码块、可折叠的工具调用/思考过程，以及支持点击放大的图片附件
- **搜索与导航** — 按工具类型筛选，标题/路径实时搜索（300ms 防抖），停止输入 1s 后自动升级为会话内容全文搜索（正文 / 思考过程 / 工具调用参数），按 Enter 可立即触发，也支持在当前对话内搜索
- **项目分组** — 按项目路径将 session 分组为可折叠的文件夹树，支持列表/分组视图切换
- **Token 用量汇总** — 当源数据包含 usage 信息时，展示单条消息和整个 session 的 token 用量
- **Subagent 下钻** — 展开 Claude Code 的 `Agent` 工具调用，并懒加载对应 subagent 对话
- **恢复、新建与定时继续** — 既可恢复历史 session，也可从项目文件夹新建 session，或等待到 reset 时间后自动恢复并附带 `continue`
- **YOLO 启动模式** — 按住 Option/Alt 或使用右键菜单，可用无人值守权限参数恢复/新建支持的工具
- **导出** — 支持将 session 导出为 JSONL 或 Markdown 格式
- **轻量中英文切换** — 默认跟随系统/浏览器语言，也支持通过 `VITE_LOCALE=zh|en` 覆盖
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

## 自定义数据源路径

如果你的工具数据存放在非默认位置（例如基于 OpenCode 衍生的工具，数据库路径不同），可以通过应用内的「设置」对话框（侧边栏齿轮按钮）为每个 provider 覆盖路径。

- 点击标题栏的齿轮图标
- 为任一 provider 输入或浏览自定义路径；留空则使用默认值
- 点击「保存」即时生效，无需重启
- 如果自定义路径无效，该 provider 会被跳过并提示警告

配置文件位置：

- macOS: `~/Library/Application Support/aicoder-session-viewer/config.json`
- Linux: `~/.config/aicoder-session-viewer/config.json`
- Windows: `%APPDATA%\aicoder-session-viewer\config.json`

## 定时继续策略

当 Claude Code、Codex、Gemini CLI 或 OpenCode 出现配额提示，例如 `You've hit your limit · resets 1am (Asia/Shanghai)` 或 `try again at 1:35 PM` 时，支持定时自动继续：

1. 从当前 session 的最近消息里推断 reset 时间，当前会识别 `resets ...`、`try again at ...`、`available again at ...` 等文案；如果没解析到，就回退到本地时区的下一个 `01:00`。
2. 在推断出的解禁时间基础上，统一再增加 5 分钟缓冲，避免踩在整点或解禁瞬间。
3. 先打开一个新的终端窗口或 tab。
4. 等到目标时间后，再执行“恢复会话 + 一条 `continue` prompt”的 CLI 命令。
5. 为了避免无人值守恢复卡在权限确认上，支持的 CLI 会额外附加各自的自动批准参数。

各工具在“定时继续”场景下当前使用的命令形态如下：

- Claude Code: `claude --permission-mode bypassPermissions --resume <session-id> "continue"`
- Codex: `codex resume --dangerously-bypass-approvals-and-sandbox <session-id> "continue"`
- Gemini CLI: `gemini --approval-mode yolo --resume <session-id> "continue"`
- OpenCode: `opencode --session <session-id> --prompt "continue"`

这个策略的本质是“在正确时间点重新发起一次原生恢复请求”，而不是“接管正在运行的旧终端”。前者依赖 CLI 官方支持的 `resume + prompt` 能力；如果 CLI 还支持无人值守权限模式，就一并带上，避免恢复后又停在审批弹窗上。

## 快捷键

| 快捷键 | 作用范围 | 行为 |
|--------|----------|------|
| `Cmd+F` / `Ctrl+F` | Session 详情 | 聚焦当前对话搜索框 |
| `Enter` | 对话搜索框 | 跳到下一个命中 |
| `Shift+Enter` | 对话搜索框 | 跳到上一个命中 |
| `Esc` | 图片预览 | 关闭大图预览 |
| 按住 `Option` / `Alt` 点击恢复会话 | Session 详情 | 在支持的工具上以 YOLO / 无人值守权限模式恢复 |
| 按住 `Option` / `Alt` 点击新建会话 | 项目树 | 在支持的工具上以 YOLO / 无人值守权限模式新建 |

OpenCode 当前 CLI 暂未暴露无人值守权限参数，因此会忽略 YOLO 模式。

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

# 临时指定中文或英文界面启动
VITE_LOCALE=zh pnpm tauri dev
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
│   ├── i18n/                   # 轻量中英文 locale 层
│   ├── stores/
│   │   └── sessionStore.ts     # Zustand 状态管理
│   ├── hooks/
│   │   ├── useAltKeyPressed.ts # 监听 Option/Alt，用于 YOLO 启动快捷入口
│   │   └── useDebounce.ts      # 防抖 hook
│   ├── utils/
│   │   ├── buildProjectTree.ts # Session 按项目路径构建树
│   │   ├── format.ts           # 通用数字/时间格式化
│   │   └── sessionSearch.ts    # 当前对话搜索与快捷键判断
│   └── components/
│       ├── Layout.tsx           # 双栏布局 + 视图切换 + 设置弹窗入口
│       ├── SettingsDialog.tsx   # Provider 路径覆盖设置
│       ├── common/
│       │   └── YoloHint.tsx     # YOLO 模式提示徽标
│       ├── Sidebar/
│       │   ├── SearchBar.tsx    # 带防抖的搜索输入
│       │   ├── ToolFilter.tsx   # 工具类型过滤标签
│       │   ├── SessionList.tsx  # 扁平 Session 列表
│       │   └── ProjectTree.tsx  # 项目分组文件夹树 + 新建会话入口
│       └── Chat/
│           ├── ChatView.tsx     # Session 详情 + 对话搜索 + 恢复/导出按钮
│           ├── MessageBubble.tsx # 消息渲染（支持所有内容块类型）
│           ├── CodeBlock.tsx    # Shiki 语法高亮
│           └── ToolCallBlock.tsx # 可折叠的工具调用/结果块 + subagent 下钻
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
