// Đếm số người dùng thật: mỗi máy cài VidSaver báo về máy chủ góp ý (feedback/Code.gs) mỗi ngày một lần, gồm một mã
// cài đặt ngẫu nhiên, phiên bản app và hệ điều hành. Không gửi tên, email, IP hay link đã tải.
// Chỉ bản cài thật mới báo: bot tải file trên GitHub không chạy app, còn `npm run dev` / `tauri dev` thì bỏ qua.
// Máy đã đăng nhập Quản trị gửi kèm mã quản trị để máy chủ đánh dấu là máy nội bộ và không tính vào thống kê.

import { getVersion } from "@tauri-apps/api/app";
import { FEEDBACK_URL, ping } from "./feedback";
import { load, save } from "./storage";

const CHECK_EVERY = 60 * 60 * 1000;

function installId(): string {
  let id = load("installId", "");
  if (!id) {
    id = crypto.randomUUID();
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

/** Báo về nếu hôm nay chưa báo. `force`: báo ngay (vd. vừa đăng nhập Quản trị). */
export async function reportUsage(force = false) {
  if (import.meta.env.DEV || !FEEDBACK_URL) return;
  const today = new Date().toDateString();
  if (!force && load("lastPing", "") === today) return;
  try {
    await ping({
      installId: installId(),
      appVersion: await appVersionTag(),
      os: systemInfo(),
      adminKey: load("feedbackAdminKey", ""),
    });
    save("lastPing", today);
  } catch {
    // Mất mạng thì lần kiểm tra sau báo lại.
  }
}

/** Gọi khi app mở; app để mở qua ngày thì vẫn báo cho ngày mới. Trả về hàm dừng. */
export function startUsageReport(): () => void {
  reportUsage();
  const timer = setInterval(reportUsage, CHECK_EVERY);
  return () => clearInterval(timer);
}
