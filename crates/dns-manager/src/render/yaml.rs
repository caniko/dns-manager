//! A tiny, deterministic YAML emitter.
//!
//! octoDNS configs are plain maps/lists/scalars, so a focused emitter avoids a
//! YAML dependency and — crucially — lets the zone-file writer inject per-name
//! `# comment` lines, which a structured serializer cannot. String scalars and
//! keys are emitted as JSON (always double-quoted), which is valid YAML and
//! mirrors the legacy `scalarToYaml`/`builtins.toJSON` behaviour. Maps are
//! key-sorted for stable output.

use std::collections::BTreeMap;

/// A minimal YAML value tree.
#[derive(Clone, Debug, PartialEq)]
pub enum Yaml {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    List(Vec<Yaml>),
    Map(BTreeMap<String, Yaml>),
}

impl Yaml {
    /// Convenience constructor for a string scalar.
    pub fn str(s: impl Into<String>) -> Yaml {
        Yaml::Str(s.into())
    }

    /// A nested value occupies following indented lines; a scalar (and empty
    /// collections) stay inline.
    fn is_block(&self) -> bool {
        match self {
            Yaml::List(items) => !items.is_empty(),
            Yaml::Map(entries) => !entries.is_empty(),
            _ => false,
        }
    }

    /// Render this value as an inline scalar token.
    fn scalar(&self) -> String {
        match self {
            Yaml::Null => "null".to_string(),
            Yaml::Bool(b) => b.to_string(),
            Yaml::Int(i) => i.to_string(),
            Yaml::Str(s) => json_string(s),
            Yaml::List(_) => "[]".to_string(),
            Yaml::Map(_) => "{}".to_string(),
        }
    }
}

/// JSON-encode a string (always double-quoted, properly escaped) — valid YAML.
fn json_string(s: &str) -> String {
    serde_json::to_string(s).expect("string is always serializable")
}

/// Build a map from `(key, value)` pairs.
pub fn map<I: IntoIterator<Item = (String, Yaml)>>(entries: I) -> Yaml {
    Yaml::Map(entries.into_iter().collect())
}

fn indent(width: usize) -> String {
    " ".repeat(width)
}

/// Emit the lines for `value` at the given indentation (mirrors `yamlLines`).
fn lines(value: &Yaml, width: usize, out: &mut Vec<String>) {
    let pad = indent(width);
    match value {
        Yaml::Map(entries) => {
            for (key, child) in entries {
                let key = json_string(key);
                if child.is_block() {
                    out.push(format!("{pad}{key}:"));
                    lines(child, width + 2, out);
                } else {
                    out.push(format!("{pad}{key}: {}", child.scalar()));
                }
            }
        }
        Yaml::List(items) => {
            for child in items {
                if child.is_block() {
                    out.push(format!("{pad}-"));
                    lines(child, width + 2, out);
                } else {
                    out.push(format!("{pad}- {}", child.scalar()));
                }
            }
        }
        scalar => out.push(format!("{pad}{}", scalar.scalar())),
    }
}

/// Render a complete YAML document with a trailing newline.
pub fn to_string(value: &Yaml) -> String {
    let mut out = Vec::new();
    lines(value, 0, &mut out);
    let mut text = out.join("\n");
    text.push('\n');
    text
}

/// Render a zone file: a top-level map of name → record-entry list, with each
/// name optionally preceded by `# comment` lines (mirrors `zoneYamlLines`).
pub fn zone_to_string(
    records: &BTreeMap<String, Yaml>,
    comments: &BTreeMap<String, Vec<String>>,
) -> String {
    let mut out = Vec::new();
    for (name, entries) in records {
        if let Some(cs) = comments.get(name) {
            for comment in cs {
                out.push(format!("# {comment}"));
            }
        }
        out.push(format!("{}:", json_string(name)));
        lines(entries, 2, &mut out);
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}
