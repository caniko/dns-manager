//! Multi-provider octoDNS configuration and YAML source zones.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::{json_to_yaml, recursive_update, yaml, yaml::Yaml, OutputFile};
use crate::model::{Config, RawDoc, Record, RecordData, Zone};
use crate::provider::ProviderKind;
use crate::resolve::ensure_trailing_dot;
use crate::{Error, Result};

/// Only explicitly configured zones are synced.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncInput {
    pub dns_config: RawDoc,
    pub providers: BTreeMap<String, ProviderConfig>,
    pub zones: BTreeMap<String, ZoneSync>,
    #[serde(default = "super::empty_object")]
    pub extra_global_config: Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    /// Defaults to the standard environment variable for this provider.
    pub token: Option<Token>,
    #[serde(default)]
    pub settings: Map<String, Value>,
    /// Override the render-time floor, e.g. for an approved deSEC exception.
    #[serde(alias = "min_ttl_override")]
    pub min_ttl_override: Option<i64>,
}

/// An octoDNS API token reference.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Token {
    /// Read from environment variable `name` at sync time (`env/NAME`).
    Env { name: String },
    /// A literal token value (discouraged; flagged in the config).
    Literal { value: String },
    /// The runtime launcher must export the file's contents into `envName`.
    File {
        path: String,
        #[serde(default, rename = "envName")]
        env_name: Option<String>,
    },
}

