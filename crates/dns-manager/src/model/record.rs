//! Typed DNS record payloads.
//!
//! [`RecordData`] is the canonical, *normalized* representation a record carries
//! after [`crate::resolve`] runs. The enum variant **is** the DNS record type, so
//! the renderers can dispatch on it directly.

use serde::{Deserialize, Serialize};

/// A fully resolved record: its TTL policy, provider flags, optional comment, and
/// typed payload. One [`Record`] holds *all* values of a single type at a name
/// (e.g. three `NS` targets live in one `Record` with three [`RecordData::Ns`]
/// entries), mirroring how a zone groups same-type RRs.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Record {
    /// Time-to-live in seconds. Ignored on output when [`ttl_auto`](Self::ttl_auto) is set.
    pub ttl: i64,
    /// Whether the provider should manage the TTL automatically (Cloudflare "auto").
    pub ttl_auto: bool,
    /// Whether supported providers should proxy this record (Cloudflare orange-cloud).
    pub proxied: bool,
    /// Optional provider-side / zonefile comment.
    pub comment: Option<String>,
    /// The typed record payload; the variant encodes the record type.
    pub data: RecordData,
}

/// Typed RDATA. The variant name is the lowercase record type, which is also the
/// key used in the resolved config maps.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordData {
    A(Vec<String>),
    Aaaa(Vec<String>),
    Cname(Vec<String>),
    Alias(Vec<String>),
    Dname(Vec<String>),
    Ns(Vec<String>),
    Txt(Vec<String>),
    Mx(Vec<Mx>),
    Soa(Vec<Soa>),
    Srv(Vec<Srv>),
    Uri(Vec<Uri>),
    Caa(Vec<Caa>),
    Tlsa(Vec<Tlsa>),
    Sshfp(Vec<Sshfp>),
}

impl RecordData {
    /// The lowercase record-type tag (`"a"`, `"mx"`, …) used as the map key.
    pub fn type_key(&self) -> &'static str {
        match self {
            RecordData::A(_) => "a",
            RecordData::Aaaa(_) => "aaaa",
            RecordData::Cname(_) => "cname",
            RecordData::Alias(_) => "alias",
            RecordData::Dname(_) => "dname",
            RecordData::Ns(_) => "ns",
            RecordData::Txt(_) => "txt",
            RecordData::Mx(_) => "mx",
            RecordData::Soa(_) => "soa",
            RecordData::Srv(_) => "srv",
            RecordData::Uri(_) => "uri",
            RecordData::Caa(_) => "caa",
            RecordData::Tlsa(_) => "tlsa",
            RecordData::Sshfp(_) => "sshfp",
        }
    }

    /// The uppercase BIND/octoDNS record-type token (`"A"`, `"MX"`, …).
    pub fn type_token(&self) -> String {
        self.type_key().to_uppercase()
    }

    /// Number of values carried (used by empty-record filtering and the single
    /// vs. plural `value`/`values` decision in octoDNS rendering).
    pub fn len(&self) -> usize {
        match self {
            RecordData::A(v)
            | RecordData::Aaaa(v)
            | RecordData::Cname(v)
            | RecordData::Alias(v)
            | RecordData::Dname(v)
            | RecordData::Ns(v)
            | RecordData::Txt(v) => v.len(),
            RecordData::Mx(v) => v.len(),
            RecordData::Soa(v) => v.len(),
            RecordData::Srv(v) => v.len(),
            RecordData::Uri(v) => v.len(),
            RecordData::Caa(v) => v.len(),
            RecordData::Tlsa(v) => v.len(),
            RecordData::Sshfp(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mx {
    pub preference: i64,
    pub exchange: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Soa {
    pub mname: String,
    pub rname: String,
    pub serial: i64,
    pub refresh: i64,
    pub retry: i64,
    pub expire: i64,
    pub ttl: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Srv {
    pub priority: i64,
    pub weight: i64,
    pub port: i64,
    pub target: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Uri {
    pub priority: i64,
    pub weight: i64,
    /// Target URI (RFC 3986). The legacy Nix module mistyped this as an integer;
    /// it is a string here.
    pub target: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Caa {
    pub flags: i64,
    pub tag: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tlsa {
    pub usage: i64,
    pub selector: i64,
    pub matching_type: i64,
    pub certificate_association_data: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sshfp {
    pub algorithm: i64,
    /// SSHFP fingerprint algorithm selector (the wire field literally named "type").
    #[serde(rename = "type")]
    pub fp_type: i64,
    pub fingerprint: String,
}
