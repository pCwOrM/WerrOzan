//! WerrOzan Membership, Admin Approval, Per-User API Keys & Public Demo Endpoints.
//!
//! Security model:
//! - Public registration (`POST /v1/auth/register`) creates a `pending` row with
//!   `key_hash = NULL` and notifies `WERROZAN_ADMIN_EMAIL` (`pcworm@pcworm.net`).
//! - NO API key is ever created unless an admin explicitly approves via either
//!   the single-use signed review link (`/v1/auth/decide` GET->POST) or the
//!   Master-Key-authenticated Admin API (`/v1/admin/users/{id}/approve`).
//! - Raw user keys (`woz_...`, 244-bit CSPRNG UUIDv4) are emailed once on approval
//!   and persisted strictly as SHA-256 hashes + 12-char prefixes.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use anyhow::{Context, Result};
use axum::{
    extract::{ConnectInfo, Extension, Form, Path as AxumPath, Query, Request, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    Json,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;

use crate::{api_error, mailer, remote::AuthPrincipal, studio_data_root, ApiError, AppState};

const APPROVAL_TTL_SECS: i64 = 72 * 3600;
const RESEND_COOLDOWN_SECS: i64 = 300;
const IP_WINDOW_SECS: i64 = 900;
const IP_MAX_ATTEMPTS: usize = 6;

const DEFAULT_DEMO_SONG_ID: &str = "01a1e4ec-a26c-7400-8db4-2867250ef268";
const DEFAULT_DEMO_FILENAME: &str = "mavi-gecelerde-yue2.flac";

#[derive(Debug, Clone, Serialize)]
pub struct UserRecord {
    pub id: String,
    pub email: String,
    pub name: String,
    pub note: String,
    pub status: String,
    pub key_prefix: Option<String>,
    pub approved_by: Option<String>,
    pub decision_note: Option<String>,
    pub created_at: i64,
    pub decided_at: Option<i64>,
    pub last_used_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutboxRecord {
    pub id: i64,
    pub to_email: String,
    pub subject: String,
    pub text_body: String,
    pub kind: String,
    pub transport: String,
    pub delivered: bool,
    pub error_message: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedMember {
    pub id: String,
    pub email: String,
    pub name: String,
    pub key_prefix: String,
}

#[derive(Clone)]
pub struct AuthStore {
    conn: Arc<Mutex<Connection>>,
    rate_limits: Arc<Mutex<HashMap<String, Vec<i64>>>>,
}

static GLOBAL_AUTH: OnceLock<AuthStore> = OnceLock::new();

pub fn init_global(data_root: &Path) -> Result<()> {
    let store = AuthStore::open(&data_root.join("membership.sqlite"))?;
    let _ = GLOBAL_AUTH.set(store);
    Ok(())
}

pub fn global() -> Option<&'static AuthStore> {
    GLOBAL_AUTH.get()
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

pub fn normalize_and_validate_email(raw: &str) -> Result<String, String> {
    let email = raw.trim().to_ascii_lowercase();
    if email.len() < 5 || email.len() > 160 {
        return Err("Geçerli bir e-posta adresi girin (5–160 karakter).".into());
    }
    if email.bytes().any(|b| b <= 32 || b == 127 || matches!(b, b'<' | b'>' | b',' | b';' | b'"' | b'\\')) {
        return Err("E-posta adresi geçersiz karakterler içeriyor.".into());
    }
    let Some((local, domain)) = email.split_once('@') else {
        return Err("E-posta adresinde '@' bulunmalıdır.".into());
    };
    if local.is_empty() || domain.contains('@') || !domain.contains('.') {
        return Err("Geçerli bir e-posta alan adı girin.".into());
    }
    if local.starts_with('.') || local.ends_with('.') || local.contains("..") {
        return Err("E-posta yerel adı geçersiz.".into());
    }
    if domain.starts_with('.') || domain.ends_with('.') || domain.contains("..") {
        return Err("E-posta alan adı geçersiz.".into());
    }
    let labels: Vec<&str> = domain.split('.').collect();
    if labels.last().is_none_or(|tld| tld.len() < 2 || !tld.bytes().all(|b| b.is_ascii_alphabetic())) {
        return Err("E-posta üst alan adı geçersiz.".into());
    }
    for label in labels {
        if label.is_empty()
            || label.starts_with('-')
            || label.ends_with('-')
            || !label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("E-posta alan adı biçimi geçersiz.".into());
        }
    }
    Ok(email)
}

fn sanitize_text_field(raw: &str, max_chars: usize) -> String {
    raw.chars()
        .filter(|c| !c.is_control() || *c == ' ')
        .collect::<String>()
        .trim()
        .chars()
        .take(max_chars)
        .collect()
}

fn client_ip_label(peer: &std::net::SocketAddr, headers: &HeaderMap) -> String {
    if let Some(cf) = headers.get("cf-connecting-ip").and_then(|v| v.to_str().ok()) {
        let trimmed = cf.trim();
        if !trimmed.is_empty() {
            return trimmed.chars().take(64).collect();
        }
    }
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = xff.split(',').next().map(str::trim).filter(|s| !s.is_empty()) {
            return first.chars().take(64).collect();
        }
    }
    peer.ip().to_string()
}

impl AuthStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| format!("create auth db parent {}", parent.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("open membership db {}", path.display()))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS users (
               id TEXT PRIMARY KEY,
               email TEXT NOT NULL UNIQUE,
               name TEXT NOT NULL DEFAULT '',
               note TEXT NOT NULL DEFAULT '',
               status TEXT NOT NULL CHECK(status IN ('pending','approved','rejected','revoked')),
               request_token TEXT NOT NULL UNIQUE,
               key_hash TEXT UNIQUE,
               key_prefix TEXT,
               approved_by TEXT,
               decision_note TEXT,
               request_ip TEXT,
               created_at INTEGER NOT NULL,
               decided_at INTEGER,
               last_used_at INTEGER
             );
             CREATE TABLE IF NOT EXISTS approval_tokens (
               token_hash TEXT PRIMARY KEY,
               user_id TEXT NOT NULL,
               created_at INTEGER NOT NULL,
               expires_at INTEGER NOT NULL,
               used_at INTEGER,
               action_taken TEXT
             );
             CREATE TABLE IF NOT EXISTS mail_outbox (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               to_email TEXT NOT NULL,
               subject TEXT NOT NULL,
               text_body TEXT NOT NULL,
               kind TEXT NOT NULL,
               transport TEXT NOT NULL,
               delivered INTEGER NOT NULL DEFAULT 0,
               error_message TEXT,
               created_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS user_songs (
               id TEXT PRIMARY KEY,
               owner_id TEXT NOT NULL,
               owner_email TEXT,
               title TEXT NOT NULL,
               style TEXT NOT NULL,
               lyrics TEXT NOT NULL DEFAULT '',
               prompt_description TEXT NOT NULL DEFAULT '',
               locale TEXT NOT NULL DEFAULT 'tr',
               cot TEXT NOT NULL DEFAULT 'full',
               steps INTEGER NOT NULL DEFAULT 16,
               duration_seconds REAL NOT NULL,
               format TEXT NOT NULL DEFAULT 'flac',
               library_song_id TEXT NOT NULL DEFAULT '',
               file_path TEXT NOT NULL,
               share_token TEXT NOT NULL UNIQUE,
               created_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_user_songs_owner ON user_songs(owner_id, created_at);
             CREATE TABLE IF NOT EXISTS user_koma_variants (
               id TEXT PRIMARY KEY,
               song_id TEXT NOT NULL REFERENCES user_songs(id) ON DELETE CASCADE,
               owner_id TEXT NOT NULL,
               makam_id TEXT NOT NULL,
               makam_name TEXT NOT NULL,
               strength REAL NOT NULL,
               file_path TEXT NOT NULL,
               share_token TEXT NOT NULL UNIQUE,
               peaks_shifted INTEGER NOT NULL DEFAULT 0,
               mean_cent_correction REAL NOT NULL DEFAULT 0.0,
               created_at INTEGER NOT NULL,
               UNIQUE(song_id, makam_id)
             );",
        )?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            rate_limits: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn check_rate_limit(&self, ip_key: &str, now: i64) -> Result<(), String> {
        let mut map = self.rate_limits.lock().unwrap_or_else(|p| p.into_inner());
        if map.len() > 2048 {
            map.retain(|_, ts| ts.iter().any(|t| now - *t <= IP_WINDOW_SECS));
        }
        let entries = map.entry(ip_key.to_string()).or_default();
        entries.retain(|t| now - *t <= IP_WINDOW_SECS);
        if entries.len() >= IP_MAX_ATTEMPTS {
            return Err("Çok fazla başvuru denemesi; lütfen 15 dakika sonra tekrar deneyin.".into());
        }
        entries.push(now);
        Ok(())
    }

    pub fn verify_active_key(&self, raw_key: &str) -> Option<AuthenticatedMember> {
        let trimmed = raw_key.trim();
        if !trimmed.starts_with("woz_") || trimmed.len() < 32 {
            return None;
        }
        let hash = sha256_hex(trimmed);
        let now = now_unix();
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let row: Option<(String, String, String, String)> = conn
            .query_row(
                "SELECT id, email, name, COALESCE(key_prefix, 'woz_***') FROM users WHERE key_hash = ?1 AND status = 'approved'",
                params![hash],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .ok()
            .flatten();
        if let Some((id, email, name, key_prefix)) = row {
            let _ = conn.execute("UPDATE users SET last_used_at = ?1 WHERE id = ?2", params![now, id]);
            Some(AuthenticatedMember {
                id,
                email,
                name,
                key_prefix,
            })
        } else {
            None
        }
    }

    pub fn register_applicant(
        &self,
        email: &str,
        name: &str,
        note: &str,
        ip: &str,
    ) -> Result<RegisterOutcome, (StatusCode, String)> {
        let now = now_unix();
        self.check_rate_limit(ip, now)
            .map_err(|msg| (StatusCode::TOO_MANY_REQUESTS, msg))?;

        let mut conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let existing: Option<(String, String, String, i64)> = conn
            .query_row(
                "SELECT id, status, request_token, created_at FROM users WHERE email = ?1",
                params![email],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if let Some((user_id, status, req_token, created_at)) = existing {
            match status.as_str() {
                "approved" => {
                    return Err((
                        StatusCode::CONFLICT,
                        "Bu e-posta zaten onaylı üyedir. E-postanıza gelen API anahtarı ile bağlanabilirsiniz.".into(),
                    ));
                }
                "rejected" | "revoked" => {
                    return Err((
                        StatusCode::FORBIDDEN,
                        "Bu e-posta adresi için üyelik başvurusu onaylanmamış veya iptal edilmiştir.".into(),
                    ));
                }
                _ => {
                    let last_tok_at: Option<i64> = conn
                        .query_row(
                            "SELECT MAX(created_at) FROM approval_tokens WHERE user_id = ?1",
                            params![user_id],
                            |r| r.get(0),
                        )
                        .optional()
                        .ok()
                        .flatten()
                        .flatten();
                    let recent = last_tok_at.unwrap_or(created_at);
                    if now - recent < RESEND_COOLDOWN_SECS {
                        return Ok(RegisterOutcome {
                            request_token: req_token,
                            approval_token: None,
                        });
                    }
                    let tx = conn.transaction().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                    tx.execute(
                        "UPDATE users SET name = CASE WHEN ?1 <> '' THEN ?1 ELSE name END, note = CASE WHEN ?2 <> '' THEN ?2 ELSE note END, request_ip = ?3 WHERE id = ?4",
                        params![name, note, ip, user_id],
                    )
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                    let approval_token = format!("apr_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
                    let tok_hash = sha256_hex(&approval_token);
                    tx.execute(
                        "INSERT INTO approval_tokens (token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
                        params![tok_hash, user_id, now, now + APPROVAL_TTL_SECS],
                    )
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                    tx.commit().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                    return Ok(RegisterOutcome {
                        request_token: req_token,
                        approval_token: Some(approval_token),
                    });
                }
            }
        }

        let user_id = Uuid::new_v4().to_string();
        let request_token = format!("req_{}", Uuid::new_v4().simple());
        let approval_token = format!("apr_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let tok_hash = sha256_hex(&approval_token);

        let tx = conn.transaction().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        tx.execute(
            "INSERT INTO users (id, email, name, note, status, request_token, key_hash, key_prefix, request_ip, created_at)
             VALUES (?1, ?2, ?3, ?4, 'pending', ?5, NULL, NULL, ?6, ?7)",
            params![user_id, email, name, note, request_token, ip, now],
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        tx.execute(
            "INSERT INTO approval_tokens (token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
            params![tok_hash, user_id, now, now + APPROVAL_TTL_SECS],
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        tx.commit().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        Ok(RegisterOutcome {
            request_token,
            approval_token: Some(approval_token),
        })
    }

    pub fn status_by_request_token(&self, request_token: &str) -> Result<Option<UserRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let rec = conn
            .query_row(
                "SELECT id, email, name, note, status, key_prefix, approved_by, decision_note, created_at, decided_at, last_used_at
                 FROM users WHERE request_token = ?1",
                params![request_token.trim()],
                row_to_user_record,
            )
            .optional()?;
        Ok(rec)
    }

    pub fn lookup_approval_token(&self, raw_token: &str) -> Result<Option<TokenPreview>> {
        let tok_hash = sha256_hex(raw_token.trim());
        let now = now_unix();
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let row: Option<(String, i64, Option<i64>, UserRecord)> = conn
            .query_row(
                "SELECT t.user_id, t.expires_at, t.used_at,
                        u.id, u.email, u.name, u.note, u.status, u.key_prefix, u.approved_by, u.decision_note, u.created_at, u.decided_at, u.last_used_at
                 FROM approval_tokens t
                 JOIN users u ON u.id = t.user_id
                 WHERE t.token_hash = ?1",
                params![tok_hash],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        UserRecord {
                            id: r.get(3)?,
                            email: r.get(4)?,
                            name: r.get(5)?,
                            note: r.get(6)?,
                            status: r.get(7)?,
                            key_prefix: r.get(8)?,
                            approved_by: r.get(9)?,
                            decision_note: r.get(10)?,
                            created_at: r.get(11)?,
                            decided_at: r.get(12)?,
                            last_used_at: r.get(13)?,
                        },
                    ))
                },
            )
            .optional()?;
        Ok(row.map(|(_, expires_at, used_at, user)| TokenPreview {
            expired: now > expires_at,
            used: used_at.is_some(),
            user,
        }))
    }

    pub fn decide_by_token(
        &self,
        raw_token: &str,
        approve: bool,
        decision_note: &str,
    ) -> Result<DecisionResult, (StatusCode, String)> {
        let tok_hash = sha256_hex(raw_token.trim());
        let now = now_unix();
        let mut conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let tx = conn.transaction().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let tok_row: Option<(String, i64, Option<i64>)> = tx
            .query_row(
                "SELECT user_id, expires_at, used_at FROM approval_tokens WHERE token_hash = ?1",
                params![tok_hash],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let Some((user_id, expires_at, used_at)) = tok_row else {
            return Err((StatusCode::NOT_FOUND, "Onay bağlantısı geçersiz veya bulunamadı.".into()));
        };
        if used_at.is_some() {
            return Err((StatusCode::CONFLICT, "Bu onay bağlantısı daha önce kullanılmış.".into()));
        }
        if now > expires_at {
            return Err((StatusCode::GONE, "Bu onay bağlantısının süresi dolmuş (72 saat).".into()));
        }

        let res = apply_decision_tx(
            &tx,
            &user_id,
            approve,
            "email_link",
            decision_note,
            now,
        )?;
        tx.commit().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(res)
    }

    pub fn decide_by_user_id(
        &self,
        user_id: &str,
        approve: bool,
        actor: &str,
        decision_note: &str,
    ) -> Result<DecisionResult, (StatusCode, String)> {
        let now = now_unix();
        let mut conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let tx = conn.transaction().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let res = apply_decision_tx(&tx, user_id, approve, actor, decision_note, now)?;
        tx.commit().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(res)
    }

    pub fn revoke_user(&self, user_id: &str, actor: &str) -> Result<UserRecord, (StatusCode, String)> {
        let now = now_unix();
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let changed = conn
            .execute(
                "UPDATE users SET status = 'revoked', key_hash = NULL, decided_at = ?1, approved_by = ?2 WHERE id = ?3",
                params![now, actor, user_id],
            )
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if changed == 0 {
            return Err((StatusCode::NOT_FOUND, "Kullanıcı bulunamadı.".into()));
        }
        let rec = conn
            .query_row(
                "SELECT id, email, name, note, status, key_prefix, approved_by, decision_note, created_at, decided_at, last_used_at
                 FROM users WHERE id = ?1",
                params![user_id],
                row_to_user_record,
            )
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(rec)
    }

    pub fn list_users(&self) -> Result<Vec<UserRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, email, name, note, status, key_prefix, approved_by, decision_note, created_at, decided_at, last_used_at
             FROM users ORDER BY created_at DESC LIMIT 200",
        )?;
        let rows = stmt.query_map([], row_to_user_record)?;
        let mut out = Vec::new();
        for item in rows {
            out.push(item?);
        }
        Ok(out)
    }

    pub fn record_outbox(&self, to_email: &str, subject: &str, text_body: &str, kind: &str, delivery: &mailer::MailDeliveryResult) {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let redacted = mailer::redact_woz_keys(text_body);
        let to_with_bcc = if delivery.bcc_delivered.is_empty() {
            to_email.to_string()
        } else {
            format!("{to_email} (BCC: {})", delivery.bcc_delivered.join(", "))
        };
        let _ = conn.execute(
            "INSERT INTO mail_outbox (to_email, subject, text_body, kind, transport, delivered, error_message, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                to_with_bcc,
                subject,
                redacted,
                kind,
                delivery.transport,
                if delivery.delivered { 1 } else { 0 },
                delivery.error,
                now_unix(),
            ],
        );
    }

    pub fn list_outbox(&self) -> Result<Vec<OutboxRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, to_email, subject, text_body, kind, transport, delivered, error_message, created_at
             FROM mail_outbox ORDER BY id DESC LIMIT 50",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(OutboxRecord {
                id: r.get(0)?,
                to_email: r.get(1)?,
                subject: r.get(2)?,
                text_body: r.get(3)?,
                kind: r.get(4)?,
                transport: r.get(5)?,
                delivered: r.get::<_, i64>(6)? != 0,
                error_message: r.get(7)?,
                created_at: r.get(8)?,
            })
        })?;
        let mut out = Vec::new();
        for item in rows {
            out.push(item?);
        }
        Ok(out)
    }
}

