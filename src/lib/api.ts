import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Mode = "best" | "compat" | "audio";
export type AudioFormat = "original" | "mp3" | "flac" | "wav";

export interface DepsStatus {
  ready: boolean;
  missing: string[];
  ytdlpVersion: string | null;
  binDir: string;
}

export interface DepsProgress {
  name: string;
  stage: "download" | "extract" | "done";
  downloaded: number;
  total: number | null;
}

export interface VideoInfo {
  title: string;
  thumbnail: string | null;
  duration: number | null;
  uploader: string | null;
  site: string;
  maxHeight: number | null;
  /** Rỗng nghĩa là chỉ có âm thanh (ví dụ SoundCloud). */
  heights: number[];
  subtitles: Subtitle[];
  url: string;
}

export interface Subtitle {
  /** Mã ngôn ngữ như yt-dlp liệt kê, ví dụ "en" hoặc "en-orig". */
  lang: string;
  name: string;
  /** Phụ đề tự động (nhận dạng giọng nói). */
  auto: boolean;
}

export interface PlaylistEntry {
  url: string;
  title: string;
  duration: number | null;
  thumbnail: string | null;
}

export interface PlaylistInfo {
  title: string;
  uploader: string | null;
  site: string;
  /** Tổng số mục nếu trang cho biết; có thể nhiều hơn số mục lấy về. */
  count: number | null;
  entries: PlaylistEntry[];
}

export type FetchResult = ({ kind: "video" } & VideoInfo) | ({ kind: "playlist" } & PlaylistInfo);

export interface DlProgress {
  id: string;
  stage: "download" | "process";
  part: number;
  percent: number | null;
  speed: number | null;
  eta: number | null;
  label: string | null;
}

export interface DlDone {
  id: string;
  ok: boolean;
  canceled: boolean;
  file: string | null;
  error: string | null;
  /** Ghi chú khi tải xong nhưng thiếu phần phụ (ví dụ phụ đề bị chặn). */
  note: string | null;
}

export const depsStatus = () => invoke<DepsStatus>("deps_status");
export const installDeps = (force = false) => invoke<void>("install_deps", { force });
export const updateYtdlp = () => invoke<string>("update_ytdlp");
/** `playlist`: link video kèm &list=... thì lấy cả danh sách thay vì một video. */
export const fetchInfo = (url: string, cookies: string, playlist = false) =>
  invoke<FetchResult>("fetch_info", { url, cookies, playlist });
export const startDownload = (args: {
  id: string;
  url: string;
  mode: Mode;
  /** Chỉ dùng khi mode là "audio". */
  audioFormat: AudioFormat | null;
  quality: number | null;
  /** Tên người dùng tự đặt; null là giữ tiêu đề gốc. */
  title: string | null;
  subtitle: Subtitle | null;
  /** Phụ đề "chỉ chữ": tải xong đổi .srt thành .txt gộp thành một đoạn. */
  subtitleText: boolean;
  /** Chỉ tải đoạn [bắt đầu, kết thúc] (giây); null là cả video. */
  section: [number, number] | null;
  outDir: string;
  cookies: string;
}) =>
  invoke<void>("start_download", args);
export const cancelDownload = (id: string) => invoke<void>("cancel_download", { id });
export const filesExist = (paths: string[]) => invoke<boolean[]>("files_exist", { paths });

// Nén video về dung lượng đích (src-tauri/src/compress.rs).
export interface VideoProbe {
  size: number;
  duration: number;
  width: number;
  height: number;
  fps: number;
  audioCodec: string | null;
  audioBps: number;
}

export interface CompressDone {
  id: string;
  ok: boolean;
  canceled: boolean;
  file: string | null;
  size: number | null;
  /** Độ cao đã hạ xuống khi nén mạnh (null là giữ nguyên độ phân giải). */
  height: number | null;
  error: string | null;
}

export const compressProbe = (path: string) => invoke<VideoProbe>("compress_probe", { path });
/** `targetMb`: dung lượng đích, 1 MB = 1024 × 1024 byte. */
export const compressVideo = (id: string, input: string, targetMb: number) =>
  invoke<void>("compress_video", { id, input, targetMb });
export const compressCancel = (id: string) => invoke<void>("compress_cancel", { id });
export const onCompressProgress = (cb: (p: { id: string; percent: number }) => void) =>
  listen<{ id: string; percent: number }>("compress-progress", (e) => cb(e.payload));
export const onCompressDone = (cb: (d: CompressDone) => void) => listen<CompressDone>("compress-done", (e) => cb(e.payload));

/** Mở cửa sổ đăng nhập trang web ngay trong app; đóng cửa sổ thì có sự kiện "login-closed". */
export const loginOpen = (url: string) => invoke<void>("login_open", { url });
/** Đăng xuất mọi trang đã đăng nhập trong app (xoá cookie), trả về số cookie đã xoá. */
export const loginClear = () => invoke<number>("login_clear");
export const onLoginClosed = (cb: () => void) => listen("login-closed", () => cb());

export const onDepsProgress = (cb: (p: DepsProgress) => void) => listen<DepsProgress>("deps-progress", (e) => cb(e.payload));
export const onProgress = (cb: (p: DlProgress) => void) => listen<DlProgress>("dl-progress", (e) => cb(e.payload));
export const onDone = (cb: (d: DlDone) => void) => listen<DlDone>("dl-done", (e) => cb(e.payload));
