import {
  MAKAMS,
  analyzeLocalWerrGunes,
  komaToHz,
  playMakamScale,
  playToneHz,
} from "./werr_koma.js";

const $ = (id) => document.getElementById(id);

const LS_API = "werrozan_api_base";
const LS_KEY = "werrozan_api_key";

let lastRenderedBlob = null;
let lastRenderedExt = "flac";
let activePollTimer = null;

function initConnectionInputs() {
  const savedBase = localStorage.getItem(LS_API);
  const savedKey = localStorage.getItem(LS_KEY);
  const params = new URLSearchParams(window.location.search);
  if (params.get("api")) $("apiBase").value = params.get("api");
  else if (savedBase) $("apiBase").value = savedBase;
  if (savedKey) $("apiKey").value = savedKey;
}

function getApiConfig() {
  const base = $("apiBase").value.trim().replace(/\/+$/, "");
  const key = $("apiKey").value.trim();
  localStorage.setItem(LS_API, base);
  if (key) localStorage.setItem(LS_KEY, key);
  return { base, key };
}

async function apiFetch(path, options = {}) {
  const { base, key } = getApiConfig();
  if (!key) throw new Error("Lütfen önce Studio Access Key girin (veya Bağlan'a basın).");
  const headers = {
    Authorization: `Bearer ${key}`,
    ...(options.headers || {}),
  };
  if (options.json !== undefined) {
    headers["Content-Type"] = "application/json";
  }
  const resp = await fetch(`${base}${path}`, {
    method: options.method || (options.json !== undefined ? "POST" : "GET"),
    headers,
    body: options.json !== undefined ? JSON.stringify(options.json) : options.body,
  });
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
  $("motorPill").textContent = local.motorReady ? "motor.js: Aktif (0 B VRAM)" : "motor.js: Yüklenemedi";
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

  if (gd?.warnings?.length) {
    appendLog(`Güneş Dili Uyarısı: ${gd.warnings.join(" | ")}`);
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

// ---------- Server Actions ----------

async function testConnection() {
  const pill = $("connPill");
  pill.textContent = "Bağlanıyor...";
  pill.className = "pill warn";
  try {
    const [health, wz, res] = await Promise.all([
      apiFetch("/health"),
      apiFetch("/v1/werrozan/status"),
      apiFetch("/v1/system/resources"),
    ]);
    const r = res.resources || {};
    const engineOk = health.music_engine?.reachable;
    pill.textContent = engineOk ? "Bağlı · YuE2 Aktif" : "Bağlı · Motor Beklemede";
    pill.className = `pill ${engineOk ? "ok" : "warn"}`;
    $("hwPill").textContent = `Xeon RAM: ${r.ram_used_mb || 0}/${r.ram_total_mb || 0} MB · CPU %${Math.round(r.cpu_percent || 0)}`;
    $("geminiPill").textContent = wz.gemini_ready ? `Gemini: ${wz.gemini_model}` : "Gemini: Kapalı";
    $("geminiPill").className = `pill ${wz.gemini_ready ? "ok" : "warn"}`;
    appendLog(`Sunucu bağlantısı doğrulandı (Faz ${wz.phase}, Koma: ${wz.koma_engine}, YuE2 reachable=${engineOk}).`);
  } catch (err) {
    pill.textContent = "Bağlantı Hatası";
    pill.className = "pill err";
    appendLog(`Bağlantı hatası: ${err.message}`);
  }
}

async function handleCevapWerr() {
  const btn = $("btnCevapWerr");
  btn.disabled = true;
  appendLog("Cevap Werr: Gemini 3.8 Flash şarkı taslağı hazırlanıyor...");
  try {
    const draft = await apiFetch("/v1/assistant/write", {
      json: {
        target: "all",
        description: $("ideaInput").value.trim(),
        locale: $("localeSelect").value,
        duration_seconds: Number($("durationInput").value) || 90,
      },
    });
    if (draft.title) $("titleInput").value = draft.title;
    if (draft.style) $("styleInput").value = draft.style;
    if (draft.lyrics) $("lyricsInput").value = draft.lyrics;
    if (draft.duration_seconds) $("durationInput").value = Math.min(180, draft.duration_seconds);
    const local = runLocalGate().local;
    if (draft.werrozan) renderAnalysisUI(local, draft.werrozan);
    appendLog(`Cevap Werr tamamlandı: "${draft.title || "Taslak"}" (${draft.werrozan?.gunes_dili?.total_syllables ?? "?"} hece).`);
  } catch (err) {
    appendLog(`Cevap Werr hatası: ${err.message}`);
  } finally {
    btn.disabled = false;
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

async function handleRenderSong() {
  const btn = $("btnRenderSong");
  btn.disabled = true;
  if (activePollTimer) clearInterval(activePollTimer);
  try {
    const payload = {
      style: $("styleInput").value.trim(),
      lyrics: $("lyricsInput").value,
      duration_seconds: Number($("durationInput").value) || 30,
      cot: $("cotSelect").value,
    };
    appendLog(`YuE2 Xeon motoruna iş gönderiliyor (${payload.duration_seconds}s, ${payload.cot})...`);
    const job = await apiFetch("/v1/music/jobs", { json: payload });
    appendLog(`İş kuyruğa alındı: id=${job.id} (${job.status})`);

    activePollTimer = setInterval(async () => {
      try {
        const cur = await apiFetch(`/v1/music/jobs/${job.id}`);
        appendLog(`Render durumu [${cur.id}]: ${cur.status} / ${cur.phase} — ${cur.message || ""}`);
        if (cur.status === "completed") {
          clearInterval(activePollTimer);
          activePollTimer = null;
          btn.disabled = false;
          const song = cur.song || cur.songs?.[0];
          if (song?.id) {
            const mediaResp = await apiFetch(`/v1/library/media/${song.id}`, { rawResponse: true });
            const blob = await mediaResp.blob();
            lastRenderedBlob = blob;
            lastRenderedExt = (song.format || "flac").toLowerCase();
            const url = URL.createObjectURL(blob);
            $("renderedAudio").src = url;
            $("downloadRendered").href = url;
            $("downloadRendered").download = `${($("titleInput").value || "werrozan").replace(/\s+/g, "_")}.${lastRenderedExt}`;
            $("downloadRendered").textContent = `⬇ İndir (${lastRenderedExt.toUpperCase()})`;
            $("playerWrap").style.display = "block";
            appendLog(`Tamamlandı! Parça (${song.id}) hazır ve oynatıcıya yüklendi.`);
          }
        } else if (cur.status === "failed" || cur.status === "cancelled") {
          clearInterval(activePollTimer);
          activePollTimer = null;
          btn.disabled = false;
          appendLog(`Render sonlandı (${cur.status}): ${cur.message}`);
        }
      } catch (err) {
        appendLog(`Durum sorgu uyarısı: ${err.message}`);
      }
    }, 4500);
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
  $("komaAudio").src = url;
  $("downloadKomaAudio").href = url;
  $("downloadKomaAudio").download = `werrozan-${makam}.flac`;
  $("komaAudioWrap").style.display = "block";
  appendLog(`53-Koma STFT tamamlandı: ${res.report.peaks_shifted} tepe düzeltildi (ort. ${res.report.mean_cent_correction}c).`);
}

// ---------- Boot ----------

initConnectionInputs();
renderMakamKeyboard();
runLocalGate();

$("btnConnect").addEventListener("click", testConnection);
$("btnCevapWerr").addEventListener("click", handleCevapWerr);
$("btnKararWerr").addEventListener("click", handleKararWerr);
$("btnRenderSong").addEventListener("click", handleRenderSong);
$("btnPlayScale").addEventListener("click", () => playMakamScale($("makamSelect").value));
$("btnExportKomaMidi").addEventListener("click", handleExportKomaMidi);
$("btnSendToKoma").addEventListener("click", () => {
  if (lastRenderedBlob) quantizeBlobWithKoma(lastRenderedBlob, lastRenderedExt).catch((e) => appendLog(`Koma hatası: ${e.message}`));
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