fn row_to_user_record(r: &rusqlite::Row<'_>) -> rusqlite::Result<UserRecord> {
    Ok(UserRecord {
        id: r.get(0)?,
        email: r.get(1)?,
        name: r.get(2)?,
        note: r.get(3)?,
        status: r.get(4)?,
        key_prefix: r.get(5)?,
        approved_by: r.get(6)?,
        decision_note: r.get(7)?,
        created_at: r.get(8)?,
        decided_at: r.get(9)?,
        last_used_at: r.get(10)?,
    })
}

fn apply_decision_tx(
    tx: &rusqlite::Transaction<'_>,
    user_id: &str,
    approve: bool,
    actor: &str,
    decision_note: &str,
    now: i64,
) -> Result<DecisionResult, (StatusCode, String)> {
    let existing: Option<UserRecord> = tx
        .query_row(
            "SELECT id, email, name, note, status, key_prefix, approved_by, decision_note, created_at, decided_at, last_used_at
             FROM users WHERE id = ?1",
            params![user_id],
            row_to_user_record,
        )
        .optional()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let Some(user) = existing else {
        return Err((StatusCode::NOT_FOUND, "Kullanıcı kaydı bulunamadı.".into()));
    };
    if user.status == "approved" {
        return Err((StatusCode::CONFLICT, "Bu kullanıcı zaten onaylanmış.".into()));
    }
    if user.status == "rejected" && !approve {
        return Err((StatusCode::CONFLICT, "Bu kullanıcı zaten reddedilmiş.".into()));
    }

    let note_clean = sanitize_text_field(decision_note, 240);
    let action_label = if approve { "approved" } else { "rejected" };

    let issued_raw_key = if approve {
        let raw_key = format!("woz_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let key_hash = sha256_hex(&raw_key);
        let key_prefix: String = raw_key.chars().take(12).collect();
        tx.execute(
            "UPDATE users SET status = 'approved', key_hash = ?1, key_prefix = ?2, approved_by = ?3, decision_note = ?4, decided_at = ?5 WHERE id = ?6",
            params![key_hash, key_prefix, actor, note_clean, now, user_id],
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Some(raw_key)
    } else {
        tx.execute(
            "UPDATE users SET status = 'rejected', key_hash = NULL, key_prefix = NULL, approved_by = ?1, decision_note = ?2, decided_at = ?3 WHERE id = ?4",
            params![actor, note_clean, now, user_id],
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        None
    };

    tx.execute(
        "UPDATE approval_tokens SET used_at = COALESCE(used_at, ?1), action_taken = COALESCE(action_taken, ?2) WHERE user_id = ?3",
        params![now, action_label, user_id],
    )
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let updated: UserRecord = tx
        .query_row(
            "SELECT id, email, name, note, status, key_prefix, approved_by, decision_note, created_at, decided_at, last_used_at
             FROM users WHERE id = ?1",
            params![user_id],
            row_to_user_record,
        )
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(DecisionResult {
        user: updated,
        issued_raw_key,
    })
}

pub struct RegisterOutcome {
    pub request_token: String,
    pub approval_token: Option<String>,
}

pub struct TokenPreview {
    pub expired: bool,
    pub used: bool,
    pub user: UserRecord,
}

pub struct DecisionResult {
    pub user: UserRecord,
    pub issued_raw_key: Option<String>,
}

fn require_store() -> Result<&'static AuthStore, (StatusCode, Json<ApiError>)> {
    global().ok_or_else(|| api_error(StatusCode::SERVICE_UNAVAILABLE, "Membership store is not initialized".into()))
}

// ---------- Email Builders ----------

async fn dispatch_applicant_received_email(
    store: &AuthStore,
    email: &str,
    name: &str,
    request_token: &str,
) -> mailer::MailDeliveryResult {
    let web_url = mailer::public_web_url();
    let bcc_to = mailer::bcc_email();
    let display_name = if name.is_empty() { email } else { name };

    let subject = "WerrOzan Üyelik Başvurunuz Alındı";
    let text = format!(
        "Merhaba {display_name},\n\n\
         WerrOzan ({web_url}) üyelik başvurunuz alınmıştır.\n\n\
         Başvurunuz yönetici incelemesindedir. Yönetici onayladığında size özel API erişim anahtarınız (woz_...) otomatik olarak üretilecek ve e-posta ile tarafınıza bildirilecektir.\n\
         Onaylanmadığı sürece herhangi bir API anahtarı oluşturulmaz.\n\n\
         Başvuru Takip Kodunuz: {request_token}\n\
         Stüdyo & İlk Demo Şarkı: {web_url}\n"
    );
    let html = format!(
        "<div style=\"font-family:system-ui,-apple-system,sans-serif;background:#0b1020;color:#e5e7eb;padding:24px;border-radius:12px;max-width:600px\">\
         <h2 style=\"margin:0 0 12px;color:#f59e0b\">WerrOzan Üyelik Başvurunuz Alındı</h2>\
         <p style=\"color:#cbd5e1;margin:0 0 14px\">Merhaba <strong>{}</strong>, WerrOzan Hibrit AI Müzik, Güneş Dili ve 53-Koma Stüdyosu üyelik başvurunuz başarıyla alınmıştır.</p>\
         <div style=\"background:#111827;border:1px solid #334155;padding:14px;border-radius:8px;margin:16px 0;color:#f8fafc;font-size:14px\">\
           Başvurunuz yönetici onayına iletilmiştir. <strong>Onaylandığında size özel API erişim anahtarınız üretilerek bu e-posta adresinize gönderilecektir.</strong> Onaylanmadığı sürece anahtar oluşturulmaz.\
         </div>\
         <p style=\"font-size:13px;color:#94a3b8;margin:0 0 16px\">Başvuru Takip Kodunuz: <code>{}</code></p>\
         <a href=\"{}\" style=\"display:inline-block;padding:11px 18px;background:#38bdf8;color:#082f49;font-weight:700;text-decoration:none;border-radius:8px\">WerrOzan Stüdyosu &amp; İlk Demo Şarkıyı Dinle</a>\
         </div>",
        mailer::html_escape(display_name),
        mailer::html_escape(request_token),
        mailer::html_escape(&web_url),
    );

    let delivery = mailer::send_email(&mailer::EmailPayload {
        to: email.to_string(),
        bcc: vec![bcc_to],
        subject: subject.to_string(),
        text: text.clone(),
        html,
    })
    .await;
    store.record_outbox(email, subject, &text, "user_signup_ack", &delivery);
    delivery
}

async fn dispatch_admin_signup_email(
    store: &AuthStore,
    email: &str,
    name: &str,
    note: &str,
    ip: &str,
    approval_token: &str,
) -> mailer::MailDeliveryResult {
    let admin_to = mailer::admin_email();
    let decide_url = format!("{}/v1/auth/decide?token={}", mailer::public_api_url(), approval_token);
    let web_url = mailer::public_web_url();
    let display_name = if name.is_empty() { "-" } else { name };
    let display_note = if note.is_empty() { "-" } else { note };

    let subject = format!("[WerrOzan] Yeni Üyelik Başvurusu: {email}");
    let text = format!(
        "WerrOzan yeni üyelik başvurusu:\n\n\
         E-posta: {email}\n\
         Ad / Sanatçı: {display_name}\n\
         Kullanım Amacı: {display_note}\n\
         İstek IP: {ip}\n\n\
         Onaylamak veya reddetmek için (72 saat geçerli tek kullanımlık bağlantı):\n\
         {decide_url}\n\n\
         Not: Siz onaylamadığınız sürece HİÇBİR API erişim anahtarı üretilmez.\n\
         Yönetim Paneli: {web_url}\n"
    );
    let html = format!(
        "<div style=\"font-family:system-ui,-apple-system,sans-serif;background:#0b1020;color:#e5e7eb;padding:24px;border-radius:12px;max-width:600px\">\
         <h2 style=\"margin:0 0 12px;color:#38bdf8\">WerrOzan Yeni Üyelik Başvurusu</h2>\
         <p style=\"color:#cbd5e1;margin:0 0 16px\">Aşağıdaki hesap e-posta ile WerrOzan Stüdyosu'na başvuruda bulundu. Onaylamanız hâlinde benzersiz bir API anahtarı üretilip kullanıcının e-postasına gönderilecektir.</p>\
         <table style=\"width:100%;border-collapse:collapse;background:#111827;border-radius:8px;margin-bottom:20px\">\
         <tr><td style=\"padding:8px 12px;color:#94a3b8\">E-posta</td><td style=\"padding:8px 12px;color:#fff;font-weight:600\">{}</td></tr>\
         <tr><td style=\"padding:8px 12px;color:#94a3b8\">Ad / Sanatçı</td><td style=\"padding:8px 12px;color:#fff\">{}</td></tr>\
         <tr><td style=\"padding:8px 12px;color:#94a3b8\">Not</td><td style=\"padding:8px 12px;color:#fff\">{}</td></tr>\
         <tr><td style=\"padding:8px 12px;color:#94a3b8\">IP</td><td style=\"padding:8px 12px;color:#94a3b8\">{}</td></tr>\
         </table>\
         <a href=\"{}\" style=\"display:inline-block;padding:12px 20px;background:#10b981;color:#052e16;font-weight:700;text-decoration:none;border-radius:8px\">Başvuruyu İncele &amp; Onayla / Reddet</a>\
         <p style=\"font-size:12px;color:#94a3b8;margin-top:18px\">Bu bağlantıya tıklamak tek başına onay vermez; açılan sayfada Onayla veya Reddet butonunu seçersiniz. Onaylamazsanız API anahtarı oluşturulmaz.</p>\
         </div>",
        mailer::html_escape(email),
        mailer::html_escape(display_name),
        mailer::html_escape(display_note),
        mailer::html_escape(ip),
        mailer::html_escape(&decide_url),
    );

    let delivery = mailer::send_email(&mailer::EmailPayload {
        to: admin_to.clone(),
        bcc: Vec::new(),
        subject: subject.clone(),
        text: text.clone(),
        html,
    })
    .await;
    store.record_outbox(&admin_to, &subject, &text, "admin_signup_notice", &delivery);
    mailer::notify_admin_webhook(email, display_name, &decide_url).await;
    delivery
}

async fn dispatch_user_approved_email(
    store: &AuthStore,
    user: &UserRecord,
    raw_key: &str,
) -> mailer::MailDeliveryResult {
    let web_url = mailer::public_web_url();
    let api_url = mailer::public_api_url();
    let bcc_to = mailer::bcc_email();
    let magic_link = format!("{web_url}/#key={raw_key}");
    let display_name = if user.name.is_empty() { user.email.as_str() } else { user.name.as_str() };

    let subject = "WerrOzan Üyeliğiniz Onaylandı — API Erişim Anahtarınız";
    let text = format!(
        "Merhaba {display_name},\n\n\
         WerrOzan (ozan.werr.io) üyelik başvurunuz yönetici tarafından onaylandı.\n\n\
         Size özel API Erişim Anahtarınız (kimseyle paylaşmayın):\n\
         {raw_key}\n\n\
         Tek tıkla stüdyoya bağlanmak için:\n\
         {magic_link}\n\n\
         API Sunucusu: {api_url}\n\
         Not: Güneş Dili, WERR Z/9Z ve 53-Koma özellikleriyle AI şarkı üretimini kullanabilirsiniz.\n"
    );
    let html = format!(
        "<div style=\"font-family:system-ui,-apple-system,sans-serif;background:#0b1020;color:#e5e7eb;padding:24px;border-radius:12px;max-width:600px\">\
         <h2 style=\"margin:0 0 12px;color:#10b981\">WerrOzan Üyeliğiniz Onaylandı</h2>\
         <p style=\"color:#cbd5e1\">Merhaba <strong>{}</strong>, başvurunuz onaylandı ve size özel API erişim anahtarınız oluşturuldu.</p>\
         <div style=\"background:#111827;border:1px solid #334155;padding:14px;border-radius:8px;margin:16px 0;font-family:monospace;font-size:13px;word-break:break-all;color:#f8fafc\">{}</div>\
         <a href=\"{}\" style=\"display:inline-block;padding:12px 20px;background:#38bdf8;color:#082f49;font-weight:700;text-decoration:none;border-radius:8px\">Stüdyoyu Aç &amp; Otomatik Bağlan</a>\
         <p style=\"font-size:12px;color:#94a3b8;margin-top:18px\">Bu anahtar yalnızca size özeldir ve sunucuda yalnızca özet (SHA-256) biçiminde tutulur. Güvenliğiniz için paylaşmayın.</p>\
         </div>",
        mailer::html_escape(display_name),
        mailer::html_escape(raw_key),
        mailer::html_escape(&magic_link),
    );

    let delivery = mailer::send_email(&mailer::EmailPayload {
        to: user.email.clone(),
        bcc: vec![bcc_to],
        subject: subject.to_string(),
        text: text.clone(),
        html,
    })
    .await;
    store.record_outbox(&user.email, subject, &text, "user_api_key", &delivery);
    delivery
}

// ---------- HTTP Handlers ----------

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub note: String,
}

pub async fn register(
    ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let store = require_store()?;
    let email = normalize_and_validate_email(&req.email)
        .map_err(|msg| api_error(StatusCode::BAD_REQUEST, msg))?;
    let name = sanitize_text_field(&req.name, 80);
    let note = sanitize_text_field(&req.note, 400);
    let ip = client_ip_label(&peer, &headers);

    let outcome = store
        .register_applicant(&email, &name, &note, &ip)
        .map_err(|(code, msg)| api_error(code, msg))?;

    let (user_delivery, admin_delivery) = if let Some(approval_token) = &outcome.approval_token {
        tokio::join!(
            dispatch_applicant_received_email(store, &email, &name, &outcome.request_token),
            dispatch_admin_signup_email(store, &email, &name, &note, &ip, approval_token),
        )
    } else {
        let dedup = mailer::MailDeliveryResult {
            delivered: true,
            transport: "cooldown_dedup".to_string(),
            bcc_delivered: Vec::new(),
            error: None,
        };
        (dedup.clone(), dedup)
    };

    Ok(Json(json!({
        "ok": true,
        "status": "pending",
        "request_id": outcome.request_token,
        "user_notified": user_delivery.delivered,
        "admin_notified": admin_delivery.delivered,
        "message": if admin_delivery.delivered || user_delivery.delivered {
            "Başvurunuz alındı ve e-posta adresinize bilgilendirme gönderildi. Yönetici onayladığında API anahtarınız e-postanıza iletilecektir."
        } else {
            "Başvurunuz bekleme sırasına alındı (key oluşturulmadı). Yönetici onayladığında anahtar üretilecektir."
        }
    })))
}

pub async fn registration_status(
    AxumPath(request_id): AxumPath<String>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let store = require_store()?;
    let rec = store
        .status_by_request_token(&request_id)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Başvuru kaydı bulunamadı.".into()))?;
    Ok(Json(json!({
        "status": rec.status,
        "email": rec.email,
        "name": rec.name,
        "key_prefix": rec.key_prefix,
        "created_at": rec.created_at,
        "decided_at": rec.decided_at,
    })))
}

#[derive(Debug, Deserialize)]
pub struct DecideQuery {
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct DecideForm {
    pub token: String,
    pub decision: String,
    #[serde(default)]
    pub note: String,
}

pub async fn decide_page(Query(query): Query<DecideQuery>) -> Html<String> {
    let Some(store) = global() else {
        return Html(render_decide_html(
            "Servis Hazır Değil",
            "<p>Üyelik veritabanı henüz başlatılmadı.</p>",
        ));
    };
    let preview = match store.lookup_approval_token(&query.token) {
        Ok(Some(p)) => p,
        Ok(None) => {
            return Html(render_decide_html(
                "Geçersiz Bağlantı",
                "<p>Bu onay bağlantısı geçersiz veya bulunamadı.</p>",
            ));
        }
        Err(e) => {
            return Html(render_decide_html(
                "Hata",
                &format!("<p>{}</p>", mailer::html_escape(&e.to_string())),
            ));
        }
    };

    let u = &preview.user;
    let esc_email = mailer::html_escape(&u.email);
    let esc_name = mailer::html_escape(if u.name.is_empty() { "-" } else { &u.name });
    let esc_note = mailer::html_escape(if u.note.is_empty() { "-" } else { &u.note });
    let esc_tok = mailer::html_escape(&query.token);

    if preview.used || u.status != "pending" {
        let status_label = match u.status.as_str() {
            "approved" => "Onaylandı (API Key üretildi)",
            "rejected" => "Reddedildi (Key oluşturulmadı)",
            "revoked" => "İptal Edildi",
            other => other,
        };
        return Html(render_decide_html(
            "Başvuru Zaten İşlendi",
            &format!(
                "<p><strong>{esc_email}</strong> hesabı için güncel durum: <span class=\"pill\">{status_label}</span></p>\
                 <p class=\"muted\">Ad: {esc_name} · Not: {esc_note}</p>"
            ),
        ));
    }

    if preview.expired {
        return Html(render_decide_html(
            "Bağlantı Süresi Doldu",
            &format!(
                "<p><strong>{esc_email}</strong> için bu e-posta bağlantısının 72 saatlik süresi dolmuştur. Yönetici panelinden onaylayabilirsiniz.</p>"
            ),
        ));
    }

    let body = format!(
        "<p class=\"muted\">Aşağıdaki üyelik başvurusu için karar verin. <strong>Onayla</strong> seçilmediği sürece API anahtarı üretilmez.</p>\
         <div class=\"box\">\
           <div><span class=\"lbl\">E-posta:</span> <strong>{esc_email}</strong></div>\
           <div><span class=\"lbl\">Ad / Sanatçı:</span> {esc_name}</div>\
           <div><span class=\"lbl\">Açıklama:</span> {esc_note}</div>\
         </div>\
         <form method=\"post\" action=\"/v1/auth/decide\">\
           <input type=\"hidden\" name=\"token\" value=\"{esc_tok}\">\
           <input type=\"text\" name=\"note\" placeholder=\"İsteğe bağlı karar notu...\" maxlength=\"200\">\
           <div class=\"actions\">\
             <button type=\"submit\" name=\"decision\" value=\"approve\" class=\"btn ok\">Onayla &amp; API Key Üretip Gönder</button>\
             <button type=\"submit\" name=\"decision\" value=\"reject\" class=\"btn no\">Reddet (Key Oluşturma)</button>\
           </div>\
         </form>"
    );
    Html(render_decide_html("WerrOzan Üyelik Onayı", &body))
}

pub async fn decide_submit(Form(form): Form<DecideForm>) -> (StatusCode, Html<String>) {
    let Some(store) = global() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Html(render_decide_html("Servis Hazır Değil", "<p>Veritabanı başlatılmadı.</p>")),
        );
    };
    let approve = form.decision.trim().eq_ignore_ascii_case("approve");
    let reject = form.decision.trim().eq_ignore_ascii_case("reject");
    if !approve && !reject {
        return (
            StatusCode::BAD_REQUEST,
            Html(render_decide_html("Geçersiz İşlem", "<p>Onay veya Red seçilmelidir.</p>")),
        );
    }

    match store.decide_by_token(&form.token, approve, &form.note) {
        Ok(result) => {
            if let Some(raw_key) = &result.issued_raw_key {
                let delivery = dispatch_user_approved_email(store, &result.user, raw_key).await;
                let esc_email = mailer::html_escape(&result.user.email);
                let esc_prefix = mailer::html_escape(result.user.key_prefix.as_deref().unwrap_or("woz_"));
                let msg = if delivery.delivered {
                    format!(
                        "<p><strong>{esc_email}</strong> onaylandı. Benzersiz anahtar (<code>{esc_prefix}...</code>) üretildi ve kullanıcının e-posta adresine gönderildi.</p>"
                    )
                } else {
                    let esc_key = mailer::html_escape(raw_key);
                    format!(
                        "<p><strong>{esc_email}</strong> onaylandı ve anahtar üretildi, ancak canlı e-posta sunucusu henüz yapılandırılmadığı için otomatik gönderilemedi:</p>\
                         <div class=\"box\"><code>{esc_key}</code></div>\
                         <p class=\"muted\">Bu anahtarı buradan bir kez kopyalayıp kullanıcıya iletebilirsiniz veya `werrozan.env` içine SMTP / Resend anahtarı ekleyebilirsiniz.</p>"
                    )
                };
                (StatusCode::OK, Html(render_decide_html("Üyelik Onaylandı", &msg)))
            } else {
                let esc_email = mailer::html_escape(&result.user.email);
                (
                    StatusCode::OK,
                    Html(render_decide_html(
                        "Başvuru Reddedildi",
                        &format!("<p><strong>{esc_email}</strong> başvurusu reddedildi. Herhangi bir API anahtarı <strong>oluşturulmadı</strong>.</p>"),
                    )),
                )
            }
        }
        Err((code, msg)) => (
            code,
            Html(render_decide_html(
                "İşlem Tamamlanamadı",
                &format!("<p>{}</p>", mailer::html_escape(&msg)),
            )),
        ),
    }
}

