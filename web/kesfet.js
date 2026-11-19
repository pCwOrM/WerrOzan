import { syncPlaybackMeta, syncPlaybackPosition, clearPlaybackMeta } from "./pwa.js?v=66";

const $ = (id) => document.getElementById(id);

const API_BASE = (localStorage.getItem("werrozan_api_base") || "https://ozan-api.werr.io").replace(/\/+$/, "");
const LS_KEY = "werrozan_api_key";
const LS_VISITOR = "werrozan_visitor_id";
const LS_VOL = "werrozan_player_volume";
const LS_SHUFFLE = "werrozan_discover_shuffle";
const RECAPTCHA_SITE_KEY = "6LdP-JgqAAAAAJLdy_W8uowqstSa4XKlJTOkVAux";

function getVisitorId() {
  let id = localStorage.getItem(LS_VISITOR);
  if (!id || id.length < 8) {
    id = (crypto?.randomUUID ? crypto.randomUUID() : `v_${Date.now()}_${Math.random().toString(36).slice(2, 12)}`);
    localStorage.setItem(LS_VISITOR, id);
  }
  return id;
}

let recaptchaLoadingPromise = null;
function ensureRecaptcha() {
  if (typeof window.grecaptcha !== "undefined") return Promise.resolve();
  if (!recaptchaLoadingPromise) {
    recaptchaLoadingPromise = new Promise((resolve) => {
      const sc = document.createElement("script");
      sc.src = `https://www.google.com/recaptcha/api.js?render=${RECAPTCHA_SITE_KEY}`;
      sc.async = true;
      sc.onload = () => resolve();
      sc.onerror = () => resolve();
      document.head.appendChild(sc);
    });
  }
  return recaptchaLoadingPromise;
}

async function getRecaptchaToken(action) {
  try {
    await ensureRecaptcha();
    if (typeof window.grecaptcha === "undefined") return "";
    return await new Promise((resolve) => {
      const timer = setTimeout(() => resolve(""), 3500);
      window.grecaptcha.ready(() => {
        window.grecaptcha
          .execute(RECAPTCHA_SITE_KEY, { action })
          .then((tok) => {
            clearTimeout(timer);
            resolve(tok || "");
          })
          .catch(() => {
            clearTimeout(timer);
            resolve("");
          });
      });
    });
  } catch {
    return "";
  }
}

function buildHeaders(extra = {}) {
  const h = { "X-Visitor-Id": getVisitorId(), ...extra };
  const key = (localStorage.getItem(LS_KEY) || "").trim();
  if (key) h["Authorization"] = `Bearer ${key}`;
  return h;
}

const state = {
  page: 1,
  limit: 25,
  sort: "popular",
  q: "",
  items: [],
  total: 0,
  totalPages: 1,
  playingId: null,
  playingUrl: null,
  shuffle: localStorage.getItem(LS_SHUFFLE) === "1",
  shuffleHistory: [],
  shuffleVisited: new Set(),
};

const audio = $("globalAudio");

