//! Compatibility input for the single-target `cloudflare` renderer.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use super::{sync, OutputFile};
use crate::model::{Config, RawDoc};
use crate::provider::ProviderKind;
use crate::Result;

pub use sync::{ExcludeRecord, Token};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudflareInput {
    pub dns_config: RawDoc,
    pub token: Token,
    pub zones: Option<BTreeMap<String, ZoneSettingsRaw>>,
    #[serde(default = "super::empty_object")]
    pub extra_provider_settings: Value,
    #[serde(default = "super::empty_object")]
    pub extra_global_config: Value,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneSettingsRaw {
    pub mode: Option<String>,
    pub manage_record_types: Option<Vec<String>>,
    #[serde(default)]
    pub exclude_records: Vec<ExcludeRecord>,
    #[serde(default)]
    pub processors: Vec<String>,
}

pub fn render(input: &CloudflareInput, config: &Config, out_abs: &str) -> Result<Vec<OutputFile>> {
    let mut settings = input
        .extra_provider_settings
        .as_object()
        .cloned()
        .unwrap_or_default();
    let min_ttl_override = settings
        .remove("min_ttl")
        .map(serde_json::from_value)
        .transpose()?;
    let provider = sync::ProviderConfig {
        kind: ProviderKind::Cloudflare,
        token: Some(input.token.clone()),
        settings,
        min_ttl_override,
    };
    let zones = config
        .keys()
        .map(|zone| {
            let raw = input
                .zones
                .as_ref()
                .and_then(|zones| zones.get(zone))
                .cloned()
                .unwrap_or_default();
            (
                zone.clone(),
                sync::ZoneSync {
                    targets: vec!["cloudflare".to_string()],
                    mode: raw.mode,
                    manage_record_types: raw.manage_record_types,
                    exclude_records: raw.exclude_records,
                    processors: raw.processors,
                },
            )
        })
        .collect();
    sync::render(
        &sync::SyncInput {
            dns_config: input.dns_config.clone(),
            providers: BTreeMap::from([("cloudflare".to_string(), provider)]),
            zones,
            extra_global_config: input.extra_global_config.clone(),
        },
        config,
        out_abs,
    )
}

/// Runtime file-token information for existing consumers that supply their own launcher.
pub fn file_token_env(token: &Token) -> Option<(String, String)> {
    match token {
        Token::File { path, env_name } => Some((
            env_name
                .clone()
                .unwrap_or_else(|| ProviderKind::Cloudflare.caps().default_env.to_string()),
            path.clone(),
        )),
        _ => None,
    }
}
