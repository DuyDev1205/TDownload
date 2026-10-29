<script lang="ts">
  import { onMount } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
  import * as api from "./api";
  import type { DepsProgress, SubtitleStream } from "./api";
  import { fmtBytes, languageName } from "./format";
  import { load, save } from "./storage";

  const LANGUAGES: [string, string][] = [
    ["auto", "Tự nhận"],
    ["vi", "Tiếng Việt"],
    ["en", "Tiếng Anh"],
  ];
  const MEDIA = ["mp4", "mkv", "mov", "webm", "avi", "m4v", "ts", "flv", "wmv", "mp3", "m4a", "wav", "flac", "opus", "ogg", "aac"];

  interface Output {
    label: string;
    file: string;
  }

  let file = $state("");
  const fileName = $derived(file.split(/[\\/]/).pop() ?? "");
  let streams = $state<SubtitleStream[] | null>(null);
  let probeError = $state("");
  let extracting = $state<number | null>(null);
  let outputs = $state<Output[]>([]);
  let actionError = $state("");

  let ready = $state<boolean | null>(null);
  let installing = $state(false);
  let installError = $state("");
  let installProgress = $state<Record<string, DepsProgress>>({});

  let language = $state(load("trLanguage", "auto"));
  let job = $state<{ id: string; stage: "convert" | "transcribe"; percent: number | null } | null>(null);

  $effect(() => save("trLanguage", language));

  onMount(() => {
    const unlisteners = [
      api.onDepsProgress((p) => {
        if (p.name === "whisper" || p.name === "model") installProgress[p.name] = p;
      }),
      api.onTrProgress((p) => {
        if (job?.id === p.id) Object.assign(job, { stage: p.stage, percent: p.percent });
      }),
      api.onTrDone((d) => {
        if (job?.id !== d.id) return;
        job = null;
        if (d.ok && d.file) {
          const lang = languageName(d.language);
          outputs.unshift({ label: `Phụ đề AI${lang ? ` · ${lang}` : ""}`, file: d.file });
        } else if (!d.canceled) actionError = d.error ?? "Nhận dạng lỗi.";
      }),
    ];
    api.whisperReady().then((r) => (ready = r)).catch(() => (ready = false));
    return () => unlisteners.forEach((p) => p.then((off) => off()));
  });

  async function chooseFile() {
    const picked = await openDialog({ filters: [{ name: "Video / âm thanh", extensions: MEDIA }] });
    if (typeof picked !== "string") return;
    file = picked;
    streams = null;
    probeError = "";
    actionError = "";
    try {
      streams = await api.probeSubtitles(picked);
    } catch (e) {
      probeError = String(e);
    }
  }

  async function extract(s: SubtitleStream) {
    extracting = s.index;
    actionError = "";
    try {
      const out = await api.extractSubtitle(file, s.index, s.language);
      outputs.unshift({ label: `Phụ đề có sẵn · ${streamName(s)}`, file: out });
    } catch (e) {
      actionError = String(e);
    } finally {
      extracting = null;
    }
  }

  async function install() {
    installing = true;
    installError = "";
    installProgress = {};
    try {
      await api.installWhisper();
      ready = true;
    } catch (e) {
      installError = String(e);
    } finally {
      installing = false;
    }
  }

  async function start() {
    if (!file || job) return;
    actionError = "";
    const id = crypto.randomUUID();
    job = { id, stage: "convert", percent: null };
    try {
      await api.transcribe(id, file, language);
    } catch (e) {
      job = null;
      actionError = String(e);
    }
  }

  function cancel() {
    if (job) api.cancelTranscribe(job.id);
  }

  function streamName(s: SubtitleStream): string {
    return [languageName(s.language) ?? s.language ?? `Luồng ${s.index}`, s.title].filter(Boolean).join(" · ");
  }

  function installLine(name: string): string {
    const p = installProgress[name];
    if (!p) return installing ? "Đang chờ…" : "";
    if (p.stage === "done") return "Xong";
    if (p.stage === "extract") return "Đang cài…";
    return p.total ? `${fmtBytes(p.downloaded)} / ${fmtBytes(p.total)}` : fmtBytes(p.downloaded);
  }

  function installPercent(name: string): number {
    const p = installProgress[name];
    if (!p) return 0;
    if (p.stage !== "download") return 100;
    return p.total ? (p.downloaded / p.total) * 100 : 0;
  }
