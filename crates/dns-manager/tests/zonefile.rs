//! Zonefile renderer golden, ported from `testWriteZoneFile` in
//! `utils/tests/zonefiles.nix`. Records are built directly (no normalization),
//! mirroring the legacy `utils.zonefiles.write` which bypassed the module.

use dns_manager::model::{Caa, Mx, Record, RecordData, Soa, Srv, Sshfp, Tlsa, Uri, Zone};
use dns_manager::render::zonefile::{format_txt, render_zone};

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

fn name(entries: Vec<(&str, Record)>) -> std::collections::BTreeMap<String, Record> {
    entries
        .into_iter()
        .map(|(t, r)| (t.to_string(), r))
        .collect()
}

#[test]
fn txt_under_255_is_single_quoted_chunk() {
    assert_eq!(format_txt("meow"), "\"meow\"");
}

#[test]
fn txt_over_255_splits_into_quoted_chunks() {
    let input = format!(
        "v=DKIM1; k=rsa {}{}{}",
        "a".repeat(252),
        "b".repeat(255),
        "c".repeat(255)
    );
    let rendered = format_txt(&input);
    // Three internal quote-space-quote separators → four chunks.
    assert_eq!(rendered.matches("\" \"").count(), 3);
    assert!(rendered.starts_with('"') && rendered.ends_with('"'));
}

#[test]
fn txt_chunks_on_bytes_within_255_octets() {
    // 200 × "é" = 400 bytes. Codepoint chunking would emit one 200-char (400-byte)
    // chunk, exceeding the 255-octet DNS limit. Byte chunking must split it, and
    // every chunk must be a valid (<=255-byte) UTF-8 character-string.
    let value = "é".repeat(200);
    let rendered = format_txt(&value);
    let inner = &rendered[1..rendered.len() - 1];
    assert!(
        rendered.contains("\" \""),
        "a 400-byte TXT value must be split"
    );
    for chunk in inner.split("\" \"") {
        assert!(
            chunk.len() <= 255,
            "chunk {} bytes exceeds 255",
            chunk.len()
        );
    }
}

#[test]
fn renders_full_zone_golden() {
    let mut zone: Zone = Zone::new();

    zone.insert(
        "*.example.com".into(),
        name(vec![(
            "alias",
            rec(60, RecordData::Alias(strs(&["example.com"]))),
        )]),
    );
    zone.insert(
        "_443._tcp.example.com".into(),
        name(vec![(
            "tlsa",
            rec(
                3600,
                RecordData::Tlsa(vec![Tlsa {
                    usage: 3,
                    selector: 1,
                    matching_type: 1,
                    certificate_association_data:
                        "9c4b5e3816504bdf4cbcfcc5c1b41ac18f2def723ec0d8299543e6d06471e610".into(),
                }]),
            ),
        )]),
    );
    zone.insert(
        "_ftp._tcp.example.com".into(),
        name(vec![(
            "uri",
            rec(
                3600,
                RecordData::Uri(vec![Uri {
                    priority: 10,
                    weight: 5,
                    target: "ftp://example.com/public".into(),
                }]),
            ),
        )]),
    );
    zone.insert(
        "_xmpp._tcp.example.com".into(),
        name(vec![(
            "srv",
            rec(
                86400,
                RecordData::Srv(vec![Srv {
                    priority: 10,
                    weight: 5,
                    port: 5223,
                    target: "xmpp.example.com".into(),
                }]),
            ),
        )]),
    );

    let mut a = rec(60, RecordData::A(strs(&["198.51.100.42"])));
    a.ttl_auto = true;
    a.comment = Some("Cloudflare keeps this TTL automatic".into());
    zone.insert(
        "example.com".into(),
        name(vec![
            ("a", a),
            (
                "aaaa",
                rec(60, RecordData::Aaaa(strs(&["2001:db8:d9a2:5198::13"]))),
            ),
            (
                "caa",
                rec(
                    60,
                    RecordData::Caa(vec![Caa {
                        flags: 0,
                        tag: "issue".into(),
                        value: "letsencrypt.org".into(),
                    }]),
                ),
            ),
            (
                "ns",
                rec(
                    60,
                    RecordData::Ns(strs(&[
                        "ns1.example.invali",
                        "ns2.example.com",
                        "ns3.example.org",
                    ])),
                ),
            ),
            (
                "mx",
                rec(
                    60,
                    RecordData::Mx(vec![Mx {
                        exchange: "mail.example.com".into(),
                        preference: 10,
                    }]),
                ),
            ),
            (
                "soa",
                rec(
                    60,
                    RecordData::Soa(vec![Soa {
                        expire: 1209600,
                        mname: "ns.example.invalid".into(),
                        refresh: 7200,
                        retry: 3600,
                        rname: "admin.example.invalid".into(),
                        serial: 1970010100,
                        ttl: 60,
                    }]),
                ),
            ),
            (
                "txt",
                rec(60, RecordData::Txt(strs(&["v=spf1 a:mail.aq0.de -all"]))),
            ),
            (
                "sshfp",
                rec(
                    60,
                    RecordData::Sshfp(vec![Sshfp {
                        algorithm: 4,
                        fp_type: 2,
                        fingerprint:
                            "f4f4ada530e64b5e574ed4459d7a40424179fd03a766282a2c1060010719626f"
                                .into(),
                    }]),
                ),
            ),
        ]),
    );
    zone.insert(
        "mail.example.com".into(),
        name(vec![(
            "cname",
            rec(60, RecordData::Cname(strs(&["e-mail.provider.invalid"]))),
        )]),
    );
    zone.insert(
        "redirect.example.com".into(),
        name(vec![(
            "dname",
            rec(60, RecordData::Dname(strs(&["example.org"]))),
        )]),
    );

    let expected = "\
*.example.com. IN 60 ALIAS example.com
_443._tcp.example.com. IN 3600 TLSA 3 1 1 9c4b5e3816504bdf4cbcfcc5c1b41ac18f2def723ec0d8299543e6d06471e610
_ftp._tcp.example.com. IN 3600 URI 10 5 ftp://example.com/public
_xmpp._tcp.example.com. IN 86400 SRV 10 5 5223 xmpp.example.com
; Cloudflare keeps this TTL automatic
example.com. IN A 198.51.100.42
example.com. IN 60 AAAA 2001:db8:d9a2:5198::13
example.com. IN 60 CAA 0 issue letsencrypt.org
example.com. IN 60 MX 10 mail.example.com.
example.com. IN 60 NS ns1.example.invali.
example.com. IN 60 NS ns2.example.com.
example.com. IN 60 NS ns3.example.org.
example.com. IN 60 SOA ns.example.invalid. admin.example.invalid. ( 1970010100 7200 3600 1209600 60 )
example.com. IN 60 SSHFP 4 2 f4f4ada530e64b5e574ed4459d7a40424179fd03a766282a2c1060010719626f
example.com. IN 60 TXT \"v=spf1 a:mail.aq0.de -all\"
mail.example.com. IN 60 CNAME e-mail.provider.invalid
redirect.example.com. IN 60 DNAME example.org
";

    assert_eq!(render_zone(&zone), expected);
}
