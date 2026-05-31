//! Cloudflare-flavoured octoDNS rendering — a port of `utils/cloudflare.nix` +
//! `generate.cloudflareConfig`.
//!
//! Output is a directory: `config.yaml` (the octoDNS manager/providers/processors/
//! zones) and `zones/<zone>.yaml` YAML-provider files with per-name comments. The
//! file-token `octodns-sync-cloudflare` wrapper is added by the Nix layer, which
//! owns the octoDNS store path.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{json_to_yaml, recursive_update, yaml, yaml::Yaml, OutputFile};
use crate::error::Result;
use crate::model::{Config, RawDoc, Record, RecordData, Zone};
use crate::resolve::ensure_trailing_dot;

/// Input document for the `cloudflare` subcommand.
#[derive(Clone, Debug, Deserialize)]
pub struct CloudflareInput {
    #[serde(rename = "dnsConfig")]
    pub dns_config: RawDoc,
    pub token: Token,
    /// Per-zone settings. `None` means every zone uses defaults.
    #[serde(default)]
    pub zones: Option<BTreeMap<String, ZoneSettingsRaw>>,
    #[serde(default = "super::empty_object", rename = "extraProviderSettings")]
    pub extra_provider_settings: Value,
    #[serde(default = "super::empty_object", rename = "extraGlobalConfig")]
    pub extra_global_config: Value,
}

/// A Cloudflare API token reference.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Token {
    /// Read from environment variable `name` at sync time (`env/NAME`).
    Env { name: String },
    /// A literal token value (discouraged; flagged in the config).
    Literal { value: String },
    /// Read from a file; the Nix wrapper exports `envName` (default
    /// `CLOUDFLARE_API_TOKEN`) from `path` before invoking octoDNS.
    File {
        #[allow(dead_code)]
        path: String,
        #[serde(default, rename = "envName")]
        env_name: Option<String>,
    },
}

impl Token {
    /// The octoDNS provider `token` value.
    fn provider_value(&self) -> String {
        match self {
            Token::Env { name } => format!("env/{name}"),
            Token::Literal { value } => value.clone(),
            Token::File { env_name, .. } => {
                format!(
                    "env/{}",
                    env_name.as_deref().unwrap_or("CLOUDFLARE_API_TOKEN")
                )
            }
        }
    }
}

/// Raw per-zone settings as declared by the user.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ZoneSettingsRaw {
    pub mode: Option<String>,
    #[serde(rename = "manageRecordTypes")]
    pub manage_record_types: Option<Vec<String>>,
    #[serde(default, rename = "excludeRecords")]
    pub exclude_records: Vec<ExcludeRecord>,
    #[serde(default)]
    pub processors: Vec<String>,
}

/// A record to exclude from management in a zone (`NameRejectlistFilter`).
#[derive(Clone, Debug, Deserialize)]
pub struct ExcludeRecord {
    pub name: String,
    #[serde(rename = "type")]
    pub rtype: String,
}

/// Normalized per-zone settings (defaults applied).
struct ZoneSettings {
    mode: String,
    manage_record_types: Option<Vec<String>>,
    exclude_records: Vec<ExcludeRecord>,
    processors: Vec<String>,
}

impl ZoneSettings {
    fn from_raw(raw: Option<&ZoneSettingsRaw>) -> Self {
        match raw {
            Some(r) => ZoneSettings {
                mode: r.mode.clone().unwrap_or_else(|| "lenient".to_string()),
                manage_record_types: r.manage_record_types.clone(),
                exclude_records: r.exclude_records.clone(),
                processors: r.processors.clone(),
            },
            None => ZoneSettings {
                mode: "lenient".to_string(),
                manage_record_types: None,
                exclude_records: Vec::new(),
                processors: Vec::new(),
            },
        }
    }
}

/// `safeZone`/`safeName`: `.`→`-`, `*`→`wildcard`, `_`→`-`.
fn safe(value: &str) -> String {
    value
        .replace('.', "-")
        .replace('*', "wildcard")
        .replace('_', "-")
}

/// Deterministic processor name for an excluded record.
fn exclude_processor_name(zone: &str, rec: &ExcludeRecord) -> String {
    let upper = rec.rtype.to_uppercase();
    let mut hasher = Sha256::new();
    hasher.update(format!("{zone}|{upper}|{}", rec.name).as_bytes());
    let digest = hasher.finalize();
    let hash: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
    format!(
        "exclude-{}-{}-{}-{}",
        safe(zone),
        upper.to_lowercase(),
        safe(&rec.name),
        hash
    )
}

