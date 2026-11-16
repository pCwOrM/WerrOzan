# WerrOzan — Architecture & Hybrid Deployment Specification

> **Base Upstream:** [`timoncool/YuE2-Studio`](https://github.com/timoncool/YuE2-Studio) (MIT License)  
> **WERR & ITouch Systems Layer:** Procedural Boundary Dynamics ($\mathbb{Z}/9\mathbb{Z}$), Güneş Dili Morpho-Semantic Diction Engine, and 53-Koma Makam Quantization.

---

## 1. Hybrid Topology: GitHub Pages + `lexsrv` Headless Xeon Engine

```text
[ Browser — GitHub Pages (pcworm.github.io/WerrOzan) ]
  ├── React Studio UI (Karar Werr / Cevap Werr actions)
  ├── Güneş Dili JS (motor.js) — Zero-server syllable, vowel harmony & diction gate
  ├── WERR JS (0-SRAM Z/9Z modular resonance & form gate)
  └── 53-Koma / Makam Editor + WebAudio Microtonal Preview
           │
           ▼  HTTPS (Cloudflare Tunnel + Bearer Token + CORS)
[ lexsrv — Headless Xeon Engine (Ubuntu 24.04.5 LTS, VirtualBox Guest) ]
  ├── Rust music-server (API + Gemini 3.8 Flash provider; API key isolated on server)
  ├── yue-server (yue2.cpp CPU / light GGUF, AVX2+FMA)
  └── crates/audio-post (STFT 53-Koma Pitch Quantizer + mastering/FLAC/MP3)
```

---

## 2. `lexsrv` Hardware & VirtualBox Sizing Profile

Observed baseline on `lexsrv` (`Intel(R) Xeon(R) CPU E5-2630 v4 @ 2.20GHz`, `avx2` + `fma` enabled, Ubuntu 24.04.5 LTS, `167 GB` free disk):
- **Minimum Viable (for `light` GGUF CPU profile):** `16 GB RAM`, `8 vCPU` (since `hardware.rs` requires `8.5–9.0 GB` for `light` GGUF alone, and existing services on `lexsrv` use `~2.9 GB`).
- **Recommended Production:** `24 GB RAM`, `12–16 vCPU` (leaves headroom for `audio-post` STFT DSP and concurrent requests with zero swap pressure).

---

## 3. Core Integration Phases

- **Phase 0 (Done):** Local clone of `timoncool/YuE2-Studio` with `upstream` remote, isolated `.env.local` (`gemini-3.8-flash`), and architecture spec.
- **Phase 1:** Install Rust toolchain & `cloudflared` on `lexsrv`, compile `music-server` + `yue-server` headless (`AVX2` + `FMA`), configure `werr-ozan.service`.
- **Phase 2:** Add `gemini-3.8-flash` provider to `crates/music-server/src/assistant.rs` (eliminating 5–12 GB local Gemma GGUF), integrate Güneş Dili diction/syllable alignment and WERR $\mathbb{Z}/9\mathbb{Z}$ form control in `crates/music-core`.
- **Phase 3:** Implement 53-Koma / Makam symbolic pitch-bend (`score/`, `midi.rs`) and post-synthesis STFT Koma Quantizer (`crates/audio-post/src/stft.rs`).
- **Phase 4:** Decouple `app/` from Tauri and deploy static WerrOzan Studio to GitHub Pages (`pcworm.github.io/WerrOzan`).
