import { createContext, useContext, useMemo, type ReactNode } from "react";
import zh, { type Locale } from "./locales/zh";
import en from "./locales/en";
import { zhCN, enUS, type Locale as DateLocale } from "date-fns/locale";

type LangKey = "zh" | "en";

interface LocaleContextValue {
  /** 当前语言 key */
  lang: LangKey;
  /** 翻译对象，直接访问属性即可 */
  t: Locale;
  /** date-fns 对应的 locale，用于日期格式化 */
  dateLocale: DateLocale;
}

const LocaleContext = createContext<LocaleContextValue | null>(null);

function parseForcedLang(value: string | undefined): LangKey | null {
  if (value === "zh" || value === "en") {
    return value;
  }
  return null;
}

/** 检测系统/浏览器语言，返回 "zh" 或 "en" */
function detectLang(): LangKey {
  // 开发时允许通过环境变量临时覆盖语言，便于快速验收文案效果。
  const forced = parseForcedLang(import.meta.env.VITE_LOCALE);
  if (forced) {
    return forced;
  }

  const raw = navigator.language || "en";
  return raw.startsWith("zh") ? "zh" : "en";
}

const LOCALE_MAP: Record<LangKey, { messages: Locale; dateLocale: DateLocale }> = {
  zh: { messages: zh, dateLocale: zhCN },
  en: { messages: en, dateLocale: enUS },
};

export function LocaleProvider({ children }: { children: ReactNode }) {
  const value = useMemo<LocaleContextValue>(() => {
    const lang = detectLang();
    const { messages, dateLocale } = LOCALE_MAP[lang];
    return { lang, t: messages, dateLocale };
  }, []);

  return (
    <LocaleContext.Provider value={value}>
      {children}
    </LocaleContext.Provider>
  );
}

/** 获取当前 locale 信息，包含 t（翻译）和 dateLocale（日期格式化用） */
export function useLocale(): LocaleContextValue {
  const ctx = useContext(LocaleContext);
  if (!ctx) {
    throw new Error("useLocale must be used within <LocaleProvider>");
  }
  return ctx;
}
