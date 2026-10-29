// Backend giả để chỉnh giao diện bằng `npm run dev` trên trình duyệt (chỉ nạp ở chế độ dev, ngoài app Tauri).
// Thêm tham số vào URL để xem các trạng thái khác:
//   ?setup   màn hình cài đặt lần đầu
//   ?update  có bản app mới
// Tab "Góp ý & báo lỗi": mã quản trị giả là "admin"; tiêu đề có chữ "duyệt" thì 5 giây sau được chấp nhận.
//   ?error   "Lấy video" luôn báo lỗi
// Link chứa chữ "fail" thì lượt tải sẽ lỗi giữa chừng; chứa "missing" thì file tải xong bị coi là đã xoá
// (F5 lại trang để thấy trạng thái "File đã bị xoá hoặc chuyển đi").
// Link chứa "playlist", "/videos" hoặc "/sets/" trả về danh sách; link soundcloud là bài chỉ có âm thanh.
// Chọn phụ đề tiếng Việt thì giả lập phụ đề bị chặn (video vẫn tải xong, có ghi chú).

import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";

const params = new URLSearchParams(location.search);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

let depsReady = !params.has("setup");
const canceled = new Set<string>();

// Máy chủ góp ý giả (thay cho Apps Script). Mã quản trị: "admin". Góp ý có chữ "duyệt" trong tiêu đề sẽ được
// "chấp nhận" sau 5 giây để xem chấm đỏ báo kết quả mới.
interface FakeFeedback {
  id: string;
  createdAt: string;
  type: string;
  title: string;
  description: string;
  contact: string;
  appVersion: string;
  os: string;
  images: string[];
  status: string;
  reply: string;
  updatedAt: string;
}
const feedbackRows: FakeFeedback[] = [
  {
    id: "demo-1",
    createdAt: new Date(Date.now() - 86_400_000).toISOString(),
    type: "bug",
    title: "Không tải được video Facebook riêng tư",
    description: "Dán link video trong nhóm kín thì báo lỗi đăng nhập.\nĐã chọn cookie Chrome.",
    contact: "Zalo 09xx",
    appVersion: "0.5.1",
    os: "Windows (NT 10.0)",
    images: [],
    status: "pending",
    reply: "",
    updatedAt: new Date().toISOString(),
  },
];
const feedbackImages: Record<string, { mime: string; data: string }> = {};

function fakeFeedback(req: Record<string, any>) {
  const admin = () => {
    if (req.adminKey !== "admin") throw "Sai mã quản trị.";
  };
  switch (req.action) {
    case "submit": {
      const id = crypto.randomUUID();
      const images = (req.images ?? []).map((img: { mime: string; data: string }, i: number) => {
        feedbackImages[`${id}-${i}`] = img;
        return `${id}-${i}`;
      });
      const now = new Date().toISOString();
      const row = { ...req, id, images, status: "pending", reply: "", createdAt: now, updatedAt: now } as FakeFeedback;
      feedbackRows.push(row);
      if (/duyệt/i.test(row.title)) {
        setTimeout(() => Object.assign(row, { status: "accepted", reply: "Ý tưởng hay, bản sau sẽ có nhé!" }), 5000);
      }
      return { id };
    }
    case "status":
      return feedbackRows.filter((r) => req.ids.includes(r.id)).map(({ id, status, reply, updatedAt }) => ({ id, status, reply, updatedAt }));
    case "list":
      admin();
      return [...feedbackRows].reverse();
    case "image":
      admin();
      return feedbackImages[req.fileId];
    case "review": {
      admin();
      const row = feedbackRows.find((r) => r.id === req.id);
      if (!row) throw "Không tìm thấy góp ý.";
      Object.assign(row, { status: req.status, reply: req.reply, updatedAt: new Date().toISOString() });
      return { id: row.id, status: row.status, updatedAt: row.updatedAt };
    }
  }
  throw "Yêu cầu không hợp lệ.";
}

