<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
  import * as api from "./api";
  import type { VideoProbe } from "./api";
  import { fmtBytes, fmtDuration } from "./format";
  import { t } from "./i18n.svelte";

  /** `incoming`: đường dẫn video App chuyển sang khi bấm "Nén" ở một mục trong lịch sử tải. */
  let { incoming = $bindable("") }: { incoming?: string } = $props();

  const MB = 1024 * 1024;
  const VIDEO_EXTENSIONS = ["mp4", "mov", "m4v", "mkv", "webm", "avi", "wmv", "flv", "3gp", "ts"];

  interface Result {
    input: string;
    file: string;
    size: number;
    original: number;
    height: number | null;
  }

  let file = $state("");
  let probe = $state<VideoProbe | null>(null);
  let loading = $state(false);
  let error = $state("");
  let targetMb = $state(0);
  let job = $state<{ id: string; percent: number } | null>(null);
  let results = $state<Result[]>([]);

  const fileName = $derived(file.split(/[\\/]/).pop() ?? "");
  const ratio = $derived(probe && targetMb > 0 ? (targetMb * MB) / probe.size : 1);
  const validTarget = $derived(!!probe && targetMb > 0 && targetMb * MB < probe.size);
  /** Mức dung lượng gợi ý: giảm 25%, giảm 50%, và các giới hạn đính kèm hay gặp (Gmail 25 MB, Discord 10 MB). */
  const presets = $derived.by(() => {
    if (!probe) return [];
    const current = probe.size / MB;
    const list: [string, number][] = [
      ["−25%", current * 0.75],
      ["−50%", current * 0.5],
      ["25 MB", 25],
      ["10 MB", 10],
    ];
    return list.filter(([, mb]) => mb < current).map(([label, mb]) => [label, Math.floor(mb * 10) / 10] as const);
  });

  $effect(() => {
    if (!incoming) return;
    const path = incoming;
    untrack(() => {
      incoming = "";
      load(path);
    });
  });

  onMount(() => {
    const unlisteners = [
      api.onCompressProgress((p) => {
        if (job && p.id === job.id) job.percent = p.percent;
      }),
      api.onCompressDone((d) => {
        if (!job || d.id !== job.id) return;
        job = null;
        if (d.ok && d.file) {
          results.unshift({ input: file, file: d.file, size: d.size ?? 0, original: probe?.size ?? 0, height: d.height });
        } else if (!d.canceled) {
          error = d.error ?? t("status.unknownError");
        }
      }),
    ];
    return () => unlisteners.forEach((p) => p.then((off) => off()));
  });

  async function choose() {
    const picked = await openDialog({ filters: [{ name: "Video", extensions: VIDEO_EXTENSIONS }] });
    if (typeof picked === "string") load(picked);
  }

  async function load(path: string) {
    if (job) return;
    file = path;
    probe = null;
    error = "";
    loading = true;
    try {
      probe = await api.compressProbe(path);
      // Gợi ý mặc định: giảm khoảng 30%, mức gần như không thấy khác.
      targetMb = Math.floor((probe.size / MB) * 0.7 * 10) / 10;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function start() {
    if (!probe || !validTarget || job) return;
    error = "";
    const id = crypto.randomUUID();
    job = { id, percent: 0 };
    try {
      await api.compressVideo(id, file, targetMb);
    } catch (e) {
      job = null;
      error = String(e);
    }
  }

  function cancel() {
    if (job) api.compressCancel(job.id);
  }

  const qualityHint = $derived(
    ratio >= 0.7 ? t("compress.qualityHigh") : ratio >= 0.45 ? t("compress.qualityMid") : t("compress.qualityLow"),
  );
</script>

<section class="card compress">
  <div class="compress-file">
    <button onclick={choose} disabled={!!job || loading}>{file ? t("compress.change") : t("compress.choose")}</button>
    {#if file}<span class="path" title={file}>{fileName}</span>{/if}
  </div>
  {#if !file}<p class="note">{t("compress.hint")}</p>{/if}
  {#if loading}<p class="muted">{t("input.reading")}</p>{/if}

  {#if probe}
    <p class="muted">
      {fmtBytes(probe.size)} · {fmtDuration(probe.duration)}{probe.height ? ` · ${probe.width}×${probe.height}` : ""}
    </p>

    <div class="qualities" role="radiogroup" aria-label={t("compress.target")}>
      <span class="label">{t("compress.target")}</span>
      <label class="compress-target">
        <input type="number" min="1" step="0.5" bind:value={targetMb} disabled={!!job} />
        MB
      </label>
      {#each presets as [label, mb] (label)}
        <button role="radio" aria-checked={targetMb === mb} class:selected={targetMb === mb} onclick={() => (targetMb = mb)} disabled={!!job}>
          {label}
        </button>
      {/each}
    </div>
    {#if validTarget}
      <p class="note format-hint">{qualityHint} {t("compress.audioNote")}</p>
    {:else if targetMb > 0}
      <p class="error">{t("compress.notSmaller")}</p>
    {/if}

    {#if job}
      <div class="bar" class:indeterminate={job.percent === 0}><div style:width="{job.percent}%"></div></div>
      <div class="compress-actions">
        <span class="muted">{t("compress.working", { pct: `${job.percent.toFixed(0)}%` })}</span>
        <button onclick={cancel}>{t("item.cancel")}</button>
      </div>
    {:else}
      <button class="primary big" onclick={start} disabled={!validTarget}>
        {t("compress.start", { size: `${targetMb} MB` })}
      </button>
    {/if}
  {/if}

  {#if error}<p class="error">{error}</p>{/if}
</section>

{#if results.length}
  <section class="queue">
    <div class="queue-head"><h2>{t("compress.results")}</h2></div>
    {#each results as r (r.file)}
      <article class="card item done">
        <div class="body">
          <div class="title" title={r.file}>{r.file.split(/[\\/]/).pop()}</div>
          <div class="status">
            {t("compress.done", {
              size: fmtBytes(r.size),
              from: fmtBytes(r.original),
              pct: `${Math.round((1 - r.size / r.original) * 100)}%`,
            })}
          </div>
          {#if r.height}<div class="item-note">{t("compress.scaled", { h: r.height })}</div>{/if}
        </div>
        <div class="actions">
          <button onclick={() => openPath(r.file)}>{t("item.open")}</button>
          <button onclick={() => revealItemInDir(r.file)}>{t("item.folder")}</button>
        </div>
      </article>
    {/each}
  </section>
{/if}
