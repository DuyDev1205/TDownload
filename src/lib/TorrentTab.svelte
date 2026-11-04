<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { readText } from "@tauri-apps/plugin-clipboard-manager";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { openPath } from "@tauri-apps/plugin-opener";
  import * as api from "./api";
  import type { TorrentAdded, TorrentStatus } from "./api";
  import { fmtBytes, fmtDuration } from "./format";
  import { t } from "./i18n.svelte";

  /**
   * `active`: tab đang hiển thị. `outDir`: thư mục lưu chung của app.
   * `incoming`: link magnet App chuyển sang khi người dùng dán vào ô tải video.
   */
  let { active, outDir, incoming = $bindable("") }: { active: boolean; outDir: string; incoming?: string } = $props();

  let source = $state("");
  let adding = $state(false);
  let addError = $state("");
  let notice = $state("");
  /** Torrent vừa mở, đang chờ người dùng chọn file. */
  let pending = $state<TorrentAdded | null>(null);
  let picked = $state<boolean[]>([]);
  let list = $state<TorrentStatus[]>([]);
  let actionError = $state("");
  let confirmRemove = $state<number | null>(null);

  const pickedCount = $derived(picked.filter(Boolean).length);
  const pickedSize = $derived(pending ? pending.files.reduce((sum, f, i) => sum + (picked[i] ? f.size : 0), 0) : 0);

  $effect(() => {
    if (!incoming) return;
    const link = incoming;
    untrack(() => {
      incoming = "";
      source = link;
      add();
    });
  });

  onMount(() => {
    refresh();
    // Đang xem tab hoặc còn torrent đang tải thì cập nhật mỗi giây.
    const timer = setInterval(() => {
      if (active || list.some((x) => x.state === "downloading" || x.state === "checking")) refresh();
    }, 1000);
    return () => clearInterval(timer);
  });

  async function refresh() {
    try {
      list = await api.torrentList();
    } catch {
      // Lỗi tạm thời: lần sau cập nhật lại.
    }
  }

  async function add(src = source) {
    if (!src.trim() || adding || !outDir) return;
    adding = true;
    addError = "";
    notice = "";
    pending = null;
    try {
      const added = await api.torrentAdd(src.trim(), outDir);
      source = "";
      if (added.already) {
        notice = t("torrent.already");
        refresh();
      } else {
        pending = added;
        picked = added.files.map(() => true);
      }
    } catch (e) {
      addError = String(e);
    } finally {
      adding = false;
    }
  }

  async function chooseFile() {
    const file = await openDialog({ filters: [{ name: "Torrent", extensions: ["torrent"] }] });
    if (typeof file === "string") add(file);
  }

  async function pasteNow() {
    try {
      const text = ((await readText()) ?? "").trim();
      if (text) source = text;
    } catch {
      // Clipboard trống hoặc không phải chữ.
    }
  }

  async function start() {
    if (!pending || !pickedCount) return;
    const files = pending.files.filter((_, i) => picked[i]).map((f) => f.index);
    try {
      await api.torrentStart(pending.id, files);
      pending = null;
      refresh();
    } catch (e) {
      addError = String(e);
    }
  }

  async function cancelPending() {
    if (!pending) return;
    const id = pending.id;
    pending = null;
    await api.torrentRemove(id, false).catch(() => {});
  }

  /** Chạy một thao tác trên torrent rồi cập nhật danh sách; lỗi thì hiện dưới danh sách. */
  async function act(run: () => Promise<void>) {
    actionError = "";
    try {
      await run();
    } catch (e) {
      actionError = String(e);
    }
    refresh();
  }

  function remove(id: number, deleteFiles: boolean) {
    confirmRemove = null;
    act(() => api.torrentRemove(id, deleteFiles));
  }

  const percent = (x: TorrentStatus) => (x.total ? (x.progress / x.total) * 100 : 0);

  function statusText(x: TorrentStatus): string {
    const pct = `${percent(x).toFixed(1)}%`;
    switch (x.state) {
      case "checking":
        return t("torrent.checking");
      case "paused":
        return t("torrent.paused", { pct });
      case "done":
        return t("torrent.done", { size: fmtBytes(x.total) });
      case "error":
        return x.error ?? t("status.unknownError");
      case "downloading": {
        const bits = [pct, `${fmtBytes(x.progress)} / ${fmtBytes(x.total)}`];
        if (x.speed) {
          bits.push(`${fmtBytes(x.speed)}/s`);
          bits.push(t("status.eta", { t: fmtDuration((x.total - x.progress) / x.speed) }));
        }
        return bits.join(" · ");
      }
    }
  }