function fmtTime(sec) {
  if (!Number.isFinite(sec) || sec < 0) return "0:00";
  const s = Math.floor(sec);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

function showStatus(msg, cls = "warn") {
  const box = $("discoverStatus");
  if (!msg) {
    box.style.display = "none";
    return;
  }
  box.style.display = "grid";
  box.className = `notice-box ${cls}`;
  box.textContent = msg;
}

function openCoverLightbox(url, title) {
  $("lightboxImg").src = url;
  $("lightboxTitle").textContent = title || "Kapak Görseli";
  $("lightboxDownload").href = url;
  $("lightboxDownload").download = `${(title || "werrozan-cover").replace(/\s+/g, "_")}.svg`;
  $("coverLightbox").style.display = "flex";
}

function closeCoverLightbox() {
  $("coverLightbox").style.display = "none";
  $("lightboxImg").src = "";
}

function openLyricsModal(item) {
  $("lmTitle").textContent = item.title || "İsimsiz Parça";
  const dt = item.created_at ? new Date(item.created_at * 1000).toLocaleString("tr-TR") : "";
  $("lmMeta").textContent = `${item.author_name || "WerrOzan Üyesi"} · ${Math.round(item.duration_seconds || 0)} sn · ${dt}`;
  $("lmStyle").textContent = `Stil Etiketleri: ${item.style || "—"}`;
  $("lmLyrics").textContent = item.lyrics || "(Söz kaydedilmemiş — enstrümantal parça)";
  $("lyricsModal").style.display = "flex";
}

function closeLyricsModal() {
  $("lyricsModal").style.display = "none";
}

function updatePlayingUI() {
  const isPaused = audio.paused;
  $("spPlayPause").textContent = isPaused ? "▶" : "⏸";
  document.querySelectorAll(".discover-card").forEach((card) => {
    const sid = card.dataset.songId;
    const isCardPlaying = Boolean(sid && sid === state.playingId && !isPaused);
    card.classList.toggle("is-playing", isCardPlaying);

    card.querySelectorAll(".js-play-btn").forEach((btn) => {
      const btnUrl = btn.dataset.playUrl || "";
      const isBtnPlaying = Boolean(btnUrl && btnUrl === state.playingUrl && !isPaused);
      btn.textContent = isBtnPlaying ? "⏸ Duraklat" : "▶ Dinle";
      const chip = btn.closest(".koma-chip");
      if (chip) {
        chip.classList.toggle("is-playing-chip", isBtnPlaying);
      }
    });
  });
}

function compactStyle(styleStr) {
  const parts = String(styleStr || "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  if (parts.length <= 4) return parts.join(", ");
  return `${parts.slice(0, 4).join(", ")} …`;
}

let playerMinimized = false;
function setPlayerMinimized(min) {
  playerMinimized = Boolean(min);
  const bar = $("stickyPlayer");
  const pill = $("spMiniPill");
  if (!state.playingUrl) {
    bar.style.display = "none";
    if (pill) pill.style.display = "none";
    return;
  }
  bar.style.display = playerMinimized ? "none" : "block";
  if (pill) pill.style.display = playerMinimized ? "inline-flex" : "none";
}

function playSong(item, overrideUrl = null, variantLabel = null) {
  const targetUrl = overrideUrl || item.audio_url;
  if (!targetUrl) return;

  if (state.playingId === item.id && state.playingUrl === targetUrl) {
    if (audio.paused) {
      audio.play().catch(() => {});
    } else {
      audio.pause();
    }
    updatePlayingUI();
    return;
  }

  // Tek seferde tek parça kilidi: önceki durur, yenisi başlar
  audio.pause();
  audio.src = targetUrl;
  state.playingId = item.id;
  state.playingUrl = targetUrl;

  const shownTitle = variantLabel ? `${item.title} (${variantLabel})` : (item.title || "İsimsiz Parça");
  $("spTitle").textContent = shownTitle;
  if ($("spMiniTitle")) $("spMiniTitle").textContent = shownTitle.slice(0, 24);
  $("spAuthor").textContent = item.author_name || "WerrOzan Üyesi";
  $("spCover").src = item.cover_url || "./assets/werrozan-logo.svg";
  $("spSeek").value = 0;
  $("spCurTime").textContent = "0:00";
  $("spDurTime").textContent = fmtTime(item.duration_seconds || 0);
  const spDl = $("spDownload");
  if (spDl) {
    spDl.href = targetUrl;
    const safeTitle = (shownTitle || "werrozan").replace(/[^\w\u00C0-\u024F-]+/g, "_").replace(/^_+|_+$/g, "");
    spDl.download = `${safeTitle || "werrozan"}.flac`;
    spDl.style.display = "inline-flex";
  }
  setPlayerMinimized(false);

  syncPlaybackMeta({
    title: shownTitle,
    artist: item.author_name || "WerrOzan Üyesi",
    album: "WerrOzan Keşfet",
    cover: item.cover_url || "./icon-512.png",
    audio,
    onPrev: () => playAdjacent(-1),
    onNext: () => playAdjacent(1),
  });

  audio.play().catch((err) => {
    showStatus(`Ses çalınamadı: ${err.message}`, "warn");
  });
  updatePlayingUI();
}

function setShuffle(on, save = true) {
  state.shuffle = Boolean(on);
  const btn = $("spShuffle");
  if (btn) {
    btn.classList.toggle("is-active", state.shuffle);
    btn.setAttribute("aria-pressed", state.shuffle ? "true" : "false");
    btn.title = state.shuffle ? "Karışık Çal (Shuffle): Açık" : "Karışık Çal (Shuffle): Kapalı";
  }
  if (save) localStorage.setItem(LS_SHUFFLE, state.shuffle ? "1" : "0");
}

function playAdjacent(step) {
  if (!state.items.length) return;
  if (state.shuffle && state.items.length > 1) {
    if (step < 0 && state.shuffleHistory.length > 0) {
      const prevId = state.shuffleHistory.pop();
      const prevItem = state.items.find((x) => x.id === prevId);
      if (prevItem) {
        playSong(prevItem);
        return;
      }
    }
    if (state.playingId) {
      state.shuffleVisited.add(state.playingId);
      if (step > 0) {
        state.shuffleHistory.push(state.playingId);
        if (state.shuffleHistory.length > 50) state.shuffleHistory.shift();
      }
    }
    let pool = state.items.filter((x) => x.id !== state.playingId && !state.shuffleVisited.has(x.id));
    if (!pool.length) {
      state.shuffleVisited.clear();
      if (state.playingId) state.shuffleVisited.add(state.playingId);
      pool = state.items.filter((x) => x.id !== state.playingId);
    }
    const pick = pool[Math.floor(Math.random() * pool.length)] || state.items[0];
    if (pick) {
      state.shuffleVisited.add(pick.id);
      playSong(pick);
    }
    return;
  }
  const idx = state.items.findIndex((x) => x.id === state.playingId);
  const nextIdx = idx < 0 ? 0 : (idx + step + state.items.length) % state.items.length;
  playSong(state.items[nextIdx]);
}

async function handleLike(item, btnEl, countEl) {
  btnEl.disabled = true;
  try {
    const token = await getRecaptchaToken("like_song");
    const resp = await fetch(`${API_BASE}/v1/public/songs/${encodeURIComponent(item.id)}/like`, {
      method: "POST",
      headers: buildHeaders({ "Content-Type": "application/json" }),
      body: JSON.stringify({ recaptcha_token: token }),
    });
    const data = await resp.json().catch(() => ({}));
    if (!resp.ok) {
      throw new Error(data.error || `HTTP ${resp.status}`);
    }
    const diff = Number(data.likes_count || 0) - Number(item.likes_count || 0);
    item.likes_count = Number(data.likes_count || 0);
    item.liked_by_me = Boolean(data.liked);
    btnEl.classList.toggle("liked", item.liked_by_me);
    countEl.textContent = String(item.likes_count);
    const totalEl = $("statTotalLikes");
    const curSum = Number(totalEl.dataset.sum || 0) + diff;
    totalEl.dataset.sum = String(Math.max(0, curSum));
    totalEl.textContent = `❤️ ${Math.max(0, curSum)} Kalp`;
  } catch (err) {
    showStatus(`Beğeni hatası: ${err.message}`, "err");
  } finally {
    btnEl.disabled = false;
  }
}

function renderList() {
  const listEl = $("discoverList");
  listEl.innerHTML = "";
  if (!state.items.length) {
    if (state.q.trim()) {
      const wrap = document.createElement("div");
      wrap.className = "sub";
      wrap.style.cssText = "display:flex;align-items:center;gap:10px;flex-wrap:wrap;";
      const msg = document.createElement("span");
      msg.textContent = `"${state.q.trim()}" aramasıyla eşleşen parça bulunamadı.`;
      const resetBtn = document.createElement("button");
      resetBtn.type = "button";
      resetBtn.className = "koma";
      resetBtn.textContent = "✕ Aramayı Temizle";
      resetBtn.addEventListener("click", () => {
        $("searchInput").value = "";
        state.q = "";
        state.page = 1;
        loadSongs();
      });
      wrap.append(msg, resetBtn);
      listEl.appendChild(wrap);
    } else {
      listEl.innerHTML = `<div class="sub">Henüz yayınlanmış parça bulunamadı.</div>`;
    }
    return;
  }

  const startRank = (state.page - 1) * state.limit;

  state.items.forEach((item, idx) => {
    const rank = startRank + idx + 1;
    const card = document.createElement("div");
    card.className = "lib-item discover-card";
    card.dataset.songId = item.id;

    const main = document.createElement("div");
    main.className = "lib-main";

    // Cover
    const coverSlot = document.createElement("div");
    if (item.cover_url) {
      const img = document.createElement("img");
      img.className = "lib-cover";
      img.src = item.cover_url;
      img.alt = `${item.title || "Parça"} kapak görseli`;
      img.loading = "lazy";
      img.title = "Büyütmek için tıklayın";
      img.addEventListener("click", () => openCoverLightbox(item.cover_url, item.title));
      coverSlot.appendChild(img);
    } else {
      const ph = document.createElement("div");
      ph.className = "lib-cover-placeholder";
      ph.textContent = "🎵";
      coverSlot.appendChild(ph);
    }

    const body = document.createElement("div");
    body.className = "lib-body";

    const topRow = document.createElement("div");
    topRow.className = "discover-title-row";

    const titleWrap = document.createElement("div");
    titleWrap.className = "discover-title-wrap";
    const rankBadge = document.createElement("span");
    rankBadge.className = `rank-pill${state.sort === "popular" && item.likes_count > 0 && rank <= 3 ? " top" : ""}`;
    rankBadge.textContent = state.sort === "popular" && item.likes_count > 0 && rank === 1 ? "#1 🏆" : `#${rank}`;

    const titleEl = document.createElement("div");
    titleEl.className = "lib-title";
    titleEl.textContent = item.title || "İsimsiz Parça";

    const authorPill = document.createElement("span");
    authorPill.className = "author-pill";
    authorPill.textContent = `👤 ${item.author_name || "WerrOzan Üyesi"}`;

    titleWrap.append(rankBadge, titleEl, authorPill);

    // Heart button
    const heartBtn = document.createElement("button");
    heartBtn.type = "button";
    heartBtn.className = `heart-btn${item.liked_by_me ? " liked" : ""}`;
    heartBtn.title = "Bu parçayı beğen (reCAPTCHA korumalı)";
    const heartIcon = document.createElement("span");
    heartIcon.textContent = "♥";
    const heartCount = document.createElement("span");
    heartCount.className = "heart-count";
    heartCount.textContent = String(item.likes_count || 0);
    heartBtn.append(heartIcon, heartCount);
    heartBtn.addEventListener("click", () => handleLike(item, heartBtn, heartCount));

    topRow.append(titleWrap, heartBtn);

    const dt = item.created_at ? new Date(item.created_at * 1000).toLocaleString("tr-TR") : "";
    const meta = document.createElement("div");
    meta.className = "lib-meta discover-meta-tags";
    const langLbl = (item.locale || "tr").toUpperCase();
    const stNum = Number(item.steps || 10);
    const stName = stNum <= 10 ? "Taslak" : stNum <= 16 ? "Dengeli" : "Stüdyo";
    const cotMap = { full: "Full", melody: "Melody", off: "Direct" };
    const cotLbl = cotMap[String(item.cot || "full").toLowerCase()] || (item.cot || "Full");
    const shortTags = compactStyle(item.style);
    meta.textContent = `${Math.round(item.duration_seconds || 0)} sn · ${dt} · ${langLbl} · Kalite: ${stNum} (${stName}) · Mod: ${cotLbl}${shortTags ? ` · ${shortTags}` : ""}`;

    const actions = document.createElement("div");
    actions.className = "lib-actions";

    const playBtn = document.createElement("button");
    playBtn.type = "button";
    playBtn.className = "lib-btn play js-play-btn";
    playBtn.dataset.playUrl = item.audio_url || "";
    playBtn.textContent = state.playingUrl === item.audio_url && !audio.paused ? "⏸ Duraklat" : "▶ Dinle";
    playBtn.addEventListener("click", () => playSong(item));

    const lyricsBtn = document.createElement("button");
    lyricsBtn.type = "button";
    lyricsBtn.className = "lib-btn";
    lyricsBtn.textContent = "📜 Söz & Detay";
    lyricsBtn.addEventListener("click", () => openLyricsModal(item));

    const linkBtn = document.createElement("button");
    linkBtn.type = "button";
    linkBtn.className = "lib-btn";
    linkBtn.textContent = "🔗 Link";
    linkBtn.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(item.audio_url);
        linkBtn.textContent = "✓ Kopyalandı";
        setTimeout(() => { linkBtn.textContent = "🔗 Link"; }, 1800);
      } catch {
        window.open(item.audio_url, "_blank", "noopener");
      }
    });

    const remixBtn = document.createElement("button");
    remixBtn.type = "button";
    remixBtn.className = "lib-btn koma";
    remixBtn.textContent = "✨ Benzerini Üret";
    remixBtn.addEventListener("click", () => {
      localStorage.setItem(
        "werrozan_discover_draft",
        JSON.stringify({
          title: `${item.title || "Yeni Parça"} (Remix)`,
          prompt_description: item.prompt_description || "",
          style: item.style || "",
          lyrics: item.lyrics || "",
          locale: item.locale || "tr",
          duration_seconds: Math.round(item.duration_seconds || 90),
          cot: item.cot || "full",
          steps: item.steps || 16,
        })
      );
      window.location.href = "./index.html?from=discover";
    });

    const komaSel = document.createElement("select");
    komaSel.className = "lib-quality-select";
    komaSel.title = "Bu parçayı 53-Koma Türk müziği makamına çekip anında dinle (30 dk geçici)";
    komaSel.setAttribute("aria-label", "Makama Çek");
    const komaPh = document.createElement("option");
    komaPh.value = "";
    komaPh.textContent = "🎛️ Makama Çek";
    komaPh.selected = true;
    komaSel.appendChild(komaPh);
    const makamList = [
      ["rast", "Rast"],
      ["ussak", "Uşşak"],
      ["huzzam", "Hüzzam"],
      ["hicaz", "Hicaz"],
      ["segah", "Segâh"],
      ["nihavent", "Nihavent"],
      ["huseyni", "Hüseynî"],
      ["kurdi", "Kürdî"],
    ];
    for (const [mId, mName] of makamList) {
      const opt = document.createElement("option");
      opt.value = mId;
      opt.textContent = `🎛️ ${mName} (%85)`;
      komaSel.appendChild(opt);
    }
    komaSel.addEventListener("change", async () => {
      const makam = komaSel.value;
      if (!makam) return;
      const mLabel = makamList.find(([id]) => id === makam)?.[1] || makam;
      komaSel.disabled = true;
      komaPh.textContent = `⏳ ${mLabel}...`;
      komaSel.value = "";
      try {
        showStatus(`🎛️ "${item.title}" → ${mLabel} (53-Koma %85) hazırlanıyor...`, "ok");
        const token = await getRecaptchaToken("public_koma");
        const resp = await fetch(`${API_BASE}/v1/public/songs/${encodeURIComponent(item.id)}/koma`, {
          method: "POST",
          headers: buildHeaders({ "Content-Type": "application/json" }),
          body: JSON.stringify({ makam, strength: 0.85, recaptcha_token: token }),
        });
        const data = await resp.json().catch(() => ({}));
        if (!resp.ok || !data.variant) {
          throw new Error(data.error || `HTTP ${resp.status}`);
        }
        const v = data.variant;
        item.koma_variants = (item.koma_variants || []).filter((x) => x.makam_id !== v.makam_id);
        item.koma_variants.push(v);
        renderList();
        playSong(item, v.share_url, v.makam_name);
        showStatus(
          data.cached
            ? `✓ ${item.title} (${v.makam_name}) hazır önbellekten açıldı.`
            : `✓ ${item.title} (${v.makam_name} · 53-Koma %85) hazır ve çalınıyor! (30 dk saklanır)`,
          "ok"
        );
      } catch (err) {
        showStatus(`53-Koma hatası: ${err.message}`, "err");
      } finally {
        komaSel.disabled = false;
        komaPh.textContent = "🎛️ Makama Çek";
        komaSel.value = "";
      }
    });

    actions.append(playBtn, lyricsBtn, linkBtn, komaSel, remixBtn);

    body.append(topRow, meta, actions);
    main.append(coverSlot, body);
    card.appendChild(main);

    if (item.koma_variants && item.koma_variants.length) {
      const kRow = document.createElement("div");
      kRow.className = "koma-chips";
      for (const kv of item.koma_variants) {
        const chip = document.createElement("div");
        chip.className = "koma-chip";
        const lbl = document.createElement("span");
        lbl.textContent = `🎛️ ${kv.makam_name} (%${Math.round((kv.strength || 0.85) * 100)})`;
        const kp = document.createElement("button");
        kp.type = "button";
        kp.className = "js-play-btn";
        kp.dataset.playUrl = kv.share_url || "";
        kp.textContent = state.playingUrl === kv.share_url && !audio.paused ? "⏸ Duraklat" : "▶ Dinle";
        kp.addEventListener("click", () => playSong(item, kv.share_url, kv.makam_name));
        chip.append(lbl, kp);
        kRow.appendChild(chip);
      }
      card.appendChild(kRow);
    }

    listEl.appendChild(card);
  });

  updatePlayingUI();
}

