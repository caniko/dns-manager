//! Resolve a [`RawDoc`] into the final [`Config`] the renderers consume.
//!
//! Pipeline (faithful to the legacy Nix):
//! 1. Each host's subdomains are resolved against *that host's* base domains
//!    (per-host inheritance), then merged across hosts (list-concat, last-wins).
//! 2. The union of base-domain names yields the apex set; resolved names are
//!    grouped under their most-specific apex.
//! 3. `extraConfig` is expanded to FQDNs and merged in (hosts win scalar conflicts).

pub mod domains;
mod inherit;
mod merge;
mod normalize;

pub use normalize::{ensure_trailing_dot, Context};

use std::collections::{BTreeMap, BTreeSet};

use crate::error::Result;
use crate::model::{Config, FqdnRecords, RawDoc};

/// Resolve a raw document into the final zone config. Assumes the document has
/// already passed [`crate::validate::check`].
pub fn resolve(doc: &RawDoc) -> Result<Config> {
    // 1. Per-host resolution + cross-host merge of subdomains.
    let mut merged_subs: BTreeMap<String, FqdnRecords> = BTreeMap::new();
    let mut all_base_names: BTreeSet<String> = BTreeSet::new();
    for host in &doc.hosts {
        all_base_names.extend(host.base_domains.keys().cloned());
        let resolved = inherit::resolve_host(host)?;
        merged_subs = merge::merge_zone(&merged_subs, &resolved);
    }

    // 2. Group resolved names under their apex zones.
    let all: Vec<String> = all_base_names.into_iter().collect();
    let apexes = inherit::reduce_base_names(&all);
    let host_config = inherit::group_into_zones(&merged_subs, &apexes);

    // 3. extraConfig, then merge (extraConfig first so hosts win scalar conflicts).
    let extra_config = match &doc.extra_config {
        Some(extra) => inherit::resolve_extra(extra)?,
        None => Config::new(),
    };

    Ok(merge::merge_config(&extra_config, &host_config))
}