fn render_decide_html(title: &str, body_html: &str) -> String {
    let esc_title = mailer::html_escape(title);
    format!(
        "<!doctype html><html lang=\"tr\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>{esc_title} — WerrOzan</title>\
         <style>\
         body{{font-family:system-ui,-apple-system,sans-serif;background:#080c17;color:#e2e8f0;display:grid;place-items:center;min-height:100vh;margin:0;padding:16px}}\
         .card{{background:#0f172a;border:1px solid #1e293b;border-radius:14px;padding:24px;max-width:540px;width:100%;box-shadow:0 20px 50px rgba(0,0,0,.5)}}\
         h1{{margin:0 0 14px;font-size:20px;color:#38bdf8}}\
         .box{{background:#111827;border:1px solid #334155;border-radius:10px;padding:14px;margin:14px 0;display:grid;gap:8px;font-size:14px;word-break:break-word}}\
         .lbl{{color:#94a3b8;margin-right:6px}}\
         .muted{{color:#94a3b8;font-size:13px}}\
         .pill{{display:inline-block;padding:3px 10px;border-radius:999px;background:#1e293b;color:#38bdf8;font-size:12px}}\
         input{{width:100%;box-sizing:border-box;padding:10px 12px;border-radius:8px;border:1px solid #334155;background:#090d16;color:#f8fafc;margin-bottom:12px}}\
         .actions{{display:flex;gap:10px;flex-wrap:wrap}}\
         .btn{{border:0;border-radius:9px;padding:11px 16px;font-weight:700;cursor:pointer;font-size:14px}}\
         .ok{{background:#10b981;color:#052e16}}\
         .no{{background:#ef4444;color:#fff}}\
         </style></head><body><!--email_off--><div class=\"card\"><h1>{esc_title}</h1>{body_html}</div><!--/email_off--></body></html>"
    )
}