function renderPagination() {
  const bar = $("paginationBar");
  bar.innerHTML = "";
  if (state.total <= 0) {
    bar.style.display = "none";
    return;
  }
  bar.style.display = "flex";

  const info = document.createElement("span");
  info.className = "sub";
  info.textContent = `Sayfa ${state.page} / ${state.totalPages} · Toplam ${state.total} parça`;

  const btns = document.createElement("div");
  btns.className = "page-btns";

  const mkBtn = (label, targetPage, disabled = false, active = false) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = `page-btn${active ? " active" : ""}`;
    b.textContent = label;
    b.disabled = disabled;
    if (!disabled && !active) {
      b.addEventListener("click", () => {
        state.page = targetPage;
        loadSongs();
        window.scrollTo({ top: 0, behavior: "smooth" });
      });
    }
    return b;
  };

  btns.appendChild(mkBtn("◀ Önceki", state.page - 1, state.page <= 1));

  const start = Math.max(1, state.page - 2);
  const end = Math.min(state.totalPages, state.page + 2);
  if (start > 1) {
    btns.appendChild(mkBtn("1", 1, false, state.page === 1));
    if (start > 2) {
      const dots = document.createElement("span");
      dots.className = "sub";
      dots.textContent = "…";
      btns.appendChild(dots);
    }
  }
  for (let p = start; p <= end; p++) {
    btns.appendChild(mkBtn(String(p), p, false, p === state.page));
  }
  if (end < state.totalPages) {
    if (end < state.totalPages - 1) {
      const dots = document.createElement("span");
      dots.className = "sub";
      dots.textContent = "…";
      btns.appendChild(dots);
    }
    btns.appendChild(mkBtn(String(state.totalPages), state.totalPages, false, state.page === state.totalPages));
  }

  btns.appendChild(mkBtn("Sonraki ▶", state.page + 1, state.page >= state.totalPages));
  bar.append(info, btns);
}

