// WerrOzan Client-Side Engine:
// 1. 53-Koma / Arel-Ezgi-Uzdilek Makam Theory & WebAudio Microtonal Synthesizer
// 2. WERR Z/9Z (0-Byte VRAM) Procedural Boundary & Section Trajectory
// 3. Güneş Dili Morpho-Semantic Prosody Bridge (via window.MOTOR)

export const CENTS_PER_KOMA = 1200 / 53;

export const MAKAMS = {
  rast: {
    name: "Rast",
    rootName: "Sol (Rast)",
    rootHz: 195.9977,
    komas: [0, 9, 17, 22, 31, 40, 48],
    degrees: ["Rast (0k)", "Dügâh (9k)", "Segâh (17k · -15c)", "Çârgâh (22k)", "Nevâ (31k)", "Hüseynî (40k)", "Eviç (48k · -13c)"],
    desc: "Parlak, dengeli Türk müziği ana makamı; 3. derece Segâh (17k) ve 7. derece Eviç (48k) mikrotonaldir.",
  },
  ussak: {
    name: "Uşşak",
    rootName: "La (Dügâh)",
    rootHz: 220.0,
    komas: [0, 8, 13, 22, 31, 35, 44],
    degrees: ["Dügâh (0k)", "Segâh (8k · -19c)", "Çârgâh (13k)", "Nevâ (22k)", "Hüseynî (31k)", "Acem (35k)", "Gerdâniye (44k)"],
    desc: "Lirik ve içten karakterli makam; 2. derece Segâh 8 koma (~181.1c) basılır.",
  },
  huzzam: {
    name: "Hüzzam",
    rootName: "Si-koma (Segâh)",
    rootHz: 244.274,
    komas: [0, 5, 17, 22, 26, 39, 44],
    degrees: ["Segâh (0k)", "Çârgâh (5k)", "Hicaz (17k)", "Hüseynî (22k)", "Eviç (26k)", "Gerdâniye (39k)", "Tiz Segâh (44k)"],
    desc: "Hüzünlü ve duygusal Segâh ailesi makamı; karakteristik 5-12-5 komalık artık ikili içerir.",
  },
  hicaz: {
    name: "Hicaz",
    rootName: "La (Dügâh)",
    rootHz: 220.0,
    komas: [0, 5, 17, 22, 31, 35, 44],
    degrees: ["Dügâh (0k)", "Dik Kürdî (5k · +13c)", "Nim Hicaz (17k · -15c)", "Nevâ (22k)", "Hüseynî (31k)", "Acem (35k)", "Gerdâniye (44k)"],
    desc: "Anadolu ve Doğu müziğinin simge makamı; 2. ve 3. derece arasında 12 komalık artık ikili vardır.",
  },
  segah: {
    name: "Segâh",
    rootName: "Si-koma (Segâh)",
    rootHz: 244.274,
    komas: [0, 5, 13, 22, 31, 36, 44],
    degrees: ["Segâh (0k)", "Çârgâh (5k)", "Nevâ (13k)", "Hüseynî (22k)", "Eviç (31k)", "Gerdâniye (36k)", "Tiz Segâh (44k)"],
    desc: "Segâh beşlisi ve Uşşak dörtlüsü birleşimli klasik makam.",
  },
  nihavent: {
    name: "Nihavent",
    rootName: "Sol (Rast)",
    rootHz: 195.9977,
    komas: [0, 9, 13, 22, 31, 35, 48],
    degrees: ["Rast (0k)", "Dügâh (9k)", "Kürdî (13k)", "Çârgâh (22k)", "Nevâ (31k)", "Nim Hisar (35k)", "Eviç (48k)"],
    desc: "Batı minörüne yakın fakat tam 53-koma aralıklarıyla kurulu parlak makam.",
  },
  huseyni: {
    name: "Hüseynî",
    rootName: "La (Dügâh)",
    rootHz: 220.0,
    komas: [0, 8, 13, 22, 31, 40, 44],
    degrees: ["Dügâh (0k)", "Segâh (8k)", "Çârgâh (13k)", "Nevâ (22k)", "Hüseynî (31k)", "Eviç (40k)", "Gerdâniye (44k)"],
    desc: "Türk halk müziğinde (bozlak ve uzun havalar) yaygın kullanılan makam.",
  },
  kurdi: {
    name: "Kürdî",
    rootName: "La (Dügâh)",
    rootHz: 220.0,
    komas: [0, 4, 13, 22, 31, 35, 44],
    degrees: ["Dügâh (0k)", "Kürdî (4k · -9c)", "Çârgâh (13k)", "Nevâ (22k)", "Hüseynî (31k)", "Acem (35k)", "Gerdâniye (44k)"],
    desc: "4 komalık bakiye ikilisiyle modern Türk pop ve halk ezgilerinde çok yaygın kullanılan makam.",
  },
};

