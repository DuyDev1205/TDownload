<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { downloadDir } from "@tauri-apps/api/path";
  import { readText } from "@tauri-apps/plugin-clipboard-manager";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import * as api from "./lib/api";
  import type {
    AudioFormat,
    DepsProgress,
    DepsStatus,
    DlDone,
    DlProgress,
    Mode,
    PlaylistInfo,
    Subtitle,
    VideoInfo,
  } from "./lib/api";
  import {
    SITE_NAME,
    extractAllUrls,
    extractUrl,
    extractUrls,
    thumbFallback,
    uniqueLinks,
    videoKey,
    fmtBytes,
    fmtDay,
    fmtDuration,
    fmtQuality,
    fmtTime,
    isKnownSite,
    normalizeUrl,
    siteOf,
    subtitleName,
  } from "./lib/format";
  import { load, save } from "./lib/storage";
  import { getTheme, setTheme, THEME_MODES } from "./lib/theme";
  import { getLang, LANGUAGES, localeTag, setLang, t, type Lang } from "./lib/i18n.svelte";
  import { getUserName, nameAsked, setUserName, skipUserName, startUsageReport } from "./lib/usage";
  import Logo from "./lib/Logo.svelte";
  import FeedbackTab from "./lib/FeedbackTab.svelte";

  type Status = "queued" | "downloading" | "processing" | "done" | "error" | "canceled";

  interface Item {
    id: string;
    info: VideoInfo;
    mode: Mode;
    audioFormat: AudioFormat | null;
    quality: number | null;
    /** Tên người dùng tự đặt; null là giữ tiêu đề gốc. */
    title: string | null;
    /** Không có ở lịch sử lưu từ bản cũ. */
    subtitle?: Subtitle | null;
    /** Phụ đề "chỉ chữ": lưu .txt gộp thành một đoạn thay cho .srt có mốc thời gian. */
    subtitleText?: boolean;
    /** Ghi chú khi tải xong nhưng thiếu phần phụ (ví dụ phụ đề bị chặn). */
    note?: string | null;
    /** Chỉ tải đoạn [bắt đầu, kết thúc] (giây); không có hoặc null là cả video. */
    section?: [number, number] | null;
    status: Status;
    percent: number | null;
    speed: number | null;
    eta: number | null;
    part: number;
    stage: string;
    file: string | null;
    error: string | null;
    addedAt: number;
    doneAt: number | null;
    /** File đã tải nhưng không còn ở chỗ cũ (bị xoá hoặc chuyển đi). */
    missing?: boolean;
  }

  const MAX_PARALLEL = 2;
  const HISTORY_LIMIT = 200;
  const DAY = 24 * 60 * 60 * 1000;
  const APP_UPDATE_EVERY = 30 * 60 * 1000;
  const isMac = navigator.userAgent.includes("Mac");

  const MODES: Mode[] = ["best", "compat", "audio"];
  const AUDIO_FORMATS: AudioFormat[] = ["original", "mp3", "flac", "wav"];

  /** Tên định dạng âm thanh: "Gốc" theo ngôn ngữ đang chọn, còn lại là MP3/FLAC/WAV. */
  const audioLabel = (f: AudioFormat) => (f === "original" ? t("audio.original") : f.toUpperCase());

  /** Tên trình duyệt giữ nguyên; "", "app" và "file" đổi theo ngôn ngữ. */
  const COOKIE_SOURCES: [string, string][] = [
    ["", ""],
    ["app", ""],
    ["firefox", "Firefox"],
    ["chrome", "Chrome"],
    ["edge", "Edge"],
    ["brave", "Brave"],
    ...(isMac ? ([["safari", "Safari"]] as [string, string][]) : []),
    ["file", ""],
  ];
  const cookieLabel = (value: string, label: string) =>
    value === ""
      ? t("settings.cookiesNone")
      : value === "app"
        ? t("settings.cookiesApp")
        : value === "file"
          ? t("settings.cookiesFile")
          : label;

  let deps = $state<DepsStatus | null>(null);
  let installing = $state(false);
  let installError = $state("");
  let installProgress = $state<Record<string, DepsProgress>>({});

  let url = $state("");
  let fetching = $state(false);
  let fetchError = $state("");
  /** Thông báo nhẹ (không phải lỗi), ví dụ đã bỏ link trùng hoặc video đã tải rồi. */
  let notice = $state("");
  /** Đang chờ người dùng đăng nhập trong cửa sổ đăng nhập; đóng cửa sổ thì tự thử lại. */
  let loginWaiting = $state(false);
  let logoutMsg = $state("");
  let info = $state<VideoInfo | null>(null);
  let customTitle = $state("");
  /** Vị trí phụ đề đã chọn trong info.subtitles; -1 là không tải phụ đề. */
  let subtitleIndex = $state(-1);
  /** Kiểu phụ đề người dùng chọn lần trước: có mốc thời gian (.srt) hay chỉ chữ (.txt). */
  let subtitleText = $state(load("subtitleText", "0") === "1");
  // Chỉ tải một đoạn của video (giây).
  let cutOn = $state(false);
  let cutStart = $state(0);
  let cutEnd = $state(0);
  /** Đoạn sẽ tải, hoặc null nếu tải cả video (chưa bật cắt, hoặc đoạn chọn chính là cả video). */
  const section = $derived<[number, number] | null>(
    cutOn && info?.duration && cutEnd > cutStart && (cutStart > 0 || cutEnd < Math.floor(info.duration))
      ? [cutStart, cutEnd]
      : null,
  );
  let urlInput = $state<HTMLInputElement>();

  // Link playlist/kênh: danh sách video kèm ô chọn.
  let playlist = $state<PlaylistInfo | null>(null);
  let picked = $state<boolean[]>([]);
  const pickedCount = $derived(picked.filter(Boolean).length);

  // Dán nhiều link một lúc: đọc lần lượt từng link rồi tự thêm vào danh sách tải.
  const urlList = $derived(extractUrls(url));
  const isBatch = $derived(urlList.length > 1);
  let batchRead = $state<number | null>(null);

  /** SoundCloud và các trang chỉ có âm thanh thì chỉ cho chọn "Chỉ âm thanh". */
  const audioOnly = $derived(
    info ? info.heights.length === 0 : playlist ? siteOf(playlist.site) === "soundcloud" : false,
  );

  // Bản cũ lưu chế độ "mp3": chuyển sang "Chỉ âm thanh" với định dạng MP3 như trước.
  const savedMode = load("mode", "best");
  let mode = $state<Mode>(savedMode === "mp3" ? "audio" : (savedMode as Mode));
  let audioFormat = $state<AudioFormat>(load("audioFormat", savedMode === "mp3" ? "mp3" : "original") as AudioFormat);
  const audioHint = $derived(t(`audio.${audioFormat}.hint`));
  const effectiveMode = $derived<Mode>(audioOnly ? "audio" : mode);
  /** Độ phân giải tối đa muốn tải; null là lấy cao nhất có thể. */
  let quality = $state<number | null>(null);
  // Chế độ Tương thích chỉ lấy tới 1080p nên ẩn các mức cao hơn.
  const qualityOptions = $derived(info ? info.heights.filter((h) => effectiveMode !== "compat" || h <= 1080) : []);
  // Chưa chọn (hoặc mức đã chọn không có ở chế độ này) thì lấy mức cao nhất của video.
  const chosenQuality = $derived(
    quality != null && qualityOptions.includes(quality) ? quality : (qualityOptions[0] ?? null),
  );
  let outDir = $state(load("outDir", ""));
  let cookieKind = $state(load("cookieKind", ""));
  let cookieFile = $state(load("cookieFile", ""));
  // "app": phần Rust tự xuất cookie của cửa sổ đăng nhập trong app.
  const cookies = $derived(
    cookieKind === "app"
      ? "app"
      : cookieKind === "file" ? (cookieFile ? `file:${cookieFile}` : "") : cookieKind ? `browser:${cookieKind}` : "",
  );

  let tab = $state<"download" | "feedback">("download");
  let feedbackUnseen = $state(0);
  let showSettings = $state(false);
  let themeMode = $state(getTheme());
  let lang = $state<Lang>(getLang());

  // Hỏi tên một lần (không bắt buộc) để tác giả biết ai đang dùng app.
  let userName = $state(getUserName());
  let nameDraft = $state(getUserName());
  let askName = $state(!nameAsked());

  function saveName() {
    setUserName(nameDraft);
    userName = nameDraft = getUserName();
    askName = false;
  }

  /** Nút giao diện cạnh Cài đặt: chuyển qua lại giữa Sáng và Tối. */
  function toggleTheme() {
    themeMode = themeMode === "dark" ? "light" : "dark";
    setTheme(themeMode);
  }
  let updating = $state(false);
  let updateMsg = $state("");
  let items = $state<Item[]>(loadHistory());
  const groups = $derived.by(() => {
    const out: { day: string; items: Item[] }[] = [];
    for (const it of items) {
      const day = fmtDay(it.addedAt);
      const last = out[out.length - 1];
      if (last?.day === day) last.items.push(it);
      else out.push({ day, items: [it] });
    }
    return out;
  });
  let lastAutoPaste = "";

  let appVersion = $state("");
  let appUpdate = $state.raw<Update | null>(null);
  let appUpdating = $state(false);
  let appUpdatePercent = $state<number | null>(null);
  let appUpdateMsg = $state("");
  let lastAppCheck = 0;

  const hasFinished = $derived(items.some((i) => ["done", "error", "canceled"].includes(i.status)));
  const busy = $derived(items.some((i) => ["queued", "downloading", "processing"].includes(i.status)));

  $effect(() => {
    save("mode", mode);
    save("audioFormat", audioFormat);
    save("outDir", outDir);
    save("cookieKind", cookieKind);
    save("cookieFile", cookieFile);
    save("subtitleText", subtitleText ? "1" : "0");
  });

  onMount(() => {
    const unlisteners = [
      api.onDepsProgress((p) => {
        installProgress[p.name] = p;
      }),
      api.onProgress(onProgress),
      api.onDone(onDone),
      api.onLoginClosed(() => {
        if (!loginWaiting) return;
        loginWaiting = false;
        cookieKind = "app";
        getInfo();
      }),
    ];
    window.addEventListener("focus", autoPaste);
    window.addEventListener("focus", maybeCheckAppUpdate);
    const updateTimer = setInterval(maybeCheckAppUpdate, 5 * 60 * 1000);
    getVersion().then((v) => (appVersion = v));
    checkAppUpdate(true);
    checkMissingFiles();
    const stopUsageReport = startUsageReport();

    (async () => {
      if (!outDir) outDir = await downloadDir();
      await refreshDeps();
      if (deps?.ready && Date.now() - Number(load("lastUpdate", "0")) > DAY) {
        api.updateYtdlp().then(() => save("lastUpdate", String(Date.now()))).catch(() => {});
      }
    })();

    return () => {
      unlisteners.forEach((p) => p.then((off) => off()));
      window.removeEventListener("focus", autoPaste);
      window.removeEventListener("focus", maybeCheckAppUpdate);
      clearInterval(updateTimer);
      stopUsageReport();
    };
  });

  async function refreshDeps() {
    try {
      deps = await api.depsStatus();
    } catch (e) {
      installError = String(e);
    }
  }

  async function install(force = false) {
    installing = true;
    installError = "";
    installProgress = {};
    try {
      await api.installDeps(force);
      await refreshDeps();
    } catch (e) {
      installError = String(e);
    } finally {
      installing = false;
    }
  }

  async function updateNow() {
    updating = true;
    updateMsg = "";
    try {
      updateMsg = await api.updateYtdlp();
      save("lastUpdate", String(Date.now()));
      await refreshDeps();
    } catch (e) {
      updateMsg = t("common.error", { e: String(e) });
    } finally {
      updating = false;
    }
  }

  /** Hỏi GitHub Releases xem có bản app mới không. `quiet`: lúc mở app thì không báo gì nếu không có bản mới hay lỗi mạng. */
  async function checkAppUpdate(quiet = false) {
    lastAppCheck = Date.now();
    appUpdateMsg = "";
    try {
      appUpdate = await check();
      if (!appUpdate && !quiet) appUpdateMsg = t("update.latest");
    } catch (e) {
      lastAppCheck = 0; // Lỗi mạng thì lần quay lại app sau thử lại ngay.
      if (!quiet) appUpdateMsg = t("update.checkFailed", { e: String(e) });
    }
  }

  /** App mở lâu vẫn biết có bản mới: kiểm tra lại khi quay lại cửa sổ hoặc theo định kỳ, tối đa 30 phút một lần. */
  function maybeCheckAppUpdate() {
    if (appUpdate || appUpdating || Date.now() - lastAppCheck < APP_UPDATE_EVERY) return;
    checkAppUpdate(true);
  }

  async function installAppUpdate() {
    if (!appUpdate || busy) return;
    appUpdating = true;
    appUpdatePercent = null;
    appUpdateMsg = "";
    let total = 0;
    let received = 0;
    try {
      await appUpdate.downloadAndInstall((ev) => {
        if (ev.event === "Started") total = ev.data.contentLength ?? 0;
        else if (ev.event === "Progress") {
          received += ev.data.chunkLength;
          appUpdatePercent = total ? Math.min((received / total) * 100, 100) : null;
        }
      });
      await relaunch();
    } catch (e) {
      appUpdateMsg = t("update.failed", { e: String(e) });
      appUpdating = false;
    }
  }

  async function clipboardUrls(): Promise<string[]> {
    try {
      return extractUrls((await readText()) ?? "");
    } catch {
      return [];
    }
  }

  /** Đưa link từ clipboard vào ô: một link thì đọc luôn; nhiều link thì chờ chọn chế độ rồi bấm tải. */
  function useLinks(links: string[]) {
    url = links.join(" ");
    if (links.length === 1) getInfo();
    else resetPreview();
  }

  /** Khi quay lại app, nếu clipboard có link YouTube/TikTok/Facebook/SoundCloud mới thì tự dán. */
  async function autoPaste() {
    if (fetching || batchRead != null || !deps?.ready || tab !== "download") return;
    const links = (await clipboardUrls()).filter(isKnownSite);
    const key = links.join(" ");
    if (links.length && key !== lastAutoPaste && key !== url.trim()) {
      lastAutoPaste = key;
      useLinks(links);
    }
  }

  function resetPreview() {
    info = null;
    playlist = null;
    picked = [];
    fetchError = "";
    notice = "";
    customTitle = "";
    subtitleIndex = -1;
    cutOn = false;
    quality = null;
  }

  /** Mở cửa sổ đăng nhập ở chính link đang dán; đóng cửa sổ thì dùng cookie đó và tự thử lại (xem onLoginClosed). */
  async function startLogin() {
    const link = extractUrl(url);
    if (!link) return;
    loginWaiting = true;
    try {
      await api.loginOpen(link);
    } catch (e) {
      loginWaiting = false;
      fetchError = String(e);
    }
  }

  async function logoutSites() {
    try {
      const n = await api.loginClear();
      logoutMsg = t("settings.loggedOut", { n });
    } catch (e) {
      logoutMsg = t("common.error", { e: String(e) });
    }
  }

  function clearUrl() {
    url = "";
    resetPreview();
    urlInput?.focus();
  }

  async function pasteNow() {
    const links = await clipboardUrls();
    if (links.length) useLinks(links);
  }

  function submit() {
    if (isBatch) addBatch();
    else getInfo();
  }

  async function getInfo() {
    const found = extractUrl(url) ?? url.trim();
    if (!found || fetching) return;
    const target = normalizeUrl(found);
    // Link video YouTube mở từ trong playlist (watch?v=...&list=...): hiện cả danh sách để chọn.
    const wholeList = /youtube\.com\/watch\?.*[?&]list=|youtu\.be\/.*[?&]list=/i.test(target);
    // Cùng một video dán hai lần (vd. dính liền nhau) thì chỉ lấy một và báo cho người dùng.
    const dupes = uniqueLinks(extractAllUrls(url)).dupes;
    url = target;
    fetching = true;
    resetPreview();
    if (dupes) notice = t("dup.links", { n: dupes });
    try {
      const result = await api.fetchInfo(target, cookies, wholeList);
      if (result.kind === "playlist") {
        playlist = result;
        picked = result.entries.map(() => true);
      } else {
        info = result;
        customTitle = result.title;
        cutStart = 0;
        cutEnd = Math.floor(result.duration ?? 0);
        // yt-dlp không mở được playlist (thường do để Riêng tư) thì lặng lẽ trả về một video.
        if (wholeList) {
          fetchError = t("fetch.privatePlaylist");
        }
      }
    } catch (e) {
      fetchError = String(e);
    } finally {
      fetching = false;
    }
  }

  /** "1:05", "01:02:03" hoặc "65" thành số giây; sai định dạng thì null. */
  function parseClock(text: string): number | null {
    const parts = text.trim().split(":");
    if (parts.length > 3 || parts.some((p) => !/^\d+(\.\d+)?$/.test(p))) return null;
    return parts.reduce((sum, p) => sum * 60 + Number(p), 0);
  }

  /** Gõ giờ bắt đầu/kết thúc đoạn cắt; giờ sai hoặc làm đoạn rỗng thì trả ô về giá trị cũ. */
  function setCut(e: Event, which: "start" | "end", total: number) {
    const input = e.currentTarget as HTMLInputElement;
    const v = parseClock(input.value);
    if (which === "start" && v != null && v < cutEnd) cutStart = Math.floor(v);
    else if (which === "end" && v != null && v > cutStart && v <= total) cutEnd = Math.floor(v);
    input.value = fmtDuration(which === "start" ? cutStart : cutEnd) || "0:00";
  }

  /** Mức chất lượng cao nhất của video cho chế độ đang chọn (Tương thích chỉ tới 1080p). */
  function bestQuality(vi: VideoInfo, m: Mode): number | null {
    return vi.heights.find((h) => m !== "compat" || h <= 1080) ?? null;
  }

  function enqueue() {
    if (!info) return;
    const typed = customTitle.trim();
    const title = typed && typed !== info.title ? typed : null;
    const added = addItem(
      { ...$state.snapshot(info), title: title ?? info.title },
      effectiveMode,
      chosenQuality,
      title,
      section ? null : (info.subtitles[subtitleIndex] ?? null),
      section,
    );
    // Trùng thì giữ phần xem trước để người dùng chọn cách tải khác nếu muốn.
    if (added !== "added") {
      notice = t(added === "active" ? "dup.active" : "dup.done");
      return;
    }
    notice = "";
    info = null;
    url = "";
    saveHistory();
    pump();
  }

  /** Thêm các video đã chọn trong playlist. Dùng luôn thông tin của danh sách nên không phải đọc lại từng video. */
  function enqueuePlaylist() {
    if (!playlist) return;
    const site = playlist.site.replace(/(Tab|Set|Playlist|User)$/i, "");
    let skipped = 0;
    playlist.entries.forEach((e, i) => {
      if (!picked[i]) return;
      const vi: VideoInfo = {
        title: e.title,
        thumbnail: e.thumbnail,
        duration: e.duration,
        uploader: playlist!.uploader,
        site,
        maxHeight: null,
        heights: [],
        subtitles: [],
        url: e.url,
      };
      if (addItem(vi, effectiveMode, null) !== "added") skipped++;
    });
    url = "";
    resetPreview();
    if (skipped) notice = t("dup.skipped", { n: skipped });
    saveHistory();
    pump();
  }

  /** Đọc lần lượt từng link đã dán; đọc được cái nào thì thêm vào danh sách tải ngay cái đó. */
  async function addBatch() {
    if (fetching || batchRead != null) return;
    const links = urlList.map(normalizeUrl);
    const dupes = uniqueLinks(extractAllUrls(url)).dupes;
    const failed: string[] = [];
    let skipped = 0;
    resetPreview();
    batchRead = 0;
    for (const link of links) {
      try {
        const result = await api.fetchInfo(link, cookies);
        if (result.kind === "playlist") {
          failed.push(`${link}\n  ${t("batch.isPlaylist")}`);
        } else {
          const m = result.heights.length ? mode : "audio";
          if (addItem(result, m, m === "audio" ? null : bestQuality(result, m)) !== "added") skipped++;
          saveHistory();
          pump();
        }
      } catch (e) {
        failed.push(`${link}\n  ${String(e).split("\n").pop()}`);
      }
      batchRead++;
    }
    batchRead = null;
    url = failed.length ? failed.map((f) => f.split("\n")[0]).join(" ") : "";
    fetchError = failed.length
      ? `${t("batch.failed", { n: failed.length, total: links.length })}\n${failed.join("\n")}`
      : "";
    notice = [dupes && t("dup.links", { n: dupes }), skipped && t("dup.skipped", { n: skipped })].filter(Boolean).join(" ");
  }

  /**
   * Thêm một lượt tải, trừ khi cùng video với cùng cách tải (chế độ, định dạng, chất lượng, phụ đề) đang tải ("active")
   * hoặc đã tải xong mà file vẫn còn ("done"). Mục cũ bị lỗi, đã huỷ hoặc mất file thì thay bằng mục mới, không để trùng.
   */
  function addItem(
    vi: VideoInfo,
    m: Mode,
    quality: number | null,
    title: string | null = null,
    subtitle: Subtitle | null = null,
    section: [number, number] | null = null,
  ): "added" | "active" | "done" {
    const key = videoKey(vi.url);
    const af = m === "audio" ? audioFormat : null;
    const q = m === "audio" ? null : quality;
    const asText = !!subtitle && subtitleText;
    const same = items.find(
      (i) =>
        videoKey(i.info.url) === key &&
        i.mode === m &&
        i.audioFormat === af &&
        i.quality === q &&
        (i.subtitle?.lang ?? null) === (subtitle?.lang ?? null) &&
        JSON.stringify(i.section ?? null) === JSON.stringify(section) &&
        (i.subtitleText ?? false) === asText,
    );
    if (same && ["queued", "downloading", "processing"].includes(same.status)) return "active";
    if (same?.status === "done" && !same.missing) return "done";
    if (same) items = items.filter((i) => i.id !== same.id);
    items.unshift({
      id: crypto.randomUUID(),
      info: vi,
      mode: m,
      audioFormat: m === "audio" ? audioFormat : null,
      quality: m === "audio" ? null : quality,
      title,
      subtitle,
      subtitleText: asText,
      section,
      note: null,
      status: "queued",
      percent: null,
      speed: null,
      eta: null,
      part: 0,
      stage: "",
      file: null,
      error: null,
      addedAt: Date.now(),
      doneAt: null,
    });
    return "added";
  }

  /** Nạp lịch sử tải từ lần trước. Mục đang tải dở khi tắt app chuyển sang lỗi để có nút Thử lại. */
  function loadHistory(): Item[] {
    try {
      const list = JSON.parse(load("history", "[]")) as Item[];
      return list.map((it) =>
        ["queued", "downloading", "processing"].includes(it.status)
          ? { ...it, status: "error", percent: null, speed: null, eta: null, error: t("history.interrupted") }
          : it,
      );
    } catch {
      return [];
    }
  }

  /** Ghi lịch sử khi trạng thái thay đổi (không ghi theo từng nhịp tiến độ). */
  function saveHistory() {
    save("history", JSON.stringify($state.snapshot(items).slice(0, HISTORY_LIMIT)));
  }

  /** Đánh dấu các file trong lịch sử đã bị xoá hoặc chuyển đi. */
  async function checkMissingFiles() {
    const done = items.filter((i) => i.status === "done" && i.file);
    if (!done.length) return;
    try {
      const exists = await api.filesExist(done.map((i) => i.file!));
      done.forEach((it, k) => (it.missing = !exists[k]));
    } catch {
      // Không kiểm tra được thì cứ để nút Mở như bình thường.
    }
  }

  async function openFile(it: Item, reveal = false) {
    if (!it.file) return;
    const [exists] = await api.filesExist([it.file]).catch(() => [true]);
    if (!exists) {
      it.missing = true;
      return;
    }
    if (reveal) revealItemInDir(it.file);
    else openPath(it.file);
  }

  /** Chạy tối đa MAX_PARALLEL lượt tải cùng lúc, lượt thêm trước chạy trước. */
  function pump() {
    let running = items.filter((i) => i.status === "downloading" || i.status === "processing").length;
    for (const it of [...items].reverse()) {
      if (running >= MAX_PARALLEL) break;
      if (it.status !== "queued") continue;
      running++;
      start(it);
    }
  }

  function start(it: Item) {
    Object.assign(it, { status: "downloading", percent: null, speed: null, eta: null, part: 0, stage: "", file: null, error: null });
    const args = {
      id: it.id,
      url: it.info.url,
      mode: it.mode,
      audioFormat: it.audioFormat,
      quality: it.quality,
      title: it.title,
      subtitle: it.subtitle ?? null,
      subtitleText: it.subtitleText ?? false,
      section: it.section ?? null,
      outDir,
      cookies,
    };
    api.startDownload(args).catch((e) => {
      it.status = "error";
      it.error = String(e);
      saveHistory();
      pump();
    });
  }

  function onProgress(p: DlProgress) {
    const it = items.find((i) => i.id === p.id);
    if (!it || it.status === "canceled") return;
    if (p.stage === "process") {
      it.status = "processing";
      it.stage = stageLabel(p.label);
      return;
    }
    Object.assign(it, { status: "downloading", part: p.part, percent: p.percent, speed: p.speed, eta: p.eta });
  }

  function onDone(d: DlDone) {
    const it = items.find((i) => i.id === d.id);
    if (it) {
      it.status = d.ok ? "done" : d.canceled ? "canceled" : "error";
      it.file = d.file;
      it.error = d.error;
      it.note = d.note;
      it.doneAt = Date.now();
      it.missing = false;
      if (d.ok) it.percent = 100;
      saveHistory();
    }
    pump();
  }

  function cancel(it: Item) {
    if (it.status === "queued") {
      it.status = "canceled";
      saveHistory();
    } else api.cancelDownload(it.id);
  }

  function retry(it: Item) {
    it.status = "queued";
    it.missing = false;
    saveHistory();
    pump();
  }

  function remove(it: Item) {
    items = items.filter((i) => i.id !== it.id);
    saveHistory();
  }

  function clearFinished() {
    items = items.filter((i) => !["done", "error", "canceled"].includes(i.status));
    saveHistory();
  }

  async function chooseDir() {
    const dir = await openDialog({ directory: true, defaultPath: outDir || undefined });
    if (typeof dir === "string") outDir = dir;
  }

  async function chooseCookieFile() {
    const file = await openDialog({ filters: [{ name: "cookies.txt", extensions: ["txt"] }] });
    if (typeof file === "string") cookieFile = file;
  }

  function stageLabel(pp: string | null): string {
    const name = (pp ?? "").toLowerCase();
    if (name.includes("merger")) return t("stage.merge");
    if (name.includes("extractaudio")) return t("stage.extractAudio");
    if (name.includes("thumbnail")) return t("stage.thumbnail");
    if (name.includes("metadata")) return t("stage.metadata");
    if (name.includes("remux") || name.includes("convertor")) return t("stage.remux");
    if (name.includes("movefiles")) return t("stage.move");
    return t("stage.processing");
  }

  function statusText(it: Item): string {
    switch (it.status) {
      case "queued":
        return t("status.queued");
      case "downloading": {
        const bits = [it.part > 1 ? t("status.part", { n: it.part }) : t("status.downloading")];
        if (it.percent != null) bits.push(`${it.percent.toFixed(1)}%`);
        if (it.speed) bits.push(`${fmtBytes(it.speed)}/s`);
        if (it.eta != null) bits.push(t("status.eta", { t: fmtDuration(it.eta) }));
        return bits.join(" · ");
      }
      case "processing":
        return it.stage;
      case "done":
        return t("status.done");
      case "canceled":
        return t("status.canceled");
      case "error":
        return it.error ?? t("status.unknownError");
    }
  }

  function depsLine(name: string): string {
    const p = installProgress[name];
    if (!p) return installing ? t("setup.waiting") : "";
    if (p.stage === "done") return t("setup.done");
    if (p.stage === "extract") return t("setup.extracting");
    return p.total ? `${fmtBytes(p.downloaded)} / ${fmtBytes(p.total)}` : fmtBytes(p.downloaded);
  }

  function depsPercent(name: string): number {
    const p = installProgress[name];
    if (!p) return 0;
    if (p.stage !== "download") return 100;
    return p.total ? (p.downloaded / p.total) * 100 : 0;
  }

  /** Tên thẻ chế độ; chế độ Video ghi luôn độ phân giải cao nhất của video đang xem, ví dụ "Video · 8K". */
  function modeTitle(m: Mode): string {
    const title = t(`mode.${m}.title`);
    return m === "best" && info?.maxHeight ? `${title} · ${fmtQuality(info.maxHeight)}` : title;
  }

  /** Mô tả ngắn cách tải của một mục, ví dụ "Video · 720p", "Video · Tương thích · 1080p" hoặc "Chỉ âm thanh · FLAC". */
  function itemFormat(it: Item): string {
    const parts = [t(`mode.${it.mode}.title`)];
    if (it.audioFormat) parts.push(audioLabel(it.audioFormat));
    else if (it.quality) parts.push(fmtQuality(it.quality));
    if (it.subtitle) {
      const name = t("item.subtitle", { name: subtitleName(it.subtitle) });
      parts.push(it.subtitleText ? `${name} (${t("sub.textShort")})` : name);
    }
    if (it.section) parts.push(`✂ ${fmtDuration(it.section[0]) || "0:00"}–${fmtDuration(it.section[1])}`);
    return parts.join(" · ");
  }
