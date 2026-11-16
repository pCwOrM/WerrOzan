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
const LS_REQ = "werrozan_reg_id";

let lastRenderedBlob = null;
let lastRenderedExt = "flac";
let lastRenderedSongId = null;
let activePollTimer = null;

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

// ---------- Membership & Admin Actions ----------

function switchAuthTab(mode) {
  const isSignup = mode === "signup";
  $("tabSignup").className = `tab-btn ${isSignup ? "active" : ""}`;
  $("tabKey").className = `tab-btn ${!isSignup ? "active" : ""}`;
  $("paneSignup").style.display = isSignup ? "grid" : "none";
  $("paneKey").style.display = !isSignup ? "grid" : "none";
}

function showRegNotice(text, cls = "") {
  const box = $("regStatusBox");
  box.style.display = "grid";
  box.className = `notice-box ${cls}`;
  box.textContent = text;
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
    const res = await publicApiFetch("/v1/auth/register", {
      json: { email, name, note },
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
    pill.textContent = engineOk ? "Bağlı · YuE2 Aktif" : "Bağlı · Motor Beklemede";
    pill.className = `pill ${engineOk ? "ok" : "warn"}`;
    $("hwPill").textContent = `Xeon RAM: ${r.ram_used_mb || 0}/${r.ram_total_mb || 0} MB · CPU %${Math.round(r.cpu_percent || 0)}`;
    $("geminiPill").textContent = wz.gemini_ready ? `Gemini: ${wz.gemini_model}` : "Gemini: Kapalı";
    $("geminiPill").className = `pill ${wz.gemini_ready ? "ok" : "warn"}`;
    $("btnLogout").style.display = "inline-block";

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
    switchAuthTab("key");
    refreshUserLibrary();

    appendLog(`Sunucu bağlantısı doğrulandı (Faz ${wz.phase}, Rol: ${me.role}, Koma: ${wz.koma_engine}, YuE2 reachable=${engineOk}).`);
  } catch (err) {
    pill.textContent = "Bağlantı Hatası";
    pill.className = "pill err";
    $("adminCard").style.display = "none";
    appendLog(`Bağlantı hatası: ${err.message}`);
  }
}

async function handleCevapWerr() {
  const btn = $("btnCevapWerr");
  const origLabel = btn.textContent;
  btn.disabled = true;
  btn.textContent = "⏳ Taslak hazırlanıyor...";
  const activeModel = ($("geminiPill")?.textContent || "Gemini").replace(/^Gemini:\s*/i, "");
  appendLog(`Cevap Werr (${activeModel}): şarkı taslağı hazırlanıyor...`);
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
    const msg = err.message === "Failed to fetch"
      ? "Sunucuya ulaşılamadı veya istek zaman aşımına uğradı. Lütfen tekrar deneyin."
      : err.message;
    appendLog(`Cevap Werr hatası: ${msg}`);
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

async function handleRenderSong() {
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
      duration_seconds: Number($("durationInput").value) || 30,
      cot: $("cotSelect").value,
      steps: Number($("stepsSelect").value) || 16,
    };
    appendLog(`YuE2 Xeon kuyruğuna iş gönderiliyor (${payload.duration_seconds}s, ${payload.cot})...`);
    const job = await apiFetch("/v1/music/jobs", { json: payload });
    appendLog(`İş sıraya alındı: id=${job.id} (Sıra #${job.queue_position || 1}, tahmini ~${job.estimated_minutes || "?"} dk).`);
    refreshUserLibrary();

    activePollTimer = setInterval(async () => {
      try {
        const cur = await apiFetch(`/v1/music/jobs/${job.id}`);
        appendLog(`Render durumu [${cur.id}]: ${cur.status} / ${cur.phase} — ${cur.message || ""}`);
        if (cur.status === "completed") {
          clearInterval(activePollTimer);
          activePollTimer = null;
          btn.disabled = false;
          lastRenderedSongId = cur.user_song_id || cur.song?.id || cur.songs?.[0]?.id || null;
          const song = cur.song || cur.songs?.[0];
          if (cur.share_url || song?.id) {
            const mediaUrl = cur.share_url || null;
            if (mediaUrl) {
              lastRenderedBlob = null;
              lastRenderedExt = "flac";
              $("renderedAudio").src = mediaUrl;
              $("downloadRendered").href = mediaUrl;
              $("downloadRendered").download = `${($("titleInput").value || "werrozan").replace(/\s+/g, "_")}.flac`;
              $("downloadRendered").textContent = "⬇ İndir (FLAC)";
              $("playerWrap").style.display = "block";
            } else {
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
            }
            appendLog(`Tamamlandı! Parça (${lastRenderedSongId || song?.id}) Medya Kitaplığınıza kaydedildi.`);
            refreshUserLibrary();
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
  if (shareUrl) {
    $("komaAudio").src = shareUrl;
    $("downloadKomaAudio").href = shareUrl;
  } else if (res.audio_base64) {
    const outRaw = atob(res.audio_base64);
    const outArr = new Uint8Array(outRaw.length);
    for (let i = 0; i < outRaw.length; i++) outArr[i] = outRaw.charCodeAt(i);
    const url = URL.createObjectURL(new Blob([outArr], { type: "audio/flac" }));
    $("komaAudio").src = url;
    $("downloadKomaAudio").href = url;
  }
  $("downloadKomaAudio").download = `werrozan-${makam}.flac`;
  $("komaAudioWrap").style.display = "block";
  appendLog(`53-Koma STFT tamamlandı: ${res.report.peaks_shifted} tepe düzeltildi (ort. ${res.report.mean_cent_correction}c).`);
  refreshUserLibrary();
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
    if (qBanner) {
      if (activeJobs.length > 0) {
        qBanner.style.display = "grid";
        qBanner.className = "notice-box warn";
        qBanner.textContent = activeJobs
          .map((j) => `• ${j.title || j.id} (${j.status}${j.queue_position ? ` · Sıra #${j.queue_position}` : ""}${j.estimated_minutes ? ` · ~${j.estimated_minutes} dk` : ""})`)
          .join("\n");
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

      const head = document.createElement("div");
      head.className = "lib-head";
      const info = document.createElement("div");
      const titleEl = document.createElement("div");
      titleEl.className = "lib-title";
      titleEl.textContent = s.title || "İsimsiz Parça";
      const metaEl = document.createElement("div");
      metaEl.className = "lib-meta";
      metaEl.textContent = `${Math.round(s.duration_seconds || 0)} sn · ${dt} · ${(s.style || "").slice(0, 64)}`;
      info.appendChild(titleEl);
      info.appendChild(metaEl);

      const acts = document.createElement("div");
      acts.className = "lib-acts";

      const playBtn = document.createElement("button");
      playBtn.type = "button";
      playBtn.className = "lib-btn";
      playBtn.textContent = "▶ Dinle";
      playBtn.addEventListener("click", () => {
        lastRenderedSongId = s.id;
        lastRenderedBlob = null;
        $("renderedAudio").src = s.share_url;
        $("downloadRendered").href = s.share_url;
        $("downloadRendered").download = `${(s.title || "werrozan").replace(/\s+/g, "_")}.${s.format || "flac"}`;
        $("playerWrap").style.display = "block";
        $("renderedAudio").play().catch(() => {});
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

      const komaBtn = document.createElement("button");
      komaBtn.type = "button";
      komaBtn.className = "lib-btn koma";
      komaBtn.textContent = "🎛️ Makama Çek";
      komaBtn.addEventListener("click", async () => {
        komaBtn.disabled = true;
        komaBtn.textContent = "⏳...";
        try {
          await quantizeSongById(s.id);
        } catch (err) {
          appendLog(`Koma hatası: ${err.message}`);
        } finally {
          komaBtn.disabled = false;
          komaBtn.textContent = "🎛️ Makama Çek";
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

      acts.append(playBtn, dlA, copyBtn, komaBtn, delBtn);
      head.append(info, acts);
      item.appendChild(head);

      const variants = s.koma_variants || [];
      if (variants.length > 0) {
        const kRow = document.createElement("div");
        kRow.className = "lib-koma-row";
        for (const v of variants) {
          const chip = document.createElement("div");
          chip.className = "lib-koma-chip";
          const lbl = document.createElement("span");
          lbl.textContent = `${v.makam_name} (%${Math.round((v.strength || 0) * 100)})`;

          const kp = document.createElement("button");
          kp.type = "button";
          kp.title = "Dinle";
          kp.textContent = "▶";
          kp.addEventListener("click", () => {
            $("komaAudio").src = v.share_url;
            $("downloadKomaAudio").href = v.share_url;
            $("downloadKomaAudio").download = `werrozan-${v.makam_id}.flac`;
            $("komaAudioWrap").style.display = "block";
            $("komaAudio").play().catch(() => {});
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
  } catch (err) {
    appendLog(`Kitaplık yükleme uyarısı: ${err.message}`);
  }
}

// ---------- Boot ----------

initConnectionInputs();
renderMakamKeyboard();
runLocalGate();

if ($("apiKey").value.trim()) {
  testConnection();
} else if (localStorage.getItem(LS_REQ)) {
  checkRegistrationStatus();
}

$("tabSignup").addEventListener("click", () => switchAuthTab("signup"));
$("tabKey").addEventListener("click", () => switchAuthTab("key"));
$("btnRegister").addEventListener("click", handleRegister);
$("btnCheckRegStatus").addEventListener("click", checkRegistrationStatus);
$("btnLogout").addEventListener("click", handleLogout);
$("btnAdminRefresh").addEventListener("click", refreshAdminUsers);
$("btnAdminOutbox").addEventListener("click", toggleAdminOutbox);

$("btnConnect").addEventListener("click", testConnection);
$("btnCevapWerr").addEventListener("click", handleCevapWerr);
$("btnKararWerr").addEventListener("click", handleKararWerr);
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
