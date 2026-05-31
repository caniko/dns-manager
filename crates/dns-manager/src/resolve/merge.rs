//! Merging resolved records, mirroring the legacy `recursiveUpdateLists`.
//!
//! Two records of the same type at the same name combine by **concatenating their
//! value lists and de-duplicating** (preserving first-seen order), while scalar
//! fields follow the legacy attrset-merge: `ttl` is last-wins; `ttlAuto`/`proxied`
//! are OR-ed (they only appear in the merged attrset when `true`); `comment` is
//! last-non-empty.

use crate::model::{Config, FqdnRecords, Record, RecordData, Zone};

/// `lib.unique (a ++ b)`: concatenation keeping the first occurrence of each value.
fn concat_unique<T: PartialEq + Clone>(a: &[T], b: &[T]) -> Vec<T> {
    let mut out: Vec<T> = Vec::with_capacity(a.len() + b.len());
    for item in a.iter().chain(b.iter()) {
        if !out.contains(item) {
            out.push(item.clone());
        }
    }
    out
}

fn merge_data(a: &RecordData, b: &RecordData) -> RecordData {
    use RecordData::*;
    match (a, b) {
        (A(x), A(y)) => A(concat_unique(x, y)),
        (Aaaa(x), Aaaa(y)) => Aaaa(concat_unique(x, y)),
        (Cname(x), Cname(y)) => Cname(concat_unique(x, y)),
        (Alias(x), Alias(y)) => Alias(concat_unique(x, y)),
        (Dname(x), Dname(y)) => Dname(concat_unique(x, y)),
        (Ns(x), Ns(y)) => Ns(concat_unique(x, y)),
        (Txt(x), Txt(y)) => Txt(concat_unique(x, y)),
        (Mx(x), Mx(y)) => Mx(concat_unique(x, y)),
        (Soa(x), Soa(y)) => Soa(concat_unique(x, y)),
        (Srv(x), Srv(y)) => Srv(concat_unique(x, y)),
        (Uri(x), Uri(y)) => Uri(concat_unique(x, y)),
        (Caa(x), Caa(y)) => Caa(concat_unique(x, y)),
        (Tlsa(x), Tlsa(y)) => Tlsa(concat_unique(x, y)),
        (Sshfp(x), Sshfp(y)) => Sshfp(concat_unique(x, y)),
        // Different types under the same key cannot happen (the key *is* the type);
        // fall back to last-wins for totality.
        _ => b.clone(),
    }
}

/// Merge two records of the same type at the same name.
pub fn merge_record(a: &Record, b: &Record) -> Record {
    Record {
        ttl: b.ttl,
        ttl_auto: a.ttl_auto || b.ttl_auto,
        proxied: a.proxied || b.proxied,
        comment: b.comment.clone().or_else(|| a.comment.clone()),
        data: merge_data(&a.data, &b.data),
    }
}

/// Merge the records at one name (type → record).
pub fn merge_fqdn_records(a: &FqdnRecords, b: &FqdnRecords) -> FqdnRecords {
    let mut out = a.clone();
    for (rtype, rb) in b {
        match out.get(rtype) {
            Some(ra) => {
                let merged = merge_record(ra, rb);
                out.insert(rtype.clone(), merged);
            }
            None => {
                out.insert(rtype.clone(), rb.clone());
            }
        }
    }
    out
}

/// Merge two zones (name → records). Also used to merge the per-host subdomain
/// maps before they are grouped into zones.
pub fn merge_zone(a: &Zone, b: &Zone) -> Zone {
    let mut out = a.clone();
    for (name, zb) in b {
        match out.get(name) {
            Some(za) => {
                let merged = merge_fqdn_records(za, zb);
                out.insert(name.clone(), merged);
            }
            None => {
                out.insert(name.clone(), zb.clone());
            }
        }
    }
    out
}

/// Merge two zone configs (zone apex → zone).
pub fn merge_config(a: &Config, b: &Config) -> Config {
    let mut out = a.clone();
    for (zone, zb) in b {
        match out.get(zone) {
            Some(za) => {
                let merged = merge_zone(za, zb);
                out.insert(zone.clone(), merged);
            }
            None => {
                out.insert(zone.clone(), zb.clone());
            }
        }
    }
    out
}
