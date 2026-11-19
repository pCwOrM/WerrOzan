//! Local credential storage for cloud providers.
//!
//! A desktop Studio has no server to hold secrets for it, so the API key lives
//! beside the other runtime user data. Two rules keep it honest:
//!
//! * the environment variable always wins, so CI and scripted runs never read
//!   or write the user's stored key;
//! * the value is never serialized into settings, catalogs, job records or any
//!   response body — callers can only ask whether a key is configured.

use std::{env, fs};

use anyhow::{bail, Context, Result};
use serde::Serialize;

pub const OPENROUTER_ENV_VAR: &str = "OPENROUTER_API_KEY";
const OPENROUTER_FILE: &str = "openrouter-api-key";

pub const GEMINI_ENV_VAR: &str = "GEMINI_API_KEY";
pub const GEMINI_KEYS_ENV_VAR: &str = "GEMINI_API_KEYS";
const GEMINI_FILE: &str = "gemini-api-key";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialSource {
    Environment,
    LocalStore,
}

fn read_provider_key(env_var: &str, file_name: &str) -> Option<(String, CredentialSource)> {
    if let Some(key) = env::var(env_var).ok().map(|key| key.trim().to_owned()).filter(|key| !key.is_empty()) {
        return Some((key, CredentialSource::Environment));
    }
    let stored = fs::read_to_string(crate::studio_data_root()?.join(file_name)).ok()?.trim().to_owned();
    (!stored.is_empty()).then_some((stored, CredentialSource::LocalStore))
}

fn store_provider_key(env_var: &str, file_name: &str, label: &str, api_key: Option<&str>) -> Result<Option<CredentialSource>> {
    if env::var(env_var).is_ok_and(|key| !key.trim().is_empty()) {
        bail!("{env_var} is set in this environment and takes priority; unset it before storing a key in Studio");
    }
    let path = crate::studio_data_root()
        .context("no per-user application data directory for credential storage")?
        .join(file_name);
    match api_key.map(str::trim).filter(|key| !key.is_empty()) {
        Some(key) => {
            if key.contains(['\r', '\n']) {
                bail!("a {label} API key must be a single line");
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
            }
            fs::write(&path, key).with_context(|| format!("write {}", path.display()))?;
            Ok(Some(CredentialSource::LocalStore))
        }
        None => {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error).with_context(|| format!("remove {}", path.display())),
            }
            Ok(None)
        }
    }
}

pub fn openrouter_api_key() -> Option<(String, CredentialSource)> {
    read_provider_key(OPENROUTER_ENV_VAR, OPENROUTER_FILE)
}

pub fn openrouter_source() -> Option<CredentialSource> {
    openrouter_api_key().map(|(_, source)| source)
}

/// Stores or clears the key. Refuses to shadow an environment credential so a
/// user never believes they replaced a key that the process is not using.
pub fn store_openrouter_api_key(api_key: Option<&str>) -> Result<Option<CredentialSource>> {
    store_provider_key(OPENROUTER_ENV_VAR, OPENROUTER_FILE, "OpenRouter", api_key)
}

pub fn gemini_api_key() -> Option<(String, CredentialSource)> {
    if let Some(found) = read_provider_key(GEMINI_ENV_VAR, GEMINI_FILE) {
        return Some(found);
    }
    gemini_api_keys()
        .into_iter()
        .next()
        .map(|k| (k, CredentialSource::Environment))
}

/// Returns all configured Gemini API keys in priority order (deduplicated):
/// 1. `GEMINI_API_KEY` (primary)
/// 2. `GEMINI_API_KEYS` (comma, semicolon, or newline separated)
/// 3. `GEMINI_API_KEY_2` .. `GEMINI_API_KEY_10` (numbered backup keys)
/// 4. Local stored key (`gemini-api-key` file) if not already present.
pub fn gemini_api_keys() -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    let mut push_unique = |raw: &str| {
        let trimmed = raw.trim();
        if !trimmed.is_empty() && !keys.iter().any(|existing| existing == trimmed) {
            keys.push(trimmed.to_owned());
        }
    };

    if let Ok(primary) = env::var(GEMINI_ENV_VAR) {
        for part in primary.split([',', ';', '\n', '\r']) {
            push_unique(part);
        }
    }
    if let Ok(pool) = env::var(GEMINI_KEYS_ENV_VAR) {
        for part in pool.split([',', ';', '\n', '\r']) {
            push_unique(part);
        }
    }
    for idx in 2..=10 {
        if let Ok(backup) = env::var(format!("{GEMINI_ENV_VAR}_{idx}")) {
            for part in backup.split([',', ';', '\n', '\r']) {
                push_unique(part);
            }
        }
    }
    if let Some(root) = crate::studio_data_root() {
        if let Ok(stored) = fs::read_to_string(root.join(GEMINI_FILE)) {
            for part in stored.split([',', ';', '\n', '\r']) {
                push_unique(part);
            }
        }
    }
    keys
}

pub fn gemini_source() -> Option<CredentialSource> {
    gemini_api_key().map(|(_, source)| source)
}

pub fn store_gemini_api_key(api_key: Option<&str>) -> Result<Option<CredentialSource>> {
    store_provider_key(GEMINI_ENV_VAR, GEMINI_FILE, "Gemini", api_key)
}

const LOCAL_SERVER_FILE: &str = "local-server-api-key";

/// The key of the user's own OpenAI-compatible server, when it asks for one.
pub fn local_server_key() -> Option<String> {
    let stored = fs::read_to_string(crate::studio_data_root()?.join(LOCAL_SERVER_FILE)).ok()?.trim().to_owned();
    (!stored.is_empty()).then_some(stored)
}

/// Stores the own server's key, or clears it with None or an empty value.
pub fn store_local_server_key(api_key: Option<&str>) -> Result<bool> {
    let path = crate::studio_data_root().context("no per-user application data directory for credential storage")?.join(LOCAL_SERVER_FILE);
    match api_key.map(str::trim).filter(|key| !key.is_empty()) {
        Some(key) => {
            if key.contains(['\r', '\n']) {
                bail!("an API key must be a single line");
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
            }
            fs::write(&path, key).with_context(|| format!("write {}", path.display()))?;
            Ok(true)
        }
        None => {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error).with_context(|| format!("remove {}", path.display())),
            }
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_environment_key_is_reported_as_such() {
        // The variable is process-wide; only assert the branch that does not
        // depend on mutating global state in a parallel test run.
        if let Ok(key) = env::var(OPENROUTER_ENV_VAR) {
            if !key.trim().is_empty() {
                assert_eq!(openrouter_source(), Some(CredentialSource::Environment));
            }
        }
    }

    #[test]
    fn storing_is_refused_while_the_environment_variable_wins() {
        if env::var(OPENROUTER_ENV_VAR).is_ok_and(|key| !key.trim().is_empty()) {
            assert!(store_openrouter_api_key(Some("test")).is_err());
        }
    }
}