export function komaToHz(rootHz, komas) {
  return rootHz * Math.pow(2, komas / 53);
}

let audioCtx = null;
function ensureAudioCtx() {
  if (!audioCtx) {
    const Ctx = window.AudioContext || window.webkitAudioContext;
    audioCtx = new Ctx();
  }
  if (audioCtx.state === "suspended") audioCtx.resume();
  return audioCtx;
}

export function playToneHz(hz, durationSec = 0.65, delaySec = 0) {
  const ctx = ensureAudioCtx();
  const start = ctx.currentTime + delaySec;
  const osc1 = ctx.createOscillator();
  const osc2 = ctx.createOscillator();
  const filter = ctx.createBiquadFilter();
  const gain = ctx.createGain();

  // Warm plucked-oud / saz-like harmonic timbre
  osc1.type = "triangle";
  osc2.type = "sawtooth";
  osc1.frequency.setValueAtTime(hz, start);
  osc2.frequency.setValueAtTime(hz * 2, start);

  filter.type = "lowpass";
  filter.frequency.setValueAtTime(Math.min(4800, hz * 5.5), start);
  filter.frequency.exponentialRampToValueAtTime(Math.max(400, hz * 1.8), start + durationSec);

  gain.gain.setValueAtTime(0.0001, start);
  gain.gain.linearRampToValueAtTime(0.22, start + 0.018);
  gain.gain.exponentialRampToValueAtTime(0.0001, start + durationSec);

  const subGain = ctx.createGain();
  subGain.gain.setValueAtTime(0.25, start);

  osc1.connect(filter);
  osc2.connect(subGain);
  subGain.connect(filter);
  filter.connect(gain);
  gain.connect(ctx.destination);

  osc1.start(start);
  osc2.start(start);
  osc1.stop(start + durationSec + 0.03);
  osc2.stop(start + durationSec + 0.03);
}

export function playMakamScale(makamKey, rootOverrideHz = null) {
  const m = MAKAMS[makamKey] || MAKAMS.rast;
  const root = rootOverrideHz > 20 ? rootOverrideHz : m.rootHz;
  const seq = [...m.komas, 53];
  seq.forEach((k, idx) => {
    playToneHz(komaToHz(root, k), 0.55, idx * 0.28);
  });
}

// ---------- Güneş Dili + WERR Local Browser Analyzer ----------

const TR_VOWELS = new Set(["a", "e", "ı", "i", "o", "ö", "u", "ü", "â", "î", "û"]);
const BACK_VOWELS = new Set(["a", "ı", "o", "u", "â", "û"]);
const FRONT_VOWELS = new Set(["e", "i", "ö", "ü", "î"]);

export function trLower(s) {
  return s.replace(/İ/g, "i").replace(/I/g, "ı").toLowerCase();
}

export function countSyllables(line) {
  let c = 0;
  for (const ch of trLower(line)) {
    if (TR_VOWELS.has(ch)) c++;
  }
  return c;
}

export function wordHarmonyOk(word) {
  let b = false, f = false;
  for (const ch of trLower(word)) {
    if (BACK_VOWELS.has(ch)) b = true;
    else if (FRONT_VOWELS.has(ch)) f = true;
  }
  return !(b && f);
}