impl Token {
    fn provider_value(&self, default_env: &str) -> String {
        match self {
            Token::Env { name } => format!("env/{name}"),
            Token::Literal { value } => value.clone(),
            Token::File { env_name, .. } => {
                format!("env/{}", env_name.as_deref().unwrap_or(default_env))
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ZoneSync {
    pub targets: Vec<String>,
    pub mode: Option<String>,
    pub manage_record_types: Option<Vec<String>>,
    #[serde(default)]
    pub exclude_records: Vec<ExcludeRecord>,
    #[serde(default)]
    pub processors: Vec<String>,
}

/// A single owner/type pair to leave unmanaged.
#[derive(Clone, Debug, Deserialize)]
pub struct ExcludeRecord {
    pub name: String,
    #[serde(rename = "type")]
    pub rtype: String,
}

fn record_entry(rtype: &str, record: &Record, cloudflare: bool, comments: bool) -> Value {
    let mut entry = value_field(&record.data);
    entry.insert("ttl".to_string(), json!(record.ttl));
    if cloudflare {
        let mut cf = Map::new();
        if record.proxied {
            cf.insert("proxied".to_string(), json!(true));
        }
        if record.ttl_auto {
            cf.insert("auto-ttl".to_string(), json!(true));
        }
        if comments {
            if let Some(comment) = &record.comment {
                cf.insert("comment".to_string(), json!(comment));
            }
        }
        if !cf.is_empty() {
            entry.insert("octodns".to_string(), json!({ "cloudflare": cf }));
        }
    }
    entry.insert("type".to_string(), json!(rtype.to_uppercase()));
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

fn relative_name(zone: &str, name: &str) -> Result<String> {
    let name = name.to_ascii_lowercase();
    let bare = name.strip_suffix('.').unwrap_or(&name);
    if matches!(name.as_str(), "" | "@") || bare == zone {
        Ok(String::new())
    } else if let Some(prefix) = bare.strip_suffix(zone).and_then(|p| p.strip_suffix('.')) {
        Ok(prefix.to_string())
    } else if name.ends_with('.') {
        Err(Error::msg(format!("owner {name} is outside zone {zone}")))
    } else {
        Ok(name)
    }
}

fn ignored_entry(rtype: &str, ttl: i64) -> Result<Value> {
    let value = match rtype {
        "A" => json!("192.0.2.1"),
        "AAAA" => json!("2001:db8::1"),
        "TXT" | "SPF" => json!("ignored"),
        "CNAME" | "ALIAS" | "DNAME" | "NS" | "PTR" => json!("ignored.invalid."),
        "MX" => json!({"preference": 10, "exchange": "ignored.invalid."}),
        "SRV" => json!({"priority": 0, "weight": 0, "port": 1, "target": "ignored.invalid."}),
        "URI" => json!({"priority": 0, "weight": 0, "target": "https://ignored.invalid/"}),
        "CAA" => json!({"flags": 0, "tag": "issue", "value": "ignored.invalid"}),
        "TLSA" => {
            json!({"certificate_usage": 3, "selector": 1, "matching_type": 1, "certificate_association_data": "00".repeat(32)})
        }
        "SSHFP" => json!({"algorithm": 1, "fingerprint_type": 1, "fingerprint": "00".repeat(20)}),
        "DS" => json!({"key_tag": 1, "algorithm": 13, "digest_type": 2, "digest": "00".repeat(32)}),
        _ => {
            return Err(Error::msg(format!(
                "no octoDNS exclusion placeholder for {rtype}"
            )))
        }
    };
    // octoDNS pairs records by owner and type, and skips ignored records on
    // both sides of the plan (including deletions from the target).
    Ok(json!({"type": rtype, "ttl": ttl, "value": value, "octodns": {"ignored": true}}))
}

impl ProviderConfig {
    fn min_ttl(&self) -> i64 {
        self.min_ttl_override.unwrap_or(self.kind.caps().min_ttl)
    }

    fn to_octo(&self, id: &str) -> Result<Value> {
        if self.min_ttl() < 1 {
            return Err(Error::msg(format!(
                "provider {id}: minimum TTL must be positive"
            )));
        }
        let caps = self.kind.caps();
        let mut entry: Map<String, Value> = caps
            .extra
            .iter()
            .map(|&(k, v)| (k.into(), json!(v)))
            .collect();
        entry.extend(self.settings.clone());
        entry.insert("class".into(), json!(caps.class));
        entry.insert(
            caps.token_key.into(),
            json!(self.token.as_ref().map_or_else(
                || format!("env/{}", caps.default_env),
                |token| token.provider_value(caps.default_env)
            )),
        );
        if matches!(self.token, Some(Token::Literal { .. })) {
            let label = if self.kind == ProviderKind::Cloudflare {
                "Cloudflare"
            } else {
                id
            };
            entry.insert("_comment_".into(), json!(format!("Literal {label} tokens are unsafe for committed configs; prefer env or file tokens.")));
        }
        // Other plugins do not necessarily accept a min_ttl constructor option.
        if self.kind == ProviderKind::Cloudflare {
            entry.insert("min_ttl".into(), json!(self.min_ttl()));
        }
        Ok(Value::Object(entry))
    }
}

fn build_zone(
    zone: &str,
    entries: &Zone,
    settings: &ZoneSync,
    targets: &[(&str, &ProviderConfig)],
) -> Result<String> {
    let cloudflare = targets
        .iter()
        .any(|(_, p)| p.kind == ProviderKind::Cloudflare);
    let supports_comments = targets.iter().any(|(_, p)| p.kind.caps().comments);
    let placeholder_ttl = targets
        .iter()
        .map(|(_, p)| p.min_ttl())
        .max()
        .unwrap_or(3600);
    let mut records: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    let mut comments: BTreeMap<String, Vec<String>> = BTreeMap::new();

    let check_apex_cname = |rel: &str, rtype: &str| {
        if rel.is_empty() && rtype == "CNAME" {
            Err(Error::msg(format!("{zone}: apex CNAME is never supported")))
        } else {
            Ok(())
        }
    };
    for rec in &settings.exclude_records {
        let rel = relative_name(zone, &rec.name)?;
        let rtype = rec.rtype.to_ascii_uppercase();
        check_apex_cname(&rel, &rtype)?;
        if rtype == "SOA" || (rel.is_empty() && rtype == "NS") {
            continue;
        }
        let entry = ignored_entry(&rtype, placeholder_ttl)?;
        records.entry(rel).or_default().insert(rtype, entry);
    }
    for (fqdn, recs) in entries {
        let rel = relative_name(zone, fqdn)?;
        for (rtype, record) in recs {
            let rtype = rtype.to_ascii_uppercase();
            check_apex_cname(&rel, &rtype)?;
            if rtype == "SOA" || (rel.is_empty() && rtype == "NS") {
                continue;
            }
            let at_name = records.entry(rel.clone()).or_default();
            if at_name.contains_key(&rtype) {
                if at_name[&rtype]["octodns"]["ignored"] == true {
                    continue;
                }
                return Err(Error::msg(format!("duplicate record {fqdn} {rtype}")));
            }
            if settings
                .manage_record_types
                .as_ref()
                .is_some_and(|types| !types.iter().any(|t| t.eq_ignore_ascii_case(&rtype)))
            {
                continue;
            }
            for (id, provider) in targets {
                let caps = provider.kind.caps();
                let fail =
                    |message: &str| Error::msg(format!("{fqdn} {rtype}, target {id}: {message}"));
                if record.proxied && !caps.proxied {
                    return Err(fail("proxied records are not supported"));
                }
                if rtype == "ALIAS" && rel.is_empty() && !caps.apex_alias {
                    return Err(fail("apex ALIAS is not supported"));
                }
                if record.ttl < provider.min_ttl() {
                    return Err(fail(&format!(
                        "TTL {} is below minimum {}",
                        record.ttl,
                        provider.min_ttl()
                    )));
                }
            }
            at_name.insert(
                rtype.clone(),
                record_entry(&rtype, record, cloudflare, supports_comments),
            );
            if let Some(comment) = record.comment.as_ref().filter(|_| supports_comments) {
                let owner_comments = comments.entry(rel.clone()).or_default();
                if !owner_comments.contains(comment) {
                    owner_comments.push(comment.clone());
                }
            }
        }
    }

    let records = records
        .into_iter()
        .filter(|(_, types)| !types.is_empty())
        .map(|(name, types)| {
            (
                name,
                Yaml::List(types.into_values().map(json_to_yaml).collect()),
            )
        })
        .collect();
    Ok(yaml::zone_to_string(&records, &comments))
}

/// Render the selected zones and provider configuration.
pub fn render(input: &SyncInput, config: &Config, out_abs: &str) -> Result<Vec<OutputFile>> {
    if !input.extra_global_config.is_object() {
        return Err(Error::msg("extraGlobalConfig must be an object"));
    }
    let mut providers = Map::new();
    for (id, provider) in &input.providers {
        if id.is_empty() || id == "config" {
            return Err(Error::msg(
                "provider ID must be nonempty and cannot be 'config'",
            ));
        }
        providers.insert(id.clone(), provider.to_octo(id)?);
    }
    providers.insert(
        "config".into(),
        json!({
            "class": "octodns.provider.yaml.YamlProvider",
            "directory": format!("{out_abs}/zones"),
            "default_ttl": 3600,
            "enforce_order": true,
            "order_mode": "simple",
            "escaped_semicolons": false,
            "supports_root_ns": false,
        }),
    );

    let mut processors = Map::new();
    processors.insert(
        "ignore-root-ns".into(),
        json!({"class": "octodns.processor.filter.IgnoreRootNsFilter"}),
    );
    let mut zones = Map::new();
    let mut files = Vec::new();
    for (zone_key, settings) in &input.zones {
        let zone = zone_key.strip_suffix('.').unwrap_or(zone_key);
        if zone.split('.').any(|label| {
            label.is_empty()
                || !label
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
        }) {
            return Err(Error::msg(format!("invalid sync zone name: {zone_key}")));
        }
        let dotted = ensure_trailing_dot(zone);
        if zones.contains_key(&dotted) {
            return Err(Error::msg(format!("duplicate sync zone {zone}")));
        }
        let entries = config
            .get(zone)
            .or_else(|| config.get(&dotted))
            .ok_or_else(|| Error::msg(format!("sync zone {zone} is not declared in dnsConfig")))?;
        let lenient = match settings.mode.as_deref().unwrap_or("lenient") {
            "lenient" => true,
            "strict" => false,
            mode => return Err(Error::msg(format!("zone {zone}: unknown mode {mode}"))),
        };
        if settings.targets.is_empty() {
            return Err(Error::msg(format!(
                "zone {zone}: at least one target is required"
            )));
        }
        let mut targets = Vec::new();
        for id in &settings.targets {
            let provider = input
                .providers
                .get(id)
                .ok_or_else(|| Error::msg(format!("zone {zone}: unknown target {id}")))?;
            if targets.iter().any(|&(seen, _)| seen == id) {
                return Err(Error::msg(format!("zone {zone}: duplicate target {id}")));
            }
            targets.push((id.as_str(), provider));
        }
        let mut procs = vec!["ignore-root-ns".to_string()];
        if let Some(types) = &settings.manage_record_types {
            let id = format!("manage-types-{zone}");
            let types: Vec<_> = types.iter().map(|t| t.to_ascii_uppercase()).collect();
            processors.insert(
                id.clone(),
                json!({
                    "class": "octodns.processor.filter.TypeAllowlistFilter",
                    "allowlist": types,
                    "include_target": true
                }),
            );
            procs.push(id);
        }
        procs.extend(settings.processors.iter().cloned());
        zones.insert(dotted, json!({
            "lenient": lenient, "sources": ["config"], "processors": procs, "targets": settings.targets
        }));
        files.push(OutputFile::new(
            format!("zones/{zone}.yaml"),
            build_zone(&zone.to_ascii_lowercase(), entries, settings, &targets)?,
        ));
    }

    // Generated wiring takes precedence: an overlay must not silently bypass
    // validation or add an unmanaged zone to the sync. Other octoDNS settings
    // (notably manager options and custom processors) remain extensible.
    let mut merged = recursive_update(
        json!({"manager": {"include_meta": false}}),
        input.extra_global_config.clone(),
    );
    merged = recursive_update(
        merged,
        json!({"providers": providers, "processors": processors, "zones": zones}),
    );
    if merged["zones"]
        .as_object()
        .is_some_and(|z| z.len() != input.zones.len())
    {
        return Err(Error::msg(
            "declare sync zones in zones, not extraGlobalConfig.zones",
        ));
    }
    files.push(OutputFile::new(
        "config.yaml",
        yaml::to_string(&json_to_yaml(merged)),
    ));
    Ok(files)
}
