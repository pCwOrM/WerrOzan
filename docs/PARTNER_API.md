# WerrOzan B2B Partner API (`ozan-api.werr.io`)

Bu doküman, **WerrOzan** AI Müzik & 53-Koma Motorunu ticari SaaS ürünlerine (`ozan.ai`, `werrad.io` vb.) bağlamak için **ITouch Systems** tarafından sağlanan **B2B Partner API** arayüzünü açıklar.

## 1. Kimlik Doğrulama & Güvenlik

- **Base URL:** `https://ozan-api.werr.io`
- **Authorization Header:** `Authorization: Bearer woz_partner_...`
- Partner anahtarı yalnızca **sunucu-tarafı (backend-to-backend)** çağrılarda kullanılmalıdır; istemci tarayıcı koduna gömülmemelidir.
- Partner anahtarı `/v1/admin/*` yönetici uç noktalarına erişemez (`403 Forbidden`).
- **İzinli CORS Originleri:** `https://ozan.ai`, `https://www.ozan.ai`, `https://werrad.io`, `https://www.werrad.io`, `https://rad.werr.io`, `https://ozan.werr.io` (ek domainler sunucuda `WERROZAN_ALLOWED_ORIGINS` ile sıfır kesintiyle eklenebilir).

## 2. Partner Profili, Kullanım & Mutabakat (Metering)

### `GET /v1/partner/me`
Partner profilinizi, aktif limitlerinizi (`max_concurrent_jobs: 5`, `max_library_songs: 1000`) ve webhook ayarınızı döner.

### `GET /v1/partner/usage`
Gelir paylaşımı ve kullanım mutabakatı için şeffaf işlem defterini (`partner_usage_ledger`) döner:
```json
{
  "ok": true,
  "partner_id": "gifted_coder",
  "summary": {
    "total_events": 42,
    "completed_jobs": 40,
    "completed_duration_seconds": 3980.0
  },
  "items": [ ... ]
}
```

### `PUT /v1/partner/webhook`
Varsayılan webhook adresinizi günceller:
```json
{ "webhook_url": "https://api.ozan.ai/webhooks/werrozan" }
```

## 3. Şarkı Üretimi, Söz/Analiz & 53-Koma Uç Noktaları

| Metod & Uç Nokta | Açıklama |
| :--- | :--- |
| `POST /v1/assistant/write` | **Söz Werr (Yorumcu):** Fikirden Türkçe/çok dilli söz, stil ve başlık üretir (`{ target: "all", description, locale: "tr", duration_seconds: 100 }`). |
| `POST /v1/werrozan/analyze` | **Güneş Dili + WERR ℤ/9ℤ:** Hece, büyük ünlü uyumu, kesilme riski ve morfolojik denetim. |
| `POST /v1/werrozan/adapt` | **El Werr:** Uyarı tespit edilen sözleri otomatik düzeltir. |
| `POST /v1/partner/jobs` | **Partner Şarkı Sentezi:** Kendi son kullanıcı ID'niz (`external_user_id`) ve opsiyonel `webhook_url` ile kuyruğa üretim gönderir (eşzamanlı 5 iş kotası). |
| `GET /v1/music/jobs/{job_id}` | Üretim durumunu (`queued` / `running` / `completed` / `failed`) ve çıktı `share_url` bilgisini sorgular. |
| `GET /v1/queue` | Canlı üretim kuyruğunu ve tahmini bekleme süresini döner. |
| `POST /v1/werrozan/koma/quantize` | Üretilen şarkıyı (`song_id`) veya yüklenen sesi 8 Türk müziği makamına (53-Koma STFT) çeker. |
| `POST /v1/werrozan/koma/score` | ABC notasyonundan 14-bit Pitch-Bend 53-Koma MIDI (`.mid`) üretir. |
| `GET /v1/public/songs` | Keşfet vitrinindeki herkese açık eserleri listeler (`WERRadio` için de kullanılır). |

### Örnek: `POST /v1/partner/jobs`
```bash
curl -X POST https://ozan-api.werr.io/v1/partner/jobs \
  -H "Authorization: Bearer $WERROZAN_PARTNER_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "external_user_id": "gc_user_1042",
    "webhook_url": "https://api.ozan.ai/webhooks/werrozan",
    "title": "Ege Esintisi",
    "locale": "tr",
    "style": "Anadolu acoustic folk, oud, cello, 86 bpm",
    "lyrics": "[Verse]\nMavi deniz kıyısında\nRüzgar eser usulca\n[Chorus]\nGönlüm açar sabaha",
    "duration_seconds": 90,
    "eos_buffer": true,
    "cot": "full",
    "steps": 16
  }'
```

## 4. Webhook Doğrulama (HMAC-SHA256)

Şarkı tamamlandığında veya hata aldığında WerrOzan, `webhook_url` adresinize `POST` gönderir:
- **Header:** `X-WerrOzan-Signature: sha256=<hex_hmac>`
- **Body:**
```json
{
  "event": "job.completed",
  "partner_id": "gifted_coder",
  "external_user_id": "gc_user_1042",
  "job_id": "5c1d5ad868044073",
  "status": "completed",
  "title": "Ege Esintisi",
  "duration_seconds": 94.2,
  "audio_url": "https://ozan-api.werr.io/v1/media/share/...",
  "error": null,
  "timestamp": 1795500000
}
```

### Node.js İmza Doğrulama Örneği
```js
import crypto from "node:crypto";

export function verifyWebhook(rawBody, signatureHeader, webhookSecret) {
  const expected = "sha256=" + crypto.createHmac("sha256", webhookSecret).update(rawBody).digest("hex");
  return crypto.timingSafeEqual(Buffer.from(expected), Buffer.from(signatureHeader || ""));
}
```
