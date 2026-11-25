use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::Duration;

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ureq::http::{Method, StatusCode};
use url::Url;

use crate::{Error, Result};

const API_BASE: &str = "https://desec.io/api/v1/";
const TOKENS: &str = "auth/tokens/";

pub(super) struct Client {
    agent: ureq::Agent,
    base: Url,
    token: String,
}

#[derive(Args, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MintArgs {
    #[arg(long)]
    name: String,
    #[arg(long)]
    pub(super) out: std::path::PathBuf,
    #[arg(long)]
    domain: Option<String>,
    #[arg(long)]
    subname: Option<String>,
    #[arg(long = "type")]
    #[serde(rename = "type")]
    rtype: Option<String>,
    #[arg(long)]
    #[serde(default)]
    write: bool,
    #[arg(long)]
    #[serde(default)]
    perm_manage_tokens: bool,
}

#[derive(Serialize)]
pub(super) struct Minted {
    id: String,
    path: std::path::PathBuf,
}

impl Client {
    pub(super) fn new(token: String) -> Result<Self> {
        Self::at(
            token,
            Url::parse(API_BASE).expect("valid static deSEC API URL"),
        )
    }

    pub(super) fn at(token: String, base: Url) -> Result<Self> {
        let token = token.trim();
        if token.is_empty() || !token.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(Error::msg(
                "invalid deSEC token (expected a single bare token)",
            ));
        }
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        }
        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .http_status_as_error(false)
                .max_redirects(0)
                .timeout_global(Some(Duration::from_secs(30)))
                .tls_config(
                    ureq::tls::TlsConfig::builder()
                        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                        .build(),
                )
                .build(),
        );
        Ok(Self {
            agent,
            base,
            token: token.to_string(),
        })
    }

    fn url(&self, path: &str) -> Url {
        // Paths are static or assembled from validated domains and UUIDs.
        self.base.join(path).expect("valid deSEC endpoint")
    }

    fn request(
        &self,
        method: Method,
        url: &Url,
        body: Option<Value>,
        expected: StatusCode,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        let mut request = ureq::http::Request::builder()
            .method(method)
            .uri(url.as_str())
            .header("Accept", "application/json")
            .header("Authorization", format!("Token {}", self.token));
        if body.is_some() {
            request = request.header("Content-Type", "application/json");
        }
        let response = self
            .agent
            .run(
                request
                    .body(body.map(|b| b.to_string()).unwrap_or_default())
                    .map_err(|_| Error::msg("could not construct deSEC API request"))?,
            )
            .map_err(|_| Error::Http("deSEC transport failed".into()))?;
        if response.status() != expected {
            // API response bodies and transport errors are untrusted and can echo credentials.
            return Err(Error::msg(format!(
                "deSEC API returned HTTP {}",
                response.status().as_u16()
            )));
        }
        Ok(response)
    }

    fn read_json(response: &mut ureq::http::Response<ureq::Body>) -> Result<Value> {
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|_| Error::msg("could not read deSEC API response"))?;
        serde_json::from_str(&body).map_err(|_| Error::msg("invalid JSON from deSEC API"))
    }

    // https://desec.readthedocs.io/en/latest/dns/rrsets.html#pagination
    fn list(&self, path: &str) -> Result<Value> {
        let endpoint = self.url(path);
        let mut page = endpoint.clone();
        page.set_query(Some("cursor="));
        let mut values = Vec::new();
        let mut seen = HashSet::new();
        // ponytail: bound synchronous listings; add caller-controlled pagination if needed.
        for _ in 0..100 {
            if !seen.insert(page.clone()) {
                return Err(Error::msg("deSEC pagination repeated a cursor"));
            }
            let mut response = self.request(Method::GET, &page, None, StatusCode::OK)?;
            let next = next_page(&endpoint, response.headers())?;
            let body = Self::read_json(&mut response)?;
            values.extend(
                body.as_array()
                    .ok_or_else(|| Error::msg("expected a list from deSEC API"))?
                    .iter()
                    .cloned(),
            );
            match next {
                Some(url) => page = url,
                None => return Ok(Value::Array(values)),
            }
        }
        Err(Error::msg("deSEC listing exceeded the 100-page limit"))
    }

    pub(super) fn domains(&self) -> Result<Value> {
        self.list("domains/")
    }

    pub(super) fn domain(&self, name: &str) -> Result<Value> {
        let name = domain_name(name)?;
        let mut response = self.request(
            Method::GET,
            &self.url(&format!("domains/{name}/")),
            None,
            StatusCode::OK,
        )?;
        // The detail endpoint already includes DNSKEY and DS values in `keys`.
        Self::read_json(&mut response)
    }

    pub(super) fn rrsets(&self, name: &str) -> Result<Value> {
        self.list(&format!("domains/{}/rrsets/", domain_name(name)?))
    }

    pub(super) fn domain_create(&self, name: &str) -> Result<Value> {
        self.post("domains/", json!({"name": domain_name(name)?}))
    }

    pub(super) fn token_revoke(&self, id: &str) -> Result<Value> {
        if !valid_id(id) {
            return Err(Error::msg(
                "deSEC token ID must be a UUID, not a token secret",
            ));
        }
        self.request(
            Method::DELETE,
            &self.url(&format!("{TOKENS}{id}/")),
            None,
            StatusCode::NO_CONTENT,
        )?;
        Ok(json!({"id": id, "revoked": true}))
    }

    fn post(&self, path: &str, body: Value) -> Result<Value> {
        let mut response = self.request(
            Method::POST,
            &self.url(path),
            Some(body),
            StatusCode::CREATED,
        )?;
        Self::read_json(&mut response)
    }

    pub(super) fn token_mint(&self, args: &MintArgs, out: &Path) -> Result<Minted> {
        let domain = args.domain.as_deref().map(domain_name).transpose()?;
        let rtype = args.rtype.as_ref().map(|r| r.to_ascii_uppercase());
        if args.name.trim().is_empty() || args.name.chars().count() > 178 {
            return Err(Error::msg("deSEC token name must be 1–178 characters"));
        }
        // Reserve the destination before minting; never truncate or follow a
        // symlink at the destination. The operator chooses a trusted directory.
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(out)?;
        let mut minted_id = None;
        let result = (|| {
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
            let created = self.post(
                TOKENS,
                json!({
                    "name": args.name, "perm_create_domain": false, "perm_delete_domain": false,
                    "perm_manage_tokens": args.perm_manage_tokens,
                }),
            )?;
            let id = created["id"].as_str().filter(|id| valid_id(id))
                .ok_or_else(|| Error::msg("deSEC token creation did not return a usable token ID; check deSEC for an orphan token"))?;
            minted_id = Some(id.to_string());
            let secret = created["token"]
                .as_str()
                .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_graphic()))
                .ok_or_else(|| Error::msg("deSEC did not return a usable token secret"))?;

            // Unscoped tokens are writable until a default policy exists.
            // Install the deny-by-default policy before granting scoped writes,
            // and only export the secret after all policies succeed.
            // https://desec.readthedocs.io/en/latest/auth/tokens.html#token-policy-management
            let policy = format!("{TOKENS}{id}/policies/rrsets/");
            let scoped = domain.is_some() || args.subname.is_some() || rtype.is_some();
            self.post(&policy, json!({"perm_write": args.write && !scoped}))?;
            if scoped {
                self.post(&policy, json!({"domain": domain, "subname": args.subname, "type": rtype, "perm_write": args.write}))?;
            }
            writeln!(file, "{secret}")?;
            file.sync_all()?;
            Ok(Minted {
                id: id.to_string(),
                path: out.to_path_buf(),
            })
        })();
        if let Err(error) = result {
            // If writing or policy setup failed, revoke before discarding any
            // partial local file. Report the ID (never the secret) on rollback failure.
            let revoke_failed = minted_id
                .as_ref()
                .filter(|id| self.token_revoke(id).is_err());
            let cleanup = fs::remove_file(out);
            if let Some(id) = revoke_failed {
                return Err(Error::msg(format!(
                    "{error}; could not revoke deSEC token {id}; revoke it manually"
                )));
            }
            if cleanup.is_err() {
                return Err(Error::msg(format!(
                    "{error}; could not remove the reserved output file"
                )));
            }
            return Err(error);
        }
        result
    }
}

