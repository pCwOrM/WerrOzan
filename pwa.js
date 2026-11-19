// Shared PWA Install & MediaSession / Dynamic Title helper for WerrOzan

const BASE_DOC_TITLE = document.title;
let titleScrollTimer = null;
let titleScrollText = "";
let titleScrollPos = 0;

function stopTitleScroll(restoreBase = true) {
  if (titleScrollTimer) {
    clearInterval(titleScrollTimer);
    titleScrollTimer = null;
  }
  titleScrollText = "";
  titleScrollPos = 0;
  if (restoreBase) {
    document.title = BASE_DOC_TITLE;
  }
}

let activePlaybackOpts = null;

export function syncPlaybackMeta(opts) {
  const { title, artist, album = "WerrOzan", cover, audio, onPrev = null, onNext = null } = opts;
  activePlaybackOpts = opts;
  if (audio && !audio._werrMetaBound) {
    audio._werrMetaBound = true;
    audio.addEventListener("play", () => activePlaybackOpts && syncPlaybackMeta(activePlaybackOpts));
    audio.addEventListener("pause", () => activePlaybackOpts && syncPlaybackMeta(activePlaybackOpts));
  }
  const cleanTitle = (title || "WerrOzan Parça").trim();
  const cleanArtist = (artist || "WerrOzan Stüdyo").trim();

  // Dynamic scrolling document.title while playing
  if (audio && !audio.paused) {
    const full = `▶ ${cleanTitle} · ${cleanArtist} — WerrOzan   |   `;
    if (titleScrollText !== full) {
      stopTitleScroll(false);
      titleScrollText = full;
      titleScrollPos = 0;
      document.title = `▶ ${cleanTitle} · ${cleanArtist} — WerrOzan`;
      titleScrollTimer = setInterval(() => {
        if (!audio || audio.paused) {
          stopTitleScroll(true);
          return;
        }
        const shifted = titleScrollText.slice(titleScrollPos) + titleScrollText.slice(0, titleScrollPos);
        document.title = shifted.slice(0, 48);
        titleScrollPos = (titleScrollPos + 1) % titleScrollText.length;
      }, 320);
    }
  } else {
    stopTitleScroll(true);
  }

  // OS / Android Lock-screen MediaSession integration
  if ("mediaSession" in navigator) {
    const isRaster = cover && /\.(png|jpe?g|webp)(\?|$)/i.test(cover);
    const artSrc = isRaster ? cover : new URL("./icon-512.png", window.location.href).href;
    const artType = /\.jpe?g(\?|$)/i.test(artSrc)
      ? "image/jpeg"
      : /\.webp(\?|$)/i.test(artSrc)
        ? "image/webp"
        : "image/png";
    try {
      navigator.mediaSession.metadata = new MediaMetadata({
        title: cleanTitle,
        artist: cleanArtist,
        album,
        artwork: [
          { src: artSrc, sizes: "192x192", type: artType },
          { src: artSrc, sizes: "512x512", type: artType },
        ],
      });
      navigator.mediaSession.playbackState = audio && !audio.paused ? "playing" : "paused";
      navigator.mediaSession.setActionHandler("play", () => audio?.play().catch(() => {}));
      navigator.mediaSession.setActionHandler("pause", () => audio?.pause());
      navigator.mediaSession.setActionHandler("stop", () => {
        if (audio) {
          audio.pause();
          audio.currentTime = 0;
        }
        stopTitleScroll(true);
      });
      navigator.mediaSession.setActionHandler("previoustrack", onPrev || null);
      navigator.mediaSession.setActionHandler("nexttrack", onNext || null);
      navigator.mediaSession.setActionHandler("seekto", (details) => {
        if (audio && Number.isFinite(details.seekTime)) {
          audio.currentTime = details.seekTime;
        }
      });
    } catch {
      /* ignore unsupported MediaSession fields */
    }
  }
}

export function syncPlaybackPosition(audio) {
  if (!("mediaSession" in navigator) || !audio || !Number.isFinite(audio.duration) || audio.duration <= 0) return;
  try {
    navigator.mediaSession.setPositionState({
      duration: audio.duration,
      playbackRate: audio.playbackRate || 1,
      position: Math.min(audio.duration, Math.max(0, audio.currentTime || 0)),
    });
  } catch {
    /* ignore */
  }
}

export function clearPlaybackMeta() {
  activePlaybackOpts = null;
  stopTitleScroll(true);
  if ("mediaSession" in navigator) {
    try {
      navigator.mediaSession.playbackState = "none";
    } catch {
      /* ignore */
    }
  }
}

// Register Service Worker & wire Desktop/Mobile Install button
if ("serviceWorker" in navigator) {
  window.addEventListener("load", () => {
    navigator.serviceWorker.register("./sw.js").catch(() => {});
  });
}

let deferredInstallPrompt = null;
function ensureInstallButton() {
  const nav = document.querySelector(".nav-links");
  if (!nav) return null;
  let btn = document.getElementById("btnPwaInstall");
  if (!btn) {
    btn = document.createElement("button");
    btn.id = "btnPwaInstall";
    btn.type = "button";
    btn.className = "nav-link";
    btn.style.cssText = "border-color:rgba(16,185,129,0.55);color:#6ee7b7;background:rgba(16,185,129,0.12);cursor:pointer;";
    btn.textContent = "📲 Uygulamayı Kur";
    btn.addEventListener("click", async () => {
      if (!deferredInstallPrompt) return;
      deferredInstallPrompt.prompt();
      try {
        await deferredInstallPrompt.userChoice;
      } catch {
        /* ignore */
      }
      deferredInstallPrompt = null;
      btn.style.display = "none";
    });
    nav.appendChild(btn);
  }
  return btn;
}

window.addEventListener("beforeinstallprompt", (e) => {
  e.preventDefault();
  deferredInstallPrompt = e;
  const btn = ensureInstallButton();
  if (btn) btn.style.display = "inline-flex";
});

window.addEventListener("appinstalled", () => {
  deferredInstallPrompt = null;
  const btn = document.getElementById("btnPwaInstall");
  if (btn) btn.style.display = "none";
});