async function loadSongs() {
  showStatus("", "");
  try {
    const params = new URLSearchParams({
      page: String(state.page),
      limit: String(state.limit),
      sort: state.sort,
    });
    if (state.q.trim()) params.set("q", state.q.trim());

    const resp = await fetch(`${API_BASE}/v1/public/songs?${params.toString()}`, {
      headers: buildHeaders(),
    });
    const data = await resp.json().catch(() => ({}));
    if (!resp.ok || !data.ok) {
      throw new Error(data.error || `HTTP ${resp.status}`);
    }

    state.items = data.items || [];
    state.total = Number(data.total || 0);
    state.page = Number(data.page || 1);
    state.totalPages = Number(data.total_pages || 1);

    $("statTotalSongs").textContent = `${state.total} Parça`;
    $("statTotalLikes").dataset.sum = String(data.total_likes || 0);
    $("statTotalLikes").textContent = `❤️ ${data.total_likes || 0} Kalp`;

    renderList();
    renderPagination();
  } catch (err) {
    showStatus(`Topluluk listesi alınamadı: ${err.message}`, "err");
  }
}

// Audio events
audio.addEventListener("timeupdate", () => {
  if (!audio.duration || !Number.isFinite(audio.duration)) return;
  $("spCurTime").textContent = fmtTime(audio.currentTime);
  $("spDurTime").textContent = fmtTime(audio.duration);
  $("spSeek").value = String(Math.round((audio.currentTime / audio.duration) * 100));
  syncPlaybackPosition(audio);
});
audio.addEventListener("play", updatePlayingUI);
audio.addEventListener("pause", updatePlayingUI);
audio.addEventListener("ended", () => playAdjacent(1));

