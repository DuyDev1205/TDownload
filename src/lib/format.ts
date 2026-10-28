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

export function fmtQuality(height: number | null): string {
  if (!height) return "";
  if (height >= 4320) return "8K";
  if (height >= 2160) return "4K";
  if (height >= 1440) return "2K";
  return `${height}p`;
}
