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
  outDir: string;
  cookies: string;
}) =>
  invoke<void>("start_download", args);
export const cancelDownload = (id: string) => invoke<void>("cancel_download", { id });
export const filesExist = (paths: string[]) => invoke<boolean[]>("files_exist", { paths });

// Torrent (src-tauri/src/torrent.rs).
export interface TorrentFile {
  index: number;
  name: string;
  size: number;
}

export interface TorrentAdded {
  id: number;
  name: string;
  folder: string;
  files: TorrentFile[];
  /** Torrent này đã có trong danh sách từ trước. */
  already: boolean;
}

export type TorrentState = "checking" | "downloading" | "paused" | "done" | "error";

export interface TorrentStatus {
  id: number;
  name: string;
  state: TorrentState;
  progress: number;
  total: number;
  /** Byte/giây. */
  speed: number;
  error: string | null;
  folder: string;
}

/** `source`: link magnet, link .torrent hoặc đường dẫn file .torrent. Mở ở trạng thái tạm dừng để chọn file. */
export const torrentAdd = (source: string, outDir: string) => invoke<TorrentAdded>("torrent_add", { source, outDir });
export const torrentStart = (id: number, files: number[]) => invoke<void>("torrent_start", { id, files });
export const torrentPause = (id: number) => invoke<void>("torrent_pause", { id });
export const torrentResume = (id: number) => invoke<void>("torrent_resume", { id });
export const torrentRemove = (id: number, deleteFiles: boolean) => invoke<void>("torrent_remove", { id, deleteFiles });
export const torrentList = () => invoke<TorrentStatus[]>("torrent_list");

export const onDepsProgress = (cb: (p: DepsProgress) => void) => listen<DepsProgress>("deps-progress", (e) => cb(e.payload));
export const onProgress = (cb: (p: DlProgress) => void) => listen<DlProgress>("dl-progress", (e) => cb(e.payload));
export const onDone = (cb: (d: DlDone) => void) => listen<DlDone>("dl-done", (e) => cb(e.payload));