$("spPlayPause").addEventListener("click", () => {
  if (!audio.src) return;
  if (audio.paused) audio.play().catch(() => {});
  else audio.pause();
});
$("spShuffle")?.addEventListener("click", () => setShuffle(!state.shuffle, true));
setShuffle(state.shuffle, false);
$("spPrev").addEventListener("click", () => playAdjacent(-1));
$("spNext").addEventListener("click", () => playAdjacent(1));
$("spSeek").addEventListener("input", () => {
  if (audio.duration && Number.isFinite(audio.duration)) {
    audio.currentTime = (Number($("spSeek").value) / 100) * audio.duration;
  }
});
$("spMinimize")?.addEventListener("click", () => setPlayerMinimized(true));
$("spMiniPill")?.addEventListener("click", () => setPlayerMinimized(false));
$("spClose").addEventListener("click", () => {
  audio.pause();
  audio.removeAttribute("src");
  state.playingId = null;
  state.playingUrl = null;
  if ($("spDownload")) $("spDownload").style.display = "none";
  setPlayerMinimized(false);
  clearPlaybackMeta();
  updatePlayingUI();
});

let lastNonZeroVol = Math.max(10, Math.min(100, Number(localStorage.getItem(LS_VOL)) || 85));
function applyVolume(pct, save = true) {
  const v = Math.max(0, Math.min(100, Number(pct)));
  audio.volume = v / 100;
  audio.muted = v === 0;
  $("spVolume").value = String(v);
  $("spMute").textContent = v === 0 ? "🔇" : v < 45 ? "🔉" : "🔊";
  if (v > 0) lastNonZeroVol = v;
  if (save) localStorage.setItem(LS_VOL, String(v));
}
applyVolume(localStorage.getItem(LS_VOL) !== null ? Number(localStorage.getItem(LS_VOL)) : 85, false);

