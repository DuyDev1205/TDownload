// Giao diện sáng/tối do người dùng chọn. Lần đầu mở app thì lấy theo hệ điều hành, sau đó giữ lựa chọn đã lưu.
// Kết quả ghi vào <html data-theme="light|dark"> để app.css và logo đổi theo; thanh tiêu đề cửa sổ cũng đổi cùng.

import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { load, save } from "./storage";

export type ThemeMode = "light" | "dark";

export const THEME_MODES: [ThemeMode, string][] = [
  ["light", "Sáng"],
  ["dark", "Tối"],
];

const systemDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
const saved = load("theme", "");
let mode: ThemeMode = saved === "light" || saved === "dark" ? saved : systemDark ? "dark" : "light";

function apply() {
  document.documentElement.dataset.theme = mode;
  if (isTauri()) getCurrentWindow().setTheme(mode).catch(() => {});
}

apply();

export const getTheme = () => mode;

export function setTheme(next: ThemeMode) {
  mode = next;
  save("theme", next);
  apply();
}
