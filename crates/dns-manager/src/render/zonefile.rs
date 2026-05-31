//! BIND zonefile rendering — a port of `utils/zonefiles.nix`.
//!
//! Records are emitted FQDN-sorted then type-sorted (`BTreeMap` order), one line
//! per value, each optionally preceded by a `; comment` line.

use crate::model::{Config, FqdnRecords, RecordData, Zone};

/// Split a TXT string into ≤255-**byte** quoted chunks per RFC 4408 (a DNS
/// character-string is at most 255 octets): `"chunk1" "chunk2"`. An empty string
/// renders as `""`. Chunking is on bytes (matching the legacy Nix, whose strings
/// are byte strings) but never splits a UTF-8 codepoint, so every chunk stays
/// valid and within the 255-octet limit.
pub fn format_txt(value: &str) -> String {
    let chunks: Vec<String> = if value.is_empty() {
        vec![String::new()]
    } else {
        let mut chunks = Vec::new();
        let mut current = String::new();
        for ch in value.chars() {
            if current.len() + ch.len_utf8() > 255 {
                chunks.push(std::mem::take(&mut current));
            }
            current.push(ch);
        }
        chunks.push(current);
        chunks
    };
    format!("\"{}\"", chunks.join("\" \""))
}

/// Render the RDATA token(s) for one record (one string per value).
fn rdata(data: &RecordData) -> Vec<String> {
    use RecordData::*;
    match data {
        A(v) => v.iter().map(|x| format!("A {x}")).collect(),
        Aaaa(v) => v.iter().map(|x| format!("AAAA {x}")).collect(),
        Cname(v) => v.iter().map(|x| format!("CNAME {x}")).collect(),
        Alias(v) => v.iter().map(|x| format!("ALIAS {x}")).collect(),
        Dname(v) => v.iter().map(|x| format!("DNAME {x}")).collect(),
        Ns(v) => v.iter().map(|x| format!("NS {x}.")).collect(),
        Txt(v) => v.iter().map(|x| format!("TXT {}", format_txt(x))).collect(),
        Mx(v) => v
            .iter()
            .map(|m| format!("MX {} {}.", m.preference, m.exchange))
            .collect(),
        Soa(v) => v
            .iter()
            .map(|s| {
                format!(
                    "SOA {}. {}. ( {} {} {} {} {} )",
                    s.mname, s.rname, s.serial, s.refresh, s.retry, s.expire, s.ttl
                )
            })
            .collect(),
        Srv(v) => v
            .iter()
            .map(|s| format!("SRV {} {} {} {}", s.priority, s.weight, s.port, s.target))
            .collect(),
        Uri(v) => v
            .iter()
            .map(|u| format!("URI {} {} {}", u.priority, u.weight, u.target))
            .collect(),
        Caa(v) => v
            .iter()
            .map(|c| format!("CAA {} {} {}", c.flags, c.tag, c.value))
            .collect(),
        Tlsa(v) => v
            .iter()
            .map(|t| {
                format!(
                    "TLSA {} {} {} {}",
                    t.usage, t.selector, t.matching_type, t.certificate_association_data
                )
            })
            .collect(),
        Sshfp(v) => v
            .iter()
            .map(|s| format!("SSHFP {} {} {}", s.algorithm, s.fp_type, s.fingerprint))
            .collect(),
    }
}

/// Render the records at a set of names (one zone's worth) into zonefile text.
pub fn render_zone(entries: &Zone) -> String {
    let mut out = String::new();
    for (fqdn, records) in entries {
        render_name(fqdn, records, &mut out);
    }
    out
}

fn render_name(fqdn: &str, records: &FqdnRecords, out: &mut String) {
    for record in records.values() {
        let ttl = if record.ttl_auto {
            String::new()
        } else {
            format!(" {}", record.ttl)
        };
        for payload in rdata(&record.data) {
            if let Some(comment) = &record.comment {
                out.push_str(&format!("; {comment}\n"));
            }
            out.push_str(&format!("{fqdn}. IN{ttl} {payload}\n"));
        }
    }
}

/// Render every zone in a resolved config to `(zone, zonefile text)` pairs.
pub fn render_all(config: &Config) -> Vec<(String, String)> {
    config
        .iter()
        .map(|(zone, entries)| (zone.clone(), render_zone(entries)))
        .collect()
}
