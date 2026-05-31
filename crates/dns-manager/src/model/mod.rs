//! The data model: the **raw** config the Nix layer serializes, and the
//! **resolved** config the renderers consume.
//!
//! The Nix↔Rust contract is the raw side ([`RawDoc`]). Everything below that is
//! produced by [`crate::resolve`] and never crosses the boundary.

mod record;

use serde::Deserialize;
use std::collections::BTreeMap;

pub use record::{Caa, Mx, Record, RecordData, Soa, Srv, Sshfp, Tlsa, Uri};

// ---------------------------------------------------------------------------
// Raw input — the JSON document emitted by the Nix `collect` layer.
// ---------------------------------------------------------------------------

/// Top-level raw document: per-host `networking.domains` declarations plus an
/// optional standalone `extraConfig`. This is exactly what `builtins.toJSON`
/// produces in Nix; field names match the Nix attribute names.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawDoc {
    /// One entry per NixOS host that enabled `networking.domains`.
    #[serde(default)]
    pub hosts: Vec<RawHost>,
    /// Standalone records declared outside any host, grouped by zone apex.
    #[serde(default, rename = "extraConfig")]
    pub extra_config: Option<RawExtra>,
}

/// A single host's `networking.domains` declaration.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawHost {
    /// TTL applied to records on this host that do not set one explicitly.
    #[serde(rename = "defaultTTL")]
    pub default_ttl: i64,
    /// Template/default records that subdomains inherit from. Never emitted directly.
    #[serde(default, rename = "baseDomains")]
    pub base_domains: BTreeMap<String, RawRecords>,
    /// Concrete records, keyed by fully-qualified name, that inherit from base domains.
    #[serde(default, rename = "subDomains")]
    pub sub_domains: BTreeMap<String, RawRecords>,
}

/// Standalone `extraConfig`: records grouped by zone apex then by relative name
/// (`""` = apex).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawExtra {
    #[serde(rename = "defaultTTL")]
    pub default_ttl: i64,
    #[serde(default)]
    pub zones: BTreeMap<String, BTreeMap<String, RawRecords>>,
}

/// Records at one name, keyed by lowercase record type.
pub type RawRecords = BTreeMap<String, RawRecord>;

/// One declared record before normalization. `data` is kept as raw JSON because
/// its shape depends on the record type (scalar, list, object, or list of
/// objects); [`crate::resolve::normalize`] turns it into typed [`RecordData`].
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawRecord {
    /// Explicit TTL, or `None` to inherit the host/zone `defaultTTL`.
    #[serde(default)]
    pub ttl: Option<i64>,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default, rename = "ttlAuto")]
    pub ttl_auto: bool,
    #[serde(default)]
    pub proxied: bool,
    /// Untyped payload; interpreted by record type during normalization.
    #[serde(default)]
    pub data: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Resolved output — what the renderers consume.
// ---------------------------------------------------------------------------

/// Records at one fully-qualified name, keyed by lowercase record type. `BTreeMap`
/// gives the bytewise key ordering BIND/octoDNS output relies on.
pub type FqdnRecords = BTreeMap<String, Record>;

/// One zone: fully-qualified name → its records.
pub type Zone = BTreeMap<String, FqdnRecords>;

/// The fully resolved DNS config: zone apex → zone.
pub type Config = BTreeMap<String, Zone>;
