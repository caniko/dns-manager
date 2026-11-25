//! Capabilities and octoDNS configuration for DNS sync targets.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Cloudflare,
    Desec,
    Hetzner,
    Gcore,
    Bunny,
}

pub struct Caps {
    pub class: &'static str,
    pub token_key: &'static str,
    pub default_env: &'static str,
    pub extra: &'static [(&'static str, &'static str)],
    pub proxied: bool,
    pub apex_alias: bool,
    pub comments: bool,
    pub min_ttl: i64,
}

impl ProviderKind {
    pub const fn caps(self) -> &'static Caps {
        match self {
            Self::Cloudflare => &Caps {
                class: "octodns_cloudflare.CloudflareProvider",
                token_key: "token",
                default_env: "CLOUDFLARE_API_TOKEN",
                extra: &[],
                proxied: true,
                apex_alias: true,
                comments: true,
                min_ttl: 1,
            },
            Self::Desec => &Caps {
                class: "octodns_desec.DesecProvider",
                token_key: "token",
                default_env: "DESEC_TOKEN",
                extra: &[],
                proxied: false,
                apex_alias: false,
                comments: false,
                min_ttl: 3600,
            },
            Self::Hetzner => &Caps {
                class: "octodns_hetzner.HetznerProvider",
                token_key: "token",
                default_env: "HETZNER_API_TOKEN",
                extra: &[("backend", "hcloud")],
                proxied: false,
                apex_alias: false,
                comments: false,
                min_ttl: 60,
            },
            Self::Gcore => &Caps {
                class: "octodns_gcore.GCoreProvider",
                token_key: "token",
                default_env: "GCORE_API_KEY",
                extra: &[("token_type", "APIKey")],
                proxied: false,
                apex_alias: true,
                comments: false,
                min_ttl: 120,
            },
            Self::Bunny => &Caps {
                class: "octodns_bunny.BunnyProvider",
                token_key: "api_key",
                default_env: "BUNNY_API_KEY",
                extra: &[],
                proxied: false,
                // Requires a plugin with ALIAS support; octodns-bunny 1.0.0 lacks it.
                apex_alias: true,
                comments: false,
                min_ttl: 60,
            },
        }
    }
}