fn domain_name(name: &str) -> Result<String> {
    let name = name.strip_suffix('.').unwrap_or(name);
    if name.len() > 191
        || name.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || !label.as_bytes()[0].is_ascii_alphanumeric()
                || !label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        })
    {
        return Err(Error::msg("invalid deSEC domain name (use ASCII/Punycode)"));
    }
    Ok(name.to_ascii_lowercase())
}

fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

fn next_page(endpoint: &Url, headers: &ureq::http::HeaderMap) -> Result<Option<Url>> {
    for header in headers.get_all("link") {
        let links = header
            .to_str()
            .map_err(|_| Error::msg("invalid deSEC pagination header"))?;
        for link in links.split(',') {
            let Some((target, params)) = link
                .trim()
                .strip_prefix('<')
                .and_then(|s| s.split_once('>'))
            else {
                continue;
            };
            if params
                .split(';')
                .any(|p| matches!(p.trim(), "rel=\"next\"" | "rel=next"))
            {
                let next = endpoint
                    .join(target)
                    .map_err(|_| Error::msg("invalid deSEC next-page URL"))?;
                if next.origin() != endpoint.origin()
                    || next.path() != endpoint.path()
                    || !next.username().is_empty()
                    || next.password().is_some()
                    || next.fragment().is_some()
                {
                    return Err(Error::msg(
                        "deSEC next-page URL leaves the expected endpoint",
                    ));
                }
                return Ok(Some(next));
            }
        }
    }
    Ok(None)
}