pub async fn me(Extension(principal): Extension<AuthPrincipal>) -> Json<Value> {
    Json(json!({
        "role": if principal.is_admin { "admin" } else { "member" },
        "is_admin": principal.is_admin,
        "user_id": principal.user_id,
        "email": principal.email,
        "name": principal.name,
        "key_prefix": principal.key_prefix,
    }))
}

// ---------- Admin Endpoints ----------

pub async fn admin_list_users() -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let store = require_store()?;
    let users = store
        .list_users()
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({
        "users": users,
        "mailer": mailer::config_summary(),
    })))
}

#[derive(Debug, Default, Deserialize)]
pub struct AdminDecisionBody {
    #[serde(default)]
    pub note: String,
}

pub async fn admin_approve_user(
    AxumPath(user_id): AxumPath<String>,
    body: Option<Json<AdminDecisionBody>>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let store = require_store()?;
    let note = body.map(|b| b.0.note).unwrap_or_default();
    let result = store
        .decide_by_user_id(&user_id, true, "admin_panel", &note)
        .map_err(|(c, m)| api_error(c, m))?;

    let raw_key = result.issued_raw_key.unwrap_or_default();
    let delivery = dispatch_user_approved_email(store, &result.user, &raw_key).await;

    Ok(Json(json!({
        "ok": true,
        "user": result.user,
        "email_delivered": delivery.delivered,
        "mail_transport": delivery.transport,
        "mail_error": delivery.error,
        // Returned to admin ONCE only if outbound mail is unconfigured/failed so admin can hand it over if testing without SMTP.
        "issued_key_once": if delivery.delivered { Value::Null } else { Value::String(raw_key) },
    })))
}

