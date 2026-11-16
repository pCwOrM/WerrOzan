# WerrOzan — Architecture & Hybrid Deployment Specification

> **Base Upstream:** [`timoncool/YuE2-Studio`](https://github.com/timoncool/YuE2-Studio) (MIT License)  
> **Repository:** [`pCwOrM/WerrOzan`](https://github.com/pCwOrM/WerrOzan)  
> **Live Studio:** [`https://ozan.werr.io`](https://ozan.werr.io) · **Headless Engine API:** `https://ozan-api.werr.io`

---

## 1. Hybrid Topology: GitHub Pages (`ozan.werr.io`) + `lexsrv` (`ozan-api.werr.io`)

```text
[ Browser — GitHub Pages (https://ozan.werr.io -> gh-pages) ]
  ├── WerrOzan Web Studio UI (Cevap Werr / Karar Werr / Şarkı Üret)
  ├── Güneş Dili JS (vendor/motor.js + motor_veri.js) — Zero-server morphology & meter
  ├── WERR JS (werr_koma.js) — 0-SRAM Z/9Z modular resonance + MOTOR.mfns probe
  └── 53-Koma / Makam WebAudio Microtonal Synthesizer & Scale Player
           │
           ▼  HTTPS (Cloudflare Strict TLS + Tunnel `werrozan-lexsrv` + Bearer Token)
[ lexsrv — Headless Xeon Engine (https://ozan-api.werr.io -> 127.0.0.1:8791) ]
  ├── PM2 `werrozan-tunnel` (cloudflared 4x QUIC outbound; zero NAT/Firewall holes)
  ├── PM2 `werrozan-server` (Rust `music-server` + Gemini 3.8 Flash fallback chain)
  ├── `yue-server` (yue2.cpp CPU native AVX2+FMA / `light` Q5_K_M GGUF, 4.30 GB)
  └── `crates/audio-post` (Laroche-Dolson Phase-Locked STFT 53-Koma Quantizer + FLAC/WAV)
```

---

## 2. Verified `lexsrv` Hardware & Measured CPU Telemetry

- **Host / Guest:** VirtualBox (`oracle`, KVM paravirtualization, `Nested Paging`, `PAE/NX`), Ubuntu 24.04.5 LTS
- **CPU:** 8 vCPU `Intel(R) Xeon(R) CPU E5-2630 v4 @ 2.20GHz` (`avx2`, `fma`, `OMP_NUM_THREADS=8`)
- **Memory & Disk:** `15991 MB` (~16 GB) RAM (`~2.5 GB` idle, `~4.0 GB` peak during `light` NAR acoustic stage), `167 GB` free disk
- **Pinned `yue2-cpp` Profile:** `light` (`Q5_K_M` backbone, `4.30 GB` installed & verified)
- **Measured CPU Inference Speed (8 vCPU / 4 GGML threads):**
  - AR + Semantic LM (`751` steps for `30s` song): **`113.5 s`** (`151.1 ms/step`)
  - NAR Acoustic Flow-Matching: **`~41.4 s/step`** (`10–16` steps recommended for fast CPU drafts; `32` steps for full final render; upgrading `lexsrv` to `16 vCPU` scales GGML threads from `4` to `8`).
- **Reboot Persistence:** User `crontab` `@reboot /usr/bin/pm2 resurrect`

---

## 3. Completed Implementation Phases

- **Phase 0 (Verified):** Forked `timoncool/YuE2-Studio` → `pCwOrM/WerrOzan`, isolated `.env.local`, architecture documented.
- **Phase 1 (Verified):** Compiled `yue-server` (`AVX2+FMA` native) and `music-server` (release), deployed `werrozan-server` + `werrozan-tunnel` (`ozan-api.werr.io`) under PM2 with `@reboot` persistence, Cloudflare `ssl: strict`.
- **Phase 2 (Verified):** Implemented `crates/music-core/src/werr.rs` (24-byte seed, $\mathbb{Z}/9\mathbb{Z}$), `crates/music-server/src/gunes_dili.rs` (Turkish prosody/vowel harmony/formants), `crates/music-server/src/providers/gemini.rs` (`gemini-3.8-flash` + automatic Flash fallback), and `remote.rs` CORS allowlist for `https://ozan.werr.io`.
- **Phase 3 (Verified):** Implemented `crates/music-core/src/koma.rs` (53-TET / AEU 8 Turkish makams), `crates/audio-post/src/koma.rs` (Laroche-Dolson peak-locked STFT Koma Quantizer), and `crates/music-server/src/score/koma_midi.rs` (14-bit Pitch-Bend MIDI export).
- **Phase 4 (Verified):** Installed `4.30 GB` `light` GGUF weights on `lexsrv` (`yue-server` live & reachable), deployed WerrOzan Hybrid Studio (`web/`) to `gh-pages` at [`https://ozan.werr.io`](https://ozan.werr.io).
