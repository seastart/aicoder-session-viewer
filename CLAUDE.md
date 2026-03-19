# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Development (compiles Rust + starts Vite at localhost:1420)
pnpm tauri dev

# Production build
pnpm tauri build

# Frontend only (no Rust)
pnpm dev

# TypeScript type check
npx tsc --noEmit

# Rust check (from src-tauri/)
cargo check

# Rust build (from src-tauri/)
cargo build
```

## Architecture

This is a Tauri v2 desktop app (Rust backend + React frontend) that reads AI coding assistant session data from local files and databases, normalizes it into a unified model, and displays it in a chat-style UI.

### Data flow

```
Local data sources (JSONL/JSON/SQLite)
  → Rust SessionProvider implementations (one per tool)
  → ProviderRegistry (aggregates all providers)
  → Tauri IPC commands (list_all_sessions, get_session, etc.)
  → React Zustand store (invoke() calls)
  → React components
```

### Key abstraction: SessionProvider trait

All four providers (`claude.rs`, `codex.rs`, `gemini.rs`, `opencode.rs`) implement the `SessionProvider` trait defined in `providers/mod.rs`. Each converts tool-specific formats into the shared types in `models.rs` (`SessionSummary`, `Session`, `Message`, `ContentBlock`).

`ProviderRegistry::new()` initializes all providers; if one fails (e.g., `~/.codex` doesn't exist), it's silently skipped. The registry is injected into Tauri commands via `tauri::Builder::manage()`.

### ContentBlock tagged union

The `ContentBlock` enum is the core normalization type — it maps all tool-specific content formats into: `Text`, `Code`, `ToolUse`, `ToolResult`, `Thinking`, `Image`. Frontend rendering in `MessageBubble.tsx` switches on `block.type`.

### Frontend–backend type mirror

`src/types.ts` mirrors `src-tauri/src/models.rs`. When modifying the Rust data model, the TypeScript types must be updated to match (and vice versa). Serde serialization uses `rename_all = "snake_case"` and tagged enums use `#[serde(tag = "type")]`.

### Tauri IPC commands

Defined in `commands.rs`, registered in `lib.rs`:
- `list_all_sessions()` — all tools, sorted by time desc
- `list_sessions(tool)` — single tool filter
- `get_session(tool, session_id)` — full session with messages
- `search_sessions(query, tool?)` — title/path text search

### State management

Single Zustand store (`stores/sessionStore.ts`) holds sessions list, current session, loading/error state, and filter/search state. Search input is debounced 300ms via `useDebounce` hook before invoking the backend.

## Adding a new provider

1. Create `src-tauri/src/providers/new_tool.rs` implementing `SessionProvider`
2. Add `pub mod new_tool;` to `providers/mod.rs`
3. Add initialization to `ProviderRegistry::new()`
4. Add variant to `ToolKind` enum in `models.rs`
5. Add matching entry in `TOOL_CONFIG` in `src/types.ts`
