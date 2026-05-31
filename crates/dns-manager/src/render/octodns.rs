//! Generic octoDNS rendering — a port of `utils/octodns.nix` +
//! `generate.octodnsConfig`.
//!
//! Output is a directory: `config.yaml` (a `ZoneFileSource` provider plus the
//! user's `config`/`zones`/`manager`) and `zones/<zone>` BIND files with a dummy
//! SOA injected (octoDNS's BIND source requires one per zone).

use serde::Deserialize;
use serde_json::{json, Value};

use super::{json_to_yaml, recursive_update, yaml, zonefile, OutputFile};
use crate::error::Result;
use crate::model::{Config, RawDoc, Record, RecordData, Soa};

/// Input document for the `octodns` subcommand.
#[derive(Clone, Debug, Deserialize)]
pub struct OctodnsInput {
    /// The raw DNS declarations to resolve and render.
    #[serde(rename = "dnsConfig")]
    pub dns_config: RawDoc,
    /// The user's octoDNS config; `providers.config`/`zones` are overlaid on top.
    #[serde(default = "super::empty_object")]
    pub config: Value,
    /// Per-zone octoDNS settings (`sources`/`targets`).
    #[serde(default = "super::empty_object")]
    pub zones: Value,
    /// Optional octoDNS `manager` block.
    #[serde(default)]
    pub manager: Option<Value>,
}

/// The dummy SOA octoDNS's BIND source requires (it is never synced).
fn fake_soa() -> Record {
    Record {
        ttl: 60,
        ttl_auto: false,
        proxied: false,
        comment: None,
        data: RecordData::Soa(vec![Soa {
            rname: "admin.example.invalid".to_string(),
            mname: "ns.example.invalid".to_string(),
            serial: 1970010100,
            refresh: 7200,
            retry: 3600,
            expire: 1209600,
            ttl: 60,
        }]),
    }
}

/// Inject a dummy SOA at each zone apex that lacks one.
fn inject_fake_soa(config: &mut Config) {
    for (zone, entries) in config.iter_mut() {
        let apex = entries.entry(zone.clone()).or_default();
        apex.entry("soa".to_string()).or_insert_with(fake_soa);
    }
}

/// Render the octoDNS output directory. `out_abs` is the absolute path the output
/// will live at; it is embedded as the `ZoneFileSource` directory.
pub fn render(input: &OctodnsInput, config: &Config, out_abs: &str) -> Result<Vec<OutputFile>> {
    let mut files = Vec::new();

    // Zone files (with a dummy SOA), under zones/<zone>.
    let mut with_soa = config.clone();
    inject_fake_soa(&mut with_soa);
    for (zone, text) in zonefile::render_all(&with_soa) {
        files.push(OutputFile::new(format!("zones/{zone}"), text));
    }

    // config.yaml: the user's config with our ZoneFileSource provider + zones overlaid.
    let mut overrides = json!({
        "providers": {
            "config": {
                "class": "octodns_bind.ZoneFileSource",
                "directory": format!("{out_abs}/zones"),
                "file_extension": "",
            }
        },
        "zones": input.zones,
    });
    if let Some(manager) = &input.manager {
        overrides["manager"] = manager.clone();
    }
    let merged = recursive_update(input.config.clone(), overrides);
    let config_yaml = yaml::to_string(&json_to_yaml(merged));
    files.push(OutputFile::new("config.yaml", config_yaml));

    Ok(files)
}
