const KNOWN_SITE = /(^|\.)(youtube\.com|youtu\.be|tiktok\.com|facebook\.com|fb\.watch|fb\.com)$/i;

/** Lấy link đầu tiên trong đoạn văn bản (link chia sẻ TikTok thường kèm chữ phía trước). */
export function extractUrl(text: string): string | null {
  const m = text.match(/https?:\/\/[^\s"'<>]+/i);
  return m ? m[0] : null;
}

export function isKnownSite(url: string): boolean {
  try {
    return KNOWN_SITE.test(new URL(url).hostname);
  } catch {
    return false;
  }
}

export type Site = "youtube" | "tiktok" | "facebook" | "other";

export function siteOf(extractor: string): Site {
  const s = extractor.toLowerCase();
  if (s.includes("youtube")) return "youtube";
  if (s.includes("tiktok")) return "tiktok";
  if (s.includes("facebook")) return "facebook";
  return "other";
}

export const SITE_NAME: Record<Site, string> = {
  youtube: "YouTube",
  tiktok: "TikTok",
  facebook: "Facebook",
  other: "Web",
};

export function fmtBytes(n: number | null | undefined): string {
  if (n == null || !isFinite(n)) return "";
  const units = ["B", "KB", "MB", "GB"];
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n.toFixed(i >= 2 ? 1 : 0)} ${units[i]}`;
}

export function fmtDuration(sec: number | null | undefined): string {
  if (sec == null || !isFinite(sec)) return "";
  const s = Math.round(sec);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${r}` : `${m}:${r}`;
}

/** Nhãn ngày cho lịch sử tải: "Hôm nay", "Hôm qua" hoặc "Thứ Hai, 26/10/2026". */
export function fmtDay(ts: number): string {
  const day = new Date(ts);
  const today = new Date();
  const yesterday = new Date();
  yesterday.setDate(today.getDate() - 1);
  if (day.toDateString() === today.toDateString()) return "Hôm nay";
  if (day.toDateString() === yesterday.toDateString()) return "Hôm qua";
  return day.toLocaleDateString("vi-VN", { weekday: "long", day: "2-digit", month: "2-digit", year: "numeric" });
}

export function fmtTime(ts: number): string {
  return new Date(ts).toLocaleTimeString("vi-VN", { hour: "2-digit", minute: "2-digit" });
}

const QUALITY_NAME: Record<number, string> = { 4320: "8K", 2160: "4K", 1440: "2K" };

export function fmtQuality(height: number | null): string {
  if (!height) return "";
  return QUALITY_NAME[height] ?? `${height}p`;
}
