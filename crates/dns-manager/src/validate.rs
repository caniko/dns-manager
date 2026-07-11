//! Declarative validation, ported from the legacy `checkRecord` `throwIf`s and the
//! NixOS module assertions. All violations are collected and reported together.

use crate::error::{Error, Result};
use crate::model::{RawDoc, RawRecord, RawRecords};
use crate::resolve::domains::get_most_specific;

/// Record types Cloudflare can proxy.
const PROXIABLE: &[&str] = &["a", "aaaa", "alias", "cname"];

/// Maximum length (bytes) of a record comment, mirroring `builtins.stringLength`.
const MAX_COMMENT_LEN: usize = 100;

/// Check every declarative rule across the document. Returns
/// [`Error::Validation`] listing every violation, or `Ok(())`.
pub fn check(doc: &RawDoc) -> Result<()> {
    let mut errors = Vec::new();

    for host in &doc.hosts {
        let base_names: Vec<String> = host.base_domains.keys().cloned().collect();
        for (name, recs) in &host.base_domains {
            check_records(&format!("baseDomains.{name}"), recs, &mut errors);
        }
        for (name, recs) in &host.sub_domains {
            check_records(&format!("subDomains.{name}"), recs, &mut errors);
            if get_most_specific(name, &base_names).is_none() {
                errors.push(format!("subdomain '{name}' has no matching base domain"));
            }
        }
    }

    if let Some(extra) = &doc.extra_config {
        for (zone, names) in &extra.zones {
            for (name, recs) in names {
                let label = if name.is_empty() {
                    zone.clone()
                } else {
                    format!("{name}.{zone}")
                };
                check_records(&format!("extraConfig.{label}"), recs, &mut errors);
            }
        }
    }
    check_redirects(doc, &mut errors);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(Error::Validation { messages: errors })
    }
}

fn check_redirects(doc: &RawDoc, errors: &mut Vec<String>) {
    for (idx, redirect) in doc.redirects.iter().enumerate() {
        let ctx = format!("redirects[{idx}]");
        if redirect.from.trim().is_empty() {
            errors.push(format!("{ctx}.from must not be empty"));
        }
        if !(redirect.to.starts_with("https://") || redirect.to.starts_with("http://")) {
            errors.push(format!("{ctx}.to must be an absolute http(s) URL"));
        }
        if redirect.to.contains("{http.request.uri}") {
            errors.push(format!(
                "{ctx}.to must not include {{http.request.uri}}; use preservePath instead"
            ));
        }
        if !(300..=399).contains(&redirect.status) {
            errors.push(format!("{ctx}.status must be a 3xx HTTP status code"));
        }
    }
}

fn check_records(ctx: &str, recs: &RawRecords, errors: &mut Vec<String>) {
    for (rtype, raw) in recs {
        check_record(ctx, rtype, raw, errors);
    }
}

fn check_record(ctx: &str, rtype: &str, raw: &RawRecord, errors: &mut Vec<String>) {
    let key = rtype.to_lowercase();
    let token = key.to_uppercase();

    if let Some(comment) = &raw.comment {
        if comment.len() > MAX_COMMENT_LEN {
            errors.push(format!(
                "{ctx} {token}: record comments must be at most {MAX_COMMENT_LEN} characters"
            ));
        }
    }
    if raw.ttl_auto && raw.ttl.is_some() {
        errors.push(format!(
            "{ctx} {token}: ttlAuto cannot be used together with an explicit ttl"
        ));
    }
    if raw.proxied && !PROXIABLE.contains(&key.as_str()) {
        errors.push(format!(
            "{ctx} {token}: proxied is only valid on A, AAAA, CNAME, ALIAS"
        ));
    }
}
