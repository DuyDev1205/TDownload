import type { Subtitle } from "./api";
import { getLang, localeTag, t } from "./i18n.svelte";

const KNOWN_SITE = /(^|\.)(youtube\.com|youtu\.be|tiktok\.com|facebook\.com|fb\.watch|fb\.com|soundcloud\.com)$/i;

/**
 * Lấy mọi link trong đoạn văn bản, theo thứ tự, kể cả link trùng (xem uniqueLinks). Ô nhập một dòng xoá dấu xuống
 * dòng khi dán nhiều link nên các link có thể dính liền nhau ("https://a...https://b..."): cắt ở mỗi chỗ bắt đầu
 * "http(s)://", ở dấu cách và dấu phân cách (, ;), rồi bỏ dấu câu dính ở cuối link.
 */
export function extractAllUrls(text: string): string[] {
  const found = text.match(/https?:\/\/.+?(?=https?:\/\/|[\s"'<>,;，；、]|$)/gi) ?? [];
  return found.map((u) => u.replace(/[).:!?\]}'"。]+$/, "")).filter((u) => /^https?:\/\/[^/]+\.[^/]+/i.test(u));
}

/** Các link khác nhau trong đoạn văn bản (cùng một video dán hai lần chỉ lấy một). */
export function extractUrls(text: string): string[] {
  return uniqueLinks(extractAllUrls(text)).links;
}

/** Lấy link đầu tiên trong đoạn văn bản (link chia sẻ TikTok thường kèm chữ phía trước). */
export function extractUrl(text: string): string | null {
  return extractAllUrls(text)[0] ?? null;
}

// Tham số chỉ để theo dõi lượt chia sẻ, không đổi video: bỏ đi khi so sánh link.
const TRACKING_PARAMS = /^(utm_\w+|si|feature|fbclid|mibextid|igshid|_t|_r|is_from_webapp|sender_device|share_\w+|ref|rdid)$/i;

/**
 * Khoá nhận diện một video để phát hiện trùng, dù link khác dạng: youtu.be/ID, watch?v=ID, shorts/ID đều là "yt:ID";
 * trang khác thì bỏ tham số theo dõi, dấu # và dấu / cuối.
 */
export function videoKey(url: string): string {
  try {
    const u = new URL(url);
    const host = u.hostname.replace(/^(www|m|music)\./i, "").toLowerCase();
    if (host === "youtu.be" || host.endsWith("youtube.com")) {
      const id =
        u.searchParams.get("v") ??
        (host === "youtu.be" ? u.pathname.slice(1) : u.pathname.match(/^\/(?:shorts|live|embed)\/([^/]+)/)?.[1]);
      const list = u.searchParams.get("list");
      if (id) return `yt:${id}${list ? `|${list}` : ""}`;
    }
    for (const k of [...u.searchParams.keys()]) if (TRACKING_PARAMS.test(k)) u.searchParams.delete(k);
    u.hash = "";
    return `${host}${u.pathname.replace(/\/+$/, "")}${u.search}`.toLowerCase();
  } catch {
    return url.trim().toLowerCase();
  }
}

/** Bỏ link trùng (cùng video), giữ thứ tự; `dupes` là số link bị bỏ. */
export function uniqueLinks(urls: string[]): { links: string[]; dupes: number } {
  const seen = new Set<string>();
  const links = urls.filter((u) => {
    const key = videoKey(u);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
  return { links, dupes: urls.length - links.length };
}

/** Ảnh bìa YouTube bản lớn nhất (maxresdefault) không phải video nào cũng có: lỗi thì đổi sang bản nhỏ hơn. */
export function thumbFallback(e: Event) {
  const img = e.currentTarget as HTMLImageElement;
  if (img.src.includes("/maxresdefault")) img.src = img.src.replace(/\/maxresdefault(_live)?\.(jpg|webp)/, "/hqdefault.jpg");
  else img.style.visibility = "hidden";
}

/** Link trang chủ kênh YouTube chỉ trả về các tab, nên chuyển sang tab Videos. */
export function normalizeUrl(url: string): string {
  const m = url.match(/^(https?:\/\/(?:www\.|m\.)?youtube\.com\/(?:@[^/?#]+|channel\/[^/?#]+|c\/[^/?#]+|user\/[^/?#]+))\/?(?:[?#].*)?$/i);
  return m ? `${m[1]}/videos` : url;
}

const languageNames = new Map<string, Intl.DisplayNames | null>();

/** Tên ngôn ngữ theo ngôn ngữ giao diện đang chọn, ví dụ "en" thành "Tiếng Anh" hoặc "English". */
function languageNamesFor(lang: string): Intl.DisplayNames | null {
  if (!languageNames.has(lang)) {
    try {
      languageNames.set(lang, new Intl.DisplayNames([lang], { type: "language" }));
    } catch {
      languageNames.set(lang, null);
    }
  }
  return languageNames.get(lang)!;
}

/** Tên phụ đề, ví dụ "Tiếng Anh" hoặc "Tiếng Anh · tự động". */
export function subtitleName(sub: Subtitle): string {
  const code = sub.lang.replace(/-orig$/, "");
  let name = sub.name;
  try {
    name = languageNamesFor(getLang())?.of(code) ?? sub.name;
  } catch {
    // Mã ngôn ngữ lạ thì giữ tên yt-dlp đưa.
  }
  name = name.charAt(0).toUpperCase() + name.slice(1);
  return sub.auto ? `${name} · ${t("subtitle.auto")}` : name;
}

export function isKnownSite(url: string): boolean {
  try {
    return KNOWN_SITE.test(new URL(url).hostname);
  } catch {
    return false;
  }
}

export type Site = "youtube" | "tiktok" | "facebook" | "soundcloud" | "other";

export function siteOf(extractor: string): Site {
  const s = extractor.toLowerCase();
  if (s.includes("youtube")) return "youtube";
  if (s.includes("tiktok")) return "tiktok";
  if (s.includes("facebook")) return "facebook";
  if (s.includes("soundcloud")) return "soundcloud";
  return "other";
}

export const SITE_NAME: Record<Site, string> = {
  youtube: "YouTube",
  tiktok: "TikTok",
  facebook: "Facebook",
  soundcloud: "SoundCloud",
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

/** Nhãn ngày cho lịch sử tải: "Hôm nay", "Hôm qua" hoặc "Thứ Hai, 26/10/2026" (theo ngôn ngữ đang chọn). */
export function fmtDay(ts: number): string {
  const day = new Date(ts);
  const today = new Date();
  const yesterday = new Date();
  yesterday.setDate(today.getDate() - 1);
  if (day.toDateString() === today.toDateString()) return t("history.today");
  if (day.toDateString() === yesterday.toDateString()) return t("history.yesterday");
  return day.toLocaleDateString(localeTag(), { weekday: "long", day: "2-digit", month: "2-digit", year: "numeric" });
}

export function fmtTime(ts: number): string {
  return new Date(ts).toLocaleTimeString(localeTag(), { hour: "2-digit", minute: "2-digit" });
}

const QUALITY_NAME: Record<number, string> = { 4320: "8K", 2160: "4K", 1440: "2K" };

export function fmtQuality(height: number | null): string {
  if (!height) return "";
  return QUALITY_NAME[height] ?? `${height}p`;
}
