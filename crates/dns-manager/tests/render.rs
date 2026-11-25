//! Renderer behaviour tests for the octoDNS and Cloudflare outputs, exercising
//! the example DNS data and the tricky provider-specific rules.

use dns_manager::model::RawDoc;
use dns_manager::render::caddy;
use dns_manager::render::cloudflare::{self, CloudflareInput};
use dns_manager::render::octodns::{self, OctodnsInput};
use dns_manager::render::sync::{self, SyncInput};
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
fn cloudflare_exclusions_keep_sibling_types_at_the_apex() {
    let mut dns = example_dns_config();
    dns["extraConfig"]["zones"]["example.com"][""]["a"] = json!({"data": "203.0.113.73"});
    let input: CloudflareInput = serde_json::from_value(json!({
        "dnsConfig": dns,
        "token": { "type": "literal", "value": "secret" },
        "zones": {
            "example.com": {
                "manageRecordTypes": ["A", "MX", "TXT"],
                "excludeRecords": [
                    { "name": "example.com.", "type": "a" },
                    { "name": "@", "type": "A" },
                    { "name": "_acme-challenge.example.com", "type": "TXT" }
                ]
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
    let zone = content(&files, "zones/example.com.yaml");
    assert_eq!(zone.matches("\"type\": \"A\"").count(), 1, "{zone}");
    assert!(zone.contains("\"value\": \"192.0.2.1\""), "{zone}");
    assert!(!zone.contains("203.0.113.73"), "{zone}");
    assert_eq!(zone.matches("\"ignored\": true").count(), 2, "{zone}");
    assert!(
        zone.contains("\"type\": \"MX\"") && zone.contains("meow"),
        "{zone}"
    );
    assert!(zone.contains("\"_acme-challenge\":"), "{zone}");
    assert!(!cfg.contains("NameRejectlistFilter"));
    // Literal token gets the safety comment.
    assert!(
        cfg.contains("Literal Cloudflare tokens are unsafe"),
        "{cfg}"
    );
}

fn sync_input(targets: &[&str], records: serde_json::Value) -> SyncInput {
    serde_json::from_value(json!({
        "dnsConfig": {"extraConfig": {"defaultTTL": 3600, "zones": {
            "example.com": {"": records},
            "unselected.example": {"www": {"a": {"data": "192.0.2.5"}}}
        }}},
        "providers": {
            "cf": {"kind": "cloudflare", "token": {"type": "env", "name": "NEW_CF_TOKEN"}},
            "eu": {"kind": "desec", "token": {"type": "file", "path": "/run/keys/desec"}},
            "hetzner": {"kind": "hetzner", "settings": {"apply_disabled": true}},
            "gcore": {"kind": "gcore"},
            "bunny": {"kind": "bunny"}
        },
        "zones": {"example.com.": {"targets": targets, "mode": "strict"}},
        "extraGlobalConfig": {"manager": {"max_workers": 2}}
    }))
    .unwrap()
}

fn render_sync(input: &SyncInput) -> dns_manager::Result<Vec<OutputFile>> {
    sync::render(input, &resolve_document(&input.dns_config)?, "/abs")
}

#[test]
fn multi_provider_config_and_migration() {
    let records = json!({"txt": {"data": "new policy", "ttlAuto": true, "comment": "DNS policy"}});
    let files = render_sync(&sync_input(&["eu", "cf"], records.clone())).unwrap();
    assert_eq!(files.len(), 2, "unselected zones are not rendered");
    let cfg = content(&files, "config.yaml");
    for snippet in [
        "octodns.provider.yaml.YamlProvider",
        "octodns_cloudflare.CloudflareProvider",
        "octodns_desec.DesecProvider",
        "octodns_hetzner.HetznerProvider",
        "octodns_gcore.GCoreProvider",
        "octodns_bunny.BunnyProvider",
        "\"token\": \"env/NEW_CF_TOKEN\"",
        "\"token\": \"env/DESEC_TOKEN\"",
        "\"token\": \"env/HETZNER_API_TOKEN\"",
        "\"token\": \"env/GCORE_API_KEY\"",
        "\"api_key\": \"env/BUNNY_API_KEY\"",
        "\"backend\": \"hcloud\"",
        "\"token_type\": \"APIKey\"",
        "\"apply_disabled\": true",
        "\"max_workers\": 2",
        "\"lenient\": false",
        "\"targets\":\n      - \"eu\"\n      - \"cf\"",
    ] {
        assert!(cfg.contains(snippet), "missing {snippet}:\n{cfg}");
    }
    assert!(!cfg.contains("/run/keys"), "{cfg}");
    let zone = content(&files, "zones/example.com.yaml");
    assert!(zone.contains("\"ttl\": 3600"), "{zone}");
    assert!(zone.contains("\"auto-ttl\": true"), "{zone}");
    assert!(zone.contains("\"comment\": \"DNS policy\""), "{zone}");
    for provider in ["eu", "hetzner", "gcore", "bunny"] {
        let files = render_sync(&sync_input(&[provider], records.clone())).unwrap();
        let zone = content(&files, "zones/example.com.yaml");
        assert!(zone.contains("\"ttl\": 3600"), "{zone}");
        assert!(
            !zone.contains("octodns") && !zone.contains("DNS policy"),
            "{zone}"
        );
    }
}

#[test]
fn sync_rejects_incompatible_records_on_any_target() {
    let alias = json!({"alias": {"data": "other.example"}});
    let cname = json!({"cname": {"data": "other.example"}});
    for (targets, records, expected) in [
        (
            vec!["cf", "eu"],
            json!({"a": {"data": "192.0.2.4", "proxied": true}}),
            "target eu: proxied",
        ),
        (vec!["cf", "eu"], alias.clone(), "target eu: apex ALIAS"),
        (
            vec!["cf", "hetzner"],
            alias.clone(),
            "target hetzner: apex ALIAS",
        ),
        (
            vec!["cf", "eu"],
            json!({"a": {"data": "192.0.2.4", "ttl": 300}}),
            "target eu: TTL 300 is below minimum 3600",
        ),
        (vec!["cf"], cname.clone(), "apex CNAME"),
        (vec!["eu"], cname, "apex CNAME"),
        (vec!["missing"], alias.clone(), "unknown target missing"),
    ] {
        let error = render_sync(&sync_input(&targets, records))
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
    for target in ["cf", "gcore", "bunny"] {
        let files = render_sync(&sync_input(&[target], alias.clone())).unwrap();
        assert!(content(&files, "zones/example.com.yaml").contains("\"type\": \"ALIAS\""));
    }
    let mut input = sync_input(&["eu"], json!({"a": {"data": "192.0.2.4", "ttl": 300}}));
    input.providers.get_mut("eu").unwrap().min_ttl_override = Some(300);
    let files = render_sync(&input).unwrap();
    assert!(content(&files, "zones/example.com.yaml").contains("\"ttl\": 300"));
    assert!(!content(&files, "config.yaml").contains("\"min_ttl\": 300"));
}

#[test]
fn sync_loads_the_typed_pkl_contract() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let schema = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pkl/DnsConfig.pkl");
    let source = format!(
        r#"
import "{schema}" as Dns
local input = new Dns.SyncInput {{
  dnsConfig {{
    extraConfig = new Dns.ExtraConfig {{
      defaultTTL = 3600
      zones {{
        ["example.com"] {{
          [""] {{
            ["txt"] {{ data = "example" }}
          }}
        }}
      }}
    }}
  }}
  providers {{ ["desec"] {{ kind = "desec" }} }}
  zones {{ ["example.com"] {{ targets {{ "desec" }}; excludeRecords {{ new {{ name = "@"; type = "A" }} }} }} }}
}}
dnsConfig = input.dnsConfig
providers = input.providers
zones = input.zones
extraGlobalConfig = input.extraGlobalConfig
"#,
        schema = schema.display()
    );
    let input: SyncInput = runtime
        .block_on(pklx::eval_source_to_typed(
            &source,
            pklx::pklr::EvalOptions::default(),
        ))
        .unwrap();
    let files = render_sync(&input).unwrap();
    assert!(content(&files, "zones/example.com.yaml").contains("\"ignored\": true"));
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
