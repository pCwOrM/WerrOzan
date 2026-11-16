//! Transactional e-mail dispatcher for WerrOzan membership workflows.
//!
//! Supports:
//! 1. Authenticated SMTP TLS/STARTTLS (`WERROZAN_SMTP_HOST`, `WERROZAN_SMTP_PORT`,
//!    `WERROZAN_SMTP_USER`, `WERROZAN_SMTP_PASS`) — works with Google Workspace /
//!    Gmail App Passwords (`smtp.gmail.com:587`) or Plesk/custom SMTP relays.
//! 2. Resend HTTPS API (`RESEND_API_KEY`) — ideal with Cloudflare-managed `werr.io`.
//! 3. Clean `queued_unconfigured` fallback so registrations & approval links still
//!    operate safely via Admin Outbox before mail secrets are configured.

use std::time::Duration;

use lettre::{
    message::{Mailbox, MultiPart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use serde::Serialize;

pub const DEFAULT_ADMIN_EMAIL: &str = "pcworm@pcworm.net";
pub const DEFAULT_PUBLIC_WEB_URL: &str = "https://ozan.werr.io";
pub const DEFAULT_PUBLIC_API_URL: &str = "https://ozan-api.werr.io";

#[derive(Debug, Clone, Serialize)]
pub struct MailerConfigSummary {
    pub configured: bool,
    pub transport: String,
    pub admin_email: String,
    pub mail_from: String,
    pub public_web_url: String,
    pub public_api_url: String,
}

#[derive(Debug, Clone)]
pub struct EmailPayload {
    pub to: String,
    pub subject: String,
    pub text: String,
    pub html: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MailDeliveryResult {
    pub delivered: bool,
    pub transport: String,
    pub error: Option<String>,
}

pub fn admin_email() -> String {
    env_non_empty("WERROZAN_ADMIN_EMAIL").unwrap_or_else(|| DEFAULT_ADMIN_EMAIL.to_string())
}

pub fn public_web_url() -> String {
    env_non_empty("WERROZAN_PUBLIC_URL")
        .unwrap_or_else(|| DEFAULT_PUBLIC_WEB_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

pub fn public_api_url() -> String {
    env_non_empty("WERROZAN_API_PUBLIC_URL")
        .unwrap_or_else(|| DEFAULT_PUBLIC_API_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn smtp_settings() -> Option<(String, u16, String, String, bool)> {
    let host = env_non_empty("WERROZAN_SMTP_HOST")?;
    let user = env_non_empty("WERROZAN_SMTP_USER")?;
    let pass = env_non_empty("WERROZAN_SMTP_PASS")?;
    let port = env_non_empty("WERROZAN_SMTP_PORT")
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(587);
    let tls_mode = env_non_empty("WERROZAN_SMTP_TLS").unwrap_or_default().to_ascii_lowercase();
    let implicit_tls = tls_mode == "tls" || tls_mode == "smtps" || (tls_mode != "starttls" && port == 465);
    Some((host, port, user, pass, implicit_tls))
}

fn mail_from_address() -> String {
    if let Some(explicit) = env_non_empty("WERROZAN_MAIL_FROM") {
        return explicit;
    }
    if let Some((_, _, user, _, _)) = smtp_settings() {
        if user.contains('@') {
            return format!("WerrOzan Studio <{user}>");
        }
    }
    "WerrOzan Studio <ozan@werr.io>".to_string()
}

pub fn config_summary() -> MailerConfigSummary {
    let (configured, transport) = if let Some((host, port, _, _, _)) = smtp_settings() {
        (true, format!("smtp:{host}:{port}"))
    } else if env_non_empty("RESEND_API_KEY").is_some() {
        (true, "resend_api".to_string())
    } else {
        (false, "queued_unconfigured".to_string())
    };
    MailerConfigSummary {
        configured,
        transport,
        admin_email: admin_email(),
        mail_from: mail_from_address(),
        public_web_url: public_web_url(),
        public_api_url: public_api_url(),
    }
}

pub fn sanitize_header_line(value: &str) -> String {
    value.chars().filter(|c| !matches!(c, '\r' | '\n' | '\0')).collect::<String>().trim().to_string()
}

pub fn html_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 16);
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn redact_woz_keys(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(idx) = rest.find("woz_") {
        result.push_str(&rest[..idx]);
        let token_tail = &rest[idx..];
        let token_len = token_tail
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
            .count();
        if token_len >= 16 {
            let prefix_len = token_len.min(12);
            result.push_str(&token_tail[..prefix_len]);
            result.push_str("...[REDACTED]");
        } else {
            result.push_str(&token_tail[..token_len.max(4)]);
        }
        rest = &token_tail[token_len.max(4)..];
    }
    result.push_str(rest);
    result
}

/// Sends an email over SMTP or Resend if configured; never fakes delivery.
pub async fn send_email(payload: &EmailPayload) -> MailDeliveryResult {
    let to = sanitize_header_line(&payload.to);
    let subject = sanitize_header_line(&payload.subject);
    let from = sanitize_header_line(&mail_from_address());

    if let Some((host, port, user, pass, implicit_tls)) = smtp_settings() {
        match send_via_smtp(&host, port, &user, &pass, implicit_tls, &from, &to, &subject, &payload.text, &payload.html).await {
            Ok(()) => {
                return MailDeliveryResult {
                    delivered: true,
                    transport: format!("smtp:{host}:{port}"),
                    error: None,
                };
            }
            Err(err) => {
                eprintln!("[mailer] SMTP send error to {to}: {err}");
                return MailDeliveryResult {
                    delivered: false,
                    transport: format!("smtp:{host}:{port}"),
                    error: Some(err),
                };
            }
        }
    }

    if let Some(api_key) = env_non_empty("RESEND_API_KEY") {
        match send_via_resend(&api_key, &from, &to, &subject, &payload.text, &payload.html).await {
            Ok(()) => {
                return MailDeliveryResult {
                    delivered: true,
                    transport: "resend_api".to_string(),
                    error: None,
                };
            }
            Err(err) => {
                eprintln!("[mailer] Resend API send error to {to}: {err}");
                return MailDeliveryResult {
                    delivered: false,
                    transport: "resend_api".to_string(),
                    error: Some(err),
                };
            }
        }
    }

    MailDeliveryResult {
        delivered: false,
        transport: "queued_unconfigured".to_string(),
        error: Some("SMTP veya RESEND_API_KEY tanımlı değil; e-posta yönetici kuyruğunda tutuldu.".to_string()),
    }
}

async fn send_via_smtp(
    host: &str,
    port: u16,
    user: &str,
    pass: &str,
    implicit_tls: bool,
    from: &str,
    to: &str,
    subject: &str,
    text: &str,
    html: &str,
) -> Result<(), String> {
    let from_mb: Mailbox = from.parse().map_err(|e| format!("invalid from mailbox: {e}"))?;
    let to_mb: Mailbox = to.parse().map_err(|e| format!("invalid to mailbox: {e}"))?;
    let email = Message::builder()
        .from(from_mb)
        .to(to_mb)
        .subject(subject)
        .multipart(MultiPart::alternative_plain_html(text.to_string(), html.to_string()))
        .map_err(|e| format!("build email error: {e}"))?;

    let creds = Credentials::new(user.to_string(), pass.to_string());
    let builder = if implicit_tls {
        AsyncSmtpTransport::<Tokio1Executor>::relay(host)
            .map_err(|e| format!("smtp relay config error: {e}"))?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
            .map_err(|e| format!("smtp starttls config error: {e}"))?
    };

    let mailer = builder
        .port(port)
        .credentials(creds)
        .timeout(Some(Duration::from_secs(15)))
        .build();

    mailer
        .send(email)
        .await
        .map(|_| ())
        .map_err(|e| format!("smtp transport error: {e}"))
}

async fn send_via_resend(
    api_key: &str,
    from: &str,
    to: &str,
    subject: &str,
    text: &str,
    html: &str,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("http client build error: {e}"))?;
    let resp = client
        .post("https://api.resend.com/emails")
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "from": from,
            "to": [to],
            "subject": subject,
            "text": text,
            "html": html,
        }))
        .send()
        .await
        .map_err(|e| format!("resend request failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("resend HTTP {status}: {body}"));
    }
    Ok(())
}

/// Optional webhook fire-and-forget notification when a new signup arrives.
pub async fn notify_admin_webhook(applicant_email: &str, applicant_name: &str, decide_url: &str) {
    let Some(webhook) = env_non_empty("WERROZAN_ADMIN_WEBHOOK_URL") else {
        return;
    };
    let Ok(client) = reqwest::Client::builder().timeout(Duration::from_secs(8)).build() else {
        return;
    };
    let msg = format!(
        "[WerrOzan] Yeni üyelik başvurusu: {applicant_email} ({applicant_name}) -> Onay/Red: {decide_url}"
    );
    let _ = client
        .post(&webhook)
        .json(&serde_json::json!({
            "content": msg,
            "text": msg,
            "applicant_email": applicant_email,
            "applicant_name": applicant_name,
            "decide_url": decide_url,
        }))
        .send()
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_and_redact_prevent_injection_and_key_leaks() {
        assert_eq!(sanitize_header_line("a@b.com\r\nBcc: evil@x.com"), "a@b.comBcc: evil@x.com");
        assert_eq!(html_escape("<script>a&b</script>"), "&lt;script&gt;a&amp;b&lt;/script&gt;");
        let raw = "Keyiniz: woz_0123456789abcdef0123456789abcdef hazir";
        let masked = redact_woz_keys(raw);
        assert!(!masked.contains("abcdef0123456789abcdef"));
        assert!(masked.contains("woz_01234567...[REDACTED]"));
    }
}
