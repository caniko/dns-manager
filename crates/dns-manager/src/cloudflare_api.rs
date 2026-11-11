//! Minimal Cloudflare cache-purge client.
//!
//! Two calls are needed to purge files: resolve a zone name to its id, then
//! `purge_cache` a bounded list of URLs. The API token is never formatted into
//! an error message.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Cloudflare's per-request limit for `purge_cache` files.
pub const MAX_URLS: usize = 30;

/// Default Cloudflare API base.
pub const DEFAULT_API_BASE: &str = "https://api.cloudflare.com/client/v4";

/// Successful purge summary printed by the CLI.
#[derive(Debug, Serialize)]
pub struct PurgeSummary {
    pub zone: String,
    pub zone_id: String,
    pub purged: Vec<String>,
}

/// Read an API token from `path`.
///
/// The file holds either the bare token or env-style `KEY=VALUE` lines; the
/// first non-comment key ending in `TOKEN` wins. Quotes around the value are
/// optional. An empty token is an error.
pub fn load_token(path: &Path) -> Result<String> {
    let contents = std::fs::read_to_string(path).map_err(|err| {
        Error::msg(format!(
            "failed to read token file {}: {err}",
            path.display()
        ))
    })?;
    parse_token(&contents)
}

fn parse_token(contents: &str) -> Result<String> {
    let mut candidates = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.ends_with("TOKEN") {
            candidates.push((key, value.trim().trim_matches(['"', '\'']).trim()));
        }
    }

    let chosen = match candidates
        .iter()
        .find(|(key, _)| *key == "CLOUDFLARE_API_TOKEN")
    {
        Some(&(_, value)) => Some(value),
        None if candidates.len() == 1 => Some(candidates[0].1),
        None if candidates.len() > 1 => {
            let keys: Vec<&str> = candidates.iter().map(|(key, _)| *key).collect();
            return Err(Error::msg(format!(
                "token file has several *TOKEN keys ({}) and no CLOUDFLARE_API_TOKEN",
                keys.join(", ")
            )));
        }
        None => None,
    };
    if let Some(token) = chosen {
        if token.is_empty() {
            return Err(Error::msg("API token in token file is empty"));
        }
        return Ok(token.to_string());
    }

    let token = contents.trim();
    if token.is_empty() {
        return Err(Error::msg("token file is empty"));
    }
    Ok(token.to_string())
}

/// Purge `urls` from the edge cache of `zone`.
///
/// The URLs are validated before any network call: there must be 1..=`MAX_URLS`
/// of them, each an absolute `http(s)` URL whose host is the zone or a
/// subdomain of it.
pub fn purge_cache(
    api_base: &str,
    token: &str,
    zone: &str,
    urls: &[String],
) -> Result<PurgeSummary> {
    // Transport errors and Cloudflare messages can echo request details;
    // nothing that leaves this function may contain the token.
    purge_cache_inner(api_base, token, zone, urls).map_err(|err| redact(err, token))
}

fn redact(err: Error, token: &str) -> Error {
    let scrub = |text: String| text.replace(token, "[redacted]");
    match err {
        Error::Cloudflare(text) => Error::Cloudflare(scrub(text)),
        Error::Http(text) => Error::Http(scrub(text)),
        Error::Message(text) => Error::Message(scrub(text)),
        other => Error::Message(scrub(other.to_string())),
    }
}