export function werrProbe(seed, u, v, maxIter = 72) {
  const span = 1.5 / Math.max(1, seed.zoom);
  const cr = seed.cx + Math.max(-1, Math.min(1, u)) * span;
  const ci = seed.cy + Math.max(-1, Math.min(1, v)) * span;
  let zr = 0, zi = 0, iter = 0, escaped = false;
  while (iter < maxIter) {
    const zr2 = zr * zr, zi2 = zi * zi;
    if (zr2 + zi2 > 4.0) { escaped = true; break; }
    zi = 2 * zr * zi + ci;
    zr = zr2 - zi2 + cr;
    iter++;
  }
  const mag = Math.min(16, Math.hypot(zr, zi));
  const norm = iter / Math.max(1, maxIter);
  const energy = escaped ? Math.min(1, Math.max(0.15, 1 - norm * 0.65)) : Math.min(0.95, Math.max(0.2, 0.35 + mag * 0.25));
  return { iterations: iter, escaped, energy };
}

export function analyzeLocalWerrGunes(lyrics, durationSec = 90, seed = { cx: -0.743643887, cy: 0.131825904, zoom: 120 }) {
  const lines = lyrics.split(/\r?\n/);
  const sections = [];
  let curTag = "[Unlabeled]";
  let curLines = [];
  const flush = () => {
    if (curTag === "[Unlabeled]" && curLines.length === 0) return;
    const counts = curLines.map((l) => l.syllables).filter((x) => x > 0);
    const minS = counts.length ? Math.min(...counts) : 0;
    const maxS = counts.length ? Math.max(...counts) : 0;
    sections.push({
      tag: curTag,
      lines: curLines,
      totalSyllables: counts.reduce((a, b) => a + b, 0),
      minSyllables: minS,
      maxSyllables: maxS,
      balanced: maxS - minS <= 4,
    });
    curLines = [];
  };

  let totalSyllables = 0, totalWords = 0, harmonyOkWords = 0;
  const morphTokens = [];
  const motor = window.MOTOR || null;

  for (const raw of lines) {
    const t = raw.trim();
    if (!t) continue;
    if (t.startsWith("[") && t.endsWith("]") && t.length >= 3) {
      flush();
      curTag = t;
      continue;
    }
    const syl = countSyllables(t);
    totalSyllables += syl;
    const words = t.split(/\s+/).map((w) => w.replace(/[^a-zA-ZçğıöşüÇĞİÖŞÜâîû]/g, "")).filter(Boolean);
    for (const w of words) {
      totalWords++;
      if (wordHarmonyOk(w)) harmonyOkWords++;
      if (motor && morphTokens.length < 18) {
        try {
          const parsed = motor.tokenize(trLower(w));
          if (parsed && parsed.tokenlar && parsed.tokenlar.length) {
            const parts = parsed.tokenlar.map((tk) => tk[0]);
            const gd = motor.turkceden(parsed);
            morphTokens.push({ word: w, parts, gd: gd ? gd.kelime : null });
          }
        } catch {
          // ignore non-Turkish token
        }
      }
    }
    curLines.push({ text: t, syllables: syl });
  }
  flush();

  const totalSec = Math.max(1, sections.length);
  const werrSections = sections.map((sec, idx) => {
    const p = werrProbe(seed, (idx / totalSec) * 2 - 1, ((sec.totalSyllables % 32) / 16) - 1, 72);
    const low = sec.tag.toLowerCase();
    let residue = 3;
    let energy = Math.min(0.75, 0.32 + p.energy * 0.30);
    if (low.includes("chorus") && !low.includes("pre")) {
      residue = 0;
      energy = Math.min(1.0, 0.78 + p.energy * 0.22);
    } else if (low.includes("pre-chorus") || low.includes("bridge")) {
      residue = 6;
      energy = Math.min(0.90, 0.55 + p.energy * 0.25);
    }
    return { tag: sec.tag, residue, energy: Number(energy.toFixed(3)), balanced: sec.balanced, totalSyllables: sec.totalSyllables };
  });

  // Compute 4x4 FNS boundary grid using MOTOR.mfns when loaded
  const mfnsGrid = motor && typeof motor.mfns === "function" ? motor.mfns(seed.cx, seed.cy, seed.zoom, 48, 48, 4) : [];

  return {
    sections,
    werrSections,
    totalSyllables,
    totalWords,
    harmonyRatio: totalWords ? Number((harmonyOkWords / totalWords).toFixed(3)) : 1.0,
    wordsPerSec: Number((totalWords / Math.max(1, durationSec)).toFixed(2)),
    morphTokens,
    mfnsGrid,
    motorReady: Boolean(motor),
  };
}
