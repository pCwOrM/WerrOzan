# WerrOzan — Architecture & Hybrid Deployment Specification

> **Base Upstream:** [`timoncool/YuE2-Studio`](https://github.com/timoncool/YuE2-Studio) (MIT License)  
> **Repository:** [`pCwOrM/WerrOzan`](https://github.com/pCwOrM/WerrOzan)  
> **WERR & ITouch Systems Layer:** Procedural Boundary Dynamics ($\mathbb{Z}/9\mathbb{Z}$), Güneş Dili Morpho-Semantic Diction Engine, 53-Koma Makam Quantization, and Gemini 3.8 Flash Cloud Assistant.

---

## 1. Hybrid Topology: GitHub Pages (`ozan.werr.io`) + `lexsrv` (`ozan-api.werr.io`)

```text
[ Browser — GitHub Pages (https://ozan.werr.io -> pcworm.github.io) ]
  ├── React Studio UI (Karar Werr / Cevap Werr actions)
  ├── Güneş Dili JS (motor.js) — Zero-server syllable, vowel harmony & diction gate
  ├── WERR JS (0-SRAM Z/9Z modular resonance & form gate)
  └── 53-Koma / Makam Editor + WebAudio Microtonal Preview
           │
           ▼  HTTPS (Cloudflare Strict TLS + Tunnel `werrozan-lexsrv` + Bearer Token)
[ lexsrv — Headless Xeon Engine (https://ozan-api.werr.io -> 127.0.0.1:8791) ]
  ├── PM2 `werrozan-tunnel` (cloudflared 4x QUIC outbound; zero NAT/Firewall holes)
  ├── PM2 `werrozan-server` (Rust `music-server` + Gemini 3.8 Flash + fallback chain)
  ├── `yue-server` (yue2.cpp CPU native AVX2+FMA / `light` Q5_K_M GGUF, ~8.94 GB RAM)
  └── `crates/audio-post` (STFT 53-Koma Pitch Quantizer + mastering/FLAC/MP3)
```

---

## 2. Verified `lexsrv` Hardware & Sizing Profile

- **Host / Guest:** VirtualBox (`oracle`, KVM paravirtualization, `Nested Paging`, `PAE/NX`), Ubuntu 24.04.5 LTS
- **CPU:** 8 vCPU `Intel(R) Xeon(R) CPU E5-2630 v4 @ 2.20GHz` (`avx2`, `fma`, `OMP_NUM_THREADS=8`)
- **Memory & Disk:** `15991 MB` (~16 GB) RAM (`~2.5 GB` baseline used), `167 GB` free disk
- **Pinned `yue2-cpp` Model Profile:** `light` (`Q5_K_M` backbone, `4.30 GB` download, `8.94 GB` peak RAM)
- **Reboot Persistence:** User `crontab` `@reboot /usr/bin/pm2 resurrect`

---

## 3. Implementation Phases

- **Phase 0 (Verified):** Forked `timoncool/YuE2-Studio` → `pCwOrM/WerrOzan`, isolated `.env.local`, documented architecture.
- **Phase 1 (Verified):** Compiled `yue-server` (`AVX2+FMA` native) and `music-server` (release), deployed `werrozan-server` + `werrozan-tunnel` (`ozan-api.werr.io`) under PM2 with `@reboot` persistence, configured `ozan.werr.io` CNAME on Cloudflare (`ssl: strict`), verified all `music-core` / `audio-post` / `music-server` unit tests and auth/origin boundary checks.
- **Phase 2 (Active):**
  1. Add `gemini` provider (`crates/music-server/src/providers/gemini.rs`) using `GEMINI_API_KEY` + `gemini-3.8-flash` (with automatic `503`/`429`/timeout fallback to `gemini-3.7-flash` → `gemini-3.5-flash` → `gemini-flash-latest`), zero local Gemma GGUF RAM.
  2. Add `WERROZAN_ALLOWED_ORIGINS` CORS & Origin allowlist in `remote.rs` for `https://ozan.werr.io`.
  3. Add **WERR $\mathbb{Z}/9\mathbb{Z}$ Modular Resonance & Form Gate** (`crates/music-core/src/werr.rs`) and **Güneş Dili Morpho-Semantic Diction & Syllable Meter Gate** (`crates/music-server/src/gunes_dili.rs`).
- **Phase 3:** Implement 53-Koma / Makam symbolic pitch-bend (`score/`, `midi.rs`) and post-synthesis STFT Koma Quantizer (`crates/audio-post/src/stft.rs`).
- **Phase 4:** Decouple `app/` from Tauri and deploy static WerrOzan Studio to GitHub Pages (`https://ozan.werr.io`).
