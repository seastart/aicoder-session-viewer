---
name: release
description: Use when the user asks to run the release workflow for this AICoder Session Viewer repository, including publishing a new version, bumping package/Tauri/Cargo versions, creating a release commit and git tag, or migrating the old Claude Code /release command.
---

# Release Workflow

Use this skill to publish a new version of this repository.

## Inputs

- Optional target version from the user, for example `$release 0.2.0`.
- If no version is provided, bump the patch version from `package.json`.

## Workflow

1. Check the working tree:
   - Run `git status --short` and `git diff --stat`.
   - If the working tree is clean, tell the user there is nothing to release and stop.
   - If unrelated or risky changes are present, summarize them before continuing.

2. Determine the new version:
   - Read the current version from `package.json`.
   - Use the user-provided version if present.
   - Otherwise increment the patch version, for example `0.1.5` to `0.1.6`.

3. Update all version files together:
   - `package.json`: top-level `"version"`.
   - `src-tauri/tauri.conf.json`: top-level `"version"`.
   - `src-tauri/Cargo.toml`: package `version`.

4. Verify before committing:
   - Run `npx tsc --noEmit`.
   - Run `cargo check` from `src-tauri/`.
   - If verification fails, report the failure and do not commit or tag.

5. Generate a Chinese release commit message from the actual diff:
   ```text
   release: vX.Y.Z — 一句话总结

   ### 新功能
   - ...

   ### 优化
   - ...

   ### 修复
   - ...
   ```
   Omit empty categories.

6. Commit and tag:
   - Use `git add` with explicit file paths only. Do not use `git add -A`.
   - Commit with the generated message.
   - Create tag `vX.Y.Z`.

7. Push:
   - Run `git push origin main --tags`.

8. Report the result:
   - Commit hash.
   - Tag name.
   - Push status.

## Safety Rules

- Do not rewrite or discard user changes.
- Do not commit if verification fails.
- Do not include unrelated files in `git add`.
- If the current branch is not `main`, ask before pushing.