pub async fn admin_reject_user(
    AxumPath(user_id): AxumPath<String>,
    body: Option<Json<AdminDecisionBody>>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let store = require_store()?;
    let note = body.map(|b| b.0.note).unwrap_or_default();
    let result = store
        .decide_by_user_id(&user_id, false, "admin_panel", &note)
        .map_err(|(c, m)| api_error(c, m))?;
    Ok(Json(json!({
        "ok": true,
        "user": result.user,
        "key_created": false,
    })))
}

pub async fn admin_revoke_user(
    AxumPath(user_id): AxumPath<String>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let store = require_store()?;
    let user = store
        .revoke_user(&user_id, "admin_panel")
        .map_err(|(c, m)| api_error(c, m))?;
    Ok(Json(json!({
        "ok": true,
        "user": user,
    })))
}

pub async fn admin_outbox() -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let store = require_store()?;
    let items = store
        .list_outbox()
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({
        "mailer": mailer::config_summary(),
        "items": items,
    })))
}

// ---------- Public Demo Song Endpoints ----------

fn resolve_demo_audio_path(state: &AppState) -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("WERROZAN_DEMO_AUDIO") {
        let p = PathBuf::from(explicit.trim());
        if p.is_file() {
            return Some(p);
        }
    }
    let root = studio_data_root().unwrap_or_else(|| state.library.media_dir().to_path_buf());
    let candidate = root.join("media").join(format!("{DEFAULT_DEMO_SONG_ID}.flac"));
    if candidate.is_file() {
        return Some(candidate);
    }
    let direct = state.library.media_dir().join(format!("{DEFAULT_DEMO_SONG_ID}.flac"));
    if direct.is_file() {
        return Some(direct);
    }
    None
}