const SUBS = [
  { lang: "en", name: "English", auto: false },
  { lang: "vi", name: "Vietnamese", auto: false },
  { lang: "en-orig", name: "English (Original)", auto: true },
];

const SAMPLES = {
  Youtube: { title: "Lofi hip hop radio – beats to relax/study to", uploader: "Lofi Girl", duration: 7384, heights: [2160, 1440, 1080, 720, 480, 360, 240, 144], subtitles: SUBS },
  TikTok: { title: "Quan quan hà quan | Lê Bảo cover 🎶 #nhachay", uploader: "lebaomusic", duration: 58, heights: [1080, 720, 540], subtitles: [] },
  Facebook: { title: "Highlight trận đấu tối qua", uploader: "VTV Thể thao", duration: 312, heights: [720, 360], subtitles: [] },
  Soundcloud: { title: "Flickermood", uploader: "Forss", duration: 213, heights: [], subtitles: [] },
};

function siteFor(url: string): keyof typeof SAMPLES {
  if (/soundcloud/i.test(url)) return "Soundcloud";
  if (/tiktok/i.test(url)) return "TikTok";
  if (/facebook|fb\.watch/i.test(url)) return "Facebook";
  return "Youtube";
}

function sampleInfo(url: string) {
  const site = siteFor(url);
  if (/playlist|\/videos|\/sets\//i.test(url)) {
    return {
      kind: "playlist",
      title: site === "Soundcloud" ? "Soulhack" : "Nhạc chill cuối tuần",
      uploader: SAMPLES[site].uploader,
      site: site === "Soundcloud" ? "SoundcloudSet" : "YoutubeTab",
      count: site === "Soundcloud" ? 11 : 182,
      entries: Array.from({ length: site === "Soundcloud" ? 11 : 24 }, (_, i) => ({
        url: `${url}#${i + 1}`,
        title: `${site === "Soundcloud" ? "track" : "Video"} số ${i + 1} – ${SAMPLES[site].title}`,
        duration: 120 + i * 17,
        thumbnail: site === "Soundcloud" ? null : placeholder(site),
      })),
    };
  }
  const s = SAMPLES[site];
  return { kind: "video", ...s, site, url, maxHeight: s.heights[0] ?? null, thumbnail: placeholder(site) };
}

