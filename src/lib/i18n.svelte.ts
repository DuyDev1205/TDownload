// Ngôn ngữ giao diện. Mặc định Tiếng Việt; người dùng đổi trong Cài đặt và lựa chọn được lưu lại.
// t() đọc ngôn ngữ từ $state nên mọi chỗ gọi t() trong giao diện tự cập nhật khi đổi ngôn ngữ.
// Phần Rust (thông báo lỗi) được báo ngôn ngữ qua lệnh set_language.

import { invoke, isTauri } from "@tauri-apps/api/core";
import { LOCALES, type Key, type Lang } from "./locales";
import { load, save } from "./storage";

export type { Key, Lang };

/** Tên mỗi ngôn ngữ viết bằng chính ngôn ngữ đó. */
export const LANGUAGES: [Lang, string][] = [
  ["vi", "Tiếng Việt"],
  ["en", "English"],
  ["es", "Español"],
  ["de", "Deutsch"],
  ["zh", "中文"],
  ["ja", "日本語"],
];

/** Mã vùng để định dạng ngày giờ. */
const LOCALE_TAG: Record<Lang, string> = { vi: "vi-VN", en: "en-US", es: "es-ES", de: "de-DE", zh: "zh-CN", ja: "ja-JP" };

const saved = load("lang", "vi");
const state = $state({ lang: (saved in LOCALES ? saved : "vi") as Lang });

function apply() {
  document.documentElement.lang = state.lang;
  if (isTauri()) invoke("set_language", { lang: state.lang }).catch(() => {});
}

apply();

export const getLang = () => state.lang;
export const localeTag = () => LOCALE_TAG[state.lang];

export function setLang(lang: Lang) {
  state.lang = lang;
  save("lang", lang);
  apply();
}

export function t(key: Key, vars?: Record<string, string | number>): string {
  const text = LOCALES[state.lang][key] ?? LOCALES.vi[key];
  return vars ? text.replace(/\{(\w+)\}/g, (m, name) => (name in vars ? String(vars[name]) : m)) : text;
}
