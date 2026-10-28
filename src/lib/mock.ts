// Backend giả để chỉnh giao diện bằng `npm run dev` trên trình duyệt (chỉ nạp ở chế độ dev, ngoài app Tauri).
// Thêm tham số vào URL để xem các trạng thái khác:
//   ?setup   màn hình cài đặt lần đầu
//   ?update  có bản app mới
//   ?error   "Lấy video" luôn báo lỗi
// Link chứa chữ "fail" thì lượt tải sẽ lỗi giữa chừng.

import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";

const params = new URLSearchParams(location.search);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

let depsReady = !params.has("setup");
const canceled = new Set<string>();

const SAMPLES = {
  Youtube: { title: "Lofi hip hop radio – beats to relax/study to", uploader: "Lofi Girl", duration: 7384, heights: [2160, 1440, 1080, 720, 480, 360, 240, 144] },
  TikTok: { title: "Quan quan hà quan | Lê Bảo cover 🎶 #nhachay", uploader: "lebaomusic", duration: 58, heights: [1080, 720, 540] },
  Facebook: { title: "Highlight trận đấu tối qua", uploader: "VTV Thể thao", duration: 312, heights: [720, 360] },
};

function sampleInfo(url: string) {
  const site = /tiktok/i.test(url) ? "TikTok" : /facebook|fb\.watch/i.test(url) ? "Facebook" : "Youtube";
  const s = SAMPLES[site];
  return { ...s, site, url, maxHeight: s.heights[0], thumbnail: placeholder(site) };
}

/** Ảnh bìa giả vẽ bằng SVG, không cần mạng. */
function placeholder(site: string) {
  const color = { Youtube: "#ff0033", TikTok: "#25f4ee", Facebook: "#1877f2" }[site] ?? "#6d5dfc";
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

async function fakeDownload(id: string, url: string, mode: string) {
  const parts = mode === "mp3" ? 1 : 2;
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
  for (const label of mode === "mp3" ? ["ExtractAudio", "EmbedThumbnail"] : ["Merger", "Metadata"]) {
    await emit("dl-progress", { id, stage: "process", part: parts, percent: null, speed: null, eta: null, label });
    await sleep(900);
  }
  const ext = mode === "mp3" ? "mp3" : "mp4";
  await emit("dl-done", { id, ok: true, canceled: false, file: `C:\\Users\\you\\Downloads\\video [${id.slice(0, 6)}].${ext}`, error: null });
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
          fakeDownload(a.id, a.url, a.mode);
          return null;
        case "cancel_download":
          canceled.add(a.id);
          return null;

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
