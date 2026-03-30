# AICoder Session Viewer

A unified desktop application for browsing conversation histories from multiple AI coding assistants — **Claude Code**, **Codex**, **Gemini CLI**, and **OpenCode** — all in one place.

[中文文档](./README_CN.md)

![Screenshot](./public/screenshot.png)

## Why

AI coding assistants store their session data in different formats (JSONL, JSON, SQLite) scattered across various directories. There's no unified way to browse, search, or compare these conversations. AICoder Session Viewer solves this by normalizing all sources into a single, consistent interface.

## Install

Download the latest release from the [Releases](https://github.com/seastart/aicoder-session-viewer/releases) page.

### macOS

The app is not signed with an Apple Developer certificate. macOS Gatekeeper may block it with an "app is damaged" warning. To fix this, run:

```bash
xattr -cr /Applications/AICoder\ Session\ Viewer.app
```

### Windows / Linux

Download the `.exe` / `.msi` (Windows) or `.deb` / `.AppImage` (Linux) from the release page and install normally.

## Features

- **Multi-tool support** — Claude Code, Codex, Gemini CLI, OpenCode
- **Unified data model** — All session formats normalized on the Rust side before reaching the frontend
- **Rich content rendering** — Markdown, syntax-highlighted code blocks (via Shiki), collapsible tool calls and thinking blocks
- **Search & filter** — Filter by tool type, search sessions by title or project path (with 300ms debounce)
- **Project grouping** — Group sessions by project path in a collapsible folder tree; toggle between flat list and grouped view
- **Resume sessions** — Resume any historical session directly in a terminal window (auto-detects iTerm2, Terminal.app, Warp, Kitty, Alacritty, Ghostty on macOS)
- **Export** — Export sessions as JSONL or Markdown via save dialog
- **Dark theme** — Purpose-built dark UI with per-tool color coding
- **Fast & lightweight** — Native Tauri app with minimal resource usage; SQLite accessed read-only

## Data Sources

| Tool | macOS / Linux | Windows | Format |
|------|--------------|---------|--------|
| Claude Code | `~/.claude/projects/{path}/{uuid}.jsonl` | `%USERPROFILE%\.claude\projects\...` | JSONL (index in `~/.claude/sessions/*.json`) |
| Codex | `~/.codex/sessions/{Y}/{M}/{D}/rollout-*.jsonl` | `%USERPROFILE%\.codex\sessions\...` | JSONL |
| Gemini CLI | `~/.gemini/tmp/{project}/chats/session-*.json` | `%USERPROFILE%\.gemini\tmp\...` | JSON |
| OpenCode | `~/.local/share/opencode/opencode.db` | `%USERPROFILE%\.local\share\opencode\opencode.db` | SQLite |

> All four tools use the user home directory (`~` / `%USERPROFILE%`) as root, with identical directory structures across platforms.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Desktop framework | [Tauri v2](https://tauri.app/) |
| Backend | Rust (serde, chrono, rusqlite, walkdir, thiserror) |
| Frontend | React 19 + TypeScript |
| Styling | Tailwind CSS v4 |
| State management | Zustand |
| Code highlighting | Shiki |
| Markdown | react-markdown + remark-gfm |

## Prerequisites

- [Node.js](https://nodejs.org/) >= 20
- [pnpm](https://pnpm.io/) >= 10
- [Rust](https://www.rust-lang.org/tools/install) >= 1.85
- Tauri v2 system dependencies — see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

## Getting Started

```bash
# Clone the repository
git clone https://github.com/user/aicoder-session-viewer.git
cd aicoder-session-viewer

# Install frontend dependencies
pnpm install

# Start in development mode (compiles Rust + starts Vite dev server)
pnpm tauri dev

# Build for production
pnpm tauri build
```

## Project Structure

```
aicoder-session-viewer/
├── src-tauri/                  # Rust backend
│   └── src/
│       ├── main.rs             # Entry point
│       ├── lib.rs              # Tauri app setup & plugin registration
│       ├── error.rs            # Unified error types (thiserror)
│       ├── models.rs           # Shared data models (SessionSummary, Message, ContentBlock...)
│       ├── commands.rs         # Tauri IPC commands
│       ├── export.rs           # Session export (JSONL / Markdown)
│       └── providers/          # Data source implementations
│           ├── mod.rs          # SessionProvider trait + ProviderRegistry
│           ├── claude.rs       # Claude Code (JSONL)
│           ├── codex.rs        # Codex (JSONL)
│           ├── gemini.rs       # Gemini CLI (JSON)
│           └── opencode.rs     # OpenCode (SQLite)
├── src/                        # React frontend
│   ├── App.tsx                 # Root component
│   ├── App.css                 # Tailwind imports + theme variables
│   ├── types.ts                # TypeScript types (mirrors models.rs)
│   ├── stores/
│   │   └── sessionStore.ts     # Zustand store
│   ├── hooks/
│   │   └── useDebounce.ts
│   ├── utils/
│   │   └── buildProjectTree.ts # Group sessions into folder tree
│   └── components/
│       ├── Layout.tsx           # Two-column layout + view mode toggle
│       ├── Sidebar/
│       │   ├── SearchBar.tsx    # Debounced search input
│       │   ├── ToolFilter.tsx   # Tool type filter tabs
│       │   ├── SessionList.tsx  # Flat session list
│       │   └── ProjectTree.tsx  # Grouped project folder tree
│       └── Chat/
│           ├── ChatView.tsx     # Session detail + resume & export buttons
│           ├── MessageBubble.tsx # Message rendering (all content block types)
│           ├── CodeBlock.tsx    # Shiki syntax highlighting
│           └── ToolCallBlock.tsx # Collapsible tool use/result blocks
├── index.html
├── package.json
├── vite.config.ts
└── tsconfig.json
```

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Frontend (React + TypeScript)                          │
│  ┌──────────────┐  ┌────────────────────────────────┐   │
│  │   Sidebar     │  │   ChatView                     │   │
│  │  - Search     │  │   - Message bubbles            │   │
│  │  - Filter     │  │   - Markdown / Code / Tools    │   │
│  │  - List       │  │   - Thinking blocks            │   │
│  └──────┬───────┘  └───────────────┬────────────────┘   │
│         │  Zustand                  │                    │
│         └──────────┬────────────────┘                    │
│                    │ invoke()                            │
├────────────────────┼────────────────────────────────────┤
│  Tauri IPC         │                                    │
├────────────────────┼────────────────────────────────────┤
│  Backend (Rust)    ▼                                    │
│  ┌─────────────────────────┐                            │
│  │    ProviderRegistry      │                            │
│  │  ┌───────┐ ┌───────┐   │                            │
│  │  │Claude │ │ Codex │   │   → SessionSummary         │
│  │  └───────┘ └───────┘   │   → Session                │
│  │  ┌───────┐ ┌────────┐  │   → Message                │
│  │  │Gemini │ │OpenCode│  │   → ContentBlock           │
│  │  └───────┘ └────────┘  │                            │
│  └─────────────────────────┘                            │
└─────────────────────────────────────────────────────────┘
```

All four data sources implement the `SessionProvider` trait and are normalized into a unified model before being sent to the frontend. If a provider is unavailable (e.g., directory doesn't exist), it is silently skipped without affecting others.

## License

MIT
