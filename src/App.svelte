<script lang="ts">
  import { onMount } from "svelte";
  import { downloadDir } from "@tauri-apps/api/path";
  import { readText } from "@tauri-apps/plugin-clipboard-manager";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
  import * as api from "./lib/api";
  import type { DepsProgress, DepsStatus, DlDone, DlProgress, Mode, VideoInfo } from "./lib/api";
  import { SITE_NAME, extractUrl, fmtBytes, fmtDuration, fmtQuality, isKnownSite, siteOf } from "./lib/format";
  import { load, save } from "./lib/storage";

  type Status = "queued" | "downloading" | "processing" | "done" | "error" | "canceled";

  interface Item {
    id: string;
    info: VideoInfo;
    mode: Mode;
    status: Status;
    percent: number | null;
    speed: number | null;
    eta: number | null;
    part: number;
    stage: string;
    file: string | null;
    error: string | null;
  }

  const MAX_PARALLEL = 2;
  const DAY = 24 * 60 * 60 * 1000;
  const isMac = navigator.userAgent.includes("Mac");

  const MODES: { value: Mode; title: string; hint: string }[] = [
    { value: "best", title: "Video · Cao nhất", hint: "Độ phân giải tối đa, đến 4K/8K nếu có" },
    { value: "compat", title: "Video · Tương thích", hint: "H.264 ≤ 1080p, mở được trên mọi máy" },
    { value: "mp3", title: "MP3 · 320 kbps", hint: "Chỉ lấy âm thanh, kèm ảnh bìa" },
  ];

  const COOKIE_SOURCES: [string, string][] = [
    ["", "Không dùng"],
    ["firefox", "Firefox"],
    ["chrome", "Chrome"],
    ["edge", "Edge"],
    ["brave", "Brave"],
    ...(isMac ? ([["safari", "Safari"]] as [string, string][]) : []),
    ["file", "File cookies.txt"],
  ];

  let deps = $state<DepsStatus | null>(null);
  let installing = $state(false);
  let installError = $state("");
  let installProgress = $state<Record<string, DepsProgress>>({});

  let url = $state("");
  let fetching = $state(false);
  let fetchError = $state("");
  let info = $state<VideoInfo | null>(null);

  let mode = $state<Mode>(load("mode", "best") as Mode);
  let outDir = $state(load("outDir", ""));
  let cookieKind = $state(load("cookieKind", ""));
  let cookieFile = $state(load("cookieFile", ""));
  const cookies = $derived(
    cookieKind === "file" ? (cookieFile ? `file:${cookieFile}` : "") : cookieKind ? `browser:${cookieKind}` : "",
  );

  let showSettings = $state(false);
  let updating = $state(false);
  let updateMsg = $state("");
  let items = $state<Item[]>([]);
  let lastAutoPaste = "";

  const hasFinished = $derived(items.some((i) => ["done", "error", "canceled"].includes(i.status)));

  $effect(() => {
    save("mode", mode);
    save("outDir", outDir);
    save("cookieKind", cookieKind);
    save("cookieFile", cookieFile);
  });

  onMount(() => {
    const unlisteners = [
      api.onDepsProgress((p) => {
        installProgress[p.name] = p;
      }),
      api.onProgress(onProgress),
      api.onDone(onDone),
    ];
    window.addEventListener("focus", autoPaste);

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
      updateMsg = `Lỗi: ${e}`;
    } finally {
      updating = false;
    }
  }

  async function clipboardUrl(): Promise<string | null> {
    try {
      return extractUrl((await readText()) ?? "");
    } catch {
      return null;
    }
  }

  /** Khi quay lại app, nếu clipboard có link YouTube/TikTok/Facebook mới thì tự dán. */
  async function autoPaste() {
    if (url.trim() || fetching || !deps?.ready) return;
    const found = await clipboardUrl();
    if (found && isKnownSite(found) && found !== lastAutoPaste) {
      lastAutoPaste = found;
      url = found;
      getInfo();
    }
  }

  async function pasteNow() {
    const found = await clipboardUrl();
    if (found) {
      url = found;
      getInfo();
    }
  }

  async function getInfo() {
    const target = extractUrl(url) ?? url.trim();
    if (!target || fetching) return;
    url = target;
    fetching = true;
    fetchError = "";
    info = null;
    try {
      info = await api.fetchInfo(target, cookies);
    } catch (e) {
      fetchError = String(e);
    } finally {
      fetching = false;
    }
  }

  function enqueue() {
    if (!info) return;
    items.unshift({
      id: crypto.randomUUID(),
      info: $state.snapshot(info),
      mode,
      status: "queued",
      percent: null,
      speed: null,
      eta: null,
      part: 0,
      stage: "",
      file: null,
      error: null,
    });
    info = null;
    url = "";
    pump();
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
    api.startDownload({ id: it.id, url: it.info.url, mode: it.mode, outDir, cookies }).catch((e) => {
      it.status = "error";
      it.error = String(e);
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
      if (d.ok) it.percent = 100;
    }
    pump();
  }

  function cancel(it: Item) {
    if (it.status === "queued") it.status = "canceled";
    else api.cancelDownload(it.id);
  }

  function retry(it: Item) {
    it.status = "queued";
    pump();
  }

  function remove(it: Item) {
    items = items.filter((i) => i.id !== it.id);
  }

  function clearFinished() {
    items = items.filter((i) => !["done", "error", "canceled"].includes(i.status));
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
    if (name.includes("merger")) return "Đang ghép hình + tiếng…";
    if (name.includes("extractaudio")) return "Đang chuyển sang MP3…";
    if (name.includes("thumbnail")) return "Đang gắn ảnh bìa…";
    if (name.includes("metadata")) return "Đang ghi thông tin…";
    if (name.includes("remux") || name.includes("convertor")) return "Đang chuyển sang MP4…";
    if (name.includes("movefiles")) return "Đang lưu file…";
    return "Đang xử lý…";
  }

  function statusText(it: Item): string {
    switch (it.status) {
      case "queued":
        return "Đang chờ…";
      case "downloading": {
        const bits = [it.part > 1 ? `Phần ${it.part}` : "Đang tải"];
        if (it.percent != null) bits.push(`${it.percent.toFixed(1)}%`);
        if (it.speed) bits.push(`${fmtBytes(it.speed)}/s`);
        if (it.eta != null) bits.push(`còn ${fmtDuration(it.eta)}`);
        return bits.join(" · ");
      }
      case "processing":
        return it.stage;
      case "done":
        return "Hoàn tất";
      case "canceled":
        return "Đã huỷ";
      case "error":
        return it.error ?? "Lỗi không xác định";
    }
  }

  function depsLine(name: string): string {
    const p = installProgress[name];
    if (!p) return installing ? "Đang chờ…" : "";
    if (p.stage === "done") return "Xong";
    if (p.stage === "extract") return "Đang giải nén…";
    return p.total ? `${fmtBytes(p.downloaded)} / ${fmtBytes(p.total)}` : fmtBytes(p.downloaded);
  }

  function depsPercent(name: string): number {
    const p = installProgress[name];
    if (!p) return 0;
    if (p.stage !== "download") return 100;
    return p.total ? (p.downloaded / p.total) * 100 : 0;
  }

  const modeTitle = (m: Mode) => MODES.find((x) => x.value === m)?.title ?? m;
</script>

<main>
  <header>
    <div class="brand">
      <span class="logo" aria-hidden="true">
        <svg viewBox="0 0 24 24"><path d="M12 4v10m-4.5-4.5L12 14l4.5-4.5M6 19h12" /></svg>
      </span>
      <div>
        <h1>VidSaver</h1>
        <p>YouTube · TikTok · Facebook</p>
      </div>
    </div>
    <button class="ghost" class:active={showSettings} onclick={() => (showSettings = !showSettings)}>
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Zm7.4-2.1.1-1.4-.1-1.4 2-1.6-2-3.4-2.4 1a7.5 7.5 0 0 0-2.4-1.4L14.2 2h-4.4l-.4 2.6A7.5 7.5 0 0 0 7 6L4.6 5 2.6 8.4l2 1.6-.1 1.4.1 1.4-2 1.6 2 3.4 2.4-1a7.5 7.5 0 0 0 2.4 1.4l.4 2.6h4.4l.4-2.6a7.5 7.5 0 0 0 2.4-1.4l2.4 1 2-3.4-2-1.6Z"
        />
      </svg>
      Cài đặt
    </button>
  </header>

  {#if showSettings}
    <section class="card settings">
      <div class="row">
        <div class="label">Thư mục lưu</div>
        <div class="value path" title={outDir}>{outDir || "Chưa chọn"}</div>
        <button onclick={chooseDir}>Đổi…</button>
      </div>

      <div class="row">
        <div class="label">Cookie đăng nhập</div>
        <select bind:value={cookieKind}>
          {#each COOKIE_SOURCES as [value, label] (value)}
            <option {value}>{label}</option>
          {/each}
        </select>
        {#if cookieKind === "file"}
          <div class="value path" title={cookieFile}>{cookieFile || "Chưa chọn file"}</div>
          <button onclick={chooseCookieFile}>Chọn…</button>
        {/if}
      </div>
      <p class="note">
        Chỉ cần khi video riêng tư, trong nhóm kín, giới hạn tuổi, hoặc trang đòi đăng nhập. Đăng nhập trên trình
        duyệt đó trước. Trên Windows, Chrome/Edge mã hoá cookie nên hay lỗi: dùng <b>Firefox</b> hoặc
        <b>file cookies.txt</b> sẽ ổn định hơn.
      </p>

      <div class="row">
        <div class="label">yt-dlp</div>
        <div class="value">{deps?.ytdlpVersion ?? "—"}</div>
        <button onclick={updateNow} disabled={updating || installing}>{updating ? "Đang cập nhật…" : "Cập nhật"}</button>
        <button onclick={() => install(true)} disabled={installing || updating}>Cài lại thành phần</button>
      </div>
      {#if updateMsg}<p class="note">{updateMsg}</p>{/if}
    </section>
  {/if}

  {#if deps && !deps.ready}
    <section class="card setup">
      <h2>Cài đặt lần đầu</h2>
      <p>
        App cần tải thêm các công cụ <b>yt-dlp</b>, <b>ffmpeg</b> và <b>deno</b> (khoảng 150–250 MB, chỉ một lần) để
        tải và ghép video chất lượng cao.
      </p>
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
        {installing ? "Đang cài đặt…" : installError ? "Thử lại" : "Cài đặt ngay"}
      </button>
    </section>
  {:else if deps}
    <section class="card">
      <form
        class="input"
        onsubmit={(e) => {
          e.preventDefault();
          getInfo();
        }}
      >
        <input
          bind:value={url}
          placeholder="Dán link video YouTube, TikTok hoặc Facebook…"
          spellcheck="false"
          autocomplete="off"
        />
        <button type="button" onclick={pasteNow} disabled={fetching}>Dán</button>
        <button type="submit" class="primary" disabled={fetching || !url.trim()}>
          {fetching ? "Đang đọc…" : "Lấy video"}
        </button>
      </form>

      {#if fetching}
        <div class="preview skeleton">
          <div class="thumb"></div>
          <div class="meta"><div></div><div></div></div>
        </div>
      {/if}

      {#if fetchError}<p class="error">{fetchError}</p>{/if}

      {#if info}
        {@const site = siteOf(info.site)}
        <div class="preview">
          <div class="thumb">
            {#if info.thumbnail}<img src={info.thumbnail} alt="" referrerpolicy="no-referrer" />{/if}
            {#if info.duration}<span class="duration">{fmtDuration(info.duration)}</span>{/if}
          </div>
          <div class="meta">
            <div class="tags">
              <span class="site {site}">{SITE_NAME[site]}</span>
              {#if info.maxHeight}<span class="tag">Tối đa {fmtQuality(info.maxHeight)}</span>{/if}
            </div>
            <h3 title={info.title}>{info.title}</h3>
            {#if info.uploader}<p class="muted">{info.uploader}</p>{/if}
          </div>
        </div>

        <div class="modes" role="radiogroup">
          {#each MODES as m (m.value)}
            <button
              role="radio"
              aria-checked={mode === m.value}
              class="mode"
              class:selected={mode === m.value}
              onclick={() => (mode = m.value)}
            >
              <b>{m.title}</b>
              <small>{m.hint}</small>
            </button>
          {/each}
        </div>

        <button class="primary big" onclick={enqueue} disabled={!outDir}>Tải về</button>
      {/if}
    </section>

    {#if items.length}
      <section class="queue">
        <div class="queue-head">
          <h2>Danh sách tải</h2>
          {#if hasFinished}<button class="link" onclick={clearFinished}>Xoá mục đã xong</button>{/if}
        </div>

        {#each items as it (it.id)}
          <article class="card item {it.status}">
            <div class="thumb small">
              {#if it.info.thumbnail}<img src={it.info.thumbnail} alt="" referrerpolicy="no-referrer" />{/if}
            </div>
            <div class="body">
              <div class="title" title={it.info.title}>{it.info.title}</div>
              <div class="sub">
                <span class="site {siteOf(it.info.site)}">{SITE_NAME[siteOf(it.info.site)]}</span>
                <span>{modeTitle(it.mode)}</span>
              </div>
              {#if it.status === "downloading" || it.status === "processing" || it.status === "queued"}
                <div class="bar" class:indeterminate={it.percent == null || it.status === "processing"}>
                  <div style:width="{it.percent ?? 0}%"></div>
                </div>
              {/if}
              <div class="status">{statusText(it)}</div>
            </div>
            <div class="actions">
              {#if it.status === "done" && it.file}
                <button onclick={() => openPath(it.file!)}>Mở</button>
                <button onclick={() => revealItemInDir(it.file!)}>Thư mục</button>
              {:else if it.status === "error" || it.status === "canceled"}
                <button onclick={() => retry(it)}>Thử lại</button>
              {/if}
              {#if it.status === "downloading" || it.status === "processing" || it.status === "queued"}
                <button onclick={() => cancel(it)}>Huỷ</button>
              {:else}
                <button class="ghost icon" title="Xoá khỏi danh sách" aria-label="Xoá" onclick={() => remove(it)}>✕</button>
              {/if}
            </div>
          </article>
        {/each}
      </section>
    {:else if !info && !fetching}
      <p class="empty">Copy link video rồi quay lại đây, app sẽ tự dán.</p>
    {/if}
  {/if}
</main>