$("spVolume").addEventListener("input", () => applyVolume($("spVolume").value, true));
$("spMute").addEventListener("click", () => {
  if (audio.muted || audio.volume === 0) {
    applyVolume(lastNonZeroVol || 85, true);
  } else {
    applyVolume(0, true);
  }
});

// Controls
$("sortPopular").addEventListener("click", () => {
  state.sort = "popular";
  state.page = 1;
  $("sortPopular").classList.add("active");
  $("sortNewest").classList.remove("active");
  loadSongs();
});
$("sortNewest").addEventListener("click", () => {
  state.sort = "newest";
  state.page = 1;
  $("sortNewest").classList.add("active");
  $("sortPopular").classList.remove("active");
  loadSongs();
});
$("limitSelect").addEventListener("change", () => {
  state.limit = Number($("limitSelect").value) || 25;
  state.page = 1;
  loadSongs();
});
$("btnRefreshDiscover").addEventListener("click", loadSongs);

let searchTimer = null;
$("searchInput").addEventListener("input", () => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    state.q = $("searchInput").value;
    state.page = 1;
    loadSongs();
  }, 300);
});

$("lightboxClose").addEventListener("click", closeCoverLightbox);
$("coverLightbox").addEventListener("click", (e) => { if (e.target === $("coverLightbox")) closeCoverLightbox(); });
$("lmClose").addEventListener("click", closeLyricsModal);
$("lyricsModal").addEventListener("click", (e) => { if (e.target === $("lyricsModal")) closeLyricsModal(); });
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    closeCoverLightbox();
    closeLyricsModal();
  } else if (e.key === " " && state.playingUrl) {
    const tag = (document.activeElement?.tagName || "").toUpperCase();
    if (tag !== "INPUT" && tag !== "TEXTAREA" && tag !== "SELECT" && tag !== "BUTTON" && !document.activeElement?.isContentEditable) {
      e.preventDefault();
      if (audio.paused) audio.play().catch(() => {});
      else audio.pause();
    }
  }
});

