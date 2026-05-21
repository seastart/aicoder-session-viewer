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
