//! Per-host subdomain resolution (inheritance from base domains) and grouping of
//! resolved names into zones — a faithful port of `getDomainsFromNixosConfigurations`
//! and the `mkSubRecord` default wiring.

use std::collections::{BTreeMap, BTreeSet};

use super::domains::get_most_specific;
use super::merge::merge_fqdn_records;
use super::normalize::{normalize_data, Context};
use crate::error::Result;
use crate::model::{Config, FqdnRecords, RawExtra, RawHost, Record, Zone};

/// Resolve a host's base domains into typed, ttl-filled records (empty/`[null]`
/// base records are dropped). Keyed by base-domain name → type → record.
fn resolve_base(host: &RawHost) -> Result<BTreeMap<String, FqdnRecords>> {
    let mut out = BTreeMap::new();
    for (name, recs) in &host.base_domains {
        let mut fr = FqdnRecords::new();
        for (rtype, raw) in recs {
            let data = normalize_data(rtype, &raw.data, Context::Base)?;
            if data.is_empty() {
                continue;
            }
            fr.insert(
                rtype.to_lowercase(),
                Record {
                    ttl: raw.ttl.unwrap_or(host.default_ttl),
                    ttl_auto: raw.ttl_auto,
                    proxied: raw.proxied,
                    comment: raw.comment.clone(),
                    data,
                },
            );
        }
        out.insert(name.clone(), fr);
    }
    Ok(out)
}

/// Resolve one host's subdomains: every non-empty base record type is inherited
/// (data + ttl only) unless the subdomain overrides it, then the subdomain's own
/// records are layered on top. Returns name → records.
pub fn resolve_host(host: &RawHost) -> Result<BTreeMap<String, FqdnRecords>> {
    let base_names: Vec<String> = host.base_domains.keys().cloned().collect();
    let base_records = resolve_base(host)?;

    let mut out: BTreeMap<String, FqdnRecords> = BTreeMap::new();
    for (sub_name, sub_recs) in &host.sub_domains {
        let matched = get_most_specific(sub_name, &base_names);
        let base_fr = matched.as_ref().and_then(|m| base_records.get(m));
        let user_types: BTreeSet<String> = sub_recs.keys().map(|k| k.to_lowercase()).collect();

        let mut fr = FqdnRecords::new();

        // Inherited records: data + ttl carry over; flags and comment reset.
        // SOA is never inherited onto subdomains (the legacy `soa.sub` defaults to
        // null); a zone's SOA only comes from the apex/extraConfig.
        if let Some(base_fr) = base_fr {
            for (rtype, brec) in base_fr {
                if rtype == "soa" || user_types.contains(rtype) {
                    continue;
                }
                fr.insert(
                    rtype.clone(),
                    Record {
                        ttl: brec.ttl,
                        ttl_auto: false,
                        proxied: false,
                        comment: None,
                        data: brec.data.clone(),
                    },
                );
            }
        }

        // The subdomain's own records (sub context). A missing ttl defaults to the
        // matching base record's ttl, else the host default.
        for (rtype, raw) in sub_recs {
            let key = rtype.to_lowercase();
            let data = normalize_data(rtype, &raw.data, Context::Sub)?;
            if data.is_empty() {
                continue;
            }
            let ttl = raw.ttl.unwrap_or_else(|| {
                base_fr
                    .and_then(|b| b.get(&key))
                    .map(|r| r.ttl)
                    .unwrap_or(host.default_ttl)
            });
            fr.insert(
                key,
                Record {
                    ttl,
                    ttl_auto: raw.ttl_auto,
                    proxied: raw.proxied,
                    comment: raw.comment.clone(),
                    data,
                },
            );
        }

        if !fr.is_empty() {
            out.insert(sub_name.clone(), fr);
        }
    }
    Ok(out)
}

/// The apex set: base domains that are not themselves a subdomain of another base
/// domain (`reducedBaseDomains`).
pub fn reduce_base_names(all: &[String]) -> Vec<String> {
    all.iter()
        .filter(|b| {
            let others: Vec<&str> = all.iter().filter(|x| x != b).map(String::as_str).collect();
            get_most_specific(b, &others).is_none()
        })
        .cloned()
        .collect()
}

/// Group resolved names under their most-specific apex zone.
pub fn group_into_zones(subs: &BTreeMap<String, FqdnRecords>, apexes: &[String]) -> Config {
    let mut cfg = Config::new();
    for (name, recs) in subs {
        if let Some(apex) = get_most_specific(name, apexes) {
            cfg.entry(apex)
                .or_default()
                .insert(name.clone(), recs.clone());
        }
    }
    cfg
}

/// Resolve `extraConfig` into a [`Config`]: relative names are expanded to FQDNs
/// (`""` → zone apex) and empty records dropped.
pub fn resolve_extra(extra: &RawExtra) -> Result<Config> {
    let mut cfg = Config::new();
    for (zone, names) in &extra.zones {
        let mut zone_map = Zone::new();
        for (name, recs) in names {
            let fqdn = if name.is_empty() {
                zone.clone()
            } else {
                format!("{name}.{zone}")
            };
            let mut fr = FqdnRecords::new();
            for (rtype, raw) in recs {
                let data = normalize_data(rtype, &raw.data, Context::Base)?;
                if data.is_empty() {
                    continue;
                }
                fr.insert(
                    rtype.to_lowercase(),
                    Record {
                        ttl: raw.ttl.unwrap_or(extra.default_ttl),
                        ttl_auto: raw.ttl_auto,
                        proxied: raw.proxied,
                        comment: raw.comment.clone(),
                        data,
                    },
                );
            }
            if fr.is_empty() {
                continue;
            }
            match zone_map.get(&fqdn) {
                Some(existing) => {
                    let merged = merge_fqdn_records(existing, &fr);
                    zone_map.insert(fqdn, merged);
                }
                None => {
                    zone_map.insert(fqdn, fr);
                }
            }
        }
        if !zone_map.is_empty() {
            cfg.insert(zone.clone(), zone_map);
        }
    }
    Ok(cfg)
}