/// octoDNS `octodns.cloudflare` metadata + `ttl`, mirroring `recordMetadata`.
fn record_entry(rtype: &str, record: &Record) -> Value {
    let mut entry = serde_json::Map::new();
    if !record.ttl_auto {
        entry.insert("ttl".to_string(), json!(record.ttl));
    }
    if record.proxied || record.ttl_auto || record.comment.is_some() {
        let mut cf = serde_json::Map::new();
        if record.proxied {
            cf.insert("proxied".to_string(), json!(true));
        }
        if record.ttl_auto {
            cf.insert("auto-ttl".to_string(), json!(true));
        }
        if let Some(comment) = &record.comment {
            cf.insert("comment".to_string(), json!(comment));
        }
        entry.insert("octodns".to_string(), json!({ "cloudflare": cf }));
    }
    entry.insert("type".to_string(), json!(rtype.to_uppercase()));
    for (key, value) in value_field(&record.data) {
        entry.insert(key, value);
    }
    Value::Object(entry)
}

/// The `value`/`values` field for a record, with provider-required trailing dots.
fn value_field(data: &RecordData) -> serde_json::Map<String, Value> {
    use RecordData::*;
    let mut map = serde_json::Map::new();
    match data {
        Mx(v) => {
            let vals: Vec<Value> = v
                .iter()
                .map(|m| json!({ "exchange": ensure_trailing_dot(&m.exchange), "preference": m.preference }))
                .collect();
            map.insert("values".to_string(), Value::Array(vals));
        }
        Srv(v) => {
            let vals: Vec<Value> = v
                .iter()
                .map(|s| json!({ "priority": s.priority, "weight": s.weight, "port": s.port, "target": ensure_trailing_dot(&s.target) }))
                .collect();
            map.insert("values".to_string(), Value::Array(vals));
        }
        Uri(v) => {
            map.insert("values".to_string(), serde_json::to_value(v).unwrap());
        }
        Caa(v) => {
            map.insert("values".to_string(), serde_json::to_value(v).unwrap());
        }
        Tlsa(v) => {
            map.insert("values".to_string(), serde_json::to_value(v).unwrap());
        }
        Sshfp(v) => {
            map.insert("values".to_string(), serde_json::to_value(v).unwrap());
        }
        Ns(v) => {
            let vals: Vec<Value> = v.iter().map(|n| json!(ensure_trailing_dot(n))).collect();
            map.insert("values".to_string(), Value::Array(vals));
        }
        Dname(v) | Alias(v) => {
            if v.len() == 1 {
                map.insert("value".to_string(), json!(ensure_trailing_dot(&v[0])));
            } else {
                let vals: Vec<Value> = v.iter().map(|x| json!(ensure_trailing_dot(x))).collect();
                map.insert("values".to_string(), Value::Array(vals));
            }
        }
        A(v) | Aaaa(v) | Cname(v) | Txt(v) => {
            if v.len() == 1 {
                map.insert("value".to_string(), json!(v[0]));
            } else {
                map.insert("values".to_string(), json!(v));
            }
        }
        // SOA is never emitted (filtered out of managed records).
        Soa(_) => {}
    }
    map
}

fn strip_zone(zone: &str, fqdn: &str) -> String {
    if fqdn == zone {
        String::new()
    } else {
        fqdn.strip_suffix(&format!(".{zone}"))
            .unwrap_or(fqdn)
            .to_string()
    }
}

/// Build the YAML-provider records and per-name comments for one zone. Root NS
/// and all SOA records are dropped (octoDNS-cloudflare manages neither).
fn build_zone(
    zone: &str,
    entries: &Zone,
) -> (BTreeMap<String, Yaml>, BTreeMap<String, Vec<String>>) {
    let mut records: BTreeMap<String, Yaml> = BTreeMap::new();
    let mut comments: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (fqdn, recs) in entries {
        let rel = strip_zone(zone, fqdn);
        let mut entry_list: Vec<Value> = Vec::new();
        let mut name_comments: Vec<String> = Vec::new();
        for (rtype, record) in recs {
            if rtype == "soa" || (rel.is_empty() && rtype == "ns") {
                continue;
            }
            entry_list.push(record_entry(rtype, record));
            if let Some(comment) = &record.comment {
                if !name_comments.contains(comment) {
                    name_comments.push(comment.clone());
                }
            }
        }
        if !entry_list.is_empty() {
            records.insert(
                rel.clone(),
                Yaml::List(entry_list.into_iter().map(json_to_yaml).collect()),
            );
        }
        if !name_comments.is_empty() {
            comments.insert(rel, name_comments);
        }
    }
    (records, comments)
}

