<script lang="ts">
  import { onMount, untrack } from "svelte";
  import * as fb from "./feedback";
  import type { AdminItem, FeedbackStatus, FeedbackType, ImageData, SentItem, UsageStats } from "./feedback";
  import { load, save } from "./storage";
  import { appVersionTag, reportUsage, systemInfo } from "./usage";
  import { localeTag, t } from "./i18n.svelte";

  /** `active`: tab đang hiển thị. `unseen`: số góp ý có kết quả mới chưa xem (để App hiện chấm đỏ). */
  let { active, unseen = $bindable(0) }: { active: boolean; unseen?: number } = $props();

  const MAX_IMAGES = 5;
  const CHECK_EVERY = 30 * 60 * 1000;

  // Khi chạy `npm run dev` trên trình duyệt, backend giả (mock.ts) đóng vai máy chủ góp ý.
  const configured = fb.FEEDBACK_URL !== "" || import.meta.env.DEV;

  let type = $state<FeedbackType>("feature");
  let title = $state("");
  let description = $state("");
  let contact = $state(load("feedbackContact", ""));
  let images = $state<(ImageData & { url: string })[]>([]);
  let sending = $state(false);
  let sendError = $state("");
  let sentNote = $state("");
  let fileInput = $state<HTMLInputElement>();

  let sent = $state<SentItem[]>(fb.loadSent());
  /** Góp ý vừa có kết quả mới trong lần mở này (để làm nổi bật). */
  let fresh = $state<string[]>([]);

  let adminKey = $state(load("feedbackAdminKey", ""));
  let keyDraft = $state("");
  let showLogin = $state(false);
  let adminItems = $state<AdminItem[] | null>(null);
  let adminError = $state("");
  let adminLoading = $state(false);
  let filter = $state<FeedbackStatus | "all">("pending");
  let expanded = $state<string | null>(null);
  let drafts = $state<Record<string, string>>({});
  let saving = $state<string | null>(null);
  let adminImages = $state<Record<string, string>>({});
  let viewer = $state<string | null>(null);
  let stats = $state<UsageStats | null>(null);
  let statsError = $state("");

  /** "v0.6.1: 3 · v0.6.0: 1", nhiều máy nhất trước. */
  const breakdown = (counts: Record<string, number>) =>
    Object.entries(counts)
      .sort((a, b) => b[1] - a[1])
      .map(([name, n]) => `${name || "?"}: ${n}`)
      .join(" · ") || "—";

  const shownItems = $derived((adminItems ?? []).filter((i) => filter === "all" || i.status === filter));
  const pendingCount = $derived((adminItems ?? []).filter((i) => i.status === "pending").length);

  const unseenCount = $derived(sent.filter((s) => !s.seen).length);
  $effect(() => {
    unseen = unseenCount;
  });

  // Mở tab là đã xem kết quả: bỏ chấm đỏ nhưng vẫn làm nổi bật mục mới trong lần mở này.
  $effect(() => {
    if (!active || unseenCount === 0) return;
    untrack(() => {
      fresh = [...fresh, ...sent.filter((s) => !s.seen).map((s) => s.id)];
      sent = sent.map((s) => ({ ...s, seen: true }));
      fb.saveSent(sent);
    });
  });

  $effect(() => {
    if (active) untrack(refreshStatuses);
  });

  onMount(() => {
    const timer = setInterval(refreshStatuses, CHECK_EVERY);
    if (adminKey) loadAdmin();
    return () => clearInterval(timer);
  });

  let lastCheck = 0;
  /** Hỏi máy chủ kết quả các góp ý mình đã gửi; tối đa 30 giây một lần. */
  async function refreshStatuses() {
    if (!configured || !sent.length || Date.now() - lastCheck < 30 * 1000) return;
    lastCheck = Date.now();
    try {
      const results = await fb.fetchStatuses(sent.map((s) => s.id));
      let changed = false;
      const next = sent.map((s) => {
        const r = results.find((x) => x.id === s.id);
        if (!r || (r.status === s.status && r.reply === s.reply)) return s;
        changed = true;
        return { ...s, status: r.status, reply: r.reply, seen: active };
      });
      if (changed) {
        sent = next;
        fb.saveSent(sent);
      }
    } catch {
      lastCheck = 0; // Lỗi mạng thì lần sau thử lại.
    }
  }

  async function addImages(files: Iterable<File>) {
    for (const file of files) {
      if (!file.type.startsWith("image/") || images.length >= MAX_IMAGES) continue;
      try {
        images.push(await fb.readImage(file));
      } catch {
        sendError = t("fb.badImage");
      }
    }
  }

  /** Dán ảnh (Ctrl+V) ở bất kỳ đâu trong form. */
  function onPaste(e: ClipboardEvent) {
    const files = [...(e.clipboardData?.items ?? [])]
      .filter((i) => i.kind === "file" && i.type.startsWith("image/"))
      .map((i) => i.getAsFile())
      .filter((f): f is File => !!f);
    if (files.length) {
      e.preventDefault();
      addImages(files);
    }
  }

  async function send() {
    if (!title.trim() || sending) return;
    sending = true;
    sendError = "";
    sentNote = "";
    // Phòng trình duyệt tự điền mã quản trị vào ô Liên hệ: không bao giờ gửi mã lên Sheet.
    if (adminKey && contact.includes(adminKey)) contact = "";
    save("feedbackContact", contact);
    try {
      const { id } = await fb.submitFeedback({
        type,
        title: title.trim(),
        description: description.trim(),
        contact: contact.trim(),
        appVersion: await appVersionTag(),
        os: systemInfo(),
        images: images.map(({ mime, data }) => ({ mime, data })),
      });
      sent = [{ id, type, title: title.trim(), createdAt: Date.now(), status: "pending", reply: "", seen: true }, ...sent];
      fb.saveSent(sent);
      title = "";
      description = "";
      images = [];
      sentNote = t("fb.thanks");
      if (adminKey) loadAdmin();
    } catch (e) {
      sendError = String(e);
    } finally {
      sending = false;
    }
  }

  async function loadAdmin(key = adminKey) {
    adminLoading = true;
    adminError = "";
    try {
      adminItems = await fb.adminList(key);
      drafts = Object.fromEntries(adminItems.map((i) => [i.id, i.reply]));
      if (key !== adminKey) {
        adminKey = key;
        save("feedbackAdminKey", key);
        reportUsage(true); // Đánh dấu ngay máy này là máy nội bộ.
      }
      showLogin = false;
      fb.adminStats(key).then(
        (s) => ((stats = s), (statsError = "")),
        (e) => (statsError = String(e)),
      );
    } catch (e) {
      adminError = String(e);
    } finally {
      adminLoading = false;
    }
  }

  function logout() {
    adminKey = "";
    adminItems = null;
    stats = null;
    save("feedbackAdminKey", "");
  }

  async function toggle(item: AdminItem) {
    expanded = expanded === item.id ? null : item.id;
    if (expanded !== item.id) return;
    for (const fileId of item.images) {
      if (adminImages[fileId]) continue;
      try {
        adminImages[fileId] = fb.imageUrl(await fb.adminImage(adminKey, fileId));
      } catch {
        adminImages[fileId] = "";
      }
    }
  }

  async function review(item: AdminItem, status: FeedbackStatus) {
    saving = item.id;
    adminError = "";
    try {
      const r = await fb.adminReview(adminKey, item.id, status, drafts[item.id] ?? "");
      Object.assign(item, { status: r.status, reply: drafts[item.id] ?? "", updatedAt: r.updatedAt });
    } catch (e) {
      adminError = String(e);
    } finally {
      saving = null;
    }
  }

  const fmtDate = (v: string | number) =>
    new Date(v).toLocaleString(localeTag(), { hour: "2-digit", minute: "2-digit", day: "2-digit", month: "2-digit", year: "numeric" });
