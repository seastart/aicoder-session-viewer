import { useEffect, useState } from "react";
import { Copy, Check } from "lucide-react";
import { codeToHtml } from "shiki";

interface Props {
  code: string;
  language?: string | null;
}

export function CodeBlock({ code, language }: Props) {
  const [html, setHtml] = useState<string>("");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    // 使用 shiki 进行语法高亮
    codeToHtml(code, {
      lang: language || "text",
      theme: "github-dark-default",
    })
      .then(setHtml)
      .catch(() => {
        // 降级为纯文本
        setHtml(`<pre><code>${escapeHtml(code)}</code></pre>`);
      });
  }, [code, language]);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="group relative my-2 overflow-hidden rounded-lg border border-border">
      {/* 语言标签 + 复制按钮 */}
      <div className="flex items-center justify-between bg-[#0d1117] px-3 py-1.5 text-xs text-text-muted">
        <span>{language || "text"}</span>
        <button
          onClick={handleCopy}
          className="opacity-0 transition-opacity group-hover:opacity-100 hover:text-text-primary"
        >
          {copied ? <Check size={14} /> : <Copy size={14} />}
        </button>
      </div>

      {/* 高亮代码 */}
      <div
        className="overflow-x-auto p-3 text-sm [&_pre]:!m-0 [&_pre]:!bg-transparent [&_pre]:!p-0"
        dangerouslySetInnerHTML={{ __html: html }}
      />
    </div>
  );
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}
