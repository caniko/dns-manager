//! Turn a raw record payload into typed [`RecordData`], applying the same
//! transforms the legacy NixOS module did in its option `apply` functions.
//!
//! The two contexts matter because the legacy module attached different `apply`s
//! to base-domain options vs. subdomain options — most visibly, a `CNAME`
//! declared as base/extraConfig gains a trailing dot while one declared as a
//! subdomain does not.

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{Error, Result};
use crate::model::{Caa, Mx, RecordData, Soa, Srv, Sshfp, Tlsa, Uri};

/// Where a record was declared, which selects the legacy `apply` behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    /// `baseDomains` or `extraConfig` (the legacy `.base`/`.common` options).
    Base,
    /// `subDomains` (the legacy `.sub` options).
    Sub,
}

/// Append a trailing dot if absent (idempotent FQDN-ification).
///
/// This is **idempotent** by design: the legacy Nix appended a dot
/// unconditionally (`"${x}."`), so an already-dotted target became a double dot
/// (`foo.` → `foo..`) — an invalid name. We canonicalize to a single trailing
/// dot instead. Identical to the legacy for the normal (un-dotted) input; a
/// deliberate, strictly-better divergence for the pre-dotted case.
pub fn ensure_trailing_dot(value: &str) -> String {
    if value.ends_with('.') {
        value.to_string()
    } else {
        format!("{value}.")
    }
}

/// Flatten the raw `data` into a list of JSON values (`toList` / `coercedTo`):
/// arrays pass through, a lone scalar/object becomes a singleton, `null` becomes
/// empty. `null` *elements* are dropped (the legacy `[ null ]` "empty" sentinel).
fn as_list(data: &Value) -> Vec<Value> {
    match data {
        Value::Null => Vec::new(),
        Value::Array(items) => items.iter().filter(|v| !v.is_null()).cloned().collect(),
        other => vec![other.clone()],
    }
}

fn as_strings(rtype: &str, data: &Value) -> Result<Vec<String>> {
    as_list(data)
        .into_iter()
        .map(|v| match v {
            Value::String(s) => Ok(s),
            other => Err(Error::msg(format!(
                "{} record expects string values, got {other}",
                rtype.to_uppercase()
            ))),
        })
        .collect()
}

fn as_objects<T: DeserializeOwned>(rtype: &str, data: &Value) -> Result<Vec<T>> {
    as_list(data)
        .into_iter()
        .map(|v| {
            serde_json::from_value(v).map_err(|e| {
                Error::msg(format!(
                    "invalid {} record value: {e}",
                    rtype.to_uppercase()
                ))
            })
        })
        .collect()
}

/// Normalize one record's payload into typed [`RecordData`].
pub fn normalize_data(rtype: &str, data: &Value, ctx: Context) -> Result<RecordData> {
    let key = rtype.to_lowercase();
    let out = match key.as_str() {
        "a" => RecordData::A(as_strings(&key, data)?),
        "aaaa" => RecordData::Aaaa(as_strings(&key, data)?),
        "ns" => RecordData::Ns(as_strings(&key, data)?),
        "txt" => RecordData::Txt(as_strings(&key, data)?),
        "alias" => RecordData::Alias(as_strings(&key, data)?),
        "dname" => RecordData::Dname(as_strings(&key, data)?),
        "cname" => {
            // Legacy: base/extraConfig CNAME gets a trailing dot; subdomain CNAME does not.
            let values = as_strings(&key, data)?;
            let values = match ctx {
                Context::Base => values.iter().map(|v| ensure_trailing_dot(v)).collect(),
                Context::Sub => values,
            };
            RecordData::Cname(values)
        }
        "mx" => RecordData::Mx(as_objects::<Mx>(&key, data)?),
        "caa" => RecordData::Caa(as_objects::<Caa>(&key, data)?),
        "tlsa" => RecordData::Tlsa(as_objects::<Tlsa>(&key, data)?),
        "sshfp" => RecordData::Sshfp(as_objects::<Sshfp>(&key, data)?),
        "uri" => RecordData::Uri(as_objects::<Uri>(&key, data)?),
        "srv" => {
            let mut values = as_objects::<Srv>(&key, data)?;
            for v in &mut values {
                v.target = ensure_trailing_dot(&v.target);
            }
            RecordData::Srv(values)
        }
        "soa" => {
            let mut values = as_objects::<Soa>(&key, data)?;
            for v in &mut values {
                v.rname = v.rname.replace('@', ".");
            }
            RecordData::Soa(values)
        }
        "spf" => {
            return Err(Error::msg(
                "SPF records are not supported (deprecated by RFC 7208); use a TXT record",
            ))
        }
        other => return Err(Error::msg(format!("unknown record type '{other}'"))),
    };
    Ok(out)
}
