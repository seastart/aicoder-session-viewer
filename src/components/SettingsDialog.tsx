import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { X } from "lucide-react";
import { useLocale } from "../i18n";
import { useSessionStore } from "../stores/sessionStore";
import type { ProviderConfig, ProviderPaths } from "../types";

interface Props {
  open: boolean;
  onClose: () => void;
}

type ProviderKey = keyof ProviderPaths;

/**
 * provider 的元信息：
 * - key:    与 ProviderPaths 的字段一致
 * - label:  界面展示名（保持原样，不走 i18n，类似 ToolFilter 的处理）
 * - isFile: true 表示选文件（OpenCode 的 SQLite），false 表示选目录
 */
const PROVIDERS: { key: ProviderKey; label: string; isFile: boolean }[] = [
  { key: "claudeCode", label: "Claude Code", isFile: false },
  { key: "codex", label: "Codex", isFile: false },
  { key: "antigravity", label: "Antigravity", isFile: false },
  { key: "opencode", label: "OpenCode", isFile: true },
  { key: "gemini", label: "Gemini", isFile: false },
];

export function SettingsDialog({ open, onClose }: Props) {
  const { t } = useLocale();
  const providerDefaults = useSessionStore((s) => s.providerDefaults);
  const loadProviderConfig = useSessionStore((s) => s.loadProviderConfig);
  const saveProviderConfig = useSessionStore((s) => s.saveProviderConfig);

  // 本地编辑态：空字符串表示「未覆盖」，最终写入后端为 null
  const [draft, setDraft] = useState<Record<ProviderKey, string>>({
    claudeCode: "",
    codex: "",
    gemini: "",
    antigravity: "",
    opencode: "",
  });
  const [saving, setSaving] = useState(false);
  // 标记初始配置是否已经拉取完成；未完成前不渲染表单，避免用户输入被 setDraft 覆盖
  const [loaded, setLoaded] = useState(false);

  // 对话框打开时：先拉一次最新配置，再用当前值初始化草稿
  useEffect(() => {
    if (!open) {
      // 关闭对话框时重置加载态，下次打开会重新拉取并显示占位
      setLoaded(false);
      return;
    }
    (async () => {
      await loadProviderConfig();
      const cfg = useSessionStore.getState().providerConfig;
      if (cfg) {
        setDraft({
          claudeCode: cfg.providerPaths.claudeCode ?? "",
          codex: cfg.providerPaths.codex ?? "",
          gemini: cfg.providerPaths.gemini ?? "",
          antigravity: cfg.providerPaths.antigravity ?? "",
          opencode: cfg.providerPaths.opencode ?? "",
        });
      }
      setLoaded(true);
    })();
  }, [open, loadProviderConfig]);

  if (!open) return null;

  // 保存进行中时禁止任何关闭交互，避免对话框消失但 session 列表还在刷新
  const closeIfIdle = () => {
    if (!saving) onClose();
  };

  const setField = (key: ProviderKey, value: string) => {
    setDraft((d) => ({ ...d, [key]: value }));
  };

  // 唤起系统文件/目录选择器；OpenCode 选 SQLite 文件，其余选目录
  const browse = async (key: ProviderKey, isFile: boolean) => {
    const selected = await openDialog(
      isFile
        ? {
            multiple: false,
            directory: false,
            filters: [{ name: "SQLite", extensions: ["db", "sqlite"] }],
          }
        : { multiple: false, directory: true }
    );
    if (typeof selected === "string") {
      setField(key, selected);
    }
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      // 把空字符串还原为 null，交后端走默认路径
      const config: ProviderConfig = {
        providerPaths: {
          claudeCode: draft.claudeCode.trim() || null,
          codex: draft.codex.trim() || null,
          gemini: draft.gemini.trim() || null,
          antigravity: draft.antigravity.trim() || null,
          opencode: draft.opencode.trim() || null,
        },
      };
      const warnings = await saveProviderConfig(config);
      if (warnings.length > 0) {
        // 项目目前没有 toast 系统，沿用 ChatView.autoContinueError 的 alert 风格
        const formatted = warnings.map((w) => {
          const idx = w.indexOf(": ");
          if (idx === -1) return w;  // fallback for unexpected shape
          const provider = w.slice(0, idx);
          const message = w.slice(idx + 2);
          return t.settingsProviderWarning(provider, message);
        }).join("\n");
        window.alert(formatted);
      }
      onClose();
    } catch (e) {
      window.alert(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      onClick={closeIfIdle}
    >
      <div
        className="w-[640px] max-w-[90vw] rounded-lg border border-border bg-surface p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        {/* 标题栏 */}
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-base font-semibold text-text-primary">
            {t.settingsTitle}
          </h2>
          <button
            onClick={closeIfIdle}
            disabled={saving}
            className="rounded p-1 text-text-muted hover:bg-surface-hover hover:text-text-primary disabled:opacity-50"
          >
            <X size={16} />
          </button>
        </div>

        {!loaded ? (
          // 初次拉取配置前显示加载占位，防止用户在草稿即将被覆盖时输入
          <div className="py-8 text-center text-xs text-text-muted">
            {t.loading}
          </div>
        ) : (
          <>
            <h3 className="mb-3 text-sm font-medium text-text-secondary">
              {t.settingsProviderPaths}
            </h3>

            {/* Provider 路径列表 */}
            <div className="space-y-4">
              {PROVIDERS.map(({ key, label, isFile }) => (
                <div key={key} className="space-y-1">
                  <label className="block text-xs font-medium text-text-primary">
                    {label}
                  </label>
                  <div className="flex gap-2">
                    <input
                      type="text"
                      value={draft[key]}
                      onChange={(e) => setField(key, e.target.value)}
                      placeholder={t.settingsPathPlaceholder}
                      className="flex-1 rounded border border-border bg-sidebar px-2 py-1 text-xs text-text-primary placeholder:text-text-muted focus:border-accent focus:outline-none"
                    />
                    <button
                      type="button"
                      onClick={() => browse(key, isFile)}
                      className="rounded border border-border bg-sidebar px-2 py-1 text-xs text-text-primary hover:bg-surface-hover"
                    >
                      {t.settingsBrowse}
                    </button>
                    <button
                      type="button"
                      onClick={() => setField(key, "")}
                      className="rounded border border-border bg-sidebar px-2 py-1 text-xs text-text-primary hover:bg-surface-hover"
                    >
                      {t.settingsReset}
                    </button>
                  </div>
                  {providerDefaults && providerDefaults[key] && (
                    <p className="text-[10px] text-text-muted">
                      {t.settingsDefault}: {providerDefaults[key]}
                    </p>
                  )}
                </div>
              ))}
            </div>

            {/* 底部按钮 */}
            <div className="mt-5 flex justify-end gap-2">
              <button
                onClick={closeIfIdle}
                disabled={saving}
                className="rounded border border-border bg-sidebar px-3 py-1 text-xs text-text-primary hover:bg-surface-hover disabled:opacity-50"
              >
                {t.settingsCancel}
              </button>
              <button
                onClick={handleSave}
                disabled={saving}
                className="rounded bg-accent px-3 py-1 text-xs text-surface hover:bg-accent-hover disabled:opacity-50"
              >
                {t.settingsSave}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