function formatQueueOwnerCell(j) {
  if (j.owner_display) return j.owner_display;
  const raw = String(j.owner_email || "").trim();
  let masked = "—";
  if (raw && raw !== "—") {
    if (raw.includes("***")) {
      masked = raw;
    } else {
      const local = raw.split("@")[0] || raw;
      const keep = local.length <= 2 ? 1 : 3;
      masked = `${local.slice(0, keep)}***`;
    }
  }
  const name = String(j.owner_name || "").trim();
  return name ? `${name} (${masked})` : masked;
}

function renderGlobalQueueTable(items) {
  const body = $("globalQueueBody");
  const pill = $("globalQueuePill");
  if (!body) return;
  const list = Array.isArray(items) ? items : [];
  if (pill) pill.textContent = `${list.length} Aktif İş`;
  if (!list.length) {
    body.innerHTML = `<tr><td colspan="6" class="sub">Şu anda kuyrukta bekleyen veya üretimde olan parça yok.</td></tr>`;
    return;
  }
  body.innerHTML = "";
  for (const j of list) {
    const tr = document.createElement("tr");
    const isRun = j.status === "running";
    const durTxt = j.duration_seconds ? `${Math.round(j.duration_seconds)} sn` : "—";
    const etaTxt = j.estimated_minutes ? `~${j.estimated_minutes} dk` : "—";
    const cells = [
      `#${j.queue_position || "?"}`,
      formatQueueOwnerCell(j),
      j.title || j.id || "Parça",
      durTxt,
      etaTxt,
    ];
    for (const c of cells) {
      const td = document.createElement("td");
      td.textContent = c;
      tr.appendChild(td);
    }
    const statusTd = document.createElement("td");
    statusTd.style.cssText = "display:flex;align-items:center;gap:6px;flex-wrap:wrap;";
    const badge = document.createElement("span");
    badge.className = `queue-badge ${isRun ? "run" : "wait"}`;
    badge.textContent = isRun ? "Üretimde" : "Sırada";
    statusTd.appendChild(badge);
    if (isRun && (j.estimated_finish_trt || j.estimated_minutes)) {
      let etaLabel = j.estimated_finish_trt || "";
      if (!etaLabel && j.estimated_minutes) {
        const startMs = Number(j.started_at || j.submitted_at || Date.now());
        const finishMs = startMs + Number(j.estimated_minutes) * 60000;
        const tsi = new Date(finishMs).toLocaleTimeString("tr-TR", {
          timeZone: "Europe/Istanbul",
          hour: "2-digit",
          minute: "2-digit",
          hour12: false,
        });
        etaLabel = `${tsi} TSİ`;
      }
      const etaBadge = document.createElement("span");
      etaBadge.className = "queue-badge eta";
      etaBadge.textContent = `Tahmini Bitiş: ${etaLabel}`;
      statusTd.appendChild(etaBadge);
    }
    tr.appendChild(statusTd);
    body.appendChild(tr);
  }
}

async function refreshGlobalQueue() {
  try {
    const resp = await fetch(`${API_BASE}/v1/queue`, { headers: buildHeaders() });
    if (!resp.ok) return;
    const data = await resp.json().catch(() => ({}));
    renderGlobalQueueTable(data.items || []);
  } catch {
    /* ignore transient offline */
  }
}

// Boot
ensureRecaptcha();
loadSongs();
refreshGlobalQueue();
setInterval(() => {
  if (document.visibilityState !== "hidden") refreshGlobalQueue();
}, 15000);