pub async fn demo_info(State(state): State<AppState>) -> Json<Value> {
    let path = resolve_demo_audio_path(&state);
    let size_bytes = path
        .as_ref()
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .unwrap_or(0);
    Json(json!({
        "title": "Mavi gecelerde yükselir sesim",
        "song_id": DEFAULT_DEMO_SONG_ID,
        "engine_id": "yue2-cpp",
        "profile_id": "light (Q5_K_M)",
        "duration_seconds": 30.0,
        "format": "flac",
        "size_bytes": size_bytes,
        "style": "Turkish, acoustic Anatolian folk ballad, warm vocal, gentle baglama and acoustic guitar, serene melody, 78 BPM",
        "lyrics": "[Verse 1]\nAkdeniz kıyısı sessiz ve serin\nYıldızlar parıldar gökyüzünde derin\n\n[Chorus]\nMavi gecelerde yükselir sesim\nUmutla uyanır yeni nefesim",
        "api_audio_url": format!("{}/v1/demo/audio", mailer::public_api_url()),
        "pages_audio_url": format!("{}/demo/{DEFAULT_DEMO_FILENAME}", mailer::public_web_url()),
        "available_on_server": path.is_some(),
    }))
}

pub async fn demo_audio(
    State(state): State<AppState>,
    request: Request,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    let path = resolve_demo_audio_path(&state)
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Demo ses dosyası sunucuda bulunamadı.".into()))?;
    let mut resp = ServeFile::new(&path).oneshot(request).await.into_response();
    resp.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        header::HeaderValue::from_static("inline; filename=\"werrozan-demo-mavi-gecelerde.flac\""),
    );
    Ok(resp)
}

fn is_safe_public_demo_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 120
        && name.ends_with(".flac")
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub async fn demo_named_file(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
    request: Request,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    if !is_safe_public_demo_name(&name) {
        return Err(api_error(StatusCode::BAD_REQUEST, "Geçersiz dosya adı.".into()));
    }
    let root = studio_data_root().unwrap_or_else(|| state.library.media_dir().to_path_buf());
    let path = root.join("public-demo").join(&name);
    if !path.is_file() {
        return Err(api_error(StatusCode::NOT_FOUND, "Paylaşılan ses dosyası bulunamadı.".into()));
    }
    let mut resp = ServeFile::new(&path).oneshot(request).await.into_response();
    if let Ok(val) = header::HeaderValue::from_str(&format!("inline; filename=\"{name}\"")) {
        resp.headers_mut().insert(header::CONTENT_DISPOSITION, val);
    }
    Ok(resp)
}

// ---------- Per-User Media Library (Max 10 Main Songs + Unique Makam Variants) ----------

pub const MAX_USER_SONGS: usize = 10;
pub const KOMA_TTL_SECS: i64 = 30 * 60;

