# WerrOzan — Architecture & Hybrid Deployment Specification

> **Base Upstream:** [`timoncool/YuE2-Studio`](https://github.com/timoncool/YuE2-Studio) (MIT License)  
> **Repository:** [`pCwOrM/WerrOzan`](https://github.com/pCwOrM/WerrOzan)  
> **Live Studio:** [`https://ozan.werr.io`](https://ozan.werr.io) · **Headless Engine API:** `https://ozan-api.werr.io`

---

## 1. Hybrid Topology: GitHub Pages (`ozan.werr.io`) + `lexsrv` (`ozan-api.werr.io`)

```text
[ Browser — GitHub Pages (https://ozan.werr.io -> gh-pages) ]
  ├── Membership Gate (Email Signup -> Pending -> Approved `woz_...` Key)
  ├── Public Demo Showcase (`Mavi gecelerde yükselir sesim`, 30s 48kHz FLAC, 4.6 MB)
  ├── WerrOzan Web Studio UI (Cevap Werr / Karar Werr / Şarkı Üret)
  ├── Güneş Dili JS (vendor/motor.js + motor_veri.js) — Zero-server morphology & meter
  ├── WERR JS (werr_koma.js) — 0-SRAM Z/9Z modular resonance + MOTOR.mfns probe
  └── 53-Koma / Makam WebAudio Microtonal Synthesizer & Scale Player
           │
           ▼  HTTPS (Cloudflare Strict TLS + Tunnel `werrozan-lexsrv` + Per-User/Master Key)
[ lexsrv — Headless Xeon Engine (https://ozan-api.werr.io -> 127.0.0.1:8791) ]
  ├── PM2 `werrozan-tunnel` (cloudflared 4x QUIC outbound; zero NAT/Firewall holes)
  ├── PM2 `werrozan-server` (Rust `music-server` + SQLite `membership.sqlite` + Gemini 3.8 Flash)
  ├── `crates/music-server/src/mailer.rs` (2048-bit RSA DKIM `ozan._domainkey.werr.io` + SPF + BCC)
  ├── `yue-server` (yue2.cpp CPU native AVX2+FMA / `light` Q5_K_M GGUF, 4.30 GB)
  └── `crates/audio-post` (Laroche-Dolson Phase-Locked STFT 53-Koma Quantizer + FLAC/WAV)
```

---

## 2. Membership, Admin Approval (`pcworm@pcworm.net`) & BCC (`ask@answerr.me`) Flow

1. **Email Registration (`POST /v1/auth/register`):**
   - Creates a `pending` record in `membership.sqlite` with `key_hash = NULL` (no API key is generated until explicit admin approval).
   - **Email 1 (Applicant Ack):** Sent to `<applicant_email>` (`BCC: ask@answerr.me`) confirming that the application was received and that the API key will be emailed once approved.
   - **Email 2 (Admin Review):** Sent to `pcworm@pcworm.net` containing a 72-hour single-use review link (`https://ozan-api.werr.io/v1/auth/decide?token=...`).
2. **Admin Decision (`GET -> POST /v1/auth/decide` or `/v1/admin/users/{id}/approve`):**
   - `GET /v1/auth/decide` is strictly read-only so email link scanners cannot accidentally trigger approval.
   - On **Approve (`POST`)**: Generates a 244-bit CSPRNG `woz_...` key, stores only its `SHA-256` hash + 12-char prefix in SQLite, and sends **Email 3** to `<applicant_email>` (`BCC: ask@answerr.me`) with the API key and 1-click sign-in link (`https://ozan.werr.io/#key=woz_...`).
   - On **Reject (`POST`)** or no approval: Marks `rejected` with `key_hash = NULL`; no API key is ever created.
3. **DNS & Mail Authentication (`werr.io`):**
   - `PTR`: `173.212.201.159 -> mail.teknosanat.com.tr` (`EHLO mail.teknosanat.com.tr`)
   - `SPF`: `v=spf1 ip4:173.212.201.159 a:mail.teknosanat.com.tr include:_spf.mx.cloudflare.net ~all`
   - `DKIM`: `ozan._domainkey.werr.io` (`2048-bit RSA`, relaxed/relaxed canonicalization)

---

## 3. Verified `lexsrv` Hardware & Measured CPU Telemetry

- **Host / Guest:** VirtualBox (`oracle`, KVM paravirtualization, `Nested Paging`, `PAE/NX`), Ubuntu 24.04.5 LTS
- **CPU:** 8 vCPU `Intel(R) Xeon(R) CPU E5-2630 v4 @ 2.20GHz` (`avx2`, `fma`, `OMP_NUM_THREADS=8`)
- **Memory & Disk:** `15991 MB` (~16 GB) RAM (`~2.5 GB` idle, `~4.0 GB` peak during `light` NAR acoustic stage), `167 GB` free disk
- **Pinned `yue2-cpp` Profile:** `light` (`Q5_K_M` backbone, `4.30 GB` installed & verified)
- **Measured CPU Inference Speed (8 vCPU / 4 GGML threads):**
  - AR + Semantic LM (`751` steps for `30s` song): **`113.5 s`** (`151.1 ms/step`)
  - NAR Acoustic Flow-Matching: **`~41.4 s/step`** (`10–16` steps recommended for fast CPU drafts; `32` steps for full final render; upgrading `lexsrv` to `16 vCPU` scales GGML threads from `4` to `8`).
- **Reboot Persistence:** User `crontab` `@reboot /usr/bin/pm2 resurrect`

---

## 4. Completed Implementation Phases

- **Phase 0 (Verified):** Forked `timoncool/YuE2-Studio` → `pCwOrM/WerrOzan`, isolated `.env.local`, architecture documented.
- **Phase 1 (Verified):** Compiled `yue-server` (`AVX2+FMA` native) and `music-server` (release), deployed `werrozan-server` + `werrozan-tunnel` (`ozan-api.werr.io`) under PM2 with `@reboot` persistence, Cloudflare `ssl: strict`.
- **Phase 2 (Verified):** Implemented `crates/music-core/src/werr.rs` (24-byte seed, $\mathbb{Z}/9\mathbb{Z}$), `crates/music-server/src/gunes_dili.rs` (Turkish prosody/vowel harmony/formants), `crates/music-server/src/providers/gemini.rs` (`gemini-3.8-flash` + automatic Flash fallback), and `remote.rs` CORS allowlist for `https://ozan.werr.io`.
- **Phase 3 (Verified):** Implemented `crates/music-core/src/koma.rs` (53-TET / AEU 8 Turkish makams), `crates/audio-post/src/koma.rs` (Laroche-Dolson peak-locked STFT Koma Quantizer), and `crates/music-server/src/score/koma_midi.rs` (single/two-voice ABC normalizer + 14-bit Pitch-Bend MIDI export).
- **Phase 4 (Verified):** Installed `4.30 GB` `light` GGUF weights on `lexsrv`, rendered first 30s Turkish AI ballad (`01a1e4ec-a26c-7400-8db4-2867250ef268.flac`, `4.6 MB`), deployed WerrOzan Hybrid Studio (`web/`) + Membership/Admin Approval/DKIM Mailer to [`https://ozan.werr.io`](https://ozan.werr.io).