</script>

<!-- Chọn chế độ tải và định dạng âm thanh; dùng chung cho một video, playlist và nhiều link.
     Trang chỉ có âm thanh (SoundCloud) thì chỉ hiện "Chỉ âm thanh" và không đổi lựa chọn đã lưu của người dùng. -->
{#snippet formatPicker(onlyAudio: boolean)}
  {@const current = onlyAudio ? "audio" : mode}
  <div class="modes" class:single={onlyAudio} role="radiogroup">
    {#each MODES.filter((m) => !onlyAudio || m === "audio") as m (m)}
      <button
        role="radio"
        aria-checked={current === m}
        class="mode"
        class:selected={current === m}
        onclick={() => !onlyAudio && (mode = m)}
      >
        <b>{modeTitle(m)}</b>
        <small>{onlyAudio ? t("picker.audioOnlySite") : t(`mode.${m}.hint`)}</small>
      </button>
    {/each}
  </div>

  {#if current === "audio"}
    <div class="qualities" role="radiogroup" aria-label={t("picker.audioFormat")}>
      <span class="label">{t("picker.format")}</span>
      {#each AUDIO_FORMATS as f (f)}
        <button role="radio" aria-checked={audioFormat === f} class:selected={audioFormat === f} onclick={() => (audioFormat = f)}>
          {audioLabel(f)}
        </button>
      {/each}
    </div>
    <p class="note format-hint">{audioHint}</p>
  {/if}
{/snippet}

<main>
  <header>
    <div class="brand">
      <Logo size={46} />
      <div>
        <h1>VidSaver</h1>
        <p>YouTube · TikTok · Facebook · SoundCloud</p>
      </div>
    </div>
    <div class="header-actions">
      <button
        class="ghost icon"
        onclick={toggleTheme}
        title={themeMode === "dark" ? t("theme.toLight") : t("theme.toDark")}
        aria-label={t("theme.toggle")}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          {#if themeMode === "dark"}
            <path d="M20.5 14.5A8.5 8.5 0 0 1 9.5 3.5a8.5 8.5 0 1 0 11 11Z" />
          {:else}
            <circle cx="12" cy="12" r="4" />
            <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
          {/if}
        </svg>
      </button>
      <button class="ghost" class:active={showSettings} onclick={() => (showSettings = !showSettings)}>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path
            d="M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Zm7.4-2.1.1-1.4-.1-1.4 2-1.6-2-3.4-2.4 1a7.5 7.5 0 0 0-2.4-1.4L14.2 2h-4.4l-.4 2.6A7.5 7.5 0 0 0 7 6L4.6 5 2.6 8.4l2 1.6-.1 1.4.1 1.4-2 1.6 2 3.4 2.4-1a7.5 7.5 0 0 0 2.4 1.4l.4 2.6h4.4l.4-2.6a7.5 7.5 0 0 0 2.4-1.4l2.4 1 2-3.4-2-1.6Z"
          />
        </svg>
        {t("header.settings")}
      </button>
    </div>
  </header>

  <!-- Bắt buộc cập nhật: có bản mới thì chắn cả app cho tới khi cập nhật. Đang có video tải dở thì chỉ hiện thanh báo,
       tải xong mới chắn. Không kiểm tra được (mất mạng) thì không chắn. -->
  {#if appUpdate}
    <div class:update-gate={!busy}>
      <section class="card app-update">
        <div class="text">
          <b>{busy ? t("update.available", { v: appUpdate.version }) : t("update.required", { v: appUpdate.version })}</b>
          <small>
            {#if appUpdating}
              {t("update.downloading")}{appUpdatePercent != null ? ` · ${appUpdatePercent.toFixed(0)}%` : "…"}
              {t("update.restart")}
            {:else if busy}
              {t("update.busy")}
            {:else}
              {t("update.current", { v: appUpdate.currentVersion })}
            {/if}
          </small>
          {#if appUpdating}
            <div class="bar" class:indeterminate={appUpdatePercent == null}>
              <div style:width="{appUpdatePercent ?? 0}%"></div>
            </div>
          {/if}
          {#if appUpdateMsg}<p class="error">{appUpdateMsg}</p>{/if}
        </div>
        {#if !appUpdating}
          <button class="primary" onclick={installAppUpdate} disabled={busy}>{t("update.now")}</button>
        {/if}
      </section>
    </div>
  {/if}

  {#if askName}
    <form
      class="card ask-name"
      onsubmit={(e) => {
        e.preventDefault();
        saveName();
      }}
    >
      <div class="text">
        <b>{t("name.ask")}</b>
        <small>{t("name.hint")}</small>
      </div>
      <input bind:value={nameDraft} maxlength="80" autocomplete="off" placeholder={t("name.placeholder")} />
      <button type="submit" class="primary" disabled={!nameDraft.trim()}>{t("common.save")}</button>
      <button
        type="button"
        class="ghost"
        onclick={() => {
          skipUserName();
          askName = false;
        }}>{t("common.skip")}</button
      >
    </form>
  {/if}

  {#if showSettings}
    <section class="card settings">
      <div class="row">
        <div class="label">{t("settings.language")}</div>
        <select bind:value={lang} onchange={() => setLang(lang)}>
          {#each LANGUAGES as [value, label] (value)}
            <option {value}>{label}</option>
          {/each}
        </select>
      </div>

      <div class="row">
        <div class="label">{t("settings.theme")}</div>
        <select bind:value={themeMode} onchange={() => setTheme(themeMode)}>
          {#each THEME_MODES as value (value)}
            <option {value}>{t(`theme.${value}`)}</option>
          {/each}
        </select>
      </div>

      <div class="row">
        <div class="label">{t("settings.name")}</div>
        <input class="value" bind:value={nameDraft} maxlength="80" autocomplete="off" placeholder={t("settings.optional")} />
        <button onclick={saveName} disabled={nameDraft.trim() === userName}>{t("common.save")}</button>
      </div>

      <div class="row">
        <div class="label">{t("settings.folder")}</div>
        <div class="value path" title={outDir}>{outDir || t("settings.notChosen")}</div>
        <button onclick={chooseDir}>{t("settings.change")}</button>
      </div>

      <div class="row">
        <div class="label">{t("settings.cookies")}</div>
        <select bind:value={cookieKind}>
          {#each COOKIE_SOURCES as [value, label] (value)}
            <option {value}>{cookieLabel(value, label)}</option>
          {/each}
        </select>
        {#if cookieKind === "file"}
          <div class="value path" title={cookieFile}>{cookieFile || t("settings.noFile")}</div>
          <button onclick={chooseCookieFile}>{t("settings.choose")}</button>
        {:else if cookieKind === "app"}
          <div class="value"></div>
          <button onclick={logoutSites}>{t("settings.logout")}</button>
        {/if}
      </div>
      {#if logoutMsg}<p class="note">{logoutMsg}</p>{/if}
      <p class="note">{cookieKind === "app" ? t("settings.cookiesAppNote") : t("settings.cookiesNote")}</p>

      <div class="row">
        <div class="label">yt-dlp</div>
        <div class="value">{deps?.ytdlpVersion ?? "—"}</div>
        <button onclick={updateNow} disabled={updating || installing}>{updating ? t("settings.updating") : t("settings.update")}</button>
        <button onclick={() => install(true)} disabled={installing || updating}>{t("settings.reinstall")}</button>
      </div>
      {#if updateMsg}<p class="note">{updateMsg}</p>{/if}

      <div class="row">
        <div class="label">{t("settings.appVersion")}</div>
        <div class="value">{appVersion || "—"}</div>
        <button onclick={() => checkAppUpdate()} disabled={appUpdating}>{t("settings.checkUpdate")}</button>
      </div>
      {#if appUpdateMsg && !appUpdate}<p class="note">{appUpdateMsg}</p>{/if}
    </section>
  {/if}

  {#if deps && !deps.ready}
    <section class="card setup">
      <h2>{t("setup.title")}</h2>
      <p>{t("setup.text")}</p>
      {#if installing || Object.keys(installProgress).length}
        <ul class="deps">
          {#each ["yt-dlp", "ffmpeg", ...(isMac ? ["ffprobe"] : []), "deno"] as name (name)}
            <li>
              <span>{name}</span>
              <div class="bar"><div style:width="{depsPercent(name)}%"></div></div>
              <small>{depsLine(name)}</small>
            </li>
          {/each}
        </ul>
      {/if}
      {#if installError}<p class="error">{installError}</p>{/if}
      <button class="primary" onclick={() => install()} disabled={installing}>
        {installing ? t("setup.installing") : installError ? t("common.retry") : t("setup.install")}
      </button>
    </section>
  {:else if deps}
    <div class="tabs" role="tablist">
      <button role="tab" aria-selected={tab === "download"} class:active={tab === "download"} onclick={() => (tab = "download")}>
        {t("tab.download")}
      </button>
      <button role="tab" aria-selected={tab === "feedback"} class:active={tab === "feedback"} onclick={() => (tab = "feedback")}>
        {t("tab.feedback")}
        {#if feedbackUnseen}<span class="dot" title={t("tab.newReply")}></span>{/if}
      </button>
    </div>

    <!-- Luôn giữ tab góp ý để nó tự kiểm tra kết quả và báo chấm đỏ cả khi đang ở tab khác. -->
    <div class="tab-page" hidden={tab !== "feedback"}>
      <FeedbackTab active={tab === "feedback"} bind:unseen={feedbackUnseen} />
    </div>
  {/if}

  {#if deps?.ready && tab === "download"}
    <section class="card">
      <form
        class="input"
        onsubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <div class="url-field">
          <input
            bind:this={urlInput}
            bind:value={url}
            placeholder={t("input.placeholder")}
            spellcheck="false"
            autocomplete="off"
          />
          {#if url && !fetching && batchRead == null}
            <button type="button" class="clear" title={t("input.clear")} aria-label={t("input.clear")} onclick={clearUrl}>✕</button>
          {/if}
        </div>
        <button type="button" onclick={pasteNow} disabled={fetching || batchRead != null}>{t("input.paste")}</button>
        <button type="submit" class="primary" disabled={fetching || batchRead != null || !url.trim() || (isBatch && !outDir)}>
          {#if batchRead != null}
            {t("input.readingBatch", { n: batchRead, total: urlList.length })}
          {:else if fetching}
            {t("input.reading")}
          {:else if isBatch}
            {t("input.batch", { n: urlList.length })}
          {:else}
            {t("input.get")}
          {/if}
        </button>
      </form>

      {#if isBatch && batchRead == null && !fetching}
        <p class="note batch-note">{t("input.batchNote", { n: urlList.length })}</p>
        {@render formatPicker(false)}
      {/if}

      {#if fetching}
        <div class="preview skeleton">
          <div class="thumb"></div>
          <div class="meta"><div></div><div></div></div>
        </div>
      {/if}

      {#if fetchError}<p class="error">{fetchError}</p>{/if}
      {#if loginWaiting}
        <p class="note">{t("login.waiting")}</p>
      {:else if fetchError && !isBatch && extractUrl(url)}
        <div class="login-ask">
          <button onclick={startLogin}>{t("login.button")}</button>
          <span class="muted">{t("login.hint")}</span>
        </div>
      {/if}
      {#if notice}<p class="note">{notice}</p>{/if}

      {#if info}
        {@const site = siteOf(info.site)}
        <div class="preview">
          <div class="thumb">
            {#if info.thumbnail}<img src={info.thumbnail} alt="" referrerpolicy="no-referrer" onerror={thumbFallback} />{/if}
            {#if info.duration}<span class="duration">{fmtDuration(info.duration)}</span>{/if}
          </div>
          <div class="meta">
            <div class="tags">
              <span class="site {site}">{SITE_NAME[site]}</span>
              {#if info.maxHeight}<span class="tag">{t("preview.max", { q: fmtQuality(info.maxHeight) })}</span>{/if}
            </div>
            <label class="title-edit" title={t("preview.rename")}>
              <input bind:value={customTitle} placeholder={info.title} spellcheck="false" aria-label={t("preview.fileName")} />
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 20h4L19 9l-4-4L4 16v4Zm11-15 4 4" /></svg>
            </label>
            {#if customTitle.trim() !== info.title}
              <button class="link restore" onclick={() => (customTitle = info!.title)}>{t("preview.restore")}</button>
            {/if}
            {#if info.uploader}<p class="muted">{info.uploader}</p>{/if}
          </div>
        </div>

        {@render formatPicker(audioOnly)}

        {#if effectiveMode !== "audio" && qualityOptions.length > 1}
          <div class="qualities" role="radiogroup" aria-label={t("preview.quality")}>
            <span class="label">{t("preview.quality")}</span>
            {#each qualityOptions as h (h)}
              <button role="radio" aria-checked={chosenQuality === h} class:selected={chosenQuality === h} onclick={() => (quality = h)}>
                {fmtQuality(h)}
              </button>
            {/each}
          </div>
        {/if}
        {#if isMac && effectiveMode === "best" && (chosenQuality ?? 0) > 1080}
          <p class="note format-hint">{t("mac.codecNote")}</p>
        {/if}

        {#if info.duration}
          <div class="qualities" role="radiogroup" aria-label={t("cut.label")}>
            <span class="label">{t("cut.label")}</span>
            <button role="radio" aria-checked={!cutOn} class:selected={!cutOn} onclick={() => (cutOn = false)}>
              {t("cut.full")} · {fmtDuration(info.duration)}
            </button>
            <button role="radio" aria-checked={cutOn} class:selected={cutOn} onclick={() => (cutOn = true)}>
              {t("cut.part")}
            </button>
          </div>
          {#if cutOn}
            {@const total = Math.floor(info.duration)}
            <div class="cut">
              <div class="cut-range" style:--a="{(cutStart / total) * 100}%" style:--b="{(cutEnd / total) * 100}%">
                <input
                  type="range"
                  min="0"
                  max={total}
                  step="1"
                  bind:value={cutStart}
                  oninput={() => (cutStart = Math.min(cutStart, cutEnd - 1))}
                  aria-label={t("cut.from")}
                />
                <input
                  type="range"
                  min="0"
                  max={total}
                  step="1"
                  bind:value={cutEnd}
                  oninput={() => (cutEnd = Math.max(cutEnd, cutStart + 1))}
                  aria-label={t("cut.to")}
                />
              </div>
              <div class="cut-times">
                <label>
                  {t("cut.from")}
                  <input value={fmtDuration(cutStart) || "0:00"} onchange={(e) => setCut(e, "start", total)} spellcheck="false" />
                </label>
                <label>
                  {t("cut.to")}
                  <input value={fmtDuration(cutEnd)} onchange={(e) => setCut(e, "end", total)} spellcheck="false" />
                </label>
                <span class="muted">{t("cut.length", { t: fmtDuration(cutEnd - cutStart) })}</span>
              </div>
              <p class="note format-hint">{t("cut.note")}</p>
            </div>
          {/if}
        {/if}

        {#if info.subtitles.length && !cutOn}
          <div class="qualities" role="radiogroup" aria-label={t("preview.subtitles")}>
            <span class="label">{t("preview.subtitles")}</span>
            <button role="radio" aria-checked={subtitleIndex < 0} class:selected={subtitleIndex < 0} onclick={() => (subtitleIndex = -1)}>
              {t("common.none")}
            </button>
            {#each info.subtitles as s, i (s.lang + s.auto)}
              <button role="radio" aria-checked={subtitleIndex === i} class:selected={subtitleIndex === i} onclick={() => (subtitleIndex = i)}>
                {subtitleName(s)}
              </button>
            {/each}
          </div>
          {#if subtitleIndex >= 0}
            <div class="qualities" role="radiogroup" aria-label={t("sub.style")}>
              <span class="label">{t("sub.style")}</span>
              <button role="radio" aria-checked={!subtitleText} class:selected={!subtitleText} onclick={() => (subtitleText = false)}>
                {t("sub.timed")}
              </button>
              <button role="radio" aria-checked={subtitleText} class:selected={subtitleText} onclick={() => (subtitleText = true)}>
                {t("sub.text")}
              </button>
            </div>
            <p class="note format-hint">
              {#if subtitleText}
                {effectiveMode === "audio" ? t("preview.subtitleTextAudio") : t("preview.subtitleTextVideo")}
              {:else}
                {effectiveMode === "audio" ? t("preview.subtitleAudio") : t("preview.subtitleVideo")}
              {/if}
            </p>
          {/if}
        {/if}

        <button class="primary big" onclick={enqueue} disabled={!outDir}>{t("preview.download")}</button>
      {/if}

      {#if playlist}
        {@const site = siteOf(playlist.site)}
        <div class="playlist">
          <div class="playlist-head">
            <div class="tags">
              <span class="site {site}">{SITE_NAME[site]}</span>
              <span class="tag">
                {playlist.count && playlist.count > playlist.entries.length
                  ? t("playlist.itemsOf", { n: playlist.entries.length, count: playlist.count })
                  : t("playlist.items", { n: playlist.entries.length })}
              </span>
            </div>
            <h3>{playlist.title}</h3>
            {#if playlist.uploader}<p class="muted">{playlist.uploader}</p>{/if}
          </div>

          <div class="playlist-tools">
            <span class="muted">{t("playlist.picked", { n: pickedCount, total: playlist.entries.length })}</span>
            <button class="link" onclick={() => (picked = picked.map(() => pickedCount < picked.length))}>
              {pickedCount < picked.length ? t("playlist.pickAll") : t("playlist.unpickAll")}
            </button>
          </div>

          <ul class="playlist-list">
            {#each playlist.entries as e, i (i)}
              <li>
                <label>
                  <input type="checkbox" bind:checked={picked[i]} />
                  <span class="thumb tiny">
                    {#if e.thumbnail}
                      <img
                        src={e.thumbnail}
                        alt=""
                        loading="lazy"
                        referrerpolicy="no-referrer"
                        onerror={thumbFallback}
                      />
                    {/if}
                  </span>
                  <span class="entry-title" title={e.title}>{e.title}</span>
                  {#if e.duration}<small class="muted">{fmtDuration(e.duration)}</small>{/if}
                </label>
              </li>
            {/each}
          </ul>

          {@render formatPicker(audioOnly)}
          <button class="primary big" onclick={enqueuePlaylist} disabled={!outDir || !pickedCount}>
            {t(audioOnly ? "playlist.downloadTracks" : "playlist.downloadVideos", { n: pickedCount })}
          </button>
        </div>
      {/if}
    </section>

    {#if items.length}
      <section class="queue">
        <div class="queue-head">
          <h2>{t("history.title")}</h2>
          {#if hasFinished}<button class="link" onclick={clearFinished}>{t("history.clear")}</button>{/if}
        </div>

        {#each groups as g (g.day)}
          <h3 class="day">{g.day}</h3>
          {#each g.items as it (it.id)}
            <article class="card item {it.status}" class:missing={it.missing}>
              <div class="thumb small">
                {#if it.info.thumbnail}
                  <!-- Link ảnh bìa (nhất là TikTok) hết hạn sau vài ngày: ẩn ảnh hỏng, để lại nền trống. -->
                  <img
                    src={it.info.thumbnail}
                    alt=""
                    referrerpolicy="no-referrer"
                    onerror={thumbFallback}
                  />
                {/if}
              </div>
              <div class="body">
                <div class="title" title={it.info.title}>{it.info.title}</div>
                <div class="sub">
                  <span class="site {siteOf(it.info.site)}">{SITE_NAME[siteOf(it.info.site)]}</span>
                  <span>{itemFormat(it)}</span>
                  <span title={new Date(it.doneAt ?? it.addedAt).toLocaleString(localeTag())}>{fmtTime(it.doneAt ?? it.addedAt)}</span>
                </div>
                {#if it.status === "downloading" || it.status === "processing" || it.status === "queued"}
                  <div class="bar" class:indeterminate={it.percent == null || it.status === "processing"}>
                    <div style:width="{it.percent ?? 0}%"></div>
                  </div>
                {/if}
                <div class="status">{it.missing ? t("item.missing") : statusText(it)}</div>
                {#if it.note && it.status === "done"}<div class="item-note">{it.note}</div>{/if}
              </div>
              <div class="actions">
                {#if it.status === "done" && it.file && !it.missing}
                  <button onclick={() => openFile(it)}>{t("item.open")}</button>
                  <button onclick={() => openFile(it, true)}>{t("item.folder")}</button>
                {:else if it.missing}
                  <button onclick={() => retry(it)}>{t("item.redownload")}</button>
                {:else if it.status === "error" || it.status === "canceled"}
                  <button onclick={() => retry(it)}>{t("common.retry")}</button>
                {/if}
                {#if it.status === "downloading" || it.status === "processing" || it.status === "queued"}
                  <button onclick={() => cancel(it)}>{t("item.cancel")}</button>
                {:else}
                  <button class="ghost icon" title={t("item.remove")} aria-label={t("item.removeShort")} onclick={() => remove(it)}>✕</button>
                {/if}
              </div>
            </article>
          {/each}
        {/each}
      </section>
    {:else if !info && !fetching}
      <p class="empty">{t("history.empty")}</p>
    {/if}
  {/if}

  <footer>
    <Logo size={24} />
    <span>© {new Date().getFullYear()} Trần Thanh Duy</span>
  </footer>
</main>
