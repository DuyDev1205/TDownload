// Đếm số người dùng thật: mỗi máy cài VidSaver báo về máy chủ góp ý (feedback/Code.gs) mỗi ngày một lần, gồm một mã
// cài đặt ngẫu nhiên, phiên bản app và hệ điều hành. Không gửi tên, email, IP hay link đã tải.
// Chỉ bản cài thật mới báo: bot tải file trên GitHub không chạy app, còn `npm run dev` / `tauri dev` thì bỏ qua.
// Máy đã đăng nhập Quản trị gửi kèm mã quản trị để máy chủ đánh dấu là máy nội bộ và không tính vào thống kê.

import { getVersion } from "@tauri-apps/api/app";
import { FEEDBACK_URL, ping } from "./feedback";
import { load, save } from "./storage";

const CHECK_EVERY = 60 * 60 * 1000;

/** UUID v4. crypto.randomUUID chỉ có ở "secure context", WebView trên macOS có thể không có. */
function uuid(): string {
  if (typeof crypto.randomUUID === "function") return crypto.randomUUID();
  const b = crypto.getRandomValues(new Uint8Array(16));
  b[6] = (b[6] & 0x0f) | 0x40;
  b[8] = (b[8] & 0x3f) | 0x80;
  const h = [...b].map((x) => x.toString(16).padStart(2, "0")).join("");
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}

function installId(): string {
  let id = load("installId", "");
  if (!id) {
    id = uuid();
    save("installId", id);
  }
  return id;
}

export function systemInfo(): string {
  const ua = navigator.userAgent;
  const mac = ua.match(/Mac OS X ([\d_]+)/);
  if (mac) return `macOS ${mac[1].replace(/_/g, ".")}`;
  const win = ua.match(/Windows NT ([\d.]+)/);
  return win ? `Windows (NT ${win[1]})` : ua.slice(0, 80);
}

/** Thêm "v" để Google Sheet không hiểu "0.5.1" thành ngày tháng. */
export const appVersionTag = () => getVersion().then((v) => `v${v}`).catch(() => "");

let sending = false;

/**
 * Báo về nếu hôm nay chưa báo, hoặc app vừa lên phiên bản mới (máy chủ tính đó là một lượt tải).
 * `force`: báo ngay (vd. vừa đăng nhập Quản trị).
 */
export async function reportUsage(force = false) {
  if (import.meta.env.DEV || !FEEDBACK_URL || sending) return;
  sending = true;
  try {
    const appVersion = await appVersionTag();
    const mark = `${new Date().toDateString()} ${appVersion}`;
    if (!force && load("lastPing", "") === mark) return;
    await ping({ installId: installId(), appVersion, os: systemInfo(), adminKey: load("feedbackAdminKey", "") });
    save("lastPing", mark);
  } catch {
    // Mất mạng thì lần kiểm tra sau báo lại.
  } finally {
    sending = false;
  }
}

/** Gọi khi app mở; app để mở qua ngày thì vẫn báo cho ngày mới. Báo hỏng thì thử lại khi quay lại cửa sổ app. */
export function startUsageReport(): () => void {
  const report = () => reportUsage();
  report();
  const timer = setInterval(report, CHECK_EVERY);
  window.addEventListener("focus", report);
  return () => {
    clearInterval(timer);
    window.removeEventListener("focus", report);
  };
}