</script>

<section class="card subtitle-tool">
  <div class="pick">
    <button class="primary" onclick={chooseFile} disabled={!!job}>Chọn video / âm thanh…</button>
    {#if file}<span class="file" title={file}>{fileName}</span>{:else}<span class="muted">Chưa chọn file</span>{/if}
  </div>

  {#if file}
    <h3>Phụ đề có sẵn trong file</h3>
    {#if probeError}
      <p class="error">{probeError}</p>
    {:else if streams == null}
      <p class="muted">Đang đọc file…</p>
    {:else if streams.length === 0}
      <p class="muted">File này không có phụ đề đi kèm. Dùng nhận dạng giọng nói bên dưới để tạo phụ đề.</p>
    {:else}
      <ul class="streams">
        {#each streams as s (s.index)}
          <li>
            <span>{streamName(s)}</span>
            <small class="muted">{s.codec}</small>
            {#if s.text}
              <button onclick={() => extract(s)} disabled={extracting != null}>
                {extracting === s.index ? "Đang tách…" : "Tách ra .srt"}
              </button>
            {:else}
              <small class="muted">Dạng hình ảnh, không chuyển thành chữ được</small>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    <h3>Nhận dạng giọng nói (AI)</h3>
    <p class="note">
      Whisper large-v3-turbo chạy ngay trên máy, không gửi file đi đâu. Phụ đề có dấu câu và mốc thời gian, lưu thành
      file .srt cạnh video. Thời gian xử lý khoảng một nửa độ dài video (nhanh hơn trên Mac chip M).
    </p>

    {#if ready === false}
      <div class="install">
        <p>Lần đầu cần tải bộ nhận dạng (khoảng 600 MB, chỉ một lần).</p>
        {#if installing || Object.keys(installProgress).length}
          <ul class="deps">
            {#each [["whisper", "Bộ nhận dạng"], ["model", "Mô hình Whisper"]] as [name, label] (name)}
              <li>
                <span>{label}</span>
                <div class="bar"><div style:width="{installPercent(name)}%"></div></div>
                <small>{installLine(name)}</small>
              </li>
            {/each}
          </ul>
        {/if}
        {#if installError}<p class="error">{installError}</p>{/if}
        <button class="primary" onclick={install} disabled={installing}>
          {installing ? "Đang tải…" : installError ? "Thử lại" : "Tải bộ nhận dạng"}
        </button>
      </div>
    {:else if ready}
      <div class="qualities" role="radiogroup" aria-label="Ngôn ngữ trong video">
        <span class="label">Ngôn ngữ</span>
        {#each LANGUAGES as [value, label] (value)}
          <button role="radio" aria-checked={language === value} class:selected={language === value} onclick={() => (language = value)} disabled={!!job}>
            {label}
          </button>
        {/each}
      </div>

      {#if job}
        <div class="job">
          <div class="bar" class:indeterminate={job.percent == null}>
            <div style:width="{job.percent ?? 0}%"></div>
          </div>
          <div class="job-line">
            <span class="muted">
              {job.stage === "convert" ? "Đang tách âm thanh…" : `Đang nhận dạng${job.percent != null ? ` · ${job.percent.toFixed(0)}%` : "…"}`}
            </span>
            <button onclick={cancel}>Huỷ</button>
          </div>
        </div>
      {:else}
        <button class="primary big" onclick={start}>Tạo phụ đề bằng AI</button>
      {/if}
    {/if}
  {/if}

  {#if actionError}<p class="error">{actionError}</p>{/if}

  {#if outputs.length}
    <h3>Đã tạo</h3>
    <ul class="outputs">
      {#each outputs as o (o.file)}
        <li>
          <div class="out-text">
            <span class="out-name" title={o.file}>{o.file.split(/[\\/]/).pop()}</span>
            <small class="muted">{o.label}</small>
          </div>
          <button onclick={() => openPath(o.file)}>Mở</button>
          <button onclick={() => revealItemInDir(o.file)}>Thư mục</button>
        </li>
      {/each}
    </ul>
  {/if}
</section>