fn purge_cache_inner(
    api_base: &str,
    token: &str,
    zone: &str,
    urls: &[String],
) -> Result<PurgeSummary> {
    validate_urls(zone, urls)?;

    let base = api_base.trim_end_matches('/');
    // ureq is built without a bundled crypto provider; install the one rustls
    // is compiled with, as pklr does, unless something already did.
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            // The API never redirects; a redirect's Location could echo the token.
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(30)))
            // The only root store compiled in is the platform verifier.
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                    .build(),
            )
            .build(),
    );
    let auth = format!("Bearer {token}");

    let mut response = agent
        .get(format!("{base}/zones"))
        .query("name", zone)
        .header("Authorization", &auth)
        .call()
        .map_err(|err| Error::Http(format!("zone lookup request failed: {err}")))?;
    let status = response.status();
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|err| Error::Http(format!("zone lookup response could not be read: {err}")))?;
    let envelope: Envelope = serde_json::from_str(&body).map_err(|err| {
        Error::Http(format!(
            "zone lookup returned HTTP {status} with an unreadable body: {err}"
        ))
    })?;
    if !envelope.success {
        return Err(Error::Cloudflare(format!(
            "zone lookup failed: {}",
            envelope.error_text()
        )));
    }
    let zones: Vec<Zone> = serde_json::from_value(envelope.result)
        .map_err(|err| Error::Http(format!("zone lookup returned an unexpected result: {err}")))?;
    let zone_id = zones
        .iter()
        .find(|candidate| candidate.name.eq_ignore_ascii_case(zone))
        .map(|candidate| candidate.id.clone())
        .ok_or_else(|| Error::Cloudflare(format!("no zone named {zone} in this account")))?;

    let payload = serde_json::json!({ "files": urls }).to_string();
    let mut response = agent
        .post(format!("{base}/zones/{zone_id}/purge_cache"))
        .header("Authorization", &auth)
        .header("Content-Type", "application/json")
        .send(payload.as_str())
        .map_err(|err| Error::Http(format!("purge request failed: {err}")))?;
    let status = response.status();
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|err| Error::Http(format!("purge response could not be read: {err}")))?;
    let envelope: Envelope = serde_json::from_str(&body).map_err(|err| {
        Error::Http(format!(
            "purge returned HTTP {status} with an unreadable body: {err}"
        ))
    })?;
    if !envelope.success {
        return Err(Error::Cloudflare(format!(
            "cache purge failed: {}",
            envelope.error_text()
        )));
    }

    Ok(PurgeSummary {
        zone: zone.to_string(),
        zone_id,
        purged: urls.to_vec(),
    })
}

fn validate_urls(zone: &str, urls: &[String]) -> Result<()> {
    if urls.is_empty() {
        return Err(Error::msg("at least one URL is required"));
    }
    if urls.len() > MAX_URLS {
        return Err(Error::msg(format!(
            "{} URLs given; Cloudflare accepts at most {MAX_URLS} per purge request",
            urls.len()
        )));
    }

    for raw in urls {
        let parsed = url::Url::parse(raw)
            .ok()
            .filter(|parsed| matches!(parsed.scheme(), "http" | "https"))
            .ok_or_else(|| Error::msg(format!("not an absolute http(s) URL: {raw}")))?;
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(Error::msg(format!("URL must not carry credentials: {raw}")));
        }
        let Some(url::Host::Domain(host)) = parsed.host() else {
            return Err(Error::msg(format!("URL host must be a DNS name: {raw}")));
        };
        if !in_zone(host, zone) {
            return Err(Error::msg(format!(
                "URL host {host} is outside zone {zone}: {raw}"
            )));
        }
    }
    Ok(())
}

/// Whether `host` is `zone` or a subdomain of it (DNS names are case-insensitive).
fn in_zone(host: &str, zone: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let zone = zone.trim_end_matches('.').to_ascii_lowercase();
    host == zone
        || host
            .strip_suffix(&zone)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    errors: Vec<CfError>,
    #[serde(default)]
    result: serde_json::Value,
}

impl Envelope {
    fn error_text(&self) -> String {
        if self.errors.is_empty() {
            return "no error details returned".to_string();
        }
        self.errors
            .iter()
            .map(|error| format!("code {}: {}", error.code, error.message))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

#[derive(Debug, Deserialize)]
struct CfError {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct Zone {
    id: String,
    name: String,
}

#[cfg(test)]
mod tests {
    use super::parse_token;

    #[test]
    fn raw_token_is_trimmed() {
        assert_eq!(parse_token("  abc123\n").unwrap(), "abc123");
    }

    #[test]
    fn env_file_token_is_used() {
        let contents = "CLOUDFLARE_ACCOUNT_ID=xyz\n# comment\nCLOUDFLARE_API_TOKEN=abc123\n";
        assert_eq!(parse_token(contents).unwrap(), "abc123");
    }

    #[test]
    fn quoted_env_value_is_unquoted() {
        assert_eq!(
            parse_token("CLOUDFLARE_API_TOKEN=\"abc123\"\n").unwrap(),
            "abc123"
        );
        assert_eq!(
            parse_token("CLOUDFLARE_API_TOKEN='abc123'\n").unwrap(),
            "abc123"
        );
    }

    #[test]
    fn cloudflare_key_wins_over_other_tokens() {
        let contents = "GITHUB_TOKEN=unrelated\nCLOUDFLARE_API_TOKEN=correct\n";
        assert_eq!(parse_token(contents).unwrap(), "correct");
    }

    #[test]
    fn several_unrelated_token_keys_are_ambiguous() {
        let error = parse_token("A_TOKEN=one\nB_TOKEN=two\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("several"), "{error}");
        assert!(!error.contains("one") && !error.contains("two"), "{error}");
    }

    #[test]
    fn empty_file_is_an_error() {
        assert!(parse_token("   \n\n").is_err());
        assert!(parse_token("").is_err());
    }
}