/// Build the octoDNS `config.yaml` value.
fn build_config(input: &CloudflareInput, config: &Config, out_abs: &str) -> Value {
    let zone_settings: BTreeMap<String, ZoneSettings> = config
        .keys()
        .map(|zone| {
            let raw = input.zones.as_ref().and_then(|z| z.get(zone));
            (zone.clone(), ZoneSettings::from_raw(raw))
        })
        .collect();

    // providers.config (YAML provider). `directory` is forced in *after* the
    // extraGlobalConfig merge below (mirroring the legacy outer recursiveUpdate),
    // so user config can never accidentally override it.
    let providers_config = json!({
        "class": "octodns.provider.yaml.YamlProvider",
        "directory": Value::Null,
        "default_ttl": 3600,
        "enforce_order": true,
        "escaped_semicolons": false,
        "supports_root_ns": false,
    });

    // providers.cloudflare.
    let mut cloudflare = serde_json::Map::new();
    cloudflare.insert(
        "class".to_string(),
        json!("octodns_cloudflare.CloudflareProvider"),
    );
    cloudflare.insert("token".to_string(), json!(input.token.provider_value()));
    if matches!(input.token, Token::Literal { .. }) {
        cloudflare.insert(
            "_comment_".to_string(),
            json!("Literal Cloudflare tokens are unsafe for committed configs; prefer env or file tokens."),
        );
    }
    let mut provider_extra = input
        .extra_provider_settings
        .as_object()
        .cloned()
        .unwrap_or_default();
    provider_extra.entry("min_ttl").or_insert(json!(1));
    for (key, value) in provider_extra {
        cloudflare.insert(key, value);
    }

    // processors.
    let mut processors = serde_json::Map::new();
    processors.insert(
        "ignore-root-ns".to_string(),
        json!({ "class": "octodns.processor.filter.IgnoreRootNsFilter" }),
    );
    for (zone, settings) in &zone_settings {
        if let Some(types) = &settings.manage_record_types {
            processors.insert(
                format!("manage-types-{zone}"),
                json!({ "class": "octodns.processor.filter.TypeAllowlistFilter", "allowlist": types, "include_target": true }),
            );
        }
        for rec in &settings.exclude_records {
            processors.insert(
                exclude_processor_name(zone, rec),
                json!({ "class": "octodns.processor.filter.NameRejectlistFilter", "rejectlist": [rec.name.clone()], "include_target": true }),
            );
        }
    }

    // zones.
    let mut zones = serde_json::Map::new();
    for (zone, settings) in &zone_settings {
        let mut procs: Vec<Value> = vec![json!("ignore-root-ns")];
        if settings.manage_record_types.is_some() {
            procs.push(json!(format!("manage-types-{zone}")));
        }
        for rec in &settings.exclude_records {
            procs.push(json!(exclude_processor_name(zone, rec)));
        }
        procs.extend(settings.processors.iter().map(|p| json!(p)));
        zones.insert(
            ensure_trailing_dot(zone),
            json!({
                "lenient": settings.mode == "lenient",
                "sources": ["config"],
                "processors": procs,
                "targets": ["cloudflare"],
            }),
        );
    }

    let base = json!({
        "manager": { "include_meta": false },
        "providers": { "config": providers_config, "cloudflare": Value::Object(cloudflare) },
        "processors": Value::Object(processors),
        "zones": Value::Object(zones),
    });
    let merged = recursive_update(base, input.extra_global_config.clone());
    // Force the zones directory last so it always wins (legacy outer recursiveUpdate).
    recursive_update(
        merged,
        json!({ "providers": { "config": { "directory": format!("{out_abs}/zones") } } }),
    )
}

/// Render the Cloudflare output directory.
pub fn render(input: &CloudflareInput, config: &Config, out_abs: &str) -> Result<Vec<OutputFile>> {
    let mut files = Vec::new();

    for (zone, entries) in config {
        let (records, comments) = build_zone(zone, entries);
        files.push(OutputFile::new(
            format!("zones/{zone}.yaml"),
            yaml::zone_to_string(&records, &comments),
        ));
    }

    let config_value = build_config(input, config, out_abs);
    files.push(OutputFile::new(
        "config.yaml",
        yaml::to_string(&json_to_yaml(config_value)),
    ));

    Ok(files)
}

/// Whether this token needs the file wrapper, and the env var to export.
/// Returned for the Nix layer (which owns the octoDNS store path) to build the
/// `octodns-sync-cloudflare` wrapper.
pub fn file_token_env(token: &Token) -> Option<(String, String)> {
    match token {
        Token::File { path, env_name } => Some((
            env_name
                .clone()
                .unwrap_or_else(|| "CLOUDFLARE_API_TOKEN".to_string()),
            path.clone(),
        )),
        _ => None,
    }
}