</script>

{#if !configured}
  <section class="card">
    <p class="muted">{t("fb.notConfigured")}</p>
  </section>
{:else}
  <section class="card feedback" onpaste={onPaste}>
    <div class="qualities" role="radiogroup" aria-label={t("fb.type")}>
      {#each fb.FEEDBACK_TYPES as value (value)}
        <button role="radio" aria-checked={type === value} class:selected={type === value} onclick={() => (type = value)}>
          {fb.typeName(value)}
        </button>
      {/each}
    </div>

    <input bind:value={title} maxlength="200" autocomplete="off" placeholder={type === "bug" ? t("fb.titleBug") : t("fb.titleFeature")} />
    <textarea bind:value={description} maxlength="5000" rows="6" placeholder={type === "bug" ? t("fb.descBug") : t("fb.descFeature")}></textarea>

    <div class="shots">
      {#each images as img, i (img.url)}
        <div class="shot">
          <img src={img.url} alt="" />
          <button class="ghost icon" title={t("fb.removeImage")} aria-label={t("fb.removeImage")} onclick={() => images.splice(i, 1)}>✕</button>
        </div>
      {/each}
      {#if images.length < MAX_IMAGES}
        <button class="add-shot" onclick={() => fileInput?.click()}><b>{t("fb.addImage")}</b></button>
      {/if}
      <input
        bind:this={fileInput}
        type="file"
        accept="image/*"
        multiple
        hidden
        onchange={(e) => {
          addImages((e.currentTarget as HTMLInputElement).files ?? []);
          (e.currentTarget as HTMLInputElement).value = "";
        }}
      />
    </div>

    <input bind:value={contact} maxlength="200" autocomplete="off" placeholder={t("fb.contact")} />

    {#if sendError}<p class="error">{sendError}</p>{/if}
    {#if sentNote}<p class="ok-note">{sentNote}</p>{/if}
    <button class="primary big" onclick={send} disabled={sending || !title.trim()}>
      {sending ? t("fb.sending") : type === "bug" ? t("fb.sendBug") : t("fb.sendFeature")}
    </button>
  </section>

  {#if sent.length}
    <section class="queue">
      <div class="queue-head"><h2>{t("fb.sent")}</h2></div>
      {#each sent as s (s.id)}
        <article class="card fb-item" class:fresh={fresh.includes(s.id)}>
          <div class="fb-line">
            <span class="fb-type {s.type}">{fb.typeName(s.type)}</span>
            <span class="fb-title" title={s.title}>{s.title}</span>
            <span class="fb-status {s.status}">{fb.statusName(s.status)}</span>
          </div>
          <small class="muted">{fmtDate(s.createdAt)}</small>
          {#if s.reply}<p class="fb-reply"><b>{t("fb.reply")}</b> {s.reply}</p>{/if}
        </article>
      {/each}
    </section>
  {/if}

  <section class="admin">
    {#if adminKey && adminItems}
      <div class="queue-head"><h2>{t("admin.users")}</h2></div>
      {#if stats}
        <div class="card usage">
          <div class="usage-tiles">
            {#each [[stats.day, t("admin.day")], [stats.week, t("admin.week")], [stats.month, t("admin.month")], [stats.total, t("admin.total")], [stats.downloads, t("admin.downloads")]] as [n, label] (label)}
              <div><b>{n}</b><small class="muted">{label}</small></div>
            {/each}
          </div>
          <small class="muted">{t("admin.newWeek", { n: stats.newWeek })}</small>
          <small class="muted">{t("admin.versions", { list: breakdown(stats.versions) })}</small>
          <small class="muted">{t("admin.systems", { list: breakdown(stats.systems) })}</small>
          {#if stats.machines?.length}
            <details class="machines">
              <summary>{t("admin.machines", { n: stats.machines.length })}</summary>
              <table>
                <thead>
                  <tr>
                    <th>{t("admin.colName")}</th>
                    <th>{t("admin.colVersion")}</th>
                    <th>{t("admin.colOs")}</th>
                    <th>{t("admin.downloads")}</th>
                    <th>{t("admin.colFirst")}</th>
                    <th>{t("admin.colLast")}</th>
                  </tr>
                </thead>
                <tbody>
                  {#each stats.machines as m, i (i)}
                    <tr>
                      <td>{m.name || t("admin.noName")}</td>
                      <td>{m.appVersion}</td>
                      <td>{m.os.split(" ")[0]}</td>
                      <td>{m.installs}</td>
                      <td>{fmtDate(m.firstSeen)}</td>
                      <td>{fmtDate(m.lastSeen)}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </details>
          {/if}
          <small class="muted">{t("admin.statsNote", { n: stats.internal })}</small>
        </div>
      {:else if statsError}
        <p class="error">{t("admin.statsError", { e: statsError })}</p>
      {:else}
        <p class="muted">{t("admin.statsLoading")}</p>
      {/if}

      <div class="queue-head">
        <h2>{t("admin.manage")} {pendingCount ? t("admin.pendingCount", { n: pendingCount }) : ""}</h2>
        <div class="admin-tools">
          <button class="link" onclick={() => loadAdmin()} disabled={adminLoading}>{adminLoading ? t("admin.loading") : t("admin.reload")}</button>
          <button class="link" onclick={logout}>{t("admin.logout")}</button>
        </div>
      </div>
      <div class="qualities" role="radiogroup" aria-label={t("admin.filter")}>
        {#each [["pending", t("fb.pending")], ["accepted", t("fb.accepted")], ["rejected", t("fb.rejected")], ["all", t("admin.all")]] as [value, label] (value)}
          <button role="radio" aria-checked={filter === value} class:selected={filter === value} onclick={() => (filter = value as FeedbackStatus | "all")}>
            {label}
          </button>
        {/each}
      </div>
      {#if adminError}<p class="error">{adminError}</p>{/if}
      {#if !shownItems.length}<p class="muted">{t("admin.empty")}</p>{/if}
      {#each shownItems as item (item.id)}
        <article class="card fb-item admin-item">
          <button class="fb-line fb-toggle" onclick={() => toggle(item)}>
            <span class="fb-type {item.type}">{fb.typeName(item.type)}</span>
            <span class="fb-title" title={item.title}>{item.title}</span>
            {#if item.images.length}<small class="muted">🖼 {item.images.length}</small>{/if}
            <span class="fb-status {item.status}">{fb.statusName(item.status)}</span>
          </button>
          <small class="muted">
            {fmtDate(item.createdAt)} · VidSaver {item.appVersion || "?"} · {item.os || "?"}{item.contact ? ` · ${item.contact}` : ""}
          </small>
          {#if expanded === item.id}
            {#if item.description}<p class="fb-desc">{item.description}</p>{/if}
            {#if item.images.length}
              <div class="shots">
                {#each item.images as fileId (fileId)}
                  <div class="shot">
                    {#if adminImages[fileId]}
                      <button class="shot-view" onclick={() => (viewer = adminImages[fileId])} aria-label={t("admin.viewImage")}>
                        <img src={adminImages[fileId]} alt="" />
                      </button>
                    {:else if adminImages[fileId] === ""}
                      <small class="muted">{t("admin.imageFailed")}</small>
                    {:else}
                      <small class="muted">{t("admin.imageLoading")}</small>
                    {/if}
                  </div>
                {/each}
              </div>
            {/if}
            <textarea bind:value={drafts[item.id]} rows="2" maxlength="2000" placeholder={t("admin.replyPlaceholder")}></textarea>
            <div class="review-actions">
              <button class="primary" onclick={() => review(item, "accepted")} disabled={saving === item.id}>{t("fb.accepted")}</button>
              <button class="danger" onclick={() => review(item, "rejected")} disabled={saving === item.id}>{t("fb.rejected")}</button>
              {#if item.status !== "pending"}
                <button onclick={() => review(item, "pending")} disabled={saving === item.id}>{t("admin.backToPending")}</button>
              {/if}
            </div>
          {/if}
        </article>
      {/each}
    {:else if showLogin}
      <form
        class="admin-login"
        onsubmit={(e) => {
          e.preventDefault();
          loadAdmin(keyDraft.trim());
        }}
      >
        <!-- Không dùng type="password": trình quản lý mật khẩu của WebView sẽ lưu mã rồi tự điền vào ô khác. -->
        <input
          bind:value={keyDraft}
          class="secret"
          autocomplete="off"
          spellcheck="false"
          placeholder={t("admin.keyPlaceholder")}
        />
        <button type="submit" class="primary" disabled={adminLoading || !keyDraft.trim()}>{adminLoading ? t("admin.checking") : t("admin.enter")}</button>
      </form>
      {#if adminError}<p class="error">{adminError}</p>{/if}
    {:else if adminKey}
      {#if adminError}<p class="error">{adminError}</p>{/if}
      <button class="link" onclick={() => loadAdmin()} disabled={adminLoading}>{adminLoading ? t("admin.loading") : t("admin.open")}</button>
    {:else}
      <button class="link admin-link" onclick={() => (showLogin = true)}>{t("admin.link")}</button>
    {/if}
  </section>
{/if}

{#if viewer}
  <button class="viewer" onclick={() => (viewer = null)} aria-label={t("fb.closeImage")}>
    <img src={viewer} alt="" />
  </button>
{/if}
