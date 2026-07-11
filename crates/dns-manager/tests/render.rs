//! Renderer behaviour tests for the octoDNS and Cloudflare outputs, exercising
//! the example DNS data and the tricky provider-specific rules.

use dns_manager::model::RawDoc;
use dns_manager::render::caddy;
use dns_manager::render::cloudflare::{self, CloudflareInput};
use dns_manager::render::octodns::{self, OctodnsInput};
use dns_manager::render::OutputFile;
use dns_manager::resolve_document;
use serde_json::json;
use std::path::Path;

/// The example zone data, as `extraConfig` (mirrors `example/dns.nix`).
fn example_dns_config() -> serde_json::Value {
    json!({
        "hosts": [],
        "extraConfig": {
            "defaultTTL": 86400,
            "zones": {
                "example.com": {
                    "": {
                        "soa": { "data": {
                            "rname": "admin.example.invalid", "mname": "ns.example.invalid",
                            "serial": 1970010100, "refresh": 7200, "retry": 3600, "ttl": 60, "expire": 1209600
                        } },
                        "ns": { "data": ["ns1.invalid", "ns2.invalid", "ns3.invalid"] },
                        "txt": { "comment": "Public policy records for the zone apex",
                                 "data": ["meow", "v=spf1 a:mail.example.com -all"] },
                        "mx": { "ttlAuto": true, "data": { "exchange": "mail.example.com", "preference": 10 } }
                    }
                },
                "example.net": {
                    "": { "a": { "data": "203.0.113.73", "ttl": 60, "proxied": true } }
                },
                "example.invalid": {
                    "": { "a": { "data": "198.51.100.35", "ttl": 60 },
                          "aaaa": { "data": "2001:DB8:42fc::64", "ttl": 60 } },
                    "redirect": { "cname": { "data": "example.net" } }
                }
            }
        }
    })
}

fn content<'a>(files: &'a [OutputFile], path: &str) -> &'a str {
    &files
        .iter()
        .find(|f| f.path == path)
        .unwrap_or_else(|| panic!("missing output file {path}"))
        .content
}

#[test]
fn pkl_config_loads_current_record_shape() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let doc: RawDoc = runtime
        .block_on(pklx::eval_source_to_typed(
            r#"
hosts = new Listing {}
extraConfig = new Mapping {
  ["defaultTTL"] = 86400
  ["zones"] = new Mapping {
    ["example.com"] = new Mapping {
      [""] = new Mapping {
        ["txt"] = new Mapping {
          ["comment"] = "Public policy records for the zone apex"
          ["data"] = new Listing { "meow"; "v=spf1 -all" }
        }
      }
      ["mail._domainkey"] = new Mapping {
        ["txt"] = new Mapping {
          ["data"] = "v=DKIM1; k=rsa; p=abc"
        }
      }
    }
  }
}
"#,
            pklx::pklr::EvalOptions::default(),
        ))
        .unwrap();

    let config = resolve_document(&doc).unwrap();
    let zone = config.get("example.com").unwrap();
    assert!(zone.contains_key("example.com"));
    assert!(zone.contains_key("mail._domainkey.example.com"));
}

#[test]
fn external_pkl_example_loads() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let config_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example/dns.pkl");
    let doc: RawDoc = runtime
        .block_on(pklx::eval_to_typed(
            &config_path,
            pklx::pklr::EvalOptions::default(),
        ))
        .unwrap();

    let config = resolve_document(&doc).unwrap();
    let zone = config.get("example.com").unwrap();
    assert!(zone.contains_key("example.com"));
    assert!(zone.contains_key("mail._domainkey.example.com"));
    assert_eq!(doc.redirects.len(), 1);
}

#[test]
fn caddy_routes_preserve_request_uri() {
    let input: RawDoc = serde_json::from_value(json!({
        "redirects": [{
            "from": "tartanoglu.com",
            "to": "https://can.tartanoglu.com",
            "status": 301,
            "preservePath": true
        }]
    }))
    .unwrap();

    let routes = caddy::render_routes(&input);
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0]["match"][0]["host"][0], "tartanoglu.com");
    assert_eq!(routes[0]["handle"][0]["status_code"], 301);
    assert_eq!(
        routes[0]["handle"][0]["headers"]["Location"][0],
        "https://can.tartanoglu.com{http.request.uri}"
    );
}

