/**
 * 轻量 i18n 模块
 *
 * 根据浏览器/系统语言自动选择中文或英文，
 * 通过 React Context + useLocale hook 提供 t 翻译对象。
 */
export { LocaleProvider, useLocale } from "./useLocale";
export type { Locale } from "./locales/zh";
