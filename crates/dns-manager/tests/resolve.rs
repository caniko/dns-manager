//! End-to-end resolve tests, ported from the legacy nix-unit `testGetDnsConfig`
//! golden in `utils/tests/domains.nix`. This is the primary oracle for the
//! per-host inheritance, cross-host merge, apex grouping, and normalization.

use std::collections::BTreeMap;

use dns_manager::model::{Config, FqdnRecords, Record, RecordData, Srv};
use dns_manager::{parse_document, resolve_document};
use serde_json::json;

fn strs(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

fn rec(ttl: i64, data: RecordData) -> Record {
    Record {
        ttl,
        ttl_auto: false,
        proxied: false,
        comment: None,
        data,
    }
}

fn fqdn(entries: Vec<(&str, Record)>) -> FqdnRecords {
    entries
        .into_iter()
        .map(|(rtype, record)| (rtype.to_string(), record))
        .collect()
}

#[test]
fn resolves_multi_host_with_extra_config() {
    // host3 (enable = false) is excluded by the Nix `collect` layer, so it never
    // reaches the binary — the input below mirrors that.
    let input = json!({
        "hosts": [
            {
                "defaultTTL": 86400,
                "baseDomains": { "example.com": {
                    "a": { "data": "198.51.100.1" },
                    "aaaa": { "data": "2001:db8:d9a2:5198::1" }
                } },
                "subDomains": { "host1.example.com": {}, "www.example.com": {} }
            },
            {
                "defaultTTL": 86400,
                "baseDomains": { "example.com": {
                    "a": { "data": "198.51.100.2" },
                    "aaaa": { "data": "2001:db8:d9a2:5198::2" }
                } },
                "subDomains": { "host2.example.com": {}, "www.example.com": {} }
            },
            {
                "defaultTTL": 86400,
                "baseDomains": { "example.com": {
                    "a": { "data": "198.51.100.4" },
                    "aaaa": { "data": "2001:db8:d9a2:5198::4" }
                } },
                "subDomains": { "host4.example.com": {} }
            }
        ],
        "extraConfig": {
            "defaultTTL": 60,
            "zones": {
                "example.com": { "": { "ns": { "data": ["ns1.invalid", "ns2.invalid", "ns3.invalid"] } } },
                "example.org": {
                    "": { "cname": { "data": "www.example.com" } },
                    "_xmpp._tcp": { "srv": { "data": {
                        "priority": 10, "weight": 5, "port": 5223, "target": "host1.example.com"
                    } } }
                }
            }
        }
    });

    let doc = parse_document(&input.to_string()).unwrap();
    let got = resolve_document(&doc).unwrap();

    let mut com: BTreeMap<String, FqdnRecords> = BTreeMap::new();
    com.insert(
        "example.com".into(),
        fqdn(vec![(
            "ns",
            rec(
                60,
                RecordData::Ns(strs(&["ns1.invalid", "ns2.invalid", "ns3.invalid"])),
            ),
        )]),
    );
    com.insert(
        "host1.example.com".into(),
        fqdn(vec![
            ("a", rec(86400, RecordData::A(strs(&["198.51.100.1"])))),
            (
                "aaaa",
                rec(86400, RecordData::Aaaa(strs(&["2001:db8:d9a2:5198::1"]))),
            ),
        ]),
    );
    com.insert(
        "host2.example.com".into(),
        fqdn(vec![
            ("a", rec(86400, RecordData::A(strs(&["198.51.100.2"])))),
            (
                "aaaa",
                rec(86400, RecordData::Aaaa(strs(&["2001:db8:d9a2:5198::2"]))),
            ),
        ]),
    );
    com.insert(
        "host4.example.com".into(),
        fqdn(vec![
            ("a", rec(86400, RecordData::A(strs(&["198.51.100.4"])))),
            (
                "aaaa",
                rec(86400, RecordData::Aaaa(strs(&["2001:db8:d9a2:5198::4"]))),
            ),
        ]),
    );
    com.insert(
        "www.example.com".into(),
        fqdn(vec![
            (
                "a",
                rec(
                    86400,
                    RecordData::A(strs(&["198.51.100.1", "198.51.100.2"])),
                ),
            ),
            (
                "aaaa",
                rec(
                    86400,
                    RecordData::Aaaa(strs(&["2001:db8:d9a2:5198::1", "2001:db8:d9a2:5198::2"])),
                ),
            ),
        ]),
    );

    let mut org: BTreeMap<String, FqdnRecords> = BTreeMap::new();
    org.insert(
        "example.org".into(),
        fqdn(vec![(
            "cname",
            rec(60, RecordData::Cname(strs(&["www.example.com."]))),
        )]),
    );
    org.insert(
        "_xmpp._tcp.example.org".into(),
        fqdn(vec![(
            "srv",
            rec(
                60,
                RecordData::Srv(vec![Srv {
                    priority: 10,
                    weight: 5,
                    port: 5223,
                    target: "host1.example.com.".into(),
                }]),
            ),
        )]),
    );

    let mut expected: Config = BTreeMap::new();
    expected.insert("example.com".into(), com);
    expected.insert("example.org".into(), org);

    assert_eq!(got, expected);
}

#[test]
fn subdomains_do_not_inherit_base_soa() {
    // A subdomain inherits a base domain's records (e.g. `a`) but never its SOA
    // (the legacy `soa.sub` defaults to null).
    let input = json!({
        "hosts": [{
            "defaultTTL": 3600,
            "baseDomains": { "example.com": {
                "a": { "data": "198.51.100.1" },
                "soa": { "data": {
                    "rname": "admin@example.invalid", "mname": "ns.example.invalid",
                    "serial": 1, "refresh": 2, "retry": 3, "expire": 4, "ttl": 5
                } }
            } },
            "subDomains": { "foo.example.com": {} }
        }]
    });
    let doc = parse_document(&input.to_string()).unwrap();
    let got = resolve_document(&doc).unwrap();

    let foo = &got["example.com"]["foo.example.com"];
    assert!(
        foo.contains_key("a"),
        "subdomain should inherit the base `a` record"
    );
    assert!(
        !foo.contains_key("soa"),
        "subdomain must not inherit the base SOA"
    );
}