#[test]
fn cloudflare_render_obeys_provider_rules() {
    let input: CloudflareInput = serde_json::from_value(json!({
        "dnsConfig": example_dns_config(),
        "token": { "type": "env", "name": "CLOUDFLARE_API_TOKEN" },
        "zones": { "example.com": { "mode": "lenient" } }
    }))
    .unwrap();
    let config = resolve_document(&input.dns_config).unwrap();
    let files = cloudflare::render(&input, &config, "/abs").unwrap();

    // Proxied flag surfaces under octodns.cloudflare (keys are JSON-quoted).
    let net = content(&files, "zones/example.net.yaml");
    assert!(
        net.contains("\"proxied\": true"),
        "example.net.yaml:\n{net}"
    );

    // Apex drops SOA and root NS, keeps txt + mx; comment is injected.
    let com = content(&files, "zones/example.com.yaml");
    assert!(
        com.contains("# Public policy records for the zone apex"),
        "{com}"
    );
    assert!(!com.contains("SOA"), "SOA must be dropped:\n{com}");
    assert!(!com.contains("\"NS\""), "root NS must be dropped:\n{com}");
    assert!(com.contains("\"auto-ttl\": true"), "{com}");
    assert!(
        com.contains("mail.example.com."),
        "MX exchange must be dotted:\n{com}"
    );

    // config.yaml wiring.
    let cfg = content(&files, "config.yaml");
    assert!(cfg.contains("octodns_cloudflare.CloudflareProvider"));
    assert!(cfg.contains("env/CLOUDFLARE_API_TOKEN"));
    assert!(cfg.contains("min_ttl"));
    assert!(cfg.contains("ignore-root-ns"));
    assert!(cfg.contains("IgnoreRootNsFilter"));
    assert!(cfg.contains("/abs/zones"));
}

#[test]
fn cloudflare_exclude_and_manage_types_processors() {
    let input: CloudflareInput = serde_json::from_value(json!({
        "dnsConfig": example_dns_config(),
        "token": { "type": "literal", "value": "secret" },
        "zones": {
            "example.com": {
                "manageRecordTypes": ["A", "AAAA"],
                "excludeRecords": [ { "name": "_acme-challenge.example.com", "type": "TXT" } ]
            }
        }
    }))
    .unwrap();
    let config = resolve_document(&input.dns_config).unwrap();
    let files = cloudflare::render(&input, &config, "/abs").unwrap();
    let cfg = content(&files, "config.yaml");

    assert!(
        cfg.contains("manage-types-example.com"),
        "config.yaml:\n{cfg}"
    );
    assert!(cfg.contains("TypeAllowlistFilter"));
    assert!(cfg.contains("NameRejectlistFilter"));
    // Deterministic exclude-processor name: exclude-<safezone>-<type>-<safename>-<hash8>,
    // where `safe()` maps `.`/`_` → `-` (so the leading `_` yields `txt--acme`).
    assert!(
        cfg.contains("exclude-example-com-txt--acme-challenge-example-com-"),
        "exclude processor name:\n{cfg}"
    );
    // Literal token gets the safety comment.
    assert!(
        cfg.contains("Literal Cloudflare tokens are unsafe"),
        "{cfg}"
    );
}

#[test]
fn octodns_injects_fake_soa_and_zone_file_source() {
    let input: OctodnsInput = serde_json::from_value(json!({
        "dnsConfig": example_dns_config(),
        "config": { "providers": { "powerdns": { "class": "octodns_powerdns.PowerDnsProvider" } } },
        "zones": { "example.com.": { "sources": ["config"], "targets": ["powerdns"] } }
    }))
    .unwrap();
    let config = resolve_document(&input.dns_config).unwrap();
    let files = octodns::render(&input, &config, "/abs").unwrap();

    let cfg = content(&files, "config.yaml");
    assert!(cfg.contains("octodns_bind.ZoneFileSource"));
    assert!(cfg.contains("octodns_powerdns.PowerDnsProvider"));
    assert!(cfg.contains("/abs/zones"));

    // example.com already declares a SOA; example.net does not → fake injected.
    let net = content(&files, "zones/example.net");
    assert!(net.contains("SOA"), "fake SOA expected:\n{net}");
    let com = content(&files, "zones/example.com");
    assert_eq!(
        com.matches("SOA").count(),
        1,
        "exactly one (real) SOA:\n{com}"
    );
}
