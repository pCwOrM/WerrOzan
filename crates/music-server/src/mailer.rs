//! Transactional e-mail dispatcher for WerrOzan membership workflows.
//!
//! Supports:
//! 1. Authenticated SMTP TLS/STARTTLS (`WERROZAN_SMTP_HOST`, `WERROZAN_SMTP_PORT`,
//!    `WERROZAN_SMTP_USER`, `WERROZAN_SMTP_PASS`) with optional 2048-bit RSA DKIM signing.
//! 2. Resend HTTPS API (`RESEND_API_KEY`) with native `bcc` support.
//! 3. Direct MX + 2048-bit RSA DKIM Relay (`direct_mx_dkim` via `~/.config/werrozan/dkim_ozan.pem`,
//!    `ozan._domainkey.werr.io`, `EHLO mail.teknosanat.com.tr`, SPF `ip4:173.212.201.159`).
//! 4. Automatic BCC (`bss`) routing to `ask@answerr.me` (`WERROZAN_BCC_EMAIL`) on user-facing
//!    application confirmation and API key delivery emails, without exposing the `Bcc` header
//!    in the MIME message seen by the primary recipient.

use std::{path::Path, time::Duration};

use lettre::{
    address::Envelope,
    message::{
        header::HeaderName, DkimCanonicalization, DkimCanonicalizationType, DkimConfig,
        DkimSigningAlgorithm, DkimSigningKey, Mailbox, MultiPart,
    },
    transport::smtp::{
        authentication::Credentials,
        client::{Tls, TlsParameters},
        extension::ClientId,
    },
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use serde::Serialize;

pub const DEFAULT_ADMIN_EMAIL: &str = "pcworm@pcworm.net";
pub const DEFAULT_BCC_EMAIL: &str = "ask@answerr.me";
pub const DEFAULT_PUBLIC_WEB_URL: &str = "https://ozan.werr.io";
pub const DEFAULT_PUBLIC_API_URL: &str = "https://ozan-api.werr.io";
pub const DEFAULT_EHLO_DOMAIN: &str = "mail.teknosanat.com.tr";
pub const DEFAULT_DKIM_KEY_PATH: &str = "/home/lexovian/.config/werrozan/dkim_ozan.pem";
pub const DEFAULT_DKIM_SELECTOR: &str = "ozan";
pub const DEFAULT_DKIM_DOMAIN: &str = "werr.io";

#[derive(Debug, Clone, Serialize)]
pub struct MailerConfigSummary {
    pub configured: bool,
    pub transport: String,
    pub dkim_ready: bool,
    pub admin_email: String,
    pub bcc_email: String,
    pub mail_from: String,
    pub public_web_url: String,
    pub public_api_url: String,
}

#[derive(Debug, Clone)]
pub struct EmailPayload {
    pub to: String,
    pub bcc: Vec<String>,
    pub subject: String,
    pub text: String,
    pub html: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MailDeliveryResult {
    pub delivered: bool,
    pub transport: String,
    pub bcc_delivered: Vec<String>,
    pub error: Option<String>,
}

pub fn admin_email() -> String {
    env_non_empty("WERROZAN_ADMIN_EMAIL").unwrap_or_else(|| DEFAULT_ADMIN_EMAIL.to_string())
}

pub fn bcc_email() -> String {
    env_non_empty("WERROZAN_BCC_EMAIL").unwrap_or_else(|| DEFAULT_BCC_EMAIL.to_string())
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

pub fn ehlo_domain() -> String {
    env_non_empty("WERROZAN_EHLO_DOMAIN").unwrap_or_else(|| DEFAULT_EHLO_DOMAIN.to_string())
}

fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn dkim_key_pem() -> Option<(String, String, String)> {
    let selector = env_non_empty("WERROZAN_DKIM_SELECTOR").unwrap_or_else(|| DEFAULT_DKIM_SELECTOR.to_string());
    let domain = env_non_empty("WERROZAN_DKIM_DOMAIN").unwrap_or_else(|| DEFAULT_DKIM_DOMAIN.to_string());
    let path_str = env_non_empty("WERROZAN_DKIM_KEY_PATH").unwrap_or_else(|| DEFAULT_DKIM_KEY_PATH.to_string());
    let path = Path::new(&path_str);
    if !path.is_file() {
        return None;
    }
    let pem = std::fs::read_to_string(path).ok()?;
    if pem.contains("BEGIN RSA PRIVATE KEY") {
        Some((selector, domain, pem))
    } else {
        None
    }
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
    let dkim_ready = dkim_key_pem().is_some();
    let (configured, transport) = if let Some((host, port, _, _, _)) = smtp_settings() {
        (true, format!("smtp:{host}:{port}"))
    } else if env_non_empty("RESEND_API_KEY").is_some() {
        (true, "resend_api".to_string())
    } else if dkim_ready {
        (true, "direct_mx_dkim".to_string())
    } else {
        (false, "queued_unconfigured".to_string())
    };
    MailerConfigSummary {
        configured,
        transport,
        dkim_ready,
        admin_email: admin_email(),
        bcc_email: bcc_email(),
        mail_from: mail_from_address(),
        public_web_url: public_web_url(),
        public_api_url: public_api_url(),
    }
}

pub fn sanitize_header_line(value: &str) -> String {
    value
        .chars()
        .filter(|c| !matches!(c, '\r' | '\n' | '\0'))
        .collect::<String>()
        .trim()
        .to_string()
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

/// Checks whether an email address belongs to an RFC 2606 synthetic test domain
/// (`@example.com`, `@example.org`, `@example.net`, `.invalid`, `.test`) so automated
/// E2E tests do not dispatch live internet messages to `pcworm@pcworm.net` or `ask@answerr.me`.
pub fn is_synthetic_test_address(text_or_addr: &str) -> bool {
    let lower = text_or_addr.to_ascii_lowercase();
    lower.contains("@example.com")
        || lower.contains("@example.org")
        || lower.contains("@example.net")
        || lower.ends_with(".invalid")
        || lower.ends_with(".test")
}

fn build_signed_mime_message(from: &str, to: &str, subject: &str, text: &str, html: &str) -> Result<(Mailbox, Mailbox, Vec<u8>), String> {
    let from_mb: Mailbox = from.parse().map_err(|e| format!("invalid from mailbox `{from}`: {e}"))?;
    let to_mb: Mailbox = to.parse().map_err(|e| format!("invalid to mailbox `{to}`: {e}"))?;
    let mut email = Message::builder()
        .from(from_mb.clone())
        .to(to_mb.clone())
        .subject(subject)
        .multipart(MultiPart::alternative_plain_html(text.to_string(), html.to_string()))
        .map_err(|e| format!("build email error: {e}"))?;

    if let Some((selector, domain, pem)) = dkim_key_pem() {
        if let Ok(signing_key) = DkimSigningKey::new(&pem, DkimSigningAlgorithm::Rsa) {
            let dkim_cfg = DkimConfig::new(
                selector,
                domain,
                signing_key,
                vec![
                    HeaderName::new_from_ascii_str("From"),
                    HeaderName::new_from_ascii_str("Subject"),
                    HeaderName::new_from_ascii_str("To"),
                    HeaderName::new_from_ascii_str("Date"),
                ],
                DkimCanonicalization {
                    header: DkimCanonicalizationType::Relaxed,
                    body: DkimCanonicalizationType::Relaxed,
                },
            );
            email.sign(&dkim_cfg);
        }
    }

    Ok((from_mb, to_mb, email.formatted()))
}

/// Sends an email (plus optional BCC recipients) over SMTP, Resend, or Direct MX + DKIM.
pub async fn send_email(payload: &EmailPayload) -> MailDeliveryResult {
    let to = sanitize_header_line(&payload.to);
    let subject = sanitize_header_line(&payload.subject);
    let from = sanitize_header_line(&mail_from_address());
    let mut bcc_list: Vec<String> = Vec::new();
    for b in &payload.bcc {
        let clean = sanitize_header_line(b);
        if !clean.is_empty()
            && !clean.eq_ignore_ascii_case(&to)
            && !bcc_list.iter().any(|existing| existing.eq_ignore_ascii_case(&clean))
        {
            bcc_list.push(clean);
        }
    }

    if is_synthetic_test_address(&to) || is_synthetic_test_address(&subject) || is_synthetic_test_address(&payload.text) {
        return MailDeliveryResult {
            delivered: false,
            transport: "test_sink_recorded".to_string(),
            bcc_delivered: bcc_list,
            error: Some("RFC 2606 test adresi (@example.*) algılandı; canlı e-posta yerine Outbox kaydı oluşturuldu.".to_string()),
        };
    }

    if let Some((host, port, user, pass, implicit_tls)) = smtp_settings() {
        match send_via_smtp(&host, port, &user, &pass, implicit_tls, &from, &to, &bcc_list, &subject, &payload.text, &payload.html).await {
            Ok(bcc_ok) => {
                return MailDeliveryResult {
                    delivered: true,
                    transport: format!("smtp:{host}:{port}"),
                    bcc_delivered: bcc_ok,
                    error: None,
                };
            }
            Err(err) => {
                eprintln!("[mailer] SMTP send error to {to}: {err}");
                return MailDeliveryResult {
                    delivered: false,
                    transport: format!("smtp:{host}:{port}"),
                    bcc_delivered: Vec::new(),
                    error: Some(err),
                };
            }
        }
    }

    if let Some(api_key) = env_non_empty("RESEND_API_KEY") {
        match send_via_resend(&api_key, &from, &to, &bcc_list, &subject, &payload.text, &payload.html).await {
            Ok(bcc_ok) => {
                return MailDeliveryResult {
                    delivered: true,
                    transport: "resend_api".to_string(),
                    bcc_delivered: bcc_ok,
                    error: None,
                };
            }
            Err(err) => {
                eprintln!("[mailer] Resend API send error to {to}: {err}");
                return MailDeliveryResult {
                    delivered: false,
                    transport: "resend_api".to_string(),
                    bcc_delivered: Vec::new(),
                    error: Some(err),
                };
            }
        }
    }

    if dkim_key_pem().is_some() {
        match send_via_direct_mx(&from, &to, &bcc_list, &subject, &payload.text, &payload.html).await {
            Ok(bcc_ok) => {
                return MailDeliveryResult {
                    delivered: true,
                    transport: "direct_mx_dkim".to_string(),
                    bcc_delivered: bcc_ok,
                    error: None,
                };
            }
            Err(err) => {
                eprintln!("[mailer] Direct MX send error to {to}: {err}");
                return MailDeliveryResult {
                    delivered: false,
                    transport: "direct_mx_dkim".to_string(),
                    bcc_delivered: Vec::new(),
                    error: Some(err),
                };
            }
        }
    }

    MailDeliveryResult {
        delivered: false,
        transport: "queued_unconfigured".to_string(),
        bcc_delivered: Vec::new(),
        error: Some("SMTP, RESEND_API_KEY veya DKIM anahtarı tanımlı değil; e-posta yönetici kuyruğunda tutuldu.".to_string()),
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
    bcc_list: &[String],
    subject: &str,
    text: &str,
    html: &str,
) -> Result<Vec<String>, String> {
    let (from_mb, to_mb, raw_bytes) = build_signed_mime_message(from, to, subject, text, html)?;
    let mut rcpts = vec![to_mb.email];
    let mut bcc_ok = Vec::new();
    for bcc in bcc_list {
        if let Ok(mb) = bcc.parse::<Mailbox>() {
            rcpts.push(mb.email);
            bcc_ok.push(bcc.clone());
        }
    }
    let envelope = Envelope::new(Some(from_mb.email), rcpts)
        .map_err(|e| format!("envelope build error: {e}"))?;

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
        .hello_name(ClientId::Domain(ehlo_domain()))
        .credentials(creds)
        .timeout(Some(Duration::from_secs(15)))
        .build();

    mailer
        .send_raw(&envelope, &raw_bytes)
        .await
        .map(|_| bcc_ok)
        .map_err(|e| format!("smtp transport error: {e}"))
}

async fn send_via_resend(
    api_key: &str,
    from: &str,
    to: &str,
    bcc_list: &[String],
    subject: &str,
    text: &str,
    html: &str,
) -> Result<Vec<String>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("http client build error: {e}"))?;
    let mut body = serde_json::json!({
        "from": from,
        "to": [to],
        "subject": subject,
        "text": text,
        "html": html,
    });
    if !bcc_list.is_empty() {
        body["bcc"] = serde_json::json!(bcc_list);
    }
    let resp = client
        .post("https://api.resend.com/emails")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("resend request failed: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = resp.text().await.unwrap_or_default();
        return Err(format!("resend HTTP {status}: {err_body}"));
    }
    Ok(bcc_list.to_vec())
}

async fn resolve_mx_hosts(domain: &str) -> Vec<String> {
    let clean_domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();
    let Ok(client) = reqwest::Client::builder().timeout(Duration::from_secs(6)).build() else {
        return vec![clean_domain];
    };
    let url = format!("https://cloudflare-dns.com/dns-query?name={clean_domain}&type=MX");
    let Ok(resp) = client
        .get(&url)
        .header("Accept", "application/dns-json")
        .send()
        .await
    else {
        return vec![clean_domain];
    };
    let Ok(json) = resp.json::<serde_json::Value>().await else {
        return vec![clean_domain];
    };
    let mut pairs: Vec<(u32, String)> = Vec::new();
    if let Some(answers) = json.get("Answer").and_then(|a| a.as_array()) {
        for ans in answers {
            if ans.get("type").and_then(|t| t.as_u64()) == Some(15) {
                if let Some(data) = ans.get("data").and_then(|d| d.as_str()) {
                    let mut parts = data.split_whitespace();
                    let pref = parts.next().and_then(|p| p.parse::<u32>().ok()).unwrap_or(50);
                    if let Some(host) = parts.next() {
                        let h = host.trim().trim_end_matches('.').to_ascii_lowercase();
                        if !h.is_empty() {
                            pairs.push((pref, h));
                        }
                    }
                }
            }
        }
    }
    pairs.sort_by_key(|(pref, _)| *pref);
    let mut hosts: Vec<String> = pairs.into_iter().map(|(_, h)| h).collect();
    if hosts.is_empty() {
        hosts.push(clean_domain);
    }
    hosts
}

async fn deliver_envelope_to_recipient(
    from_mb: &Mailbox,
    rcpt_addr_str: &str,
    raw_bytes: &[u8],
) -> Result<(), String> {
    let rcpt_mb: Mailbox = rcpt_addr_str
        .parse()
        .map_err(|e| format!("invalid recipient `{rcpt_addr_str}`: {e}"))?;
    let domain = rcpt_mb
        .email
        .domain()
        .to_string();
    let envelope = Envelope::new(Some(from_mb.email.clone()), vec![rcpt_mb.email])
        .map_err(|e| format!("envelope error: {e}"))?;

    let mx_hosts = resolve_mx_hosts(&domain).await;
    let hello = ClientId::Domain(ehlo_domain());
    let mut last_err = String::from("no MX host tried");

    for mx in mx_hosts.iter().take(3) {
        // 1. Try opportunistic STARTTLS first
        if let Ok(tls_params) = TlsParameters::new(mx.clone()) {
            let transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(mx.clone())
                .port(25)
                .hello_name(hello.clone())
                .tls(Tls::Opportunistic(tls_params))
                .timeout(Some(Duration::from_secs(12)))
                .build();
            match transport.send_raw(&envelope, raw_bytes).await {
                Ok(_) => return Ok(()),
                Err(err) => {
                    last_err = format!("MX {mx} (STARTTLS): {err}");
                }
            }
        }
        // 2. Fallback to plain port 25 if the remote MX presented a shared-hostname TLS cert
        let plain_transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(mx.clone())
            .port(25)
            .hello_name(hello.clone())
            .tls(Tls::None)
            .timeout(Some(Duration::from_secs(12)))
            .build();
        match plain_transport.send_raw(&envelope, raw_bytes).await {
            Ok(_) => return Ok(()),
            Err(err) => {
                last_err = format!("MX {mx} (Port25): {err}");
            }
        }
    }

    Err(last_err)
}

async fn send_via_direct_mx(
    from: &str,
    to: &str,
    bcc_list: &[String],
    subject: &str,
    text: &str,
    html: &str,
) -> Result<Vec<String>, String> {
    let (from_mb, _, raw_bytes) = build_signed_mime_message(from, to, subject, text, html)?;
    deliver_envelope_to_recipient(&from_mb, to, &raw_bytes).await?;

    let mut bcc_ok = Vec::new();
    for bcc in bcc_list {
        match deliver_envelope_to_recipient(&from_mb, bcc, &raw_bytes).await {
            Ok(()) => bcc_ok.push(bcc.clone()),
            Err(err) => eprintln!("[mailer] BCC delivery warning to {bcc}: {err}"),
        }
    }
    Ok(bcc_ok)
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
        assert!(is_synthetic_test_address("user@example.com"));
        assert!(!is_synthetic_test_address("pcworm@pcworm.net"));
    }
}