</script>

<section class="card">
  <form
    class="input"
    onsubmit={(e) => {
      e.preventDefault();
      add();
    }}
  >
    <div class="url-field">
      <input bind:value={source} placeholder={t("torrent.placeholder")} spellcheck="false" autocomplete="off" disabled={adding} />
    </div>
    <button type="button" onclick={pasteNow} disabled={adding}>{t("input.paste")}</button>
    <button type="button" onclick={chooseFile} disabled={adding || !outDir}>{t("torrent.chooseFile")}</button>
    <button type="submit" class="primary" disabled={adding || !source.trim() || !outDir}>
      {adding ? t("torrent.opening") : t("torrent.open")}
    </button>
  </form>
  <p class="note">{adding ? t("torrent.waitHint") : t("torrent.note")}</p>

  {#if addError}<p class="error">{addError}</p>{/if}
  {#if notice}<p class="note">{notice}</p>{/if}

  {#if pending}
    <div class="playlist">
      <div class="playlist-head">
        <h3>{pending.name}</h3>
        <p class="muted path" title={pending.folder}>{pending.folder}</p>
      </div>
      <div class="playlist-tools">
        <span class="muted">{t("playlist.picked", { n: pickedCount, total: pending.files.length })}</span>
        <button class="link" onclick={() => (picked = picked.map(() => pickedCount < picked.length))}>
          {pickedCount < picked.length ? t("playlist.pickAll") : t("playlist.unpickAll")}
        </button>
      </div>
      <ul class="playlist-list">
        {#each pending.files as f, i (f.index)}
          <li>
            <label>
              <input type="checkbox" bind:checked={picked[i]} />
              <span class="entry-title" title={f.name}>{f.name}</span>
              <small class="muted">{fmtBytes(f.size)}</small>
            </label>
          </li>
        {/each}
      </ul>
      <div class="torrent-actions">
        <button class="primary big" onclick={start} disabled={!pickedCount}>
          {t("torrent.download", { n: pickedCount, size: fmtBytes(pickedSize) })}
        </button>
        <button onclick={cancelPending}>{t("item.cancel")}</button>
      </div>
    </div>
  {/if}
</section>

<section class="queue">
  <div class="queue-head"><h2>{t("torrent.list")}</h2></div>
  {#if !list.length}<p class="empty">{t("torrent.empty")}</p>{/if}
  {#each list as x (x.id)}
    <article class="card item torrent {x.state}">
      <div class="body">
        <div class="title" title={x.name}>{x.name}</div>
        {#if x.state !== "done"}
          <div class="bar" class:indeterminate={x.state === "checking"}>
            <div style:width="{percent(x)}%"></div>
          </div>
        {/if}
        <div class="status">{statusText(x)}</div>
        {#if confirmRemove === x.id}
          <div class="torrent-remove">
            <span>{t("torrent.removeAsk")}</span>
            <button onclick={() => remove(x.id, false)}>{t("torrent.removeKeep")}</button>
            <button class="danger" onclick={() => remove(x.id, true)}>{t("torrent.removeFiles")}</button>
            <button class="ghost" onclick={() => (confirmRemove = null)}>{t("item.cancel")}</button>
          </div>
        {/if}
      </div>
      <div class="actions">
        {#if x.state === "downloading" || x.state === "checking"}
          <button onclick={() => act(() => api.torrentPause(x.id))}>{t("torrent.pause")}</button>
        {:else if x.state === "paused" || x.state === "error"}
          <button onclick={() => act(() => api.torrentResume(x.id))}>{t("torrent.resume")}</button>
        {/if}
        <button onclick={() => openPath(x.folder)}>{t("item.folder")}</button>
        <button class="ghost icon" title={t("item.remove")} aria-label={t("item.removeShort")} onclick={() => (confirmRemove = x.id)}>✕</button>
      </div>
    </article>
  {/each}
  {#if actionError}<p class="error">{actionError}</p>{/if}
</section>