/** Ảnh bìa giả vẽ bằng SVG, không cần mạng. */
function placeholder(site: string) {
  const color = { Youtube: "#ff0033", TikTok: "#25f4ee", Facebook: "#1877f2", Soundcloud: "#ff5500" }[site] ?? "#6d5dfc";
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 360">
    <defs><linearGradient id="g" x2="1" y2="1"><stop offset="0" stop-color="${color}"/><stop offset="1" stop-color="#1b1d27"/></linearGradient></defs>
    <rect width="640" height="360" fill="url(#g)"/><polygon points="290,140 290,220 360,180" fill="#fff" opacity=".85"/></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

async function installDeps() {
  for (const name of ["yt-dlp", "ffmpeg", "deno"]) {
    const total = 40_000_000;
    for (let done = 0; done <= total; done += 8_000_000) {
      await emit("deps-progress", { name, stage: "download", downloaded: done, total });
      await sleep(150);
    }
    await emit("deps-progress", { name, stage: "extract", downloaded: total, total });
    await sleep(300);
    await emit("deps-progress", { name, stage: "done", downloaded: total, total });
  }
  depsReady = true;
}

async function fakeDownload(id: string, url: string, mode: string, audioFormat: string | null, subtitleLang: string | null) {
  const audio = mode === "audio";
  const parts = audio ? 1 : 2;
  for (let part = 1; part <= parts; part++) {
    for (let p = 0; p <= 100; p += 4) {
      if (canceled.has(id)) return emit("dl-done", { id, ok: false, canceled: true, file: null, error: null });
      if (url.includes("fail") && p > 40) {
        return emit("dl-done", {
          id,
          ok: false,
          canceled: false,
          file: null,
          error: "Trang web có thể vừa thay đổi. Thử Cài đặt → Cập nhật yt-dlp rồi tải lại.\nERROR: HTTP Error 403: Forbidden",
        });
      }
      await emit("dl-progress", { id, stage: "download", part, percent: p, speed: 2_500_000 + Math.random() * 1_000_000, eta: (100 - p) / 8, label: null });
      await sleep(120);
    }
  }
  for (const label of audio ? ["ExtractAudio", "EmbedThumbnail"] : ["Merger", "Metadata"]) {
    await emit("dl-progress", { id, stage: "process", part: parts, percent: null, speed: null, eta: null, label });
    await sleep(900);
  }
  const ext = !audio ? "mp4" : audioFormat === "original" || !audioFormat ? "opus" : audioFormat;
  const name = `${url.includes("missing") ? "missing " : ""}video [${id.slice(0, 6)}].${ext}`;
  const note = subtitleLang === "vi" ? "Không tải được phụ đề (trang tạm chặn, thử lại sau)." : null;
  await emit("dl-done", { id, ok: true, canceled: false, file: `C:\\Users\\you\\Downloads\\${name}`, error: null, note });
}

async function fakeAppUpdate(onEvent: { onmessage: (e: unknown) => void }) {
  const total = 9_000_000;
  onEvent.onmessage({ event: "Started", data: { contentLength: total } });
  for (let done = 0; done < total; done += 300_000) {
    await sleep(80);
    onEvent.onmessage({ event: "Progress", data: { chunkLength: 300_000 } });
  }
  onEvent.onmessage({ event: "Finished" });
}

export function install() {
  mockIPC(
    async (cmd, args) => {
      const a = (args ?? {}) as Record<string, any>;
      switch (cmd) {
        case "deps_status":
          return { ready: depsReady, missing: depsReady ? [] : ["yt-dlp", "ffmpeg", "deno"], ytdlpVersion: depsReady ? "2026.10.26" : null, binDir: "C:\\mock\\bin" };
        case "install_deps":
          return installDeps();
        case "update_ytdlp":
          await sleep(1200);
          return "yt-dlp is up to date (mock)";
        case "fetch_info":
          await sleep(900);
          if (params.has("error") || a.url.includes("error")) {
            throw "Video cần đăng nhập hoặc là video riêng tư. Vào Cài đặt → Cookie đăng nhập, chọn trình duyệt bạn đã đăng nhập.\nERROR: [TikTok] 123: This video is private";
          }
          return sampleInfo(a.url);
        case "start_download":
          canceled.delete(a.id);
          fakeDownload(a.id, a.url, a.mode, a.audioFormat, a.subtitle?.lang ?? null);
          return null;
        case "cancel_download":
          canceled.add(a.id);
          return null;
        case "feedback_request":
          await sleep(500);
          return fakeFeedback(a.body);
        case "files_exist":
          // Giả lập file có chữ "missing" trong tên là đã bị xoá.
          return (a.paths as string[]).map((p) => !p.includes("missing"));

        case "plugin:app|version":
          return "0.2.0";
        case "plugin:path|resolve_directory":
          return "C:\\Users\\you\\Downloads";
        case "plugin:clipboard-manager|read_text":
          return navigator.clipboard.readText().catch(() => "");
        case "plugin:dialog|open":
          return a.options?.directory ? "D:\\Videos" : "C:\\Users\\you\\cookies.txt";
        case "plugin:updater|check":
          return params.has("update")
            ? { rid: 1, currentVersion: "0.2.0", version: "0.3.0", date: new Date().toISOString(), body: "", rawJson: {} }
            : null;
        case "plugin:updater|download_and_install":
          return fakeAppUpdate(a.onEvent);
        case "plugin:process|restart":
          location.search = "";
          return null;
        default:
          console.info("[mock] bỏ qua", cmd, a);
          return null;
      }
    },
    { shouldMockEvents: true },
  );
  console.info("[mock] Đang dùng backend giả. Thử ?setup, ?update, ?error trên URL.");
}