#[derive(Debug, Clone, Serialize)]
pub struct UserKomaVariantRecord {
    pub id: String,
    pub song_id: String,
    pub owner_id: String,
    pub makam_id: String,
    pub makam_name: String,
    pub strength: f64,
    #[serde(skip_serializing)]
    pub file_path: String,
    pub share_token: String,
    pub share_url: String,
    pub peaks_shifted: i64,
    pub mean_cent_correction: f64,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserSongRecord {
    pub id: String,
    pub owner_id: String,
    pub owner_email: Option<String>,
    pub title: String,
    pub style: String,
    pub lyrics: String,
    pub prompt_description: String,
    pub locale: String,
    pub cot: String,
    pub steps: i64,
    pub duration_seconds: f64,
    pub format: String,
    pub library_song_id: String,
    #[serde(skip_serializing)]
    pub file_path: String,
    pub share_token: String,
    pub share_url: String,
    pub created_at: i64,
    pub koma_variants: Vec<UserKomaVariantRecord>,
}

pub struct NewUserSong {
    pub id: String,
    pub owner_id: String,
    pub owner_email: Option<String>,
    pub title: String,
    pub style: String,
    pub lyrics: String,
    pub prompt_description: String,
    pub locale: String,
    pub cot: String,
    pub steps: i64,
    pub duration_seconds: f64,
    pub format: String,
    pub library_song_id: String,
    pub file_path: String,
}

pub struct NewKomaVariant {
    pub song_id: String,
    pub owner_id: String,
    pub makam_id: String,
    pub makam_name: String,
    pub strength: f64,
    pub file_path: String,
    pub peaks_shifted: i64,
    pub mean_cent_correction: f64,
}

pub fn share_url_for(token: &str, format: &str) -> String {
    let ext = format.trim().trim_start_matches('.').to_ascii_lowercase();
    let ext = if ext.is_empty() { "flac" } else { ext.as_str() };
    format!("{}/v1/media/share/{token}.{ext}", mailer::public_api_url())
}

pub fn principal_owner_info(principal: &AuthPrincipal) -> (String, Option<String>, String) {
    if principal.is_admin {
        (
            "admin".to_string(),
            Some(mailer::admin_email()),
            "WerrOzan Yönetici".to_string(),
        )
    } else {
        (
            principal.user_id.clone().unwrap_or_else(|| "member".to_string()),
            principal.email.clone(),
            principal.name.clone().unwrap_or_else(|| "Üye".to_string()),
        )
    }
}

pub fn user_media_dir(owner_id: &str) -> PathBuf {
    let safe_owner: String = owner_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let root = studio_data_root().unwrap_or_else(|| PathBuf::from("."));
    root.join("user_media").join(safe_owner)
}

fn remove_file_if_exists(path: &str) {
    let p = Path::new(path);
    if p.is_file() {
        let _ = std::fs::remove_file(p);
    }
}

impl AuthStore {
    pub fn record_user_song(&self, item: NewUserSong) -> Result<UserSongRecord> {
        let now = now_unix();
        let share_token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let mut stale_files: Vec<String> = Vec::new();
        {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT id, file_path FROM user_songs WHERE owner_id = ?1 ORDER BY created_at ASC, rowid ASC",
            )?;
            let existing: Vec<(String, String)> = stmt
                .query_map(params![&item.owner_id], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(stmt);

            let excess = (existing.len() + 1).saturating_sub(MAX_USER_SONGS);
            for (old_id, old_path) in existing.into_iter().take(excess) {
                let mut vstmt = conn.prepare("SELECT file_path FROM user_koma_variants WHERE song_id = ?1")?;
                let vpaths: Vec<String> = vstmt
                    .query_map(params![&old_id], |row| row.get(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                drop(vstmt);
                stale_files.extend(vpaths);
                stale_files.push(old_path);
                conn.execute("DELETE FROM user_songs WHERE id = ?1", params![&old_id])?;
            }

            conn.execute(
                "INSERT INTO user_songs (id, owner_id, owner_email, title, style, lyrics, prompt_description, locale, cot, steps, duration_seconds, format, library_song_id, file_path, share_token, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    &item.id,
                    &item.owner_id,
                    &item.owner_email,
                    &item.title,
                    &item.style,
                    &item.lyrics,
                    &item.prompt_description,
                    &item.locale,
                    &item.cot,
                    item.steps,
                    item.duration_seconds,
                    &item.format,
                    &item.library_song_id,
                    &item.file_path,
                    &share_token,
                    now,
                ],
            )?;
        }

        for p in stale_files {
            remove_file_if_exists(&p);
        }

        let share_url = share_url_for(&share_token, &item.format);
        Ok(UserSongRecord {
            id: item.id,
            owner_id: item.owner_id,
            owner_email: item.owner_email,
            title: item.title,
            style: item.style,
            lyrics: item.lyrics,
            prompt_description: item.prompt_description,
            locale: item.locale,
            cot: item.cot,
            steps: item.steps,
            duration_seconds: item.duration_seconds,
            format: item.format,
            library_song_id: item.library_song_id,
            file_path: item.file_path,
            share_token,
            share_url,
            created_at: now,
            koma_variants: Vec::new(),
        })
    }

    pub fn upsert_koma_variant(&self, v: NewKomaVariant) -> Result<UserKomaVariantRecord> {
        let now = now_unix();
        let makam_id = v.makam_id.trim().to_ascii_lowercase();
        let id = Uuid::new_v4().to_string();
        let share_token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let mut old_path_to_remove: Option<String> = None;

        let (final_id, final_token) = {
            let conn = self.conn.lock().unwrap();
            let owned: Option<String> = conn
                .query_row(
                    "SELECT owner_id FROM user_songs WHERE id = ?1",
                    params![&v.song_id],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(song_owner) = owned else {
                anyhow::bail!("Şarkı bulunamadı.");
            };
            if song_owner != v.owner_id {
                anyhow::bail!("Bu şarkı üzerinde işlem yetkiniz yok.");
            }

            let existing: Option<(String, String, String)> = conn
                .query_row(
                    "SELECT id, file_path, share_token FROM user_koma_variants WHERE song_id = ?1 AND makam_id = ?2",
                    params![&v.song_id, &makam_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;

            if let Some((ex_id, ex_path, ex_token)) = existing {
                if ex_path != v.file_path {
                    old_path_to_remove = Some(ex_path);
                }
                conn.execute(
                    "UPDATE user_koma_variants SET makam_name = ?1, strength = ?2, file_path = ?3, peaks_shifted = ?4, mean_cent_correction = ?5, created_at = ?6 WHERE id = ?7",
                    params![&v.makam_name, v.strength, &v.file_path, v.peaks_shifted, v.mean_cent_correction, now, &ex_id],
                )?;
                (ex_id, ex_token)
            } else {
                conn.execute(
                    "INSERT INTO user_koma_variants (id, song_id, owner_id, makam_id, makam_name, strength, file_path, share_token, peaks_shifted, mean_cent_correction, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        &id,
                        &v.song_id,
                        &v.owner_id,
                        &makam_id,
                        &v.makam_name,
                        v.strength,
                        &v.file_path,
                        &share_token,
                        v.peaks_shifted,
                        v.mean_cent_correction,
                        now,
                    ],
                )?;
                (id, share_token)
            }
        };

        if let Some(old_p) = old_path_to_remove {
            remove_file_if_exists(&old_p);
        }

        let share_url = share_url_for(&final_token, "flac");
        Ok(UserKomaVariantRecord {
            id: final_id,
            song_id: v.song_id,
            owner_id: v.owner_id,
            makam_id,
            makam_name: v.makam_name,
            strength: v.strength,
            file_path: v.file_path,
            share_token: final_token,
            share_url,
            peaks_shifted: v.peaks_shifted,
            mean_cent_correction: v.mean_cent_correction,
            created_at: now,
            expires_at: now + KOMA_TTL_SECS,
        })
    }

    pub fn purge_expired_koma_variants(&self) -> Result<usize> {
        let cutoff = now_unix() - KOMA_TTL_SECS;
        let stale: Vec<String> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn.prepare("SELECT file_path FROM user_koma_variants WHERE created_at <= ?1")?;
            let paths: Vec<String> = stmt
                .query_map(params![cutoff], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(stmt);
            conn.execute("DELETE FROM user_koma_variants WHERE created_at <= ?1", params![cutoff])?;
            paths
        };
        let count = stale.len();
        for p in stale {
            remove_file_if_exists(&p);
        }
        Ok(count)
    }

    pub fn list_user_songs(&self, owner_id: &str) -> Result<Vec<UserSongRecord>> {
        let _ = self.purge_expired_koma_variants();
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, owner_id, owner_email, title, style, lyrics, prompt_description, locale, cot, steps, duration_seconds, format, library_song_id, file_path, share_token, created_at
             FROM user_songs WHERE owner_id = ?1 ORDER BY created_at DESC, rowid DESC",
        )?;
        let mut songs: Vec<UserSongRecord> = stmt
            .query_map(params![owner_id], |row| {
                let format: String = row.get(11)?;
                let share_token: String = row.get(14)?;
                Ok(UserSongRecord {
                    id: row.get(0)?,
                    owner_id: row.get(1)?,
                    owner_email: row.get(2)?,
                    title: row.get(3)?,
                    style: row.get(4)?,
                    lyrics: row.get(5)?,
                    prompt_description: row.get(6)?,
                    locale: row.get(7)?,
                    cot: row.get(8)?,
                    steps: row.get(9)?,
                    duration_seconds: row.get(10)?,
                    format: format.clone(),
                    library_song_id: row.get(12)?,
                    file_path: row.get(13)?,
                    share_url: share_url_for(&share_token, &format),
                    share_token,
                    created_at: row.get(15)?,
                    koma_variants: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        let mut vstmt = conn.prepare(
            "SELECT id, song_id, owner_id, makam_id, makam_name, strength, file_path, share_token, peaks_shifted, mean_cent_correction, created_at
             FROM user_koma_variants WHERE song_id = ?1 ORDER BY created_at ASC",
        )?;
        for song in &mut songs {
            let vars: Vec<UserKomaVariantRecord> = vstmt
                .query_map(params![&song.id], |row| {
                    let share_token: String = row.get(7)?;
                    let created_at: i64 = row.get(10)?;
                    Ok(UserKomaVariantRecord {
                        id: row.get(0)?,
                        song_id: row.get(1)?,
                        owner_id: row.get(2)?,
                        makam_id: row.get(3)?,
                        makam_name: row.get(4)?,
                        strength: row.get(5)?,
                        file_path: row.get(6)?,
                        share_url: share_url_for(&share_token, "flac"),
                        share_token,
                        peaks_shifted: row.get(8)?,
                        mean_cent_correction: row.get(9)?,
                        created_at,
                        expires_at: created_at + KOMA_TTL_SECS,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            song.koma_variants = vars;
        }
        Ok(songs)
    }

    pub fn get_user_song_for_owner(&self, song_id: &str, owner_id: &str) -> Result<Option<UserSongRecord>> {
        let songs = self.list_user_songs(owner_id)?;
        Ok(songs.into_iter().find(|s| s.id == song_id))
    }

    pub fn delete_user_song(&self, song_id: &str, owner_id: &str) -> Result<bool> {
        let mut to_remove: Vec<String> = Vec::new();
        let deleted = {
            let conn = self.conn.lock().unwrap();
            let row: Option<(String, String)> = conn
                .query_row(
                    "SELECT owner_id, file_path FROM user_songs WHERE id = ?1",
                    params![song_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let Some((song_owner, main_path)) = row else {
                return Ok(false);
            };
            if song_owner != owner_id {
                return Ok(false);
            }
            let mut vstmt = conn.prepare("SELECT file_path FROM user_koma_variants WHERE song_id = ?1")?;
            let vpaths: Vec<String> = vstmt
                .query_map(params![song_id], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(vstmt);
            to_remove.extend(vpaths);
            to_remove.push(main_path);
            let n = conn.execute("DELETE FROM user_songs WHERE id = ?1 AND owner_id = ?2", params![song_id, owner_id])?;
            n > 0
        };
        for p in to_remove {
            remove_file_if_exists(&p);
        }
        Ok(deleted)
    }

    pub fn resolve_share_token(&self, raw_token: &str) -> Result<Option<(String, PathBuf)>> {
        let token = raw_token
            .trim()
            .strip_suffix(".flac")
            .or_else(|| raw_token.trim().strip_suffix(".wav"))
            .unwrap_or(raw_token.trim());
        if token.is_empty() || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Ok(None);
        }
        let _ = self.purge_expired_koma_variants();
        let conn = self.conn.lock().unwrap();
        if let Some((title, fmt, path)) = conn
            .query_row(
                "SELECT title, format, file_path FROM user_songs WHERE share_token = ?1",
                params![token],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
            )
            .optional()?
        {
            let slug: String = title
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect();
            let slug = slug.trim_matches('-');
            let fname = if slug.is_empty() {
                format!("werrozan-track.{fmt}")
            } else {
                format!("{slug}.{fmt}")
            };
            return Ok(Some((fname, PathBuf::from(path))));
        }
        if let Some((makam_id, path)) = conn
            .query_row(
                "SELECT makam_id, file_path FROM user_koma_variants WHERE share_token = ?1",
                params![token],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?
        {
            return Ok(Some((format!("werrozan-{makam_id}.flac"), PathBuf::from(path))));
        }
        Ok(None)
    }
}

pub async fn delete_user_library_song(
    Extension(principal): Extension<AuthPrincipal>,
    AxumPath(song_id): AxumPath<String>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let store = require_store()?;
    let (owner_id, _, _) = principal_owner_info(&principal);
    let deleted = store
        .delete_user_song(&song_id, &owner_id)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !deleted {
        return Err(api_error(StatusCode::NOT_FOUND, "Şarkı bulunamadı veya size ait değil.".into()));
    }
    Ok(Json(json!({ "ok": true, "deleted_id": song_id })))
}

pub async fn share_media_file(
    AxumPath(token): AxumPath<String>,
    request: Request,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    let store = require_store()?;
    let Some((filename, path)) = store
        .resolve_share_token(&token)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    else {
        return Err(api_error(StatusCode::NOT_FOUND, "Paylaşım bağlantısı bulunamadı.".into()));
    };
    if !path.is_file() {
        return Err(api_error(StatusCode::NOT_FOUND, "Ses dosyası diskte bulunamadı.".into()));
    }
    let mut resp = ServeFile::new(&path).oneshot(request).await.into_response();
    if let Ok(val) = header::HeaderValue::from_str(&format!("inline; filename=\"{filename}\"")) {
        resp.headers_mut().insert(header::CONTENT_DISPOSITION, val);
    }
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_validation_and_registration_create_no_key_until_approved() {
        assert!(normalize_and_validate_email("User@Example.COM").is_ok());
        assert_eq!(normalize_and_validate_email(" User@Example.COM ").unwrap(), "user@example.com");
        assert!(normalize_and_validate_email("bad\r\n@example.com").is_err());
        assert!(normalize_and_validate_email("noatsign.com").is_err());

        let tmp = tempfile::tempdir().unwrap();
        let store = AuthStore::open(&tmp.path().join("membership.sqlite")).unwrap();

        let reg = store
            .register_applicant("ozan.test@example.com", "Ozan", "Makam test", "127.0.0.1")
            .unwrap();
        assert!(reg.approval_token.is_some());

        // Pending user has NO API key
        let st = store.status_by_request_token(&reg.request_token).unwrap().unwrap();
        assert_eq!(st.status, "pending");
        assert!(st.key_prefix.is_none());

        // Rejecting creates NO key and blocks authentication
        let rej = store
            .register_applicant("reject.me@example.com", "Red", "", "127.0.0.2")
            .unwrap();
        let rej_res = store
            .decide_by_token(rej.approval_token.as_ref().unwrap(), false, "no")
            .unwrap();
        assert_eq!(rej_res.user.status, "rejected");
        assert!(rej_res.issued_raw_key.is_none());
        assert!(rej_res.user.key_prefix.is_none());

        // Approving generates woz_ key hashed in DB, active immediately, and revocable
        let app_res = store
            .decide_by_token(reg.approval_token.as_ref().unwrap(), true, "welcome")
            .unwrap();
        assert_eq!(app_res.user.status, "approved");
        let raw_key = app_res.issued_raw_key.expect("approved user receives key");
        assert!(raw_key.starts_with("woz_"));
        let member = store.verify_active_key(&raw_key).expect("active key authenticates");
        assert_eq!(member.email, "ozan.test@example.com");

        // Second reuse of same single-use approval token fails
        assert!(store
            .decide_by_token(reg.approval_token.as_ref().unwrap(), true, "replay")
            .is_err());

        // Revoking immediately invalidates key
        let revoked = store.revoke_user(&app_res.user.id, "admin").unwrap();
        assert_eq!(revoked.status, "revoked");
        assert!(store.verify_active_key(&raw_key).is_none());
    }

    #[test]
    fn user_song_quota_evicts_oldest_and_koma_variants_are_unique_per_makam() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AuthStore::open(&tmp.path().join("membership.sqlite")).unwrap();

        // Create 11 songs for user-a; oldest (#1) must be evicted and #2..#11 kept
        for i in 1..=11 {
            let fpath = tmp.path().join(format!("song-{i}.flac"));
            std::fs::write(&fpath, format!("audio-{i}")).unwrap();
            store
                .record_user_song(NewUserSong {
                    id: format!("song-{i}"),
                    owner_id: "user-a".into(),
                    owner_email: Some("a@example.com".into()),
                    title: format!("Parca {i}"),
                    style: "Turkish folk".into(),
                    lyrics: "Soz".into(),
                    prompt_description: "Deneme".into(),
                    locale: "tr".into(),
                    cot: "full".into(),
                    steps: 16,
                    duration_seconds: 30.0,
                    format: "flac".into(),
                    library_song_id: format!("lib-{i}"),
                    file_path: fpath.to_string_lossy().into_owned(),
                })
                .unwrap();
        }

        let songs = store.list_user_songs("user-a").unwrap();
        assert_eq!(songs.len(), MAX_USER_SONGS);
        assert!(!songs.iter().any(|s| s.id == "song-1"));
        assert!(songs.iter().any(|s| s.id == "song-2"));
        assert!(songs.iter().any(|s| s.id == "song-11"));
        assert!(!tmp.path().join("song-1.flac").exists());

        // User B cannot see or delete User A's songs
        assert!(store.list_user_songs("user-b").unwrap().is_empty());
        assert!(!store.delete_user_song("song-11", "user-b").unwrap());

        // Upserting kurdi twice keeps 1 kurdi; adding ussak + huseyni keeps 3 total
        for (makam, name, idx) in [("kurdi", "Kürdî", 1), ("kurdi", "Kürdî", 2), ("ussak", "Uşşak", 1), ("huseyni", "Hüseynî", 1)] {
            let vpath = tmp.path().join(format!("song-11-{makam}-{idx}.flac"));
            std::fs::write(&vpath, b"koma").unwrap();
            store
                .upsert_koma_variant(NewKomaVariant {
                    song_id: "song-11".into(),
                    owner_id: "user-a".into(),
                    makam_id: makam.into(),
                    makam_name: name.into(),
                    strength: 0.85,
                    file_path: vpath.to_string_lossy().into_owned(),
                    peaks_shifted: 120,
                    mean_cent_correction: 11.4,
                })
                .unwrap();
        }

        let updated = store.get_user_song_for_owner("song-11", "user-a").unwrap().unwrap();
        assert_eq!(updated.koma_variants.len(), 3);
        assert_eq!(updated.koma_variants.iter().filter(|v| v.makam_id == "kurdi").count(), 1);

        // Backdate one variant past KOMA_TTL_SECS (30 min) and verify purge removes DB row + file
        {
            let conn = store.conn.lock().unwrap();
            let expired_ts = now_unix() - KOMA_TTL_SECS - 5;
            conn.execute(
                "UPDATE user_koma_variants SET created_at = ?1 WHERE song_id = 'song-11' AND makam_id = 'ussak'",
                params![expired_ts],
            )
            .unwrap();
        }
        assert_eq!(store.purge_expired_koma_variants().unwrap(), 1);
        assert!(!tmp.path().join("song-11-ussak-1.flac").exists());
        let after_purge = store.get_user_song_for_owner("song-11", "user-a").unwrap().unwrap();
        assert_eq!(after_purge.koma_variants.len(), 2);
    }
}
