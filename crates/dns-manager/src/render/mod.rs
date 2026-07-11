//! Rendering the resolved [`Config`](crate::model::Config) into output formats:
//! BIND zonefiles, generic octoDNS, and Cloudflare-flavoured octoDNS.

pub mod caddy;
pub mod cloudflare;
pub mod octodns;
pub mod yaml;
pub mod zonefile;

use serde_json::Value;
use yaml::Yaml;

/// One file in a rendered output directory.
#[derive(Clone, Debug, PartialEq)]
pub struct OutputFile {
    /// Path relative to the output directory (e.g. `config.yaml`, `zones/example.com`).
    pub path: String,
    pub content: String,
    /// Whether the file should be marked executable (the octodns-sync wrapper).
    pub executable: bool,
}

impl OutputFile {
    pub fn new(path: impl Into<String>, content: impl Into<String>) -> Self {
        OutputFile {
            path: path.into(),
            content: content.into(),
            executable: false,
        }
    }
}

/// serde default for "arbitrary config" fields: an empty object (the Nix `{}`),
/// so `recursive_update(base, default)` is a no-op rather than nulling `base`.
pub(crate) fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// `lib.recursiveUpdate`: deep-merge two JSON values, with `over` winning on
/// scalar/array conflicts and maps merged key-wise.
pub fn recursive_update(base: Value, over: Value) -> Value {
    match (base, over) {
        (Value::Object(mut b), Value::Object(o)) => {
            for (key, ov) in o {
                let merged = match b.remove(&key) {
                    Some(bv) => recursive_update(bv, ov),
                    None => ov,
                };
                b.insert(key, merged);
            }
            Value::Object(b)
        }
        (_, over) => over,
    }
}

/// Convert a JSON value into the YAML tree for emission. Integer numbers map to
/// [`Yaml::Int`]; any non-integer number falls back to its string form.
pub fn json_to_yaml(value: Value) -> Yaml {
    match value {
        Value::Null => Yaml::Null,
        Value::Bool(b) => Yaml::Bool(b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => Yaml::Int(i),
            None => Yaml::Str(n.to_string()),
        },
        Value::String(s) => Yaml::Str(s),
        Value::Array(items) => Yaml::List(items.into_iter().map(json_to_yaml).collect()),
        Value::Object(entries) => Yaml::Map(
            entries
                .into_iter()
                .map(|(k, v)| (k, json_to_yaml(v)))
                .collect(),
        ),
    }
}
