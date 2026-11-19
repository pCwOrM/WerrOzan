import {
  MAKAMS,
  analyzeLocalWerrGunes,
  komaToHz,
  playMakamScale,
  playToneHz,
  yue2LyricBudget,
} from "./werr_koma.js?v=21";

const $ = (id) => document.getElementById(id);

const LS_API = "werrozan_api_base";
const LS_KEY = "werrozan_api_key";
const LS_REQ = "werrozan_reg_id";
const LS_EPOCH = "werrozan_session_epoch";
const SESSION_EPOCH = "2026-11-17T15:45:00Z";

let lastRenderedBlob = null;
let lastRenderedExt = "flac";
let lastRenderedSongId = null;
let lastRenderedUrl = null;
let lastKomaUrl = null;
let activePollTimer = null;

// ---------- Tekil Global Stüdyo Çalar Motoru ----------
const LS_VOL = "werrozan_player_volume";
let studioPlayingUrl = null;
let studioMinimized = false;
let lastNonZeroVol = Math.max(10, Math.min(100, Number(localStorage.getItem(LS_VOL)) || 85));

function fmtPlayerTime(sec) {
  if (!Number.isFinite(sec) || sec < 0) return "0:00";
  const s = Math.floor(sec);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

function updateStudioPlayingUI() {
  const audio = $("globalAudio");
  if (!audio) return;
  const isPaused = audio.paused;
  if ($("spPlayPause")) $("spPlayPause").textContent = isPaused ? "▶" : "⏸";

  document.querySelectorAll(".js-play-btn").forEach((btn) => {
    const u = btn.dataset.playUrl || "";
    const active = Boolean(u && u === studioPlayingUrl && !isPaused);
    if (btn.dataset.compact === "1") {
      btn.textContent = active ? "⏸" : "▶";
    } else {
      btn.textContent = active ? "⏸ Duraklat" : "▶ Dinle";
    }
    const chip = btn.closest(".lib-koma-chip");
    if (chip) chip.classList.toggle("is-playing-chip", active);
  });

  document.querySelectorAll(".lib-item").forEach((card) => {
    const anyActive = Array.from(card.querySelectorAll(".js-play-btn")).some(
      (b) => b.dataset.playUrl && b.dataset.playUrl === studioPlayingUrl && !isPaused
    );
    card.classList.toggle("is-playing", anyActive);
  });
}

function setStudioPlayerMinimized(min) {
  studioMinimized = Boolean(min);
  const bar = $("stickyPlayer");
  const pill = $("spMiniPill");
  if (!studioPlayingUrl) {
    if (bar) bar.style.display = "none";
    if (pill) pill.style.display = "none";
    return;
  }
  if (bar) bar.style.display = studioMinimized ? "none" : "block";
  if (pill) pill.style.display = studioMinimized ? "inline-flex" : "none";
}

function playStudioTrack({ url, title, subtitle = "WerrOzan Stüdyo", cover = "./assets/werrozan-logo.svg", downloadName = null }) {
  const audio = $("globalAudio");
  if (!audio || !url) return;

  if (studioPlayingUrl === url) {
    if (audio.paused) audio.play().catch(() => {});
    else audio.pause();
    updateStudioPlayingUI();
    return;
  }

  audio.pause();
  audio.src = url;
  studioPlayingUrl = url;
  if ($("spTitle")) $("spTitle").textContent = title || "WerrOzan Parça";
  if ($("spMiniTitle")) $("spMiniTitle").textContent = (title || "Çalıyor").slice(0, 24);
  if ($("spAuthor")) $("spAuthor").textContent = subtitle;
  if ($("spCover")) $("spCover").src = cover || "./assets/werrozan-logo.svg";
  if ($("spSeek")) $("spSeek").value = "0";
  if ($("spCurTime")) $("spCurTime").textContent = "0:00";
  if ($("spDurTime")) $("spDurTime").textContent = "0:00";
  const dl = $("spDownload");
  if (dl) {
    if (downloadName) {
      dl.href = url;
      dl.download = downloadName;
      dl.style.display = "inline-flex";
    } else {
      dl.style.display = "none";
    }
  }
  setStudioPlayerMinimized(false);
  audio.play().catch((err) => appendLog(`Ses çalınamadı: ${err.message}`));
  updateStudioPlayingUI();
}

function initStudioGlobalPlayer() {
  const audio = $("globalAudio");
  if (!audio) return;

  const applyVol = (pct, save = true) => {
    const v = Math.max(0, Math.min(100, Number(pct)));
    audio.volume = v / 100;
    audio.muted = v === 0;
    if ($("spVolume")) $("spVolume").value = String(v);
    if ($("spMute")) $("spMute").textContent = v === 0 ? "🔇" : v < 45 ? "🔉" : "🔊";
    if (v > 0) lastNonZeroVol = v;
    if (save) localStorage.setItem(LS_VOL, String(v));
  };
  applyVol(localStorage.getItem(LS_VOL) !== null ? Number(localStorage.getItem(LS_VOL)) : 85, false);

  audio.addEventListener("timeupdate", () => {
    if (!audio.duration || !Number.isFinite(audio.duration)) return;
    if ($("spCurTime")) $("spCurTime").textContent = fmtPlayerTime(audio.currentTime);
    if ($("spDurTime")) $("spDurTime").textContent = fmtPlayerTime(audio.duration);
    if ($("spSeek")) $("spSeek").value = String(Math.round((audio.currentTime / audio.duration) * 100));
  });
  audio.addEventListener("play", updateStudioPlayingUI);
  audio.addEventListener("pause", updateStudioPlayingUI);
  audio.addEventListener("ended", updateStudioPlayingUI);

  $("spPlayPause")?.addEventListener("click", () => {
    if (!audio.src) return;
    if (audio.paused) audio.play().catch(() => {});
    else audio.pause();
  });
  $("spSeek")?.addEventListener("input", () => {
    if (audio.duration && Number.isFinite(audio.duration)) {
      audio.currentTime = (Number($("spSeek").value) / 100) * audio.duration;
    }
  });
  $("spVolume")?.addEventListener("input", () => applyVol($("spVolume").value, true));
  $("spMute")?.addEventListener("click", () => {
    if (audio.muted || audio.volume === 0) applyVol(lastNonZeroVol || 85, true);
    else applyVol(0, true);
  });
  $("spMinimize")?.addEventListener("click", () => setStudioPlayerMinimized(true));
  $("spMiniPill")?.addEventListener("click", () => setStudioPlayerMinimized(false));
  $("spClose")?.addEventListener("click", () => {
    audio.pause();
    audio.removeAttribute("src");
    studioPlayingUrl = null;
    setStudioPlayerMinimized(false);
    updateStudioPlayingUI();
  });

  const DEMO_BASE = "https://ozan-api.werr.io/v1/demo/files";
  const DEMO_VARIANTS = {
    orig: { label: "Orijinal (12-TET)", file: "mavi-gecelerde-werrozan.flac" },
    rast: { label: "Rast · 53-Koma %85", file: "mavi-gecelerde-werrozan-rast.flac" },
    ussak: { label: "Uşşak · 53-Koma %85", file: "mavi-gecelerde-werrozan-ussak.flac" },
    huzzam: { label: "Hüzzam · 53-Koma %85", file: "mavi-gecelerde-werrozan-huzzam.flac" },
    hicaz: { label: "Hicaz · 53-Koma %85", file: "mavi-gecelerde-werrozan-hicaz.flac" },
    segah: { label: "Segâh · 53-Koma %85", file: "mavi-gecelerde-werrozan-segah.flac" },
    nihavent: { label: "Nihavent · 53-Koma %85", file: "mavi-gecelerde-werrozan-nihavent.flac" },
    huseyni: { label: "Hüseynî · 53-Koma %85", file: "mavi-gecelerde-werrozan-huseyni.flac" },
    kurdi: { label: "Kürdî · 53-Koma %85", file: "mavi-gecelerde-werrozan-kurdi.flac" },
  };
  let activeDemoMakam = "orig";

  function demoTrackFor(key) {
    const v = DEMO_VARIANTS[key] || DEMO_VARIANTS.orig;
    const url = `${DEMO_BASE}/${v.file}`;
    const title = key === "orig" ? "Mavi gecelerde yükselir sesim" : `Mavi gecelerde yükselir sesim (${v.label})`;
    return {
      url,
      title,
      subtitle: key === "orig" ? "WerrOzan Demo · Orijinal" : `WerrOzan Demo · ${v.label}`,
      cover: "./demo/mavi-gecelerde-cover.jpg",
      downloadName: v.file,
    };
  }

  function selectDemoMakam(key) {
    if (!DEMO_VARIANTS[key]) key = "orig";
    const wasPlayingDemo = Boolean(
      studioPlayingUrl && studioPlayingUrl.startsWith(`${DEMO_BASE}/mavi-gecelerde-werrozan`) && !audio.paused
    );
    const resumeTime = wasPlayingDemo ? audio.currentTime : 0;
    activeDemoMakam = key;
    const v = DEMO_VARIANTS[key];
    const track = demoTrackFor(key);
    const btn = $("btnPlayDemo");
    if (btn) btn.dataset.playUrl = track.url;
    const hint = $("demoSelectedHint");
    if (hint) hint.textContent = `Seçili: ${v.label}`;
    document.querySelectorAll("#demoMakamPills .demo-pill").forEach((p) => {
      p.classList.toggle("is-active", p.dataset.demoMakam === key);
    });
    // Çalarken makam değiştirilirse anlık A/B karşılaştırma için kaldığı saniyeden devam et;
    // duruyorken sadece makamı seç (Dinle'ye basmadan çalma!).
    if (wasPlayingDemo && studioPlayingUrl !== track.url) {
      playStudioTrack(track);
      if (resumeTime > 0.2) {
        const seekOnce = () => {
          if (Number.isFinite(audio.duration) && resumeTime < audio.duration - 0.5) {
            audio.currentTime = resumeTime;
          }
          audio.removeEventListener("loadedmetadata", seekOnce);
        };
        audio.addEventListener("loadedmetadata", seekOnce);
      }
    } else {
      updateStudioPlayingUI();
    }
  }

  $("btnPlayDemo")?.addEventListener("click", () => {
    playStudioTrack(demoTrackFor(activeDemoMakam));
  });
  document.querySelectorAll("#demoMakamPills .demo-pill").forEach((pill) => {
    pill.addEventListener("click", () => {
      selectDemoMakam(pill.dataset.demoMakam);
    });
  });
  $("btnPlayRendered")?.addEventListener("click", () => {
    if (!lastRenderedUrl) return;
    playStudioTrack({
      url: lastRenderedUrl,
      title: $("titleInput")?.value || "Son Üretilen Parça",
      subtitle: "WerrOzan Engine",
      downloadName: $("downloadRendered")?.download || "werrozan.flac",
    });
  });
  $("btnPlayKomaOut")?.addEventListener("click", () => {
    if (!lastKomaUrl) return;
    const makam = $("makamSelect")?.value || "makam";
    playStudioTrack({
      url: lastKomaUrl,
      title: `${$("titleInput")?.value || "Parça"} (${makam})`,
      subtitle: "53-Koma STFT Çıktısı",
      downloadName: $("downloadKomaAudio")?.download || `werrozan-${makam}.flac`,
    });
  });
}

function consumeHashKey() {
  const hash = window.location.hash || "";
  const m = hash.match(/(?:^#|&)key=([^&]+)/);
  if (m && m[1]) {
    const k = decodeURIComponent(m[1]).trim();
    if (k) {
      localStorage.setItem(LS_KEY, k);
      const cleanUrl = window.location.pathname + window.location.search;
      window.history.replaceState(null, "", cleanUrl);
      return k;
    }
  }
  return null;
}

function initConnectionInputs() {
  const hashKey = consumeHashKey();
  const prevEpoch = localStorage.getItem(LS_EPOCH);
  if (prevEpoch !== SESSION_EPOCH) {
    localStorage.setItem(LS_EPOCH, SESSION_EPOCH);
    if (!hashKey && localStorage.getItem(LS_KEY)) {
      localStorage.removeItem(LS_KEY);
      appendLog("Sunucu geçişi sonrası güvenlik yenilemesi: lütfen API anahtarınızı yeniden girip Bağlan'a tıklayın.");
    }
  }
  const savedBase = localStorage.getItem(LS_API);
  const savedKey = hashKey || localStorage.getItem(LS_KEY);
  const params = new URLSearchParams(window.location.search);
  if (params.get("api")) $("apiBase").value = params.get("api");
  else if (savedBase) $("apiBase").value = savedBase;
  if (savedKey) $("apiKey").value = savedKey;
  if (localStorage.getItem(LS_REQ)) {
    $("btnCheckRegStatus").style.display = "inline-block";
  }
}

function getApiConfig() {
  const base = $("apiBase").value.trim().replace(/\/+$/, "");
  const key = $("apiKey").value.trim();
  localStorage.setItem(LS_API, base);
  if (key) localStorage.setItem(LS_KEY, key);
  return { base, key };
}

async function publicApiFetch(path, options = {}) {
  const base = $("apiBase").value.trim().replace(/\/+$/, "");
  const headers = { ...(options.headers || {}) };
  if (options.json !== undefined) headers["Content-Type"] = "application/json";
  const resp = await fetch(`${base}${path}`, {
    method: options.method || (options.json !== undefined ? "POST" : "GET"),
    headers,
    body: options.json !== undefined ? JSON.stringify(options.json) : options.body,
  });
  if (!resp.ok) {
    let msg = "";
    try {
      const j = await resp.json();
      msg = j.error || j.message || JSON.stringify(j);
    } catch {
      msg = await resp.text();
    }
    throw new Error(`HTTP ${resp.status}: ${String(msg).slice(0, 220)}`);
  }
  return resp.json();
}

async function apiFetch(path, options = {}) {
  const { base, key } = getApiConfig();
  if (!key) throw new Error("Lütfen önce e-posta ile üye olup onaylı API anahtarınızı girin.");
  const headers = {
    Authorization: `Bearer ${key}`,
    ...(options.headers || {}),
  };
  if (options.json !== undefined) {
    headers["Content-Type"] = "application/json";
  }
  const ctrl = new AbortController();
  const timeoutMs = options.timeoutMs || 65000;
  const timer = setTimeout(() => ctrl.abort(), timeoutMs);
  let resp;
  try {
    resp = await fetch(`${base}${path}`, {
      method: options.method || (options.json !== undefined ? "POST" : "GET"),
      headers,
      body: options.json !== undefined ? JSON.stringify(options.json) : options.body,
      signal: ctrl.signal,
    });
  } finally {
    clearTimeout(timer);
  }
  if (!resp.ok) {
    const txt = await resp.text();
    throw new Error(`HTTP ${resp.status}: ${txt.slice(0, 220)}`);
  }
  return options.rawResponse ? resp : resp.json();
}

function appendLog(msg) {
  const el = $("renderStatus");
  const ts = new Date().toLocaleTimeString("tr-TR", { hour12: false });
  el.textContent = `[${ts}] ${msg}\n` + el.textContent.slice(0, 1800);
}

// ---------- Güneş Dili + WERR Render ----------

function renderAnalysisUI(local, serverData = null) {
  $("motorPill").textContent = local.motorReady ? "Motor: Aktif (0 B VRAM)" : "Motor: Yüklenemedi";
  $("motorPill").className = `pill ${local.motorReady ? "ok" : "warn"}`;

  const gd = serverData?.gunes_dili;
  const wr = serverData?.werr;

  $("mSyl").textContent = gd ? gd.total_syllables : local.totalSyllables;
  const harm = gd ? gd.vowel_harmony_ratio : local.harmonyRatio;
  $("mHarm").textContent = `${Math.round(harm * 100)}%`;
  $("mVram").textContent = "0 B";
  $("mBudget").textContent = wr
    ? `${wr.cpu_budget.recommended_duration_s}s / ${wr.cpu_budget.recommended_max_seq}`
    : `${Math.min(180, Math.round(Number($("durationInput").value) || 90))}s`;

  const bars = $("werrBars");
  bars.innerHTML = "";
  const items = wr
    ? wr.sections.map((s) => ({ tag: s.tag, residue: s.residue_z9, energy: s.target_energy }))
    : local.werrSections;

  if (!items.length) {
    bars.innerHTML = `<div class="sub">Bölüm etiketleri ([Verse 1], [Chorus]) girildiğinde Z/9Z yörüngesi çizilir.</div>`;
  } else {
    for (const item of items) {
      const cls = item.residue === 0 ? "z0" : item.residue === 6 ? "z6" : "z3";
      const pct = Math.round(item.energy * 100);
      const row = document.createElement("div");
      row.className = "bar-row";
      row.innerHTML = `<span>${item.tag}</span>
        <div class="bar-track"><div class="bar-fill ${cls}" style="width:${pct}%"></div></div>
        <span>Z9=${item.residue} · ${pct}%</span>`;
      bars.appendChild(row);
    }
  }

  const tokBox = $("morphTokens");
  tokBox.innerHTML = "";
  if (!local.morphTokens.length) {
    tokBox.innerHTML = `<span class="sub">Türkçe sözcükler yazıldıkça kök+ek ayrıştırması burada belirir.</span>`;
  } else {
    for (const tk of local.morphTokens) {
      const span = document.createElement("span");
      span.className = "tok";
      span.innerHTML = `<b>${tk.parts[0]}</b>${tk.parts.slice(1).map((p) => `+${p}`).join("")}${tk.gd ? ` → <i>${tk.gd}</i>` : ""}`;
      tokBox.appendChild(span);
    }
  }

  const warnBox = $("gdWarningBox");
  const warnings = [...(gd?.warnings || [])];
  if (!warnings.length) {
    // Also check local per-section line syllable balance & vowel harmony
    const lines = $("lyricsInput").value.split(/\r?\n/);
    let curSec = "Genel";
    const secSyls = {};
    for (const raw of lines) {
      const t = raw.trim();
      if (!t) continue;
      if (/^\[.+\]$/.test(t)) {
        curSec = t;
        continue;
      }
      const syl = (t.match(/[aeıioöuüAEIİOÖUÜ]/g) || []).length;
      if (syl > 0) (secSyls[curSec] ||= []).push(syl);
    }
    for (const [sec, arr] of Object.entries(secSyls)) {
      const lowSec = sec.toLowerCase();
      if ((lowSec === "[intro]" || lowSec === "[outro]") && arr.length > 0) {
        warnings.push(`${sec} altında ${arr.length} söylenecek satır var; kesilme olmaması için [Intro] ve [Outro] altını boş bırakın.`);
      }
      if (arr.length >= 2) {
        const diff = Math.max(...arr) - Math.min(...arr);
        if (diff >= 4) {
          warnings.push(`${sec}: dizeler arası hece farkı yüksek (${Math.min(...arr)}–${Math.max(...arr)} hece, fark=${diff}).`);
        }
      }
    }
    const hasHyphenSplit = lines.some((l) => !/^\[.+\]$/.test(l.trim()) && l.trim().split(/\s+/).some((w) => (w.match(/-/g) || []).length >= 2));
    if (hasHyphenSplit) {
      warnings.push("Sözlerde tireyle hece bölünmesi (örn. ka-ran-lık) tespit edildi; kelimeleri bitişik yazın.");
    }
    const dur = Number($("durationInput")?.value) || 90;
    const budget = yue2LyricBudget(dur);
    const totalSyl = gd?.total_syllables ?? local.totalSyllables;
    const totalSungLines = Object.values(secSyls).reduce((a, arr) => a + arr.length, 0);
    if (totalSyl > 0 && (totalSyl > budget.maxSyllables || totalSungLines > budget.maxLines)) {
      warnings.push(`Kesilme riski (${dur} sn): ${totalSyl} hece / ${totalSungLines} satır bütçeyi (maks ~${budget.maxSyllables} hece, ~${budget.maxLines} satır) aşıyor. '✨ El Werr' ile sığdırın veya Hedef Süreyi artırın.`);
    }
    if (harm < 0.72 && totalSyl > 12) {
      warnings.push(`Ünlü uyumu düşük (%${Math.round(harm * 100)}); daha akıcı vokal için kök/ek uyumunu artırabilirsiniz.`);
    }
  }
  const warnBoxes = [warnBox, $("gdWarningBox2")].filter(Boolean);
  for (const box of warnBoxes) {
    box.style.display = "grid";
    if (warnings.length) {
      box.className = "notice-box warn";
      box.textContent = `⚠ ${warnings.join(" | ")}`;
    } else {
      box.className = "notice-box ok";
      box.textContent = `✓ ${gd?.total_syllables ?? local.totalSyllables} hece · %${Math.round(harm * 100)} ünlü uyumu · Kesilme riski yok — Üretime hazır!`;
    }
  }
  lastRenderedWarnings = warnings;
  const adaptRow = $("gdAdaptRow");
  if (adaptRow) {
    adaptRow.style.display = warnings.length ? "block" : "none";
  }
  if (gd?.warnings?.length) {
    appendLog(`Güneş Dili Uyarısı: ${gd.warnings.join(" | ")}`);
  }
}

let lastRenderedWarnings = [];
let currentUserEmail = "";

const ALLOWED_YUE2_TAGS = new Set([
  "[intro]", "[verse]", "[verse 1]", "[verse 2]", "[verse 3]", "[verse 4]",
  "[pre-chorus]", "[chorus]", "[chorus 1]", "[chorus 2]", "[post-chorus]",
  "[bridge]", "[interlude]", "[solo]", "[instrumental]", "[instrumental break]", "[outro]",
]);

function stripParenthesesLocal(lyrics) {
  return lyrics
    .split(/\r?\n/)
    .map((line) => {
      const t = line.trim();
      if (/^\[.+\]$/.test(t) && !ALLOWED_YUE2_TAGS.has(t.toLowerCase())) {
        return "";
      }
      return line
        .replace(/\([^)]*\)/g, "")
        .split(/\s+/)
        .map((w) => {
          const h = (w.match(/-/g) || []).length;
          const allShort = h >= 1 && w.replace(/[^a-zA-ZçğıöşüÇĞİÖŞÜâîû-]/g, "").split("-").every((p) => p.length > 0 && p.length <= 5);
          return (h >= 2 || allShort) && /^[a-zA-ZçğıöşüÇĞİÖŞÜâîû\-.,!?;:'’]+$/.test(w) ? w.replace(/-/g, "") : w;
        })
        .join(" ")
        .replace(/\s{2,}/g, " ")
        .trim();
    })
    .filter((line, idx, arr) => line !== "" || (idx > 0 && arr[idx - 1] !== ""))
    .join("\n")
    .trim();
}

async function handleAdaptLyrics() {
  const btns = [$("btnAdaptLyrics"), $("btnAdaptLyrics2")].filter(Boolean);
  const lyrics = $("lyricsInput").value;
  if (!lyrics.trim()) return;

  if (!$("apiKey").value.trim()) {
    const cleaned = stripParenthesesLocal(lyrics);
    $("lyricsInput").value = cleaned;
    runLocalGate();
    appendLog("Parantez içi yönlendirmeler yerel olarak temizlendi. Hece/yoğunluk uyarlaması için API anahtarınızla bağlanın.");
    return;
  }

  const prevText = btns[0]?.textContent || "✨ El Werr (Hataları Otomatik Düzelt)";
  for (const b of btns) {
    b.disabled = true;
    b.textContent = "⏳ Güneş Dili + Gemini ile uyarlanıyor...";
  }
  try {
    const dur = Number($("durationInput").value) || 90;
    const seed = {
      cx: Number($("seedCx").value) || -0.743644,
      cy: Number($("seedCy").value) || 0.131826,
      zoom: Number($("seedZoom").value) || 120,
    };
    const payload = {
      lyrics,
      style: $("styleInput").value,
      title: $("titleInput").value,
      duration_seconds: dur,
      warnings: lastRenderedWarnings,
      seed,
      cpu_only: true,
    };
    let res;
    try {
      res = await apiFetch("/v1/werrozan/adapt", { method: "POST", timeoutMs: 95000, json: payload });
    } catch (firstErr) {
      const m = String(firstErr?.message || "");
      if (m === "Failed to fetch" || firstErr?.name === "AbortError" || /HTTP (502|503|504|429)/.test(m)) {
        appendLog("El Werr (Gemini): yanıt gecikti, yedek modelle tekrar deneniyor...");
        res = await apiFetch("/v1/werrozan/adapt", { method: "POST", timeoutMs: 95000, json: payload });
      } else {
        throw firstErr;
      }
    }
    if (res?.lyrics) {
      $("lyricsInput").value = res.lyrics;
      const local = runLocalGate().local;
      if (res.analysis) {
        renderAnalysisUI(local, res.analysis);
      } else {
        await handleKararWerr();
      }
      const fixedLabel = /^gemini/i.test(res.fixed_by || "") ? "Gemini" : (res.fixed_by || "ok");
      appendLog(`El Werr tamamlandı (${fixedLabel}).`);
    }
  } catch (err) {
    appendLog(`El Werr hatası: ${err.message}`);
  } finally {
    for (const b of btns) {
      b.disabled = false;
      b.textContent = prevText;
    }
  }
}

function runLocalGate() {
  const lyrics = $("lyricsInput").value;
  const dur = Number($("durationInput").value) || 90;
  const seed = {
    cx: Number($("seedCx").value) || -0.743644,
    cy: Number($("seedCy").value) || 0.131826,
    zoom: Number($("seedZoom").value) || 120,
  };
  const local = analyzeLocalWerrGunes(lyrics, dur, seed);
  renderAnalysisUI(local, null);
  return { local, seed, dur, lyrics };
}

// ---------- Makam Studio ----------

function renderMakamKeyboard() {
  const key = $("makamSelect").value;
  const m = MAKAMS[key] || MAKAMS.rast;
  $("makamDesc").textContent = `${m.name} (${m.rootName}): ${m.desc}`;
  const container = $("komaKeys");
  container.innerHTML = "";
  m.komas.forEach((k, idx) => {
    const hz = komaToHz(m.rootHz, k);
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "koma-key";
    btn.innerHTML = `<span class="deg">${m.degrees[idx]}</span><span class="hz">${hz.toFixed(2)} Hz</span>`;
    btn.addEventListener("click", () => playToneHz(hz, 0.75));
    container.appendChild(btn);
  });
}

// ---------- Membership & Admin Actions ----------

function switchAuthTab(mode) {
  const isSignup = mode === "signup";
  const isKey = mode === "key";
  const isRecover = mode === "recover";
  $("tabSignup").className = `tab-btn ${isSignup ? "active" : ""}`;
  $("tabKey").className = `tab-btn ${isKey ? "active" : ""}`;
  if ($("tabRecover")) $("tabRecover").className = `tab-btn ${isRecover ? "active" : ""}`;
  $("paneSignup").style.display = isSignup ? "grid" : "none";
  $("paneKey").style.display = isKey ? "grid" : "none";
  if ($("paneRecover")) $("paneRecover").style.display = isRecover ? "grid" : "none";
}

function showRecoverNotice(text, cls = "") {
  const box = $("recoverStatusBox");
  if (!box) return;
  box.style.display = "grid";
  box.className = `notice-box ${cls}`;
  box.textContent = text;
}

async function handleRecoverKey() {
  const email = ($("recoverEmail")?.value || "").trim().toLowerCase();
  if (!email || !email.includes("@")) {
    return showRecoverNotice("Lütfen kayıtlı, geçerli bir e-posta adresi girin.", "err");
  }
  const btn = $("btnRecoverKey");
  btn.disabled = true;
  const orig = btn.textContent;
  btn.textContent = "⏳ Gönderiliyor...";
  try {
    const recaptcha_token = await getRecaptchaToken("recover_key");
    const res = await publicApiFetch("/v1/auth/recover-key", {
      json: { email, recaptcha_token },
    });
    showRecoverNotice(res.message || "Onay bağlantısı e-posta adresinize gönderildi.", "ok");
  } catch (err) {
    showRecoverNotice(`İşlem başarısız: ${err.message}`, "err");
  } finally {
    btn.disabled = false;
    btn.textContent = orig;
  }
}

async function handleEmailNotifToggle() {
  const chk = $("emailNotifCheck");
  if (!chk) return;
  const enabled = Boolean(chk.checked);
  chk.disabled = true;
  try {
    await apiFetch("/v1/auth/preferences", {
      method: "POST",
      json: { email_notifications_enabled: enabled },
    });
    appendLog(
      enabled
        ? "📧 E-posta bildirimleri açıldı (kuyruk, üretim başlangıcı ve tamamlanma bildirimleri gönderilecek)."
        : "🔕 E-posta bildirimleri kapatıldı (artık üretim bildirim e-postası gönderilmeyecek)."
    );
  } catch (err) {
    chk.checked = !enabled;
    appendLog(`Bildirim tercihi kaydedilemedi: ${err.message}`);
  } finally {
    chk.disabled = false;
  }
}

async function handleRotateKey() {
  const ok = window.confirm(
    "Yeni bir woz_... API anahtarı oluşturmak istiyor musunuz?\n\nEski anahtarınız anında geçersiz kalacak, yeni anahtar hem bu tarayıcıya otomatik yüklenecek hem de e-postanıza gönderilecektir. Şarkılarınız ve beğenileriniz aynen korunur."
  );
  if (!ok) return;
  const btn = $("btnRotateKey");
  btn.disabled = true;
  const orig = btn.textContent;
  btn.textContent = "⏳ Anahtar yenileniyor...";
  try {
    const res = await apiFetch("/v1/auth/rotate-key", { method: "POST" });
    if (res.api_key) {
      $("apiKey").value = res.api_key;
      localStorage.setItem(LS_KEY, res.api_key);
      appendLog(`API anahtarınız yenilendi (yeni önek: ${res.key_prefix || "woz_..."}). Yeni anahtar e-postanıza da gönderildi.`);
      await testConnection();
      window.alert(`Yeni API anahtarınız aktif edildi ve e-postanıza (${res.email || ""}) gönderildi:\n\n${res.api_key}`);
    }
  } catch (err) {
    appendLog(`Anahtar yenileme hatası: ${err.message}`);
    window.alert(`Anahtar yenilenemedi: ${err.message}`);
  } finally {
    btn.disabled = false;
    btn.textContent = orig;
  }
}

function showRegNotice(text, cls = "") {
  const box = $("regStatusBox");
  box.style.display = "grid";
  box.className = `notice-box ${cls}`;
  box.textContent = text;
}

const RECAPTCHA_SITE_KEY = "6LdP-JgqAAAAAJLdy_W8uowqstSa4XKlJTOkVAux";
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

async function getRecaptchaToken(action = "werrozan_register") {
  try {
    await ensureRecaptcha();
    if (typeof window.grecaptcha === "undefined" || !window.grecaptcha.ready) return null;
    return await new Promise((resolve) => {
      const t = setTimeout(() => resolve(null), 5000);
      window.grecaptcha.ready(() => {
        window.grecaptcha
          .execute(RECAPTCHA_SITE_KEY, { action })
          .then((token) => {
            clearTimeout(t);
            resolve(token || null);
          })
          .catch(() => {
            clearTimeout(t);
            resolve(null);
          });
      });
    });
  } catch {
    return null;
  }
}

async function handleRegister() {
  const email = $("regEmail").value.trim();
  const name = $("regName").value.trim();
  const note = $("regNote").value.trim();
  if (!email) {
    showRegNotice("Lütfen geçerli bir e-posta adresi girin.", "err");
    return;
  }
  const btn = $("btnRegister");
  btn.disabled = true;
  try {
    const recaptcha_token = await getRecaptchaToken("werrozan_register");
    const res = await publicApiFetch("/v1/auth/register", {
      json: { email, name, note, recaptcha_token },
    });
    if (res.request_id) {
      localStorage.setItem(LS_REQ, res.request_id);
      $("btnCheckRegStatus").style.display = "inline-block";
    }
    $("authRolePill").textContent = "Onay Bekleniyor";
    $("authRolePill").className = "pill warn";
    showRegNotice(
      `${res.message} (Durum: Onay Bekleniyor)`,
      res.admin_notified ? "ok" : "warn"
    );
    appendLog(`Üyelik başvurusu gönderildi: ${email} -> yönetici onayı bekleniyor.`);
  } catch (err) {
    showRegNotice(`Başvuru hatası: ${err.message}`, "err");
    appendLog(`Üyelik başvuru hatası: ${err.message}`);
  } finally {
    btn.disabled = false;
  }
}

async function checkRegistrationStatus() {
  const reqId = localStorage.getItem(LS_REQ);
  if (!reqId) return;
  try {
    const st = await publicApiFetch(`/v1/auth/status/${encodeURIComponent(reqId)}`);
    if (st.status === "approved") {
      $("authRolePill").textContent = "Onaylandı · E-posta Kontrol Et";
      $("authRolePill").className = "pill ok";
      showRegNotice(
        `${st.email} başvurunuz onaylandı! Anahtar (${st.key_prefix || "woz_"}...) e-postanıza gönderildi. Mailinizdeki linke tıklayın veya anahtarı yukarıya yapıştırın.`,
        "ok"
      );
      switchAuthTab("key");
    } else if (st.status === "rejected" || st.status === "revoked") {
      $("authRolePill").textContent = "Onaylanmadı";
      $("authRolePill").className = "pill err";
      showRegNotice(`${st.email} için mevcut durum: ${st.status}. API anahtarı oluşturulmadı.`, "err");
    } else {
      $("authRolePill").textContent = "Onay Bekleniyor";
      $("authRolePill").className = "pill warn";
      showRegNotice(`${st.email} başvurusu beklemede (yönetici onayı bekleniyor, henüz anahtar üretilmedi).`, "warn");
    }
  } catch (err) {
    showRegNotice(`Durum sorgu notu: ${err.message}`, "warn");
  }
}

function handleLogout() {
  localStorage.removeItem(LS_KEY);
  $("apiKey").value = "";
  $("btnLogout").style.display = "none";
  $("adminCard").style.display = "none";
  if ($("wizardBanner")) $("wizardBanner").style.display = "none";
  if ($("wizardModal")) $("wizardModal").style.display = "none";
  if ($("memberKeyActions")) $("memberKeyActions").style.display = "none";
  $("memberInfoBox").style.display = "none";
  $("connPill").textContent = "Üyelik Gerekli";
  $("connPill").className = "pill warn";
  $("authRolePill").textContent = "Ziyaretçi Modu";
  $("authRolePill").className = "pill warn";
  switchAuthTab("signup");
  appendLog("Oturum kapatıldı.");
}

async function refreshAdminUsers() {
  const listEl = $("adminUsersList");
  listEl.innerHTML = "";
  try {
    const data = await apiFetch("/v1/admin/users");
    const m = data.mailer || {};
    $("mailerStatusPill").textContent = m.configured ? `Mailer: ${m.transport}` : "Mailer: SMTP/Resend Eksik (Outbox)";
    $("mailerStatusPill").className = `pill ${m.configured ? "ok" : "warn"}`;

    if (!data.users || !data.users.length) {
      const empty = document.createElement("div");
      empty.className = "sub";
      empty.textContent = "Henüz e-posta ile üyelik başvurusu yok.";
      listEl.appendChild(empty);
      return;
    }

    for (const u of data.users) {
      const row = document.createElement("div");
      row.className = "user-row";

      const info = document.createElement("div");
      info.className = "user-info";
      const em = document.createElement("div");
      em.className = "u-email";
      em.textContent = `${u.email} ${u.name ? `(${u.name})` : ""}`;
      const meta = document.createElement("div");
      meta.className = "u-meta";
      meta.textContent = `Durum: ${u.status} · Key: ${u.key_prefix || "Yok"} · Not: ${u.note || "-"}`;
      info.appendChild(em);
      info.appendChild(meta);

      const acts = document.createElement("div");
      acts.className = "user-actions";

      if (u.status === "pending") {
        const okBtn = document.createElement("button");
        okBtn.type = "button";
        okBtn.className = "btn-sm btn-ok";
        okBtn.textContent = "Onayla & Key Üret";
        okBtn.addEventListener("click", async () => {
          okBtn.disabled = true;
          try {
            const res = await apiFetch(`/v1/admin/users/${u.id}/approve`, { json: {} });
            const n = $("adminNoticeBox");
            n.style.display = "grid";
            if (res.email_delivered) {
              n.className = "notice-box ok";
              n.textContent = `${u.email} onaylandı ve API anahtarı e-postayla gönderildi.`;
            } else {
              n.className = "notice-box warn";
              n.textContent = `${u.email} onaylandı. Canlı SMTP/Resend tanımlı olmadığı için tek seferlik anahtar: ${res.issued_key_once}`;
            }
            await refreshAdminUsers();
          } catch (err) {
            appendLog(`Onay hatası: ${err.message}`);
          }
        });

        const noBtn = document.createElement("button");
        noBtn.type = "button";
        noBtn.className = "btn-sm btn-no";
        noBtn.textContent = "Reddet";
        noBtn.addEventListener("click", async () => {
          noBtn.disabled = true;
          try {
            await apiFetch(`/v1/admin/users/${u.id}/reject`, { json: {} });
            const n = $("adminNoticeBox");
            n.style.display = "grid";
            n.className = "notice-box err";
            n.textContent = `${u.email} başvurusu reddedildi (API anahtarı oluşturulmadı).`;
            await refreshAdminUsers();
          } catch (err) {
            appendLog(`Red hatası: ${err.message}`);
          }
        });

        acts.appendChild(okBtn);
        acts.appendChild(noBtn);
      } else if (u.status === "approved") {
        const revBtn = document.createElement("button");
        revBtn.type = "button";
        revBtn.className = "btn-sm btn-no";
        revBtn.textContent = "Anahtarı İptal Et";
        revBtn.addEventListener("click", async () => {
          revBtn.disabled = true;
          try {
            await apiFetch(`/v1/admin/users/${u.id}/revoke`, { method: "POST" });
            await refreshAdminUsers();
          } catch (err) {
            appendLog(`İptal hatası: ${err.message}`);
          }
        });
        acts.appendChild(revBtn);
      }

      row.appendChild(info);
      row.appendChild(acts);
      listEl.appendChild(row);
    }
  } catch (err) {
    appendLog(`Yönetici listesi hatası: ${err.message}`);
  }
}

async function toggleAdminOutbox() {
  const box = $("adminOutboxList");
  if (box.style.display === "block") {
    box.style.display = "none";
    return;
  }
  try {
    const res = await apiFetch("/v1/admin/outbox");
    const lines = (res.items || []).map(
      (i) => `[#${i.id}] ${i.delivered ? "DELIVERED" : "QUEUED"} (${i.transport}) -> ${i.to_email}\nKonu: ${i.subject}\n${i.text_body}\n---`
    );
    box.textContent = lines.length ? lines.join("\n") : "Giden kutusu boş.";
    box.style.display = "block";
  } catch (err) {
    appendLog(`Outbox hatası: ${err.message}`);
  }
}

// ---------- Server Actions ----------

async function testConnection() {
  const pill = $("connPill");
  pill.textContent = "Bağlanıyor...";
  pill.className = "pill warn";
  try {
    const [health, wz, res, me] = await Promise.all([
      apiFetch("/health"),
      apiFetch("/v1/werrozan/status"),
      apiFetch("/v1/system/resources"),
      apiFetch("/v1/auth/me"),
    ]);
    const r = res.resources || {};
    const engineOk = health.music_engine?.reachable;
    pill.textContent = engineOk ? "Bağlı · WerrOzan Aktif" : "Bağlı · Motor Beklemede";
    pill.className = `pill ${engineOk ? "ok" : "warn"}`;
    $("hwPill").textContent = `Sunucu Yükü: %${Math.round(r.cpu_percent || 0)}`;
    $("geminiPill").textContent = wz.gemini_ready ? "System 2: Gemini" : "System 2: Kapalı";
    $("geminiPill").className = `pill ${wz.gemini_ready ? "ok" : "warn"}`;
    $("btnLogout").style.display = "inline-block";
    if ($("wizardBanner")) $("wizardBanner").style.display = "flex";
    currentUserEmail = (me.email || "").trim().toLowerCase();

    const rolePill = $("authRolePill");
    if (me.is_admin) {
      rolePill.textContent = "Yönetici Aktif (Master)";
      rolePill.className = "pill ok";
      $("adminCard").style.display = "flex";
      refreshAdminUsers();
    } else {
      rolePill.textContent = `Üye Aktif (${me.email || me.key_prefix})`;
      rolePill.className = "pill ok";
      $("adminCard").style.display = "none";
    }
    const mb = $("memberInfoBox");
    mb.style.display = "grid";
    mb.className = "notice-box ok";
    mb.textContent = `Oturum açık: ${me.email || "Master Admin"} (${me.role}) · Anahtar öneki: ${me.key_prefix || "-"}`;
    if ($("memberKeyActions")) $("memberKeyActions").style.display = me.is_admin ? "none" : "flex";
    if ($("emailNotifCheck")) $("emailNotifCheck").checked = me.email_notifications_enabled !== false;
    switchAuthTab("key");
    refreshUserLibrary();

    appendLog(`Sunucu bağlantısı doğrulandı (Faz ${wz.phase}, Rol: ${me.role}, Koma: ${wz.koma_engine}, WerrOzan Engine reachable=${engineOk}).`);
  } catch (err) {
    pill.textContent = "Bağlantı Hatası";
    pill.className = "pill err";
    $("adminCard").style.display = "none";
    if ($("wizardBanner")) $("wizardBanner").style.display = "none";
    appendLog(`Bağlantı hatası: ${err.message}`);
  }
}

async function handleCevapWerr() {
  const btn = $("btnCevapWerr");
  const origLabel = btn.textContent;
  btn.disabled = true;
  btn.textContent = "⏳ Taslak hazırlanıyor...";
  appendLog("Söz Werr (Gemini): şarkı taslağı hazırlanıyor...");
  try {
    const payload = {
      target: "all",
      description: $("ideaInput").value.trim(),
      locale: $("localeSelect").value,
      duration_seconds: Number($("durationInput").value) || 90,
    };
    let draft;
    try {
      draft = await apiFetch("/v1/assistant/write", { timeoutMs: 95000, json: payload });
    } catch (firstErr) {
      const m = String(firstErr?.message || "");
      if (m === "Failed to fetch" || firstErr?.name === "AbortError" || /HTTP (502|503|504|429)/.test(m)) {
        appendLog("Söz Werr (Gemini): yanıt gecikti, yedek modelle tekrar deneniyor...");
        draft = await apiFetch("/v1/assistant/write", { timeoutMs: 95000, json: payload });
      } else {
        throw firstErr;
      }
    }
    if (draft.title) $("titleInput").value = draft.title;
    if (draft.style) $("styleInput").value = draft.style;
    if (draft.lyrics) $("lyricsInput").value = draft.lyrics;
    if (draft.duration_seconds) $("durationInput").value = Math.min(180, draft.duration_seconds);
    const local = runLocalGate().local;
    if (draft.werrozan) renderAnalysisUI(local, draft.werrozan);
    appendLog(`Söz Werr tamamlandı: "${draft.title || "Taslak"}" (${draft.werrozan?.gunes_dili?.total_syllables ?? "?"} hece).`);
  } catch (err) {
    const msg = err.message === "Failed to fetch"
      ? "Sunucuya ulaşılamadı veya istek zaman aşımına uğradı. Lütfen tekrar deneyin."
      : err.message;
    appendLog(`Söz Werr hatası: ${msg}`);
  } finally {
    btn.disabled = false;
    btn.textContent = origLabel;
  }
}

async function handleKararWerr() {
  const { local, seed, dur, lyrics } = runLocalGate();
  appendLog(`Karar Werr (lokal): ${local.totalSyllables} hece, Ünlü uyumu %${Math.round(local.harmonyRatio * 100)}, 0 B VRAM.`);
  if (!$("apiKey").value.trim()) return;
  try {
    const serverData = await apiFetch("/v1/werrozan/analyze", {
      json: { lyrics, duration_seconds: dur, seed, cpu_only: true },
    });
    renderAnalysisUI(local, serverData);
    appendLog(`Karar Werr (sunucu doğrulaması): ${serverData.werr.cpu_budget.reason}`);
  } catch (err) {
    appendLog(`Sunucu doğrulama notu: ${err.message}`);
  }
}

function startJobPolling(jobId) {
  if (!jobId) return;
  if (activePollTimer) clearInterval(activePollTimer);
  const btn = $("btnRenderSong");
  activePollTimer = setInterval(async () => {
    try {
      const cur = await apiFetch(`/v1/music/jobs/${jobId}`);
      appendLog(`Render durumu [${cur.id}]: ${cur.status} / ${cur.phase} — ${cur.message || ""}`);
      if (cur.status === "completed") {
        clearInterval(activePollTimer);
        activePollTimer = null;
        if (btn) btn.disabled = false;
        lastRenderedSongId = cur.user_song_id || cur.song?.id || cur.songs?.[0]?.id || null;
        const song = cur.song || cur.songs?.[0];
        if (cur.share_url || song?.id) {
          const mediaUrl = cur.share_url || null;
          if (mediaUrl) {
            lastRenderedBlob = null;
            lastRenderedExt = "flac";
            lastRenderedUrl = mediaUrl;
            const dlName = `${(cur.title || $("titleInput").value || "werrozan").replace(/\s+/g, "_")}.flac`;
            $("downloadRendered").href = mediaUrl;
            $("downloadRendered").download = dlName;
            $("downloadRendered").textContent = "⬇ İndir (FLAC)";
            if ($("btnPlayRendered")) $("btnPlayRendered").dataset.playUrl = mediaUrl;
            $("playerWrap").style.display = "grid";
            playStudioTrack({ url: mediaUrl, title: cur.title || $("titleInput").value || "Yeni Parça", subtitle: "WerrOzan Engine", downloadName: dlName });
          } else {
            const mediaResp = await apiFetch(`/v1/library/media/${song.id}`, { rawResponse: true });
            const blob = await mediaResp.blob();
            lastRenderedBlob = blob;
            lastRenderedExt = (song.format || "flac").toLowerCase();
            const url = URL.createObjectURL(blob);
            lastRenderedUrl = url;
            const dlName = `${(cur.title || $("titleInput").value || "werrozan").replace(/\s+/g, "_")}.${lastRenderedExt}`;
            $("downloadRendered").href = url;
            $("downloadRendered").download = dlName;
            $("downloadRendered").textContent = `⬇ İndir (${lastRenderedExt.toUpperCase()})`;
            if ($("btnPlayRendered")) $("btnPlayRendered").dataset.playUrl = url;
            $("playerWrap").style.display = "grid";
            playStudioTrack({ url, title: cur.title || $("titleInput").value || "Yeni Parça", subtitle: "WerrOzan Engine", downloadName: dlName });
          }
          appendLog(`Tamamlandı! Parça (${lastRenderedSongId || song?.id}) Medya Kitaplığınıza kaydedildi.`);
          refreshUserLibrary();
        }
      } else if (cur.status === "failed" || cur.status === "cancelled") {
        clearInterval(activePollTimer);
        activePollTimer = null;
        if (btn) btn.disabled = false;
        appendLog(`Render sonlandı (${cur.status}): ${cur.message}`);
        refreshUserLibrary();
      }
    } catch (err) {
      appendLog(`Durum sorgu uyarısı: ${err.message}`);
    }
  }, 4500);
}

async function handleRenderSong() {
  if (!lastRenderedWarnings.length) {
    runLocalGate();
  }
  if (lastRenderedWarnings.length > 0) {
    const preview = lastRenderedWarnings.slice(0, 3).map((w) => `• ${w}`).join("\n");
    const extra = lastRenderedWarnings.length > 3 ? `\n(+${lastRenderedWarnings.length - 3} ek uyarı daha)` : "";
    const proceed = window.confirm(
      `⚠ Dil uyumu ve Güneş Dili kontrolleri henüz tamamlanmamış:\n\n${preview}${extra}\n\nUyarılar düzeltilmeden üretim yapılması önerilmez (AI vokal ritimden düşebilir veya parantez içi notları seslendirebilir).\n\nYine de üretmek istiyor musunuz?\n(İptal'e basarsanız '✨ El Werr' düğmesine yönlendirilirsiniz.)`
    );
    if (!proceed) {
      const adaptBtn = $("btnAdaptLyrics");
      if (adaptBtn) {
        adaptBtn.scrollIntoView({ behavior: "smooth", block: "center" });
        adaptBtn.focus();
      }
      appendLog("Üretim durduruldu: Önce '✨ El Werr' düğmesiyle sözleri uyumlamanız önerilir.");
      return;
    }
  }
  const btn = $("btnRenderSong");
  btn.disabled = true;
  if (activePollTimer) clearInterval(activePollTimer);
  try {
    const payload = {
      title: $("titleInput").value.trim() || undefined,
      description: $("ideaInput").value.trim() || undefined,
      locale: $("localeSelect").value,
      style: $("styleInput").value.trim(),
      lyrics: $("lyricsInput").value,
      duration_seconds: Number($("durationInput").value) || 90,
      eos_buffer: Boolean($("eosBufferCheck")?.checked),
      cot: $("cotSelect").value,
      steps: Number($("stepsSelect").value) || 16,
    };
    appendLog(`WerrOzan Engine kuyruğuna iş gönderiliyor (${payload.duration_seconds}s, ${payload.cot})...`);
    const job = await apiFetch("/v1/music/jobs", { json: payload });
    appendLog(`İş sıraya alındı: id=${job.id} (Sıra #${job.queue_position || 1}, tahmini ~${job.estimated_minutes || "?"} dk).`);
    refreshUserLibrary();
    startJobPolling(job.id);
  } catch (err) {
    btn.disabled = false;
    appendLog(`Render başlatma hatası: ${err.message}`);
  }
}

async function handleExportKomaMidi() {
  try {
    const makam = $("makamSelect").value;
    const res = await apiFetch("/v1/werrozan/koma/score", {
      json: {
        abc: $("abcInput").value,
        makam,
        bend_range_semitones: 2.0,
      },
    });
    const raw = atob(res.midi_base64);
    const arr = new Uint8Array(raw.length);
    for (let i = 0; i < raw.length; i++) arr[i] = raw.charCodeAt(i);
    const blob = new Blob([arr], { type: "audio/midi" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `werrozan-${makam}-53koma.mid`;
    a.click();
    appendLog(`53-Koma MIDI indirildi: ${makam} (${res.note_mappings.length} nota pitch-bend haritalandı).`);
  } catch (err) {
    appendLog(`Koma MIDI hatası: ${err.message}`);
  }
}

async function quantizeBlobWithKoma(blob, ext) {
  const makam = $("makamSelect").value;
  const strength = Number($("komaStrength").value) / 100;
  appendLog(`STFT Koma Quantizer çalışıyor (${makam}, güç=${Math.round(strength * 100)}%)...`);
  const ab = await blob.arrayBuffer();
  const bytes = new Uint8Array(ab);
  let bin = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    bin += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  const b64 = btoa(bin);
  const res = await apiFetch("/v1/werrozan/koma/quantize", {
    json: {
      audio_base64: b64,
      input_format: ext,
      output_format: "flac",
      makam,
      strength,
    },
  });
  const outRaw = atob(res.audio_base64);
  const outArr = new Uint8Array(outRaw.length);
  for (let i = 0; i < outRaw.length; i++) outArr[i] = outRaw.charCodeAt(i);
  const outBlob = new Blob([outArr], { type: "audio/flac" });
  const url = URL.createObjectURL(outBlob);
  lastKomaUrl = url;
  $("downloadKomaAudio").href = url;
  $("downloadKomaAudio").download = `werrozan-${makam}.flac`;
  if ($("btnPlayKomaOut")) $("btnPlayKomaOut").dataset.playUrl = url;
  $("komaAudioWrap").style.display = "grid";
  playStudioTrack({ url, title: `53-Koma (${makam})`, subtitle: "53-Koma STFT Çıktısı", downloadName: `werrozan-${makam}.flac` });
  appendLog(`53-Koma STFT tamamlandı: ${res.report.peaks_shifted} tepe düzeltildi (ort. ${res.report.mean_cent_correction}c).`);
}

async function quantizeSongById(songId) {
  const makam = $("makamSelect").value;
  const strength = Number($("komaStrength").value) / 100;
  appendLog(`STFT Koma Quantizer çalışıyor (${makam}, güç=${Math.round(strength * 100)}%, kitaplık kaydı)...`);
  const res = await apiFetch("/v1/werrozan/koma/quantize", {
    json: {
      song_id: songId,
      output_format: "flac",
      makam,
      strength,
    },
  });
  const shareUrl = res.variant?.share_url;
  let finalUrl = shareUrl || null;
  if (shareUrl) {
    $("downloadKomaAudio").href = shareUrl;
  } else if (res.audio_base64) {
    const outRaw = atob(res.audio_base64);
    const outArr = new Uint8Array(outRaw.length);
    for (let i = 0; i < outRaw.length; i++) outArr[i] = outRaw.charCodeAt(i);
    finalUrl = URL.createObjectURL(new Blob([outArr], { type: "audio/flac" }));
    $("downloadKomaAudio").href = finalUrl;
  }
  if (finalUrl) {
    lastKomaUrl = finalUrl;
    if ($("btnPlayKomaOut")) $("btnPlayKomaOut").dataset.playUrl = finalUrl;
    playStudioTrack({ url: finalUrl, title: `${$("titleInput")?.value || "Parça"} (${makam})`, subtitle: "53-Koma STFT Çıktısı", downloadName: `werrozan-${makam}.flac` });
  }
  $("downloadKomaAudio").download = `werrozan-${makam}.flac`;
  $("komaAudioWrap").style.display = "grid";
  const rpt = res.report || {};
  appendLog(
    res.cached
      ? `✓ Bu parçanın ${res.makam_name || makam} (53-Koma) sürümü zaten mevcut (30 dk süresi dolmadan yeniden işlenmedi) — doğrudan açıldı.`
      : `53-Koma STFT tamamlandı: ${rpt.peaks_shifted ?? "?"} tepe düzeltildi (ort. ${rpt.mean_cent_correction ?? "?"}c).`
  );
  refreshUserLibrary();
}

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
    const data = await publicApiFetch("/v1/queue");
    renderGlobalQueueTable(data.items || []);
  } catch {
    /* ignore transient offline */
  }
}

const coverBackfillRequested = new Set();

function openCoverLightbox(url, title) {
  const modal = $("coverLightbox");
  const img = $("lightboxImg");
  const titleEl = $("lightboxTitle");
  const dlEl = $("lightboxDownload");
  if (!modal || !img || !url) return;
  img.src = url;
  img.alt = `${title || "Şarkı"} kapak görseli`;
  if (titleEl) titleEl.textContent = title || "Kapak Görseli";
  if (dlEl) {
    const extMatch = url.match(/\.(png|jpg|jpeg|webp|svg)(?:\?|$)/i);
    const ext = extMatch ? extMatch[1].toLowerCase() : "svg";
    const safeName = (title || "werrozan-cover").replace(/[^\p{L}\p{N}_-]+/gu, "_").replace(/^_+|_+$/g, "") || "werrozan-cover";
    dlEl.href = url;
    dlEl.download = `${safeName}-kapak.${ext}`;
  }
  modal.style.display = "flex";
}

function closeCoverLightbox() {
  const modal = $("coverLightbox");
  const img = $("lightboxImg");
  if (modal) modal.style.display = "none";
  if (img) img.src = "";
}

async function refreshUserLibrary() {
  const listEl = $("userLibraryList");
  const pill = $("libCountPill");
  const qBanner = $("queueBanner");
  if (!listEl || !$("apiKey").value.trim()) return;
  try {
    const data = await apiFetch("/v1/user/library");
    const songs = data.songs || [];
    const maxSongs = data.max_songs || 10;
    if (pill) pill.textContent = `${songs.length} / ${maxSongs} Şarkı`;

    const activeJobs = data.queue?.active_jobs || [];
    renderGlobalQueueTable(activeJobs);
    const myJobs = Array.isArray(data.queue?.my_jobs)
      ? data.queue.my_jobs
      : activeJobs.filter(
          (j) => currentUserEmail && j.owner_email && j.owner_email.toLowerCase() === currentUserEmail
        );
    if (qBanner) {
      if (myJobs.length > 0) {
        qBanner.style.display = "grid";
        qBanner.className = "notice-box warn";
        qBanner.innerHTML = "";
        for (const j of myJobs) {
          const row = document.createElement("div");
          row.style.cssText = "display:flex;justify-content:space-between;align-items:center;gap:8px;flex-wrap:wrap;";
          const txt = document.createElement("span");
          const isQueued = j.status === "queued";
          txt.textContent = `• ${j.title || j.id} (${isQueued ? `Sırada #${j.queue_position || "?"}` : "Üretimde — iptal edilemez"}${j.estimated_minutes ? ` · ~${j.estimated_minutes} dk` : ""})`;
          row.appendChild(txt);
          if (isQueued) {
            const cBtn = document.createElement("button");
            cBtn.type = "button";
            cBtn.className = "lib-btn del";
            cBtn.textContent = "✕ Kuyruktan Çıkar";
            cBtn.addEventListener("click", async () => {
              cBtn.disabled = true;
              try {
                await apiFetch(`/v1/music/jobs/${j.id}`, { method: "POST" });
                appendLog(`Kuyruktan çıkarıldı: ${j.title || j.id}`);
                await refreshUserLibrary();
              } catch (err) {
                appendLog(`İptal uyarısı: ${err.message}`);
                cBtn.disabled = false;
              }
            });
            row.appendChild(cBtn);
          }
          qBanner.appendChild(row);
        }
        if (!activePollTimer && myJobs[0]?.id) {
          startJobPolling(myJobs[0].id);
        }
      } else {
        qBanner.style.display = "none";
      }
    }

    if (!songs.length) {
      listEl.innerHTML = `<div class="sub">Henüz kayıtlı parçanız yok. İlk şarkınızı ürettiğinizde burada otomatik listelenecek.</div>`;
      return;
    }

    listEl.innerHTML = "";
    for (const s of songs) {
      const item = document.createElement("div");
      item.className = "lib-item";
      const dt = s.created_at ? new Date(s.created_at * 1000).toLocaleString("tr-TR") : "";

      const main = document.createElement("div");
      main.className = "lib-main";

      const coverSlot = document.createElement("div");
      const attachCoverImg = (coverUrl) => {
        coverSlot.innerHTML = "";
        const img = document.createElement("img");
        img.className = "lib-cover";
        img.src = coverUrl;
        img.alt = `${s.title || "Şarkı"} kapak görseli`;
        img.title = "Büyütmek ve indirmek için tıklayın";
        img.addEventListener("click", () => openCoverLightbox(coverUrl, s.title || "Kapak Görseli"));
        coverSlot.appendChild(img);
      };

      if (s.cover_url) {
        attachCoverImg(s.cover_url);
      } else {
        const ph = document.createElement("div");
        ph.className = "lib-cover-placeholder";
        ph.textContent = "Kapak üretiliyor...";
        coverSlot.appendChild(ph);
        if (!coverBackfillRequested.has(s.id)) {
          coverBackfillRequested.add(s.id);
          apiFetch(`/v1/user/library/${s.id}/cover`, { method: "POST" })
            .then((res) => {
              if (res?.cover_url) {
                s.cover_url = res.cover_url;
                attachCoverImg(res.cover_url);
              } else {
                ph.textContent = "Kapak yok";
              }
            })
            .catch(() => {
              ph.textContent = "Kapak yok";
            });
        }
      }

      const head = document.createElement("div");
      head.className = "lib-head";
      head.style.flex = "1";
      head.style.minWidth = "0";
      const info = document.createElement("div");
      const titleRow = document.createElement("div");
      titleRow.className = "lib-title-row";
      const titleEl = document.createElement("div");
      titleEl.className = "lib-title";
      titleEl.textContent = s.title || "İsimsiz Parça";
      const likesNum = Number(s.likes_count || 0);
      const likesBadge = document.createElement("span");
      likesBadge.className = `lib-likes-pill${likesNum > 0 ? " has-likes" : ""}`;
      likesBadge.title = "Keşfet topluluk beğenisi";
      likesBadge.textContent = `♥ ${likesNum}`;
      titleRow.append(titleEl, likesBadge);
      const metaEl = document.createElement("div");
      metaEl.className = "lib-meta";
      const langLbl = (s.locale || "tr").toUpperCase();
      const stNum = Number(s.steps || 10);
      const stName = stNum <= 10 ? "Taslak" : stNum <= 16 ? "Dengeli" : "Stüdyo";
      const cotMap = { full: "Full", melody: "Melody", off: "Direct" };
      const cotLbl = cotMap[String(s.cot || "full").toLowerCase()] || (s.cot || "Full");
      const rawStyle = (s.style || "").trim();
      const shortStyle = rawStyle.length > 64 ? `${rawStyle.slice(0, 64).trimEnd()}...` : rawStyle;
      metaEl.textContent = `${Math.round(s.duration_seconds || 0)} sn · ${dt} · ${langLbl} · Kalite: ${stNum} (${stName}) · Mod: ${cotLbl}${shortStyle ? ` · ${shortStyle}` : ""}`;
      info.appendChild(titleRow);
      info.appendChild(metaEl);

      const acts = document.createElement("div");
      acts.className = "lib-acts";

      const playBtn = document.createElement("button");
      playBtn.type = "button";
      playBtn.className = "lib-btn js-play-btn";
      playBtn.dataset.playUrl = s.share_url || "";
      playBtn.textContent = "▶ Dinle";
      playBtn.addEventListener("click", () => {
        lastRenderedSongId = s.id;
        lastRenderedBlob = null;
        lastRenderedUrl = s.share_url;
        const dlName = `${(s.title || "werrozan").replace(/\s+/g, "_")}.${s.format || "flac"}`;
        playStudioTrack({
          url: s.share_url,
          title: s.title || "İsimsiz Parça",
          subtitle: "Medya Kitaplığım",
          cover: s.cover_url || "./assets/werrozan-logo.svg",
          downloadName: dlName,
        });
      });

      const dlA = document.createElement("a");
      dlA.className = "lib-btn";
      dlA.href = s.share_url;
      dlA.download = `${(s.title || "werrozan").replace(/\s+/g, "_")}.${s.format || "flac"}`;
      dlA.textContent = "⬇ FLAC";

      const copyBtn = document.createElement("button");
      copyBtn.type = "button";
      copyBtn.className = "lib-btn";
      copyBtn.textContent = "🔗 Link";
      copyBtn.addEventListener("click", async () => {
        try {
          await navigator.clipboard.writeText(s.share_url);
          copyBtn.textContent = "✓ Kopyalandı";
          setTimeout(() => { copyBtn.textContent = "🔗 Link"; }, 1800);
        } catch {
          appendLog(`Paylaşım linki: ${s.share_url}`);
        }
      });

      const komaBtn = document.createElement("select");
      komaBtn.className = "lib-quality-select";
      komaBtn.title = "Makam seçip 53-Koma STFT uygula (3. Makam Stüdyosu ile senkron)";
      komaBtn.setAttribute("aria-label", "Makama Çek");
      const komaPh = document.createElement("option");
      komaPh.value = "";
      komaPh.textContent = "🎛️ Makama Çek";
      komaPh.selected = true;
      komaBtn.appendChild(komaPh);
      const komaChoices = [
        ["rast", "Rast"],
        ["ussak", "Uşşak"],
        ["huzzam", "Hüzzam"],
        ["hicaz", "Hicaz"],
        ["segah", "Segâh"],
        ["nihavent", "Nihavent"],
        ["huseyni", "Hüseynî"],
        ["kurdi", "Kürdî"],
      ];
      for (const [mId, mName] of komaChoices) {
        const opt = document.createElement("option");
        opt.value = mId;
        opt.textContent = `🎛️ ${mName}`;
        komaBtn.appendChild(opt);
      }
      komaBtn.addEventListener("change", async () => {
        const chosenMakam = komaBtn.value;
        if (!chosenMakam) return;
        const chosenLabel = komaChoices.find(([id]) => id === chosenMakam)?.[1] || chosenMakam;
        // 3. 53-Koma Stüdyosu seçimini de senkronla
        if ($("makamSelect")) {
          $("makamSelect").value = chosenMakam;
          renderMakamKeyboard();
        }
        // Aynı makamın süresi dolmamış (30 dk) dosyası zaten varsa yeniden STFT çalıştırma!
        const nowSec = Date.now() / 1000;
        const existingVar = (s.koma_variants || []).find((v) => {
          const vm = String(v.makam_id || "").toLowerCase();
          const same = vm === chosenMakam || (vm === "nihavend" && chosenMakam === "nihavent");
          return same && (!v.expires_at || v.expires_at > nowSec) && v.share_url;
        });
        if (existingVar) {
          komaBtn.value = "";
          lastKomaUrl = existingVar.share_url;
          playStudioTrack({
            url: existingVar.share_url,
            title: `${s.title || "Parça"} (${existingVar.makam_name || chosenLabel})`,
            subtitle: "53-Koma (Mevcut Sürüm)",
            cover: s.cover_url || "./assets/werrozan-logo.svg",
            downloadName: `${(s.title || "werrozan").replace(/\s+/g, "_")}-${existingVar.makam_id}.flac`,
          });
          appendLog(`✓ "${s.title}" için ${chosenLabel} sürümü zaten mevcut (30 dk süresi dolmadan yeniden işlenmedi) — doğrudan çalınıyor.`);
          return;
        }
        komaBtn.disabled = true;
        komaPh.textContent = `⏳ ${chosenLabel}...`;
        komaBtn.value = "";
        try {
          await quantizeSongById(s.id);
        } catch (err) {
          appendLog(`Koma hatası: ${err.message}`);
        } finally {
          komaBtn.disabled = false;
          komaPh.textContent = "🎛️ Makama Çek";
          komaBtn.value = "";
        }
      });

      const loadBtn = document.createElement("button");
      loadBtn.type = "button";
      loadBtn.className = "lib-btn";
      loadBtn.textContent = "📥 Atölyeye Yükle";
      loadBtn.title = "Bu parçanın istem, söz, stil ve ayarlarını 1. Şarkı & Söz Atölyesi'ne geri yükler";
      loadBtn.addEventListener("click", async () => {
        if ($("ideaInput")) {
          $("ideaInput").value = s.prompt_description || "";
          updateIdeaCharCount();
        }
        if ($("localeSelect") && s.locale) $("localeSelect").value = s.locale;
        if ($("durationInput") && s.duration_seconds) $("durationInput").value = Math.round(s.duration_seconds);
        if ($("cotSelect") && s.cot) $("cotSelect").value = s.cot;
        if ($("stepsSelect") && s.steps) $("stepsSelect").value = String(s.steps);
        if ($("titleInput")) $("titleInput").value = s.title || "";
        if ($("styleInput")) $("styleInput").value = s.style || "";
        if ($("lyricsInput")) $("lyricsInput").value = s.lyrics || "";
        $("ideaInput")?.scrollIntoView({ behavior: "smooth", block: "center" });
        await handleKararWerr();
        appendLog(`Atölyeye yüklendi: "${s.title || "Parça"}" (${s.steps || 16} adım, ${Math.round(s.duration_seconds || 0)} sn) — söz/ayar değiştirmeden doğrudan 32 adımda yeniden üretebilirsiniz.`);
      });

      const qualitySel = document.createElement("select");
      qualitySel.className = "lib-quality-select";
      qualitySel.title = "Aynı melodi ve vokalle ses kalitesini (NAR Steps) yükselt veya düşür";
      qualitySel.setAttribute("aria-label", "Kalite değiştir");
      const phOpt = document.createElement("option");
      phOpt.value = "";
      phOpt.textContent = "⚡ Kalite Değiştir";
      phOpt.selected = true;
      qualitySel.appendChild(phOpt);
      const allLevels = [
        { v: 10, label: "10 · Taslak" },
        { v: 16, label: "16 · Dengeli" },
        { v: 32, label: "32 · Stüdyo" },
      ];
      for (const lv of allLevels) {
        if (lv.v === stNum) continue;
        const opt = document.createElement("option");
        opt.value = String(lv.v);
        opt.textContent = `${lv.v > stNum ? "↑" : "↓"} ${lv.label}`;
        qualitySel.appendChild(opt);
      }
      qualitySel.addEventListener("change", async () => {
        const targetSteps = Number(qualitySel.value);
        if (!targetSteps) return;
        const dir = targetSteps > stNum ? "yükseltmek" : "düşürmek";
        const ok = window.confirm(
          `"${s.title || "Bu parça"}" için birebir aynı melodi ve vokalle kaliteyi ${stNum} → ${targetSteps} (NAR) olarak ${dir} üzere üretim kuyruğuna göndermek istiyor musunuz?\n\n(Not: Normal üretim süreci uygulanır; kuyrukta aktif işiniz varsa gönderilemez. Aşama 1 atlandığı için normal sürenin ~%25'inde tamamlanır.)`
        );
        if (!ok) {
          qualitySel.value = "";
          return;
        }
        qualitySel.disabled = true;
        try {
          const job = await apiFetch(`/v1/user/library/${encodeURIComponent(s.id)}/rerender-steps`, {
            method: "POST",
            json: { steps: targetSteps },
          });
          appendLog(`Kalite güncelleme kuyruğa alındı: "${s.title}" (${stNum} → ${targetSteps} NAR, iş=${job.id || "?"}).`);
          if (job?.id) startJobPolling(job.id);
          await refreshUserLibrary();
          await refreshGlobalQueue();
          $("globalQueueCard")?.scrollIntoView({ behavior: "smooth", block: "nearest" });
        } catch (err) {
          appendLog(`Kalite güncelleme uyarısı: ${err.message}`);
          window.alert(err.message);
        } finally {
          qualitySel.disabled = false;
          qualitySel.value = "";
        }
      });

      const delBtn = document.createElement("button");
      delBtn.type = "button";
      delBtn.className = "lib-btn del";
      delBtn.textContent = "🗑";
      delBtn.title = "Parçayı Sil";
      delBtn.addEventListener("click", async () => {
        delBtn.disabled = true;
        try {
          await apiFetch(`/v1/user/library/${s.id}`, { method: "DELETE" });
          await refreshUserLibrary();
        } catch (err) {
          appendLog(`Silme hatası: ${err.message}`);
          delBtn.disabled = false;
        }
      });

      acts.append(playBtn, dlA, copyBtn, komaBtn, loadBtn, qualitySel, delBtn);
      head.append(info, acts);
      main.append(coverSlot, head);
      item.appendChild(main);

      const variants = s.koma_variants || [];
      if (variants.length > 0) {
        const kRow = document.createElement("div");
        kRow.className = "lib-koma-row";
        for (const v of variants) {
          const chip = document.createElement("div");
          chip.className = "lib-koma-chip";
          const lbl = document.createElement("span");
          const minsLeft = v.expires_at ? Math.max(0, Math.ceil((v.expires_at - Date.now() / 1000) / 60)) : null;
          lbl.textContent = `${v.makam_name} (%${Math.round((v.strength || 0) * 100)})${minsLeft !== null ? ` · ${minsLeft}dk` : ""}`;

          const kp = document.createElement("button");
          kp.type = "button";
          kp.className = "js-play-btn";
          kp.dataset.playUrl = v.share_url || "";
          kp.dataset.compact = "1";
          kp.title = "Dinle / Duraklat";
          kp.textContent = "▶";
          kp.addEventListener("click", () => {
            lastKomaUrl = v.share_url;
            playStudioTrack({
              url: v.share_url,
              title: `${s.title || "Parça"} (${v.makam_name})`,
              subtitle: "53-Koma Makam Sürümü",
              cover: s.cover_url || "./assets/werrozan-logo.svg",
              downloadName: `werrozan-${v.makam_id}.flac`,
            });
          });

          const kd = document.createElement("a");
          kd.href = v.share_url;
          kd.download = `werrozan-${v.makam_id}.flac`;
          kd.title = "İndir";
          kd.textContent = "⬇";

          const kc = document.createElement("button");
          kc.type = "button";
          kc.title = "Linki Kopyala";
          kc.textContent = "🔗";
          kc.addEventListener("click", async () => {
            try {
              await navigator.clipboard.writeText(v.share_url);
              kc.textContent = "✓";
              setTimeout(() => { kc.textContent = "🔗"; }, 1500);
            } catch {
              appendLog(`Koma linki: ${v.share_url}`);
            }
          });

          chip.append(lbl, kp, kd, kc);
          kRow.appendChild(chip);
        }
        item.appendChild(kRow);
      }
      listEl.appendChild(item);
    }
    updateStudioPlayingUI();
  } catch (err) {
    appendLog(`Kitaplık yükleme uyarısı: ${err.message}`);
  }
}

// ---------- Boot ----------

setInterval(() => {
  if (document.visibilityState !== "hidden") refreshGlobalQueue();
}, 15000);
initConnectionInputs();
initStudioGlobalPlayer();
refreshGlobalQueue();

function updateIdeaCharCount() {
  const el = $("ideaInput");
  const cnt = $("ideaCharCount");
  if (el && cnt) cnt.textContent = `${el.value.length} / 4000`;
}
updateIdeaCharCount();
$("ideaInput")?.addEventListener("input", updateIdeaCharCount);
renderMakamKeyboard();
try {
  const rawDraft = localStorage.getItem("werrozan_discover_draft");
  if (rawDraft && new URLSearchParams(window.location.search).get("from") === "discover") {
    const d = JSON.parse(rawDraft);
    localStorage.removeItem("werrozan_discover_draft");
    if (d.prompt_description) $("ideaInput").value = d.prompt_description;
    if (d.locale) $("localeSelect").value = d.locale;
    if (d.duration_seconds) $("durationInput").value = d.duration_seconds;
    if (d.cot) $("cotSelect").value = d.cot;
    if (d.steps) $("stepsSelect").value = String(d.steps);
    if (d.title) $("titleInput").value = d.title;
    if (d.style) $("styleInput").value = d.style;
    if (d.lyrics) $("lyricsInput").value = d.lyrics;
    updateIdeaCharCount();
    appendLog(`Keşfet vitrininden atölyeye yüklendi: "${d.title || "Remix"}"`);
    requestAnimationFrame(() => {
      setTimeout(() => {
        const wc = $("workshopCard");
        if (wc) {
          wc.scrollIntoView({ behavior: "smooth", block: "start" });
          wc.classList.remove("is-focus-pulse");
          void wc.offsetWidth;
          wc.classList.add("is-focus-pulse");
        }
        $("ideaInput")?.focus({ preventScroll: true });
      }, 120);
    });
  }
} catch {}
// ---------- 🧙‍♂️ WerrOzan Büyücü (4 Adımlı Rehberli Üretim Sihirbazı) ----------

let wizCurrentStep = 1;

function showWizNotice(msg = "", cls = "err") {
  const box = $("wizNotice");
  if (!box) return;
  if (!msg) {
    box.style.display = "none";
    return;
  }
  box.style.display = "grid";
  box.className = `notice-box ${cls}`;
  box.textContent = msg;
}

function setWizardStep(step) {
  wizCurrentStep = step;
  showWizNotice("");
  for (let i = 1; i <= 4; i++) {
    const pane = $(`wizPane${i}`);
    const pill = $(`wizPill${i}`);
    if (pane) pane.classList.toggle("active", i === step);
    if (pill) {
      pill.classList.toggle("active", i === step);
      pill.classList.toggle("done", i < step);
    }
  }
  if (step === 2) {
    $("wizTitleInput").value = $("titleInput").value;
    $("wizStyleInput").value = $("styleInput").value;
    $("wizLyricsInput").value = $("lyricsInput").value;
  } else if (step === 3) {
    $("wizLyricsFixInput").value = $("lyricsInput").value;
    $("wizWarnBox").textContent = $("gdWarningBox").textContent || "⚠ Güneş Dili uyarısı tespit edildi.";
  } else if (step === 4) {
    const okTxt = $("gdWarningBox").textContent || "✓ Üretime hazır!";
    const okBox = $("wizOkBox");
    okBox.className = lastRenderedWarnings.length ? "notice-box warn" : "notice-box ok";
    okBox.textContent = okTxt;
    const sum = $("wizSummaryBox");
    const dur = $("durationInput").value || "90";
    const stepsTxt = $("stepsSelect").options[$("stepsSelect").selectedIndex]?.textContent || $("stepsSelect").value;
    sum.innerHTML = "";
    const rows = [
      ["Parça Adı", $("titleInput").value || "İsimsiz Parça"],
      ["Hedef Süre", `${dur} sn (+45/60 sn Akıllı EOS Koruması)`],
      ["Ses Kalitesi", stepsTxt],
      ["Stil Etiketleri", $("styleInput").value || "—"],
    ];
    for (const [k, v] of rows) {
      const div = document.createElement("div");
      const b = document.createElement("b");
      b.textContent = `${k}: `;
      div.append(b, document.createTextNode(v));
      sum.appendChild(div);
    }
    const pre = document.createElement("pre");
    pre.className = "lm-lyrics";
    pre.style.maxHeight = "180px";
    pre.textContent = $("lyricsInput").value || "";
    sum.appendChild(pre);
  }
}

function openWizard() {
  if (!$("apiKey").value.trim()) {
    appendLog("Büyücü yalnızca onaylı API anahtarıyla giriş yapan üyeler içindir.");
    return;
  }
  $("wizIdeaInput").value = $("ideaInput").value;
  $("wizLocaleSelect").value = $("localeSelect").value;
  $("wizDurationInput").value = $("durationInput").value || "90";
  $("wizStepsSelect").value = $("stepsSelect").value || "10";
  syncWizDurPills($("wizDurationInput").value);
  setWizardStep(1);
  $("wizardModal").style.display = "flex";
}

function closeWizard() {
  if ($("wizardModal")) $("wizardModal").style.display = "none";
}

function syncWizDurPills(val) {
  const num = String(Number(val) || 90);
  document.querySelectorAll("[data-wiz-dur]").forEach((p) => {
    p.classList.toggle("is-active", p.dataset.wizDur === num);
  });
}

function initWizard() {
  $("btnOpenWizard")?.addEventListener("click", openWizard);
  $("wizClose")?.addEventListener("click", closeWizard);
  $("wizardModal")?.addEventListener("click", (e) => {
    if (e.target === $("wizardModal")) closeWizard();
  });

  document.querySelectorAll("[data-wiz-dur]").forEach((pill) => {
    pill.addEventListener("click", () => {
      $("wizDurationInput").value = pill.dataset.wizDur;
      syncWizDurPills(pill.dataset.wizDur);
    });
  });
  $("wizDurationInput")?.addEventListener("input", () => syncWizDurPills($("wizDurationInput").value));

  $("wizBack2")?.addEventListener("click", () => setWizardStep(1));
  $("wizBack3")?.addEventListener("click", () => setWizardStep(2));
  $("wizBack4")?.addEventListener("click", () => setWizardStep(lastRenderedWarnings.length ? 3 : 2));
  $("wizSkipToRender")?.addEventListener("click", () => {
    $("lyricsInput").value = $("wizLyricsFixInput").value;
    runLocalGate();
    setWizardStep(4);
  });

  $("wizBtnSozWerr")?.addEventListener("click", async () => {
    const idea = $("wizIdeaInput").value.trim();
    if (!idea) {
      showWizNotice("Lütfen önce şarkı fikrinizi kısaca yazın.", "warn");
      return;
    }
    $("ideaInput").value = idea;
    $("localeSelect").value = $("wizLocaleSelect").value;
    $("durationInput").value = $("wizDurationInput").value || "90";
    $("stepsSelect").value = $("wizStepsSelect").value || "10";
    updateIdeaCharCount();

    const btn = $("wizBtnSozWerr");
    const orig = btn.textContent;
    btn.disabled = true;
    btn.textContent = "⏳ System 2: Gemini taslak hazırlıyor...";
    showWizNotice("");
    try {
      await handleCevapWerr();
      setWizardStep(2);
    } catch (err) {
      showWizNotice(`Taslak hazırlanamadı: ${err.message}`, "err");
    } finally {
      btn.disabled = false;
      btn.textContent = orig;
    }
  });

  $("wizBtnKararWerr")?.addEventListener("click", async () => {
    $("titleInput").value = $("wizTitleInput").value.trim() || "İsimsiz Parça";
    $("styleInput").value = $("wizStyleInput").value.trim();
    $("lyricsInput").value = $("wizLyricsInput").value;

    const btn = $("wizBtnKararWerr");
    const orig = btn.textContent;
    btn.disabled = true;
    btn.textContent = "⏳ Güneş Dili denetleniyor...";
    showWizNotice("");
    try {
      await handleKararWerr();
      if (lastRenderedWarnings.length > 0) {
        setWizardStep(3);
      } else {
        setWizardStep(4);
      }
    } catch (err) {
      showWizNotice(`Karar Werr hatası: ${err.message}`, "err");
    } finally {
      btn.disabled = false;
      btn.textContent = orig;
    }
  });

  $("wizBtnElWerr")?.addEventListener("click", async () => {
    $("lyricsInput").value = $("wizLyricsFixInput").value;
    const btn = $("wizBtnElWerr");
    const orig = btn.textContent;
    btn.disabled = true;
    btn.textContent = "⏳ El Werr sözleri düzeltiyor...";
    showWizNotice("");
    try {
      await handleAdaptLyrics();
      $("wizLyricsFixInput").value = $("lyricsInput").value;
      if (lastRenderedWarnings.length === 0) {
        setWizardStep(4);
      } else {
        $("wizWarnBox").textContent = $("gdWarningBox").textContent;
        showWizNotice("Bazı küçük uyarılar kaldı; tekrar El Werr deneyebilir veya Son Onaya Geçebilirsiniz.", "warn");
      }
    } catch (err) {
      showWizNotice(`El Werr hatası: ${err.message}`, "err");
    } finally {
      btn.disabled = false;
      btn.textContent = orig;
    }
  });

  $("wizBtnSarkiWerr")?.addEventListener("click", async () => {
    const btn = $("wizBtnSarkiWerr");
    const orig = btn.textContent;
    btn.disabled = true;
    btn.textContent = "⏳ Kuyruğa gönderiliyor...";
    try {
      closeWizard();
      await handleRenderSong();
      $("globalQueueCard")?.scrollIntoView({ behavior: "smooth", block: "nearest" });
    } finally {
      btn.disabled = false;
      btn.textContent = orig;
    }
  });
}

runLocalGate();
initWizard();
ensureRecaptcha();

if ($("apiKey").value.trim()) {
  testConnection();
} else if (localStorage.getItem(LS_REQ)) {
  checkRegistrationStatus();
}

$("tabSignup").addEventListener("click", () => switchAuthTab("signup"));
$("tabKey").addEventListener("click", () => switchAuthTab("key"));
$("tabRecover")?.addEventListener("click", () => switchAuthTab("recover"));
$("btnRecoverKey")?.addEventListener("click", handleRecoverKey);
$("btnRotateKey")?.addEventListener("click", handleRotateKey);
$("emailNotifCheck")?.addEventListener("change", handleEmailNotifToggle);
$("btnRegister").addEventListener("click", handleRegister);
$("btnCheckRegStatus").addEventListener("click", checkRegistrationStatus);
$("btnLogout").addEventListener("click", handleLogout);
$("btnAdminRefresh").addEventListener("click", refreshAdminUsers);
$("btnAdminOutbox").addEventListener("click", toggleAdminOutbox);

$("btnConnect").addEventListener("click", testConnection);
$("btnCevapWerr").addEventListener("click", handleCevapWerr);
$("btnKararWerr").addEventListener("click", handleKararWerr);
$("btnAdaptLyrics")?.addEventListener("click", handleAdaptLyrics);
$("btnRenderSong").addEventListener("click", handleRenderSong);
$("btnPlayScale").addEventListener("click", () => playMakamScale($("makamSelect").value));
$("btnExportKomaMidi").addEventListener("click", handleExportKomaMidi);
$("btnSendToKoma").addEventListener("click", () => {
  if (lastRenderedSongId) {
    quantizeSongById(lastRenderedSongId).catch((e) => appendLog(`Koma hatası: ${e.message}`));
  } else if (lastRenderedBlob) {
    quantizeBlobWithKoma(lastRenderedBlob, lastRenderedExt).catch((e) => appendLog(`Koma hatası: ${e.message}`));
  }
});
$("btnQuantizeFile").addEventListener("click", () => {
  const f = $("komaFileInput").files?.[0];
  if (!f) return appendLog("Önce bir ses dosyası (WAV/FLAC/MP3) seçin.");
  const ext = (f.name.split(".").pop() || "wav").toLowerCase();
  quantizeBlobWithKoma(f, ext).catch((e) => appendLog(`Koma hatası: ${e.message}`));
});

$("makamSelect").addEventListener("change", renderMakamKeyboard);
$("komaStrength").addEventListener("input", () => {
  $("komaStrLbl").textContent = `${$("komaStrength").value}%`;
});
["lyricsInput", "durationInput", "seedCx", "seedCy", "seedZoom"].forEach((id) => {
  $(id).addEventListener("input", runLocalGate);
});

$("lightboxClose")?.addEventListener("click", closeCoverLightbox);
$("coverLightbox")?.addEventListener("click", (e) => {
  if (e.target === $("coverLightbox")) closeCoverLightbox();
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    if ($("wizardModal")?.style.display !== "none") closeWizard();
    if ($("coverLightbox")?.style.display !== "none") closeCoverLightbox();
  } else if (e.key === " " && studioPlayingUrl) {
    const tag = (document.activeElement?.tagName || "").toUpperCase();
    if (tag !== "INPUT" && tag !== "TEXTAREA" && tag !== "SELECT" && tag !== "BUTTON" && !document.activeElement?.isContentEditable) {
      e.preventDefault();
      const ga = $("globalAudio");
      if (ga) {
        if (ga.paused) ga.play().catch(() => {});
        else ga.pause();
      }
    }
  }
});
$("lightboxDownload")?.addEventListener("click", async (e) => {
  const dlEl = $("lightboxDownload");
  const url = dlEl?.href;
  if (!url || url === "#") return;
  e.preventDefault();
  try {
    const resp = await fetch(url);
    const blob = await resp.blob();
    const objUrl = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = objUrl;
    a.download = dlEl.download || "werrozan-cover.svg";
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(objUrl), 5000);
  } catch {
    window.open(url, "_blank", "noopener");
  }
});
document.querySelector(".demo-cover")?.addEventListener("click", (e) => {
  const img = e.currentTarget;
  openCoverLightbox(img.src, "Mavi gecelerde yükselir sesim");
});
