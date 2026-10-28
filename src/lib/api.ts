import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Mode = "best" | "compat" | "mp3";

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
  heights: number[];
  url: string;
}

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
}

export const depsStatus = () => invoke<DepsStatus>("deps_status");
export const installDeps = (force = false) => invoke<void>("install_deps", { force });
export const updateYtdlp = () => invoke<string>("update_ytdlp");
export const fetchInfo = (url: string, cookies: string) => invoke<VideoInfo>("fetch_info", { url, cookies });
export const startDownload = (args: {
  id: string;
  url: string;
  mode: Mode;
  quality: number | null;
  outDir: string;
  cookies: string;
}) =>
  invoke<void>("start_download", args);
export const cancelDownload = (id: string) => invoke<void>("cancel_download", { id });

export const onDepsProgress = (cb: (p: DepsProgress) => void) => listen<DepsProgress>("deps-progress", (e) => cb(e.payload));
export const onProgress = (cb: (p: DlProgress) => void) => listen<DlProgress>("dl-progress", (e) => cb(e.payload));
export const onDone = (cb: (d: DlDone) => void) => listen<DlDone>("dl-done", (e) => cb(e.payload));
