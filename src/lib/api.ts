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

export interface SubtitleStream {
  index: number;
  codec: string;
  language: string | null;
  title: string | null;
  /** Phụ đề dạng chữ, tách ra .srt được (dạng hình ảnh như PGS/DVD thì không). */
  text: boolean;
}

export interface TrProgress {
  id: string;
  stage: "convert" | "transcribe";
  percent: number | null;
}

export interface TrDone {
  id: string;
  ok: boolean;
  canceled: boolean;
  file: string | null;
  error: string | null;
  /** Ngôn ngữ Whisper nhận ra khi chọn "Tự nhận". */
  language: string | null;
}

export const depsStatus = () => invoke<DepsStatus>("deps_status");
export const installDeps = (force = false) => invoke<void>("install_deps", { force });
export const updateYtdlp = () => invoke<string>("update_ytdlp");
export const fetchInfo = (url: string, cookies: string) => invoke<FetchResult>("fetch_info", { url, cookies });
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

export const whisperReady = () => invoke<boolean>("whisper_ready");
export const installWhisper = () => invoke<void>("install_whisper");
export const probeSubtitles = (path: string) => invoke<SubtitleStream[]>("probe_subtitles", { path });
export const extractSubtitle = (path: string, index: number, language: string | null) =>
  invoke<string>("extract_subtitle", { path, index, language });
export const transcribe = (id: string, path: string, language: string) => invoke<void>("transcribe", { id, path, language });
export const cancelTranscribe = (id: string) => invoke<void>("cancel_transcribe", { id });

export const onDepsProgress = (cb: (p: DepsProgress) => void) => listen<DepsProgress>("deps-progress", (e) => cb(e.payload));
export const onProgress = (cb: (p: DlProgress) => void) => listen<DlProgress>("dl-progress", (e) => cb(e.payload));
export const onDone = (cb: (d: DlDone) => void) => listen<DlDone>("dl-done", (e) => cb(e.payload));
export const onTrProgress = (cb: (p: TrProgress) => void) => listen<TrProgress>("tr-progress", (e) => cb(e.payload));
export const onTrDone = (cb: (d: TrDone) => void) => listen<TrDone>("tr-done", (e) => cb(e.payload));
